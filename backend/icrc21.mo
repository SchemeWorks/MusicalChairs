module {

    // ICRC-21 Types

    public type ConsentMessageMetadata = {
        language : Text;
        utc_offset_minutes : ?Int16;
    };

    public type DeviceSpec = {
        #GenericDisplay;
        #LineDisplay : {
            characters_per_line : Nat16;
            lines_per_page : Nat16;
        };
    };

    public type ConsentMessageSpec = {
        metadata : ConsentMessageMetadata;
        device_spec : ?DeviceSpec;
    };

    public type ConsentMessageRequest = {
        method : Text;
        arg : Blob;
        user_preferences : ConsentMessageSpec;
    };

    public type LineDisplayPage = {
        lines : [Text];
    };

    public type ConsentMessage = {
        #GenericDisplayMessage : Text;
        #LineDisplayMessage : { pages : [LineDisplayPage] };
    };

    public type ConsentInfo = {
        consent_message : ConsentMessage;
        metadata : ConsentMessageMetadata;
    };

    public type Icrc21Error = {
        #GenericError : { error_code : Nat; description : Text };
        #UnsupportedCanisterCall : { description : Text };
        #ConsentMessageUnavailable : { description : Text };
    };

    public type ConsentMessageResponse = {
        #Ok : ConsentInfo;
        #Err : Icrc21Error;
    };

    public type TrustedOriginsResponse = {
        trusted_origins : [Text];
    };

    public type StandardRecord = {
        name : Text;
        url : Text;
    };

    // ICRC-21: Generate human-readable consent messages
    public func consentMessage(request : ConsentMessageRequest) : ConsentMessageResponse {
        let label = switch (request.method) {
            case "saveCallerUserProfile" { ?"Set Display Name" };
            case "createGame" { ?"Open Investment Position" };
            case "withdrawEarnings" { ?"Withdraw Earnings" };
            case "addDealerMoney" { ?"Fund as Backer" };
            case "addDownstreamDealer" { ?"Fund as Series B Backer" };
            case "initializeAccessControl" { ?"Initialize Account" };
            case "depositICP" { ?"Deposit ICP" };
            case "withdrawICP" { ?"Withdraw ICP" };
            case _ { null };
        };

        switch (label) {
            case null {
                #Err(
                    #UnsupportedCanisterCall {
                        description = "Method '" # request.method # "' does not support consent messages.";
                    }
                );
            };
            case (?msg) {
                #Ok({
                    consent_message = #GenericDisplayMessage(msg);
                    metadata = request.user_preferences.metadata;
                });
            };
        };
    };

    // ICRC-28: Trusted origins
    public func trustedOrigins() : TrustedOriginsResponse {
        {
            trusted_origins = [
                "https://5qu42-fqaaa-aaaac-qecla-cai.icp0.io",
                "https://5qu42-fqaaa-aaaac-qecla-cai.raw.icp0.io",
                "https://musicalchairs.fun",
                "https://www.musicalchairs.fun",
            ];
        };
    };

    // ICRC-10: Supported standards
    public func supportedStandards() : [StandardRecord] {
        [
            { name = "ICRC-10"; url = "https://github.com/dfinity/ICRC/blob/main/ICRCs/ICRC-10/ICRC-10.md" },
            { name = "ICRC-21"; url = "https://github.com/dfinity/ICRC/blob/main/ICRCs/ICRC-21/ICRC-21.md" },
            { name = "ICRC-28"; url = "https://github.com/dfinity/ICRC/blob/main/ICRCs/ICRC-28/ICRC-28.md" },
        ];
    };

};
