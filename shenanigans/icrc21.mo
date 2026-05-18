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

    // ICRC-21: Generate human-readable consent messages.
    // Every update method that a signer wallet (Oisy) might call needs a label
    // here — without one, Oisy fails the icrc49_call_canister request entirely.
    public func consentMessage(request : ConsentMessageRequest) : ConsentMessageResponse {
        let methodLabel = switch (request.method) {
            // ── User-facing chip custody ─────────────────────────────────
            case "depositChips" { ?"Deposit PP to Position" };
            case "requestCashOut" { ?"Queue PP Redemption (7-day lockup)" };
            case "claimCashOut" { ?"Claim Pending PP Redemption" };
            case "cancelCashOut" { ?"Cancel Pending PP Redemption" };

            // ── Shenanigans (spells) ────────────────────────────────────
            case "castShenanigan" { ?"Cast Spell" };

            // ── Referral ────────────────────────────────────────────────
            case "registerReferral" { ?"Register Referrer" };
            case "getOrCreateReferralCode" { ?"Get Your Referral Code" };

            // ── Admin: shenanigan config ────────────────────────────────
            case "updateShenaniganConfig" { ?"Admin: Update Spell Config" };
            case "resetShenaniganConfig" { ?"Admin: Reset Spell Config" };
            case "saveAllShenaniganConfigs" { ?"Admin: Save All Spell Configs" };

            // ── Admin: mint config ──────────────────────────────────────
            case "setSimple21DayPpPerIcp" { ?"Admin: Set Simple PP Rate" };
            case "setCompounding15DayPpPerIcp" { ?"Admin: Set 15-day Compounding PP Rate" };
            case "setCompounding30DayPpPerIcp" { ?"Admin: Set 30-day Compounding PP Rate" };
            case "setBackerPpPerIcp" { ?"Admin: Set Backer PP Rate" };
            case "setReferralBps" { ?"Admin: Set Referral BPS" };
            case "setMinDepositPp" { ?"Admin: Set Min Deposit (PP)" };
            case "setCashOutDelaySeconds" { ?"Admin: Set Cash-Out Delay" };
            case "setObserverIntervalSeconds" { ?"Admin: Set Observer Interval" };
            case "setHousePrincipal" { ?"Admin: Set House Principal" };
            case "setCascadeBps" { ?"Admin: Set Cascade BPS" };
            case "setSignupGiftPp" { ?"Admin: Set Signup Gift" };
            case "setActivityRequiresDeposit" { ?"Admin: Set Activity-Requires-Deposit" };
            case "setActivityWindowDays" { ?"Admin: Set Activity Window" };

            // ── Admin: observer + misc ──────────────────────────────────
            case "stopObserver" { ?"Admin: Stop Observer" };
            case "resumeObserver" { ?"Admin: Resume Observer" };
            case "runObserverOnce" { ?"Admin: Run Observer Once" };
            case "primeObserverCursors" { ?"Admin: Prime Observer Cursors" };
            case "clearMissedGameMint" { ?"Admin: Clear Missed Game Mint" };
            case "clearMissedBackerMint" { ?"Admin: Clear Missed Backer Mint" };
            case "adminMint" { ?"Admin: Mint PP" };
            case "rotateAdmin" { ?"Admin: Rotate Admin" };
            case "initialize" { ?"Initialize Canister" };
            case "seedMigrationV2" { ?"Admin: Seed Migration V2" };

            case _ { null };
        };

        switch (methodLabel) {
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
