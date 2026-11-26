import Array "mo:base/Array";

/// Policy - the pure decisions of the ICP watchdog keeper. No I/O, no clock, no
/// state: everything here takes its inputs as arguments so that
/// `test/policy.test.mo` can pin the behaviour.
///
/// WHAT THE THRESHOLDS MEAN (spec section 7, ICP budget doc): 60 s without
/// progress is an ALERT, 300 s is an ESCALATION. They are monitoring signals,
/// not completion guarantees. The watchdog is the emergency BACKUP of an
/// external primary worker, so it signs only after the primary has been silent
/// for the full escalation window (`signAfterNs`), never earlier.
///
/// PROGRESS. `lastProgressAt` is the last time the on-chain progress marker
/// (round id, ticket phase/cursor, close stage) moved for a reason other than
/// this keeper's own landed step; see `observe`. While nothing is pending
/// (no open ticket, no close in progress) nothing can be stalled, so the clock
/// simply follows `now`.
module {
    public let ALERT_AFTER_NS : Nat = 60_000_000_000;
    public let SIGN_AFTER_NS : Nat = 300_000_000_000;

    // ------------------------------------------------------------------
    // The decision
    // ------------------------------------------------------------------

    public type Input = {
        now : Nat;
        lastProgressAt : Nat;
        /// 0 = this keeper never signed.
        lastSignedAt : Nat;
        alertAfterNs : Nat;
        signAfterNs : Nat;
        journalHasUnknown : Bool;
        configured : Bool;
    };

    public type Decision = { #Idle; #Alert; #Sign; #Blocked : Text };

    /// Saturating: a progress stamp in the future (clock skew) is zero staleness.
    public func staleFor(now : Nat, lastProgressAt : Nat) : Nat {
        if (now > lastProgressAt) { now - lastProgressAt } else { 0 };
    };

    /// Order matters and is the contract:
    ///   1. not configured                         -> Blocked
    ///   2. thresholds invalid                     -> Blocked (never an accidental Sign)
    ///   3. stale <  alertAfterNs                  -> Idle
    ///   4. stale <  signAfterNs                   -> Alert
    ///   5. unresolved unknown journal entry       -> Blocked (read its status first)
    ///   6. signed < alertAfterNs ago              -> Alert (wait for the result)
    ///   7. otherwise                              -> Sign
    public func decide(i : Input) : Decision {
        if (not i.configured) { return #Blocked("not configured") };
        if (i.signAfterNs == 0 or i.signAfterNs < i.alertAfterNs) { return #Blocked("invalid thresholds") };
        let stale = staleFor(i.now, i.lastProgressAt);
        if (stale < i.alertAfterNs) { return #Idle };
        if (stale < i.signAfterNs) { return #Alert };
        if (i.journalHasUnknown) { return #Blocked("unresolved unknown journal entry") };
        if (i.lastSignedAt != 0 and staleFor(i.now, i.lastSignedAt) < i.alertAfterNs) { return #Alert };
        #Sign;
    };

    // ------------------------------------------------------------------
    // Alert level for the external monitor (`getStatus`)
    // ------------------------------------------------------------------

    public type Level = { #Ok; #Alert; #Escalate };

    public func level(workPending : Bool, staleNs : Nat, alertAfterNs : Nat, signAfterNs : Nat) : Level {
        if (not workPending) { return #Ok };
        if (staleNs >= signAfterNs) { return #Escalate };
        if (staleNs >= alertAfterNs) { return #Alert };
        #Ok;
    };

    /// A keeper that is armed but whose timer has not fired for a while is dead
    /// (lost timer, out of cycles, frozen): the monitor must not read that as Ok.
    public func escalateIfDead(l : Level, armed : Bool, timerAgeNs : Nat, maxTimerAgeNs : Nat) : Level {
        if (armed and timerAgeNs > maxTimerAgeNs) {
            switch (l) { case (#Ok) { #Alert }; case (_) { l } };
        } else { l };
    };

    // ------------------------------------------------------------------
    // Progress tracker
    // ------------------------------------------------------------------

    public type Tracker = {
        /// Text form of the progress marker at the last observation.
        marker : Text;
        /// Last time the marker moved for a reason that was not our own landed step.
        lastProgressAt : Nat;
        /// Last time the marker moved because our own step landed.
        lastOwnProgressAt : Nat;
        observedAt : Nat;
    };

    public func newTracker(now : Nat) : Tracker {
        { marker = ""; lastProgressAt = now; lastOwnProgressAt = 0; observedAt = now };
    };

    /// Fold one observation into the tracker.
    ///   * nothing pending            -> the clock follows `now`;
    ///   * same marker                -> only `observedAt` moves;
    ///   * marker changed, own step landed recently (`ownLanded`)
    ///                                -> RESCUE CONTINUITY: the stale clock keeps
    ///                                   running, so the watchdog goes on with
    ///                                   the next step instead of waiting another
    ///                                   300 s after every step it lands itself;
    ///   * marker changed otherwise   -> foreign progress, reset the clock.
    public func observe(t : Tracker, marker : Text, workPending : Bool, now : Nat, ownLanded : Bool) : Tracker {
        if (not workPending) {
            return { marker; lastProgressAt = now; lastOwnProgressAt = t.lastOwnProgressAt; observedAt = now };
        };
        if (marker == t.marker) {
            return { t with observedAt = now };
        };
        if (ownLanded) {
            return { marker; lastProgressAt = t.lastProgressAt; lastOwnProgressAt = now; observedAt = now };
        };
        { marker; lastProgressAt = now; lastOwnProgressAt = t.lastOwnProgressAt; observedAt = now };
    };

    // ------------------------------------------------------------------
    // Polling cadence (the recurring timer fires often; work happens when due)
    // ------------------------------------------------------------------

    public func pollDelayNs(workPending : Bool, activePollNs : Nat, idlePollNs : Nat) : Nat {
        if (workPending) { activePollNs } else { idlePollNs };
    };

    public func due(now : Nat, nextPollAt : Nat) : Bool { now >= nextPollAt };

    // ------------------------------------------------------------------
    // Hourly signature budget
    // ------------------------------------------------------------------

    public func pruneWindow(times : [Nat], now : Nat, windowNs : Nat) : [Nat] {
        Array.filter<Nat>(times, func(t : Nat) : Bool { staleFor(now, t) < windowNs });
    };

    public func signaturesInWindow(times : [Nat], now : Nat, windowNs : Nat) : Nat {
        pruneWindow(times, now, windowNs).size();
    };

    public func budgetAllows(times : [Nat], now : Nat, windowNs : Nat, max : Nat) : Bool {
        signaturesInWindow(times, now, windowNs) < max;
    };

    // ------------------------------------------------------------------
    // Timer lifecycle (re-arm after an upgrade)
    // ------------------------------------------------------------------

    /// `Timer.recurringTimer` is NOT persisted across upgrades of a persistent
    /// actor. The stable `armed` flag survives and `postupgrade` re-arms from it.
    public func rearmAfterUpgrade(s : { armed : Bool; configured : Bool }) : Bool {
        s.armed and s.configured;
    };

    public type TimerState = { armed : Bool; timerLive : Bool };
    /// #Upgrade models the replica dropping live timers; #PostUpgrade is the
    /// actor's `system func postupgrade`.
    public type TimerEvent = { #Arm; #Disarm; #Upgrade; #PostUpgrade };
    public type TimerAction = { #Start; #Cancel; #None };

    public func timerStep(s : TimerState, configured : Bool, e : TimerEvent) : (TimerState, TimerAction) {
        switch (e) {
            case (#Arm) {
                if (not configured) { return (s, #None) };
                if (s.timerLive) { return ({ armed = true; timerLive = true }, #None) };
                ({ armed = true; timerLive = true }, #Start);
            };
            case (#Disarm) {
                if (s.timerLive) { return ({ armed = false; timerLive = false }, #Cancel) };
                ({ armed = false; timerLive = false }, #None);
            };
            case (#Upgrade) { ({ armed = s.armed; timerLive = false }, #None) };
            case (#PostUpgrade) {
                if (s.timerLive) { return (s, #None) };
                if (rearmAfterUpgrade({ armed = s.armed; configured })) { return ({ armed = true; timerLive = true }, #Start) };
                (s, #None);
            };
        };
    };
};
