import Principal "mo:base/Principal";

persistent actor class PonziMathSol(initArgs : {
    backendPrincipal : Principal;
    testAdmin : Principal;
}) = Self {
    transient let _BACKEND_PRINCIPAL : Principal = initArgs.backendPrincipal;
    transient let _TEST_ADMIN : Principal = initArgs.testAdmin;

    public query func ping() : async Text {
        "ponzi_math_sol skeleton";
    };
};
