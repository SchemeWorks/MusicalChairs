import Principal "mo:base/Principal";
import Nat64 "mo:base/Nat64";

module {
    public type CycleManagerTargetKind = {
        #SelfReport;
        #ControllerStatus;
        #InventoryOnly;
    };

    public type CycleManagerEnvironment = {
        #Production;
        #Staging;
        #Test;
        #Local;
        #Archived;
    };

    public type CycleManagerCriticality = {
        #Critical;
        #Important;
        #Standard;
        #Experimental;
    };

    public type CycleManagerCyclesStatus = {
        balance : Nat;
        low_watermark : Nat;
        healthy : Bool;
        freeze_threshold_secs : Nat64;
        stable_memory_bytes : ?Nat64;
        heap_memory_bytes : ?Nat64;
        idle_burn_cycles_per_day : ?Nat;
    };

    public type CycleManagerMetric = {
        key : Text;
        count : Nat64;
        value : Nat;
        label : ?Text;
    };

    public type CycleManagerTarget = {
        canister_id : Principal;
        canister_name : Text;
        display_name : Text;
        project : Text;
        environment : CycleManagerEnvironment;
        criticality : CycleManagerCriticality;
        kind : CycleManagerTargetKind;
        low_threshold_cycles : Nat;
        topup_cycles : Nat;
        owner : ?Text;
        tags : [Text];
        expected_controllers : [Principal];
        expected_freeze_threshold_secs : ?Nat64;
        metrics_schema_version : Nat32;
    };

    public type ControlledStatus = {
        canister_id : Principal;
        observed_at : Nat64;
        status : Text;
        cycles : Nat;
        memory_size : Nat;
        idle_cycles_burned_per_day : Nat;
        freezing_threshold_secs : Nat64;
        controllers : [Principal];
        module_hash : ?Blob;
    };
}
