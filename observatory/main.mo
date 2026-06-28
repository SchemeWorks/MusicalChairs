import Array "mo:base/Array";
import Debug "mo:base/Debug";
import Error "mo:base/Error";
import Int "mo:base/Int";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";
import Principal "mo:base/Principal";
import Time "mo:base/Time";

import Types "types";

shared ({ caller = initialAdmin }) persistent actor class MusicalChairsObservatory() = Self {
    type CycleManagerTarget = Types.CycleManagerTarget;
    type ControlledStatus = Types.ControlledStatus;

    type CanisterStatusResult = {
        status : { #running; #stopping; #stopped };
        memory_size : Nat;
        cycles : Nat;
        idle_cycles_burned_per_day : Nat;
        module_hash : ?Blob;
        settings : {
            controllers : [Principal];
            freezing_threshold : Nat;
        };
    };

    transient let ic : actor {
        canister_status : shared { canister_id : Principal } -> async CanisterStatusResult;
    } = actor "aaaaa-aa";

    var admins : [Principal] = if (Principal.isAnonymous(initialAdmin)) { [] } else { [initialAdmin] };
    var latestControlledStatuses : [ControlledStatus] = [];

    transient let PROJECT : Text = "musical-chairs";
    transient let DEFAULT_FREEZE_THRESHOLD_SECS : Nat64 = 2_592_000;

    transient let BACKEND_ID = Principal.fromText("5zxxg-tyaaa-aaaac-qeckq-cai");
    transient let PONZI_MATH_ID = Principal.fromText("guy42-yqaaa-aaaaj-qr5pq-cai");
    transient let PONZI_MATH_SOL_ID = Principal.fromText("spc6q-xyaaa-aaaac-qg2ma-cai");
    transient let SHENANIGANS_ID = Principal.fromText("j56tm-oaaaa-aaaac-qf34q-cai");
    transient let PP_LEDGER_ID = Principal.fromText("5xv2o-iiaaa-aaaac-qeclq-cai");
    transient let SIWS_PROVIDER_ID = Principal.fromText("tcm26-yqaaa-aaaac-qg2lq-cai");
    transient let FRONTEND_ID = Principal.fromText("5qu42-fqaaa-aaaac-qecla-cai");
    transient let PP_ASSETS_ID = Principal.fromText("4236a-haaaa-aaaac-qecma-cai");

    transient let CHARLES_CONTROLLER = Principal.fromText("6pwpo-d5iaw-mfjrn-owfb3-v4oz6-72woh-pc5t2-cwn73-zrzeq-4bjeh-tqe");
    transient let CONTROLLER_CANISTER = Principal.fromText("cpbhu-5iaaa-aaaad-aalta-cai");
    transient let ROBVECTOR_CONTROLLER = Principal.fromText("ft3ml-xex6k-ppiwj-ie6tc-zwkgb-ybm2x-eat4a-5p2jg-auzl3-latf4-aae");

    transient let TWO_PARTY_EXISTING_CONTROLLERS : [Principal] = [
        CHARLES_CONTROLLER,
        CONTROLLER_CANISTER,
    ];

    transient let THREE_PARTY_EXISTING_CONTROLLERS : [Principal] = [
        CHARLES_CONTROLLER,
        CONTROLLER_CANISTER,
        ROBVECTOR_CONTROLLER,
    ];

    func nowNat64() : Nat64 {
        Nat64.fromNat(Int.abs(Time.now()));
    };

    func natToNat64(n : Nat) : Nat64 {
        Nat64.fromNat(n);
    };

    func principalEq(a : Principal, b : Principal) : Bool {
        Principal.equal(a, b);
    };

    func isAdmin(caller : Principal) : Bool {
        if (Principal.isAnonymous(caller)) { return false };
        switch (Array.find<Principal>(admins, func(admin) { principalEq(admin, caller) })) {
            case (?_) { true };
            case (null) { false };
        };
    };

    func requireAdmin(caller : Principal) {
        if (not isAdmin(caller)) {
            Debug.trap("Unauthorized: admin only");
        };
    };

    func target(
        id : Principal,
        name : Text,
        display : Text,
        criticality : Types.CycleManagerCriticality,
        kind : Types.CycleManagerTargetKind,
        lowThreshold : Nat,
        topup : Nat,
        tags : [Text],
        expectedControllers : [Principal],
        schemaVersion : Nat32,
    ) : CycleManagerTarget {
        {
            canister_id = id;
            canister_name = name;
            display_name = display;
            project = PROJECT;
            environment = #Production;
            criticality;
            kind;
            low_threshold_cycles = lowThreshold;
            topup_cycles = topup;
            owner = null;
            tags;
            expected_controllers = expectedControllers;
            expected_freeze_threshold_secs = ?DEFAULT_FREEZE_THRESHOLD_SECS;
            metrics_schema_version = schemaVersion;
        };
    };

    func expectedControllers(existingControllers : [Principal]) : [Principal] {
        Array.append<Principal>(existingControllers, [Principal.fromActor(Self)]);
    };

    func ppLedgerExpectedControllers() : [Principal] {
        expectedControllers(THREE_PARTY_EXISTING_CONTROLLERS);
    };

    func siwsExpectedControllers() : [Principal] {
        expectedControllers(TWO_PARTY_EXISTING_CONTROLLERS);
    };

    func assetExpectedControllers() : [Principal] {
        expectedControllers(THREE_PARTY_EXISTING_CONTROLLERS);
    };

    func targets() : [CycleManagerTarget] {
        [
            target(
                BACKEND_ID,
                "backend",
                "Musical Chairs Backend",
                #Critical,
                #SelfReport,
                2_000_000_000_000,
                5_000_000_000_000,
                ["backend", "auth", "icp-ledger"],
                [],
                1,
            ),
            target(
                PONZI_MATH_ID,
                "ponzi_math",
                "Ponzi Math ICP Game",
                #Critical,
                #SelfReport,
                2_000_000_000_000,
                5_000_000_000_000,
                ["game", "icp", "ledger", "payouts"],
                [],
                1,
            ),
            target(
                PONZI_MATH_SOL_ID,
                "ponzi_math_sol",
                "Ponzi Math SOL Game",
                #Critical,
                #SelfReport,
                3_000_000_000_000,
                7_000_000_000_000,
                ["game", "solana", "chain-fusion", "rpc", "signing"],
                [],
                1,
            ),
            target(
                SHENANIGANS_ID,
                "shenanigans",
                "Shenanigans",
                #Important,
                #SelfReport,
                2_000_000_000_000,
                5_000_000_000_000,
                ["spells", "pp-ledger", "gameplay"],
                [],
                1,
            ),
            target(
                PP_LEDGER_ID,
                "pp_ledger",
                "Ponzi Points Ledger",
                #Critical,
                #ControllerStatus,
                3_000_000_000_000,
                7_000_000_000_000,
                ["ledger", "icrc"],
                ppLedgerExpectedControllers(),
                0,
            ),
            target(
                SIWS_PROVIDER_ID,
                "siws_provider",
                "SIWS Provider",
                #Important,
                #ControllerStatus,
                1_000_000_000_000,
                3_000_000_000_000,
                ["auth", "solana"],
                siwsExpectedControllers(),
                0,
            ),
            target(
                FRONTEND_ID,
                "frontend",
                "Frontend Assets",
                #Important,
                #ControllerStatus,
                1_000_000_000_000,
                3_000_000_000_000,
                ["frontend", "assets"],
                assetExpectedControllers(),
                0,
            ),
            target(
                PP_ASSETS_ID,
                "pp_assets",
                "Ponzi Points Assets",
                #Standard,
                #ControllerStatus,
                1_000_000_000_000,
                3_000_000_000_000,
                ["assets", "token-media"],
                assetExpectedControllers(),
                0,
            ),
        ];
    };

    func isControlledStatusTarget(canister : Principal) : Bool {
        principalEq(canister, PP_LEDGER_ID)
        or principalEq(canister, SIWS_PROVIDER_ID)
        or principalEq(canister, FRONTEND_ID)
        or principalEq(canister, PP_ASSETS_ID);
    };

    func storeLatest(status : ControlledStatus) {
        latestControlledStatuses := Array.append<ControlledStatus>(
            Array.filter<ControlledStatus>(
                latestControlledStatuses,
                func(existing) { not principalEq(existing.canister_id, status.canister_id) },
            ),
            [status],
        );
    };

    func statusText(status : { #running; #stopping; #stopped }) : Text {
        switch (status) {
            case (#running) { "running" };
            case (#stopping) { "stopping" };
            case (#stopped) { "stopped" };
        };
    };

    public query func observatory_version() : async Text {
        "musical-chairs-observatory/0.1.0";
    };

    public query func cycle_manager_targets() : async [CycleManagerTarget] {
        targets();
    };

    public shared ({ caller }) func collect_controlled_statuses() : async { #Ok : Nat; #Err : Text } {
        requireAdmin(caller);

        var collected : Nat = 0;
        var failures : Nat = 0;
        var lastError : Text = "";

        for (item in targets().vals()) {
            if (item.kind == #ControllerStatus and isControlledStatusTarget(item.canister_id)) {
                try {
                    let observed = await ic.canister_status({ canister_id = item.canister_id });
                    storeLatest({
                        canister_id = item.canister_id;
                        observed_at = nowNat64();
                        status = statusText(observed.status);
                        cycles = observed.cycles;
                        memory_size = observed.memory_size;
                        idle_cycles_burned_per_day = observed.idle_cycles_burned_per_day;
                        freezing_threshold_secs = natToNat64(observed.settings.freezing_threshold);
                        controllers = observed.settings.controllers;
                        module_hash = observed.module_hash;
                    });
                    collected += 1;
                } catch (e) {
                    failures += 1;
                    lastError := item.canister_name # ": " # Error.message(e);
                };
            };
        };

        if (failures > 0) {
            if (collected > 0) {
                return #Err("Collected " # Nat.toText(collected) # " controlled status snapshots; failed " # Nat.toText(failures) # "; last error: " # lastError);
            };
            return #Err("No controlled status snapshots collected; last error: " # lastError);
        };
        #Ok(collected);
    };

    public query func controlled_statuses() : async [ControlledStatus] {
        latestControlledStatuses;
    };

    public query func controlled_status(canister : Principal) : async ?ControlledStatus {
        Array.find<ControlledStatus>(
            latestControlledStatuses,
            func(status) { principalEq(status.canister_id, canister) },
        );
    };

    public shared ({ caller }) func add_admin(admin : Principal) : async () {
        requireAdmin(caller);
        if (Principal.isAnonymous(admin)) {
            Debug.trap("anonymous admin not allowed");
        };
        if (not isAdmin(admin)) {
            admins := Array.append<Principal>(admins, [admin]);
        };
    };

    public shared ({ caller }) func remove_admin(admin : Principal) : async { #Ok; #Err : Text } {
        requireAdmin(caller);
        if (admins.size() <= 1) {
            return #Err("cannot remove final admin");
        };
        if (not isAdmin(admin)) {
            return #Err("admin not found");
        };
        admins := Array.filter<Principal>(admins, func(existing) { not principalEq(existing, admin) });
        if (admins.size() == 0) {
            admins := [caller];
            return #Err("cannot remove final admin");
        };
        #Ok;
    };

    public query func list_admins() : async [Principal] {
        admins;
    };
}
