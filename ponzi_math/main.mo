import Principal "mo:base/Principal";
import OrderedMap "mo:base/OrderedMap";
import Time "mo:base/Time";
import Nat "mo:base/Nat";
import Nat64 "mo:base/Nat64";
import Float "mo:base/Float";
import Int "mo:base/Int";
import Text "mo:base/Text";
import List "mo:base/List";
import Iter "mo:base/Iter";
import Debug "mo:base/Debug";
import Blob "mo:base/Blob";
import Error "mo:base/Error";

import Ledger "ledger";
import Icrc21 "icrc21";

persistent actor class PonziMath(initArgs : {
    backendPrincipal : Principal;
    testAdmin : Principal;
}) = Self {
    transient let BACKEND_PRINCIPAL : Principal = initArgs.backendPrincipal;
    transient let TEST_ADMIN : Principal = initArgs.testAdmin;
    transient let icpLedger : Ledger.LedgerActor = actor(Ledger.ICP_LEDGER_CANISTER_ID);
};
