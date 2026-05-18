/// Reginald — automated compliance-bot flavor pool. Pure data + selection.
/// Triggered from event emitters in main.mo. Hardcoded in v1; admin-editable
/// is explicitly out of scope.

import Nat "mo:base/Nat";
import Int "mo:base/Int";
import Time "mo:base/Time";

module {
    let spellBackfire : [Text] = [
        "Compliance notes: market forces are not your friends.",
        "Filing this one under \"learning experience\" for our records.",
        "Our adjusters have been alerted. Please remain calm.",
        "I have, on your behalf, removed you from the dividend memo.",
        "This is, statistically, what we'd call a self-inflicted loss event.",
    ];

    let rankUp : [Text] = [
        "Congratulations. Please review the Form 1099 obligations applicable to your new tier.",
        "I have prepared a fresh non-disclosure for your signature.",
        "Welcome to the next tier. Past performance is not indicative of future results.",
        "Your promotion has been escrowed pending compliance review.",
        "Your new title comes with a strictly metaphorical pay raise.",
    ];

    let roundResult : [Text] = [
        "This outcome is not indicative of future performance.",
        "We hope you enjoyed your round. We retain all marketing rights.",
        "A reminder that participation is voluntary and irreversible.",
    ];

    let buzzword : [Text] = [
        "Compliance flag: please refrain from forward-looking statements.",
        "Friendly reminder that nothing in this chat is investment advice.",
        "Our legal department wishes to disassociate from that sentence.",
        "Please attach a disclaimer to that thought.",
    ];

    let karma : [Text] = [
        "A generous contribution. The Foundation appreciates your appreciation.",
        "Your gratitude has been registered for tax purposes.",
        "Karma is processed in 4-7 business days.",
        "We have, on your behalf, set fire to a small portion of your net worth.",
    ];

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
            case (_) { [] };
        };
        if (pool.size() == 0) { return null };
        ?pool[pickIndex(pool.size())];
    };
};
