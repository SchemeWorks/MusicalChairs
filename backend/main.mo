import Principal "mo:base/Principal";
import OrderedMap "mo:base/OrderedMap";
import Text "mo:base/Text";
import Nat "mo:base/Nat";
import Debug "mo:base/Debug";

import AccessControl "authorization/access-control";
import Icrc21 "icrc21";

persistent actor {
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
