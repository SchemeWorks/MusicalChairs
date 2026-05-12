import Principal "mo:base/Principal";
import OrderedMap "mo:base/OrderedMap";
import Text "mo:base/Text";
import Nat "mo:base/Nat";
import Debug "mo:base/Debug";
import Error "mo:base/Error";

import AccessControl "authorization/access-control";
import Icrc21 "icrc21";
import Ledger "ledger";

persistent actor Self {
    transient let icpLedger : Ledger.LedgerActor = actor(Ledger.ICP_LEDGER_CANISTER_ID);

    // Access Control State
    let accessControlState = AccessControl.initState();

    // Reject anonymous principals on authenticated endpoints
    func requireAuthenticated(caller : Principal) {
        if (Principal.isAnonymous(caller)) {
            Debug.trap("Anonymous principal not allowed");
        };
    };

    // Text length limit
    func validateTextLength(text : Text, maxLen : Nat, fieldName : Text) {
        if (Text.size(text) > maxLen) {
            Debug.trap(fieldName # " exceeds maximum length of " # Nat.toText(maxLen) # " characters");
        };
    };

    // Initialize Access Control (first caller becomes admin; cannot be re-initialized)
    public shared ({ caller }) func initializeAccessControl() : async () {
        requireAuthenticated(caller);
        AccessControl.initialize(accessControlState, caller);
    };

    // Get Caller User Role
    public query ({ caller }) func getCallerUserRole() : async AccessControl.UserRole {
        AccessControl.getUserRole(accessControlState, caller);
    };

    // Get User Role for a given principal (anonymous-callable sibling)
    public query func getUserRole(user : Principal) : async AccessControl.UserRole {
        AccessControl.getUserRole(accessControlState, user);
    };

    // Assign Caller User Role
    public shared ({ caller }) func assignCallerUserRole(user : Principal, role : AccessControl.UserRole) : async () {
        AccessControl.assignRole(accessControlState, caller, user, role);
    };

    // Check if Caller is Admin
    public query ({ caller }) func isCallerAdmin() : async Bool {
        AccessControl.isAdmin(accessControlState, caller);
    };

    // Check if a given principal is admin (anonymous-callable sibling)
    public query func isAdmin(user : Principal) : async Bool {
        AccessControl.isAdmin(accessControlState, user);
    };

    // User Profile
    public type UserProfile = {
        name : Text;
    };

    transient let principalMap = OrderedMap.Make<Principal>(Principal.compare);
    var userProfiles = principalMap.empty<UserProfile>();

    public query ({ caller }) func getCallerUserProfile() : async ?UserProfile {
        principalMap.get(userProfiles, caller);
    };

    public query func getUserProfile(user : Principal) : async ?UserProfile {
        principalMap.get(userProfiles, user);
    };

    public shared ({ caller }) func saveCallerUserProfile(profile : UserProfile) : async () {
        requireAuthenticated(caller);
        validateTextLength(profile.name, 64, "Display name");
        userProfiles := principalMap.put(userProfiles, caller, profile);
    };

    // ========================================================================
    // ponzi_math canister reference. Set once at cutover by admin.
    // ========================================================================

    var ponziMathPrincipal : ?Principal = null;

    public shared ({ caller }) func setPonziMathPrincipal(p : Principal) : async () {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized: admin only");
        };
        ponziMathPrincipal := ?p;
    };

    public query func getPonziMathPrincipal() : async ?Principal {
        ponziMathPrincipal;
    };

    type PonziMathActor = actor {
        sweepCoverCharges : shared () -> async { #Ok : Nat; #Err : Text };
    };

    // ========================================================================
    // Pay Management — admin pay-out for accrued cover charges.
    // 1. Calls ponzi_math.sweepCoverCharges() to pull accumulated balance.
    // 2. Transfers `amount` from backend's ICP balance to `to`.
    // ========================================================================

    public shared ({ caller }) func payManagement(
        to : Principal,
        amount : Nat,
    ) : async { #Ok : Nat; #Err : Text } {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            return #Err("Unauthorized: admin only");
        };
        if (amount == 0) { return #Err("Amount must be greater than zero") };
        if (amount <= Ledger.ICP_TRANSFER_FEE) {
            return #Err("Amount must exceed the ledger transfer fee of " # Nat.toText(Ledger.ICP_TRANSFER_FEE) # " e8s");
        };

        let ponziMath : PonziMathActor = switch (ponziMathPrincipal) {
            case (null) { return #Err("ponzi_math principal not set") };
            case (?p) { actor(Principal.toText(p)) };
        };

        // Step 1: pull cover charges from ponzi_math. Ignore failures —
        // admin may want to pay out from pre-existing backend balance.
        let _ = try { await ponziMath.sweepCoverCharges() }
        catch (_) { #Err("sweep call failed; proceeding with existing backend balance") };

        // Step 2: transfer `amount` from backend's balance to `to`.
        let transferAmount : Nat = amount - Ledger.ICP_TRANSFER_FEE;
        let transferResult = try {
            await icpLedger.icrc1_transfer({
                from_subaccount = null;
                to = { owner = to; subaccount = null };
                amount = transferAmount;
                fee = null;
                memo = null;
                created_at_time = null;
            });
        } catch (e) {
            return #Err("Failed to contact ICP ledger: " # Error.message(e));
        };

        switch (transferResult) {
            case (#Ok(blockIndex)) { #Ok(blockIndex) };
            case (#Err(err)) {
                let msg = switch (err) {
                    case (#InsufficientFunds(_)) { "Backend has insufficient ICP. Sweep may not have funded enough yet." };
                    case (#BadFee(_)) { "Bad fee" };
                    case (#BadBurn(_)) { "Bad burn" };
                    case (#TooOld) { "Transaction too old" };
                    case (#CreatedInFuture(_)) { "Transaction created in future" };
                    case (#Duplicate(_)) { "Duplicate transaction" };
                    case (#TemporarilyUnavailable) { "Ledger temporarily unavailable" };
                    case (#GenericError(e)) { "Error: " # e.message };
                };
                #Err(msg);
            };
        };
    };

    // Backend's on-ledger ICP balance — usually the sum of swept cover charges
    // waiting to be paid out. Admin only.
    public shared ({ caller }) func getBackendICPBalance() : async Nat {
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            Debug.trap("Unauthorized");
        };
        try {
            await icpLedger.icrc1_balance_of({ owner = Principal.fromActor(Self); subaccount = null });
        } catch (_) { 0 };
    };

    // ICRC-21 Consent Messages
    public shared func icrc21_canister_call_consent_message(request : Icrc21.ConsentMessageRequest) : async Icrc21.ConsentMessageResponse {
        Icrc21.consentMessage(request);
    };

    // ICRC-28 Trusted Origins
    public query func icrc28_trusted_origins() : async Icrc21.TrustedOriginsResponse {
        Icrc21.trustedOrigins();
    };

    // ICRC-10 Supported Standards
    public query func icrc10_supported_standards() : async [Icrc21.StandardRecord] {
        Icrc21.supportedStandards();
    };
};
