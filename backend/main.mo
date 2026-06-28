import Principal "mo:base/Principal";
import OrderedMap "mo:base/OrderedMap";
import Text "mo:base/Text";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";
import Debug "mo:base/Debug";
import Error "mo:base/Error";
import Cycles "mo:base/ExperimentalCycles";

import AccessControl "authorization/access-control";
import CycleManager "../observatory/types";
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
        cmRoleChangeAttempts += 1;
        AccessControl.assignRole(accessControlState, caller, user, role);
        cmRoleChangeSuccesses += 1;
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

    transient let CYCLE_MANAGER_LOW_WATERMARK : Nat = 2_000_000_000_000;
    transient let CYCLE_MANAGER_FREEZE_THRESHOLD_SECS : Nat64 = 2_592_000;

    var cmUserProfileSaves : Nat64 = 0;
    var cmRoleChangeAttempts : Nat64 = 0;
    var cmRoleChangeSuccesses : Nat64 = 0;
    var cmPayManagementAttempts : Nat64 = 0;
    var cmPayManagementSuccesses : Nat64 = 0;
    var cmPayManagementFailures : Nat64 = 0;
    var cmSweepCoverChargeAttempts : Nat64 = 0;
    var cmSweepCoverChargeSuccesses : Nat64 = 0;
    var cmSweepCoverChargeFailures : Nat64 = 0;
    var cmIcpLedgerTransferAttempts : Nat64 = 0;
    var cmIcpLedgerTransferSuccesses : Nat64 = 0;
    var cmIcpLedgerTransferFailures : Nat64 = 0;
    var cmIcpLedgerBalanceQueries : Nat64 = 0;
    var cmUnauthorizedAdminOperations : Nat64 = 0;

    func cmCounter(key : Text, count : Nat64, labelValue : ?Text) : CycleManager.CycleManagerMetric {
        {
            key;
            count;
            value = Nat64.toNat(count);
            label = labelValue;
        };
    };

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
        cmUserProfileSaves += 1;
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
        cmPayManagementAttempts += 1;
        if (not (AccessControl.hasPermission(accessControlState, caller, #admin))) {
            cmUnauthorizedAdminOperations += 1;
            cmPayManagementFailures += 1;
            return #Err("Unauthorized: admin only");
        };
        if (amount == 0) {
            cmPayManagementFailures += 1;
            return #Err("Amount must be greater than zero");
        };
        if (amount <= Ledger.ICP_TRANSFER_FEE) {
            cmPayManagementFailures += 1;
            return #Err("Amount must exceed the ledger transfer fee of " # Nat.toText(Ledger.ICP_TRANSFER_FEE) # " e8s");
        };

        let ponziMath : PonziMathActor = switch (ponziMathPrincipal) {
            case (null) {
                cmPayManagementFailures += 1;
                return #Err("ponzi_math principal not set");
            };
            case (?p) { actor(Principal.toText(p)) };
        };

        // Step 1: pull cover charges from ponzi_math. Ignore failures —
        // admin may want to pay out from pre-existing backend balance.
        cmSweepCoverChargeAttempts += 1;
        let sweepResult = try { await ponziMath.sweepCoverCharges() }
        catch (_) { #Err("sweep call failed; proceeding with existing backend balance") };
        switch (sweepResult) {
            case (#Ok(_)) { cmSweepCoverChargeSuccesses += 1 };
            case (#Err(_)) { cmSweepCoverChargeFailures += 1 };
        };

        // Step 2: transfer `amount` from backend's balance to `to`.
        let transferAmount : Nat = amount - Ledger.ICP_TRANSFER_FEE;
        cmIcpLedgerTransferAttempts += 1;
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
            cmIcpLedgerTransferFailures += 1;
            cmPayManagementFailures += 1;
            return #Err("Failed to contact ICP ledger: " # Error.message(e));
        };

        switch (transferResult) {
            case (#Ok(blockIndex)) {
                cmIcpLedgerTransferSuccesses += 1;
                cmPayManagementSuccesses += 1;
                #Ok(blockIndex);
            };
            case (#Err(err)) {
                cmIcpLedgerTransferFailures += 1;
                cmPayManagementFailures += 1;
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
            cmUnauthorizedAdminOperations += 1;
            Debug.trap("Unauthorized");
        };
        cmIcpLedgerBalanceQueries += 1;
        try {
            await icpLedger.icrc1_balance_of({ owner = Principal.fromActor(Self); subaccount = null });
        } catch (_) { 0 };
    };

    public query func cycles_status() : async CycleManager.CycleManagerCyclesStatus {
        let balance = Cycles.balance();
        {
            balance;
            low_watermark = CYCLE_MANAGER_LOW_WATERMARK;
            healthy = balance >= CYCLE_MANAGER_LOW_WATERMARK;
            freeze_threshold_secs = CYCLE_MANAGER_FREEZE_THRESHOLD_SECS;
            stable_memory_bytes = null;
            heap_memory_bytes = null;
            idle_burn_cycles_per_day = null;
        };
    };

    public query func cycle_manager_metrics() : async [CycleManager.CycleManagerMetric] {
        [
            cmCounter("op:user_profile_save:count", cmUserProfileSaves, null),
            cmCounter("op:access_control_role_change:count", cmRoleChangeSuccesses, ?"success"),
            cmCounter("op:access_control_role_change:rejects", cmRoleChangeAttempts - cmRoleChangeSuccesses, ?"failed_or_trapped"),
            cmCounter("op:pay_management:count", cmPayManagementSuccesses, ?"success"),
            cmCounter("op:pay_management:rejects", cmPayManagementFailures, ?"failure"),
            cmCounter("op:sweep_cover_charges:count", cmSweepCoverChargeSuccesses, ?"success"),
            cmCounter("op:sweep_cover_charges:rejects", cmSweepCoverChargeFailures, ?"failure"),
            cmCounter("op:icp_ledger_transfer:count", cmIcpLedgerTransferSuccesses, ?"success"),
            cmCounter("op:icp_ledger_transfer:rejects", cmIcpLedgerTransferFailures, ?"failure"),
            cmCounter("op:icp_ledger_transfer:cycles", cmIcpLedgerTransferAttempts, ?"attempts"),
            cmCounter("op:icp_ledger_balance_query:count", cmIcpLedgerBalanceQueries, null),
            cmCounter("op:admin_unauthorized:rejects", cmUnauthorizedAdminOperations, null),
        ];
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
