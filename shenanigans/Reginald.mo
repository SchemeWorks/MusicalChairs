/// Reginald — automated compliance-bot flavor pool. Pure data + selection.
/// Triggered from event emitters in main.mo. Hardcoded in v1; admin-editable
/// is explicitly out of scope.

import Nat "mo:base/Nat";
import Int "mo:base/Int";
import Time "mo:base/Time";

module {
    public let spellBackfire : [Text] = [
        "Compliance notes: market forces are not your friends.",
        "Filing this one under \"learning experience\" for our records.",
        "Our adjusters have been alerted. Please remain calm.",
        "I have, on your behalf, removed you from the dividend memo.",
        "This is, statistically, what we'd call a self-inflicted loss event.",
    ];

    public let rankUp : [Text] = [
        "Congratulations. Please review the Form 1099 obligations applicable to your new tier.",
        "I have prepared a fresh non-disclosure for your signature.",
        "Welcome to the next tier. Past performance is not indicative of future results.",
        "Your promotion has been escrowed pending compliance review.",
        "Your new title comes with a strictly metaphorical pay raise.",
    ];

    public let roundResult : [Text] = [
        "The rotation completes. We continue.",
        "A measured round. The thesis holds.",
        "Position closed. The carry has been distributed.",
        "Another vintage in the books.",
        "The firm makes no editorial comment.",
        "We welcome the patient.",
        "Net of carry, of course.",
    ];

    public let tenderOffer : [Text] = [
        "Notable: a position has been taken private.",
        "Cap table integration in progress. We assist.",
        "A measured acquisition.",
        "The carry has been concentrated.",
        "Another position consolidates. The thesis holds.",
        "We have seen this thesis underwritten before.",
    ];

    public let whitelisted : [Text] = [
        "A new gold name on the cap table. We make no editorial comment.",
        "The firm welcomes the latest member of the patient class.",
        "Gold accrues to the patient. As ever.",
        "The leaderboard, denominated in self-regard.",
        "A measured purchase.",
    ];

    public let stimulus : [Text] = [
        "Money brrr. The firm has, as ever, no opinion.",
        "Aggregate windfall distributed. Net of carry.",
        "A measured stimulus.",
        "The LP base, on the whole, approves.",
        "We do not call it that.",
    ];

    public let bearRaid : [Text] = [
        "Coordinated short executed. The rotation continues.",
        "Drawdown across the LP base. The firm notes the event.",
        "A measured haircut.",
        "Carry redistribution, of a sort.",
        "The position holders, on the whole, do not approve.",
    ];

    public let foundersRound : [Text] = [
        "Another founder rounds up. We approve.",
        "Underwriting locked. The thesis is, of course, the speaker's.",
        "A measured raise.",
        "The flat round, denominated in confidence.",
    ];

    public let buzzword : [Text] = [
        "Compliance flag: please refrain from forward-looking statements.",
        "Friendly reminder that nothing in this chat is investment advice.",
        "Our legal department wishes to disassociate from that sentence.",
        "Please attach a disclaimer to that thought.",
    ];

    public let karma : [Text] = [
        "A generous contribution. The Foundation appreciates your appreciation.",
        "Your gratitude has been registered for tax purposes.",
        "Karma is processed in 4-7 business days.",
        "We have, on your behalf, set fire to a small portion of your net worth.",
    ];

    /// Returns the hardcoded default pool for the given trigger name, or []
    /// for unknown triggers. Used by main.mo to seed effectivePool fallbacks
    /// and to answer adminGetFlavorPoolDefaults queries.
    public func defaults(triggerKind : Text) : [Text] {
        switch (triggerKind) {
            case ("spellBackfire") { spellBackfire };
            case ("rankUp") { rankUp };
            case ("roundResult") { roundResult };
            case ("buzzword") { buzzword };
            case ("karma") { karma };
            case ("tenderOffer") { tenderOffer };
            case ("whitelisted") { whitelisted };
            case ("stimulus") { stimulus };
            case ("bearRaid") { bearRaid };
            case ("foundersRound") { foundersRound };
            case (_) { [] };
        };
    };

    /// Pseudo-random index selector seeded on Time.now(). Adequate for flavor.
    func pickIndex(size : Nat) : Nat {
        if (size == 0) { return 0 };
        let raw = Int.abs(Time.now());
        raw % size;
    };

    public func pickFor(triggerKind : Text) : ?Text {
        let pool : [Text] = switch (triggerKind) {
            case ("spellBackfire") { spellBackfire };
            case ("rankUp") { rankUp };
            case ("roundResult") { roundResult };
            case ("buzzword") { buzzword };
            case ("karma") { karma };
            case ("tenderOffer") { tenderOffer };
            case ("whitelisted") { whitelisted };
            case ("stimulus") { stimulus };
            case ("bearRaid") { bearRaid };
            case ("foundersRound") { foundersRound };
            case (_) { [] };
        };
        if (pool.size() == 0) { return null };
        ?pool[pickIndex(pool.size())];
    };
};
