import Array "mo:base/Array";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";

import Journal "Journal";
import Policy "Policy";
import Tx "Tx";

/// Gate - the pure local checks the tick applies before it spends a signature.
/// They are a module of their own so that `test/gate.test.mo` pins them; `main.mo`
/// only sequences them. None of them reads the chain.
module {
    // ------------------------------------------------------------------
    // Before planning (cheap, local): one transaction in flight + hourly budget
    // ------------------------------------------------------------------

    public type Local = { #Proceed; #Wait : Text; #Blocked : Text };

    public func beforePlan(journal : Journal.Journal, signTimes : [Nat], now : Nat, windowNs : Nat, maxPerWindow : Nat) : Local {
        if (Journal.unresolved(journal).size() > 0) { return #Wait("waiting for the previous transaction to resolve") };
        if (not Policy.budgetAllows(signTimes, now, windowNs, maxPerWindow)) { return #Blocked("hourly signature budget spent") };
        #Proceed;
    };

    // ------------------------------------------------------------------
    // After planning: attempts at this position
    // ------------------------------------------------------------------

    public type Position = { #Go; #Fallback; #Blocked : Text };

    /// The first try at a position uses the planned batch; after a failed or expired
    /// attempt an 8-row plan is re-planned as ONE row; `maxAttempts` failures block it.
    public func afterPlan(journal : Journal.Journal, position : Text, plannedRows : Nat, maxAttempts : Nat) : Position {
        switch (Journal.maySubmit(journal, position, maxAttempts)) {
            case (#No(m)) { #Blocked(m) };
            case (#Yes({ fallback })) { if (fallback and plannedRows > 1) { #Fallback } else { #Go } };
        };
    };

    // ------------------------------------------------------------------
    // Fee caps
    // ------------------------------------------------------------------

    /// `null` = fine. The per-transaction ceiling first, then the balance floor: the
    /// fee address must hold the configured floor PLUS this transaction's fee.
    public func feeGate(balance : Nat64, floor : Nat64, fee : Nat64, maxFee : Nat64) : ?Text {
        if (fee > maxFee) { return ?("fee " # Nat64.toText(fee) # " exceeds maxFeeLamportsPerTx " # Nat64.toText(maxFee)) };
        if (balance < floor + fee) {
            return ?("fee address balance " # Nat64.toText(balance) # " is below the floor " # Nat64.toText(floor) # " plus the " # Nat64.toText(fee) # " fee");
        };
        null;
    };

    // ------------------------------------------------------------------
    // Last look at the message about to be signed
    // ------------------------------------------------------------------

    /// Re-parse the compiled message: the fee address is account 0 and the ONLY signer,
    /// no instruction lists it, and every instruction targets one of `programs`
    /// (the native program and ComputeBudget). `null` = fine.
    public func checkMessage(message : Blob, feePayer : Blob, programs : [Blob]) : ?Text {
        let p = switch (Tx.parse(message)) { case (null) { return ?"the compiled message does not parse" }; case (?p) { p } };
        if (p.numRequiredSignatures != 1 or p.accountKeys.size() == 0 or p.accountKeys[0] != feePayer) { return ?"the message is not fee-payer-only" };
        for (ix in Tx.resolve(p).vals()) {
            if (Array.find<Blob>(programs, func(k : Blob) : Bool { k == ix.programId }) == null) { return ?"the message names a foreign program" };
            for (m in ix.accounts.vals()) {
                if (m.key == feePayer) { return ?"an instruction references the fee address" };
            };
        };
        null;
    };
};
