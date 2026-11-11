#!/usr/bin/env python3
"""IMPL-49h: actor-level signup-gift interleave regression on the LOCAL dfx replica (Codex R1).

Everything here talks to the real canisters through `dfx --network local` (127.0.0.1:4943). It is called
by gift-interleave.sh, which sets the environment, installs the restore-on-exit trap and refuses any
non-local network. Standard library only.

What it proves, and what it does not (the receipt repeats this):

  PART A  REAL stack: the committed shenanigans wasm (dfx-built, installed at the pinned id j56tm-...)
          next to the REAL pp_ledger (5xv2o-...) and the REAL siws_provider (tcm26-...). The legacy
          observer gets one qualifying game from a stub game source. Determinism comes from stopping and
          starting pp_ledger BETWEEN ticks. Observed: stopping the ledger makes the gift-mint await FAIL
          FAST (a caught reject), it does not leave the await pending; so Part A covers failed-mint ->
          legacy retry, failed legacy -> native takeover, native Reserved blocks legacy, and durability
          across `install --mode upgrade` at rest. It cannot hold the legacy await open.

  PART B  LATCH variant: a build of the SAME sources (main.mo / NativeRewards.mo byte-identical) whose only
          difference is the PP ledger id constant in PpLedger.mo, pointing at a latch ledger stub that can
          withhold the reply to the legacy gift mint. This is the deterministic hold: the legacy
          reservation is observed (owner legacy, Reserved) while its await is unresolved, a native
          bindAndReserve runs to completion inside the window, then the latch is released. The gifts it
          counts are the stub's mint records, NOT the real ledger's.

The 3,190-interleaving enumeration (native-rewards-gift-interleave.test.mo) is a pure model of the reservation
protocol and is NOT evidence for the actor wiring; nothing in this file relies on it.
"""
import base64, binascii, datetime, hashlib, json, os, shutil, subprocess, sys, threading, time

E = os.environ
ROOT = E.get("MC_REPLICA_ROOT") or os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
HERE = E.get("MC_REPLICA_HERE") or os.path.dirname(os.path.abspath(__file__))
STUBDIR = os.path.join(HERE, "legacy_games_stub")
BUILD_DIR = E.get("MC_GIFT_BUILD_DIR") or os.path.join(E.get("TMPDIR", "/tmp"), "mc-gift-latch-build")
RECEIPT_PATH = E.get("MC_GIFT_RECEIPT") or os.path.join(HERE, "receipt-gift-interleave-2026-10-15.json")
SKIP_BUILD = bool(E.get("MC_GIFT_SKIP_BUILD"))

LEDGER = "5xv2o-iiaaa-aaaac-qeclq-cai"
SIWS = "tcm26-yqaaa-aaaac-qg2lq-cai"
SHEN = "j56tm-oaaaa-aaaac-qf34q-cai"
BOUND_PUBKEY = "Bow1CGKGDB9mNxeWdw85E2aCthQ1oZX4oFEe7fYT17ew"
BOUND_PRINCIPAL = "m7ydg-dfxxf-u5wan-ft6m4-xmzeg-wj6mw-npaab-cn6oc-5tgu4-rm2s7-yqe"
GIFT_NET = 45_000_000_000  # 500 PP gift less the 10% cascade, in 1e-8 PP units
NS = 10**9

STEPS, CHECKS, OBS, FINDINGS, FAILED = [], [], [], [], []
LOCK = threading.Lock()
T_START = time.time()
IDS = {}  # name -> canister id
NAMES = {LEDGER: "pp_ledger", SIWS: "siws_provider", SHEN: "shenanigans"}


def now_iso():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def log(msg):
    print("[%6.1fs] %s" % (time.time() - T_START, msg), flush=True)


# ---------------------------------------------------------------- json / candid helpers
def n(x):
    if isinstance(x, int):
        return x
    return int(str(x).replace("_", ""))


def V(x):
    assert isinstance(x, dict) and len(x) == 1, x
    k = next(iter(x))
    return k, x[k]


def tag(x):
    return V(x)[0]


def opt(x):
    return x[0] if x else None


def shorten(s, k=700):
    return s if len(s) <= k else s[:k] + "...(%d chars)" % len(s)


def canon(x):
    return json.dumps(x, sort_keys=True)


def principal_bytes(text):
    s = text.replace("-", "").upper()
    s += "=" * (-len(s) % 8)
    raw = base64.b32decode(s)
    crc, body = raw[:4], raw[4:]
    assert binascii.crc32(body).to_bytes(4, "big") == crc, "bad principal checksum"
    return body


def chip_hex(text):
    """Shenanigans' chip subaccount for a principal: its bytes zero-padded to 32 (Subaccount.principalToChipSubaccount)."""
    b = principal_bytes(text)
    return (b + bytes(32 - len(b))).hex()


# ---------------------------------------------------------------- dfx
class Reply:
    def __init__(self, ok, val, err, rc=0):
        self.ok, self.val, self.err, self.rc = ok, val, err, rc


def dfx_call(cid, method, arg="()", identity=None, timeout=900):
    cmd = ["dfx"] + (["--identity", identity] if identity else [])
    cmd += ["canister", "call", cid, method, arg, "--network", "local", "--output", "json"]
    r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=timeout)
    if r.returncode == 0:
        out = r.stdout.strip()
        try:
            return Reply(True, json.loads(out) if out else None, "")
        except ValueError:
            return Reply(True, out, "")
    return Reply(False, None, (r.stderr or r.stdout).strip(), r.returncode)


def label_of(cid):
    return NAMES.get(cid, cid)


def record_step(entry):
    with LOCK:
        entry["n"] = len(STEPS) + 1
        STEPS.append(entry)


def call(step, cid, method, arg="()", identity=None, record=True):
    t0 = time.time()
    rep = dfx_call(cid, method, arg, identity)
    if record:
        record_step({"t": now_iso(), "step": step, "canister": label_of(cid), "canister_id": cid, "method": method,
                     "caller": identity or "operator (default dfx identity)", "arg": shorten(arg),
                     "ok": rep.ok, "reply": rep.val if rep.ok else None, "error": shorten(rep.err, 1800) if not rep.ok else None,
                     "seconds": round(time.time() - t0, 2)})
    return rep


def dfx_admin(step, *args, cwd=None, record=True, timeout=1200):
    cmd = ["dfx"] + list(args) + ["--network", "local"]
    t0 = time.time()
    r = subprocess.run(cmd, cwd=cwd or ROOT, capture_output=True, text=True, timeout=timeout)
    out = (r.stdout + "\n" + r.stderr).strip()
    if record:
        record_step({"t": now_iso(), "step": step, "dfx": " ".join(args), "cwd": cwd or ROOT, "ok": r.returncode == 0,
                     "output": shorten(out, 1500), "seconds": round(time.time() - t0, 2)})
    return r.returncode == 0, out


def canister_status(cid):
    ok, out = dfx_admin("status", "canister", "status", cid, record=False)
    st = mh = None
    for line in out.splitlines():
        if line.startswith("Status:"):
            st = line.split(":", 1)[1].strip()
        if line.startswith("Module hash:"):
            mh = line.split(":", 1)[1].strip()
    return st, mh


def wait_until(desc, fn, timeout=90, every=0.4):
    t0 = time.time()
    while True:
        v = fn()
        if v:
            return v
        if time.time() - t0 > timeout:
            log("TIMEOUT waiting for: " + desc)
            return None
        time.sleep(every)


def chk(cid, desc, cond, detail=None):
    cond = bool(cond)
    with LOCK:
        CHECKS.append({"id": cid, "check": desc, "pass": cond, "detail": detail})
        if not cond:
            FAILED.append(cid)
    log("%s %s: %s%s" % ("PASS" if cond else "FAIL", cid, desc, "" if cond or detail is None else "  [" + shorten(str(detail), 500) + "]"))
    return cond


def obs(oid, text, data=None):
    with LOCK:
        OBS.append({"id": oid, "observed": text, "data": data})
    log("OBSERVED %s: %s" % (oid, text))


def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


# ---------------------------------------------------------------- reading state
def R(cid, step=None):
    """The shared gift reservation of the bound principal: None or (owner, state, at_ns)."""
    rep = call(step or "getSignupGiftReservation", cid, "getSignupGiftReservation", '(principal "%s")' % BOUND_PRINCIPAL, record=step is not None)
    if not rep.ok:
        raise RuntimeError("getSignupGiftReservation failed: " + rep.err)
    v = opt(rep.val)
    if v is None:
        return None
    return (tag(v["owner"]), tag(v["state"]), n(v["at"]))


def eid(sig):
    return ('record { cluster_domain = blob "localnet"; program_id = "MCprogLocal1111111111111111111111111111111"; signature = "%s"; '
            'instruction_index = 0 : nat; inner_instruction_index = 0 : nat; event_schema_version = 1 : nat }' % sig)


def observe_native(stack, sig, step=None):
    arg = ('(record { event_id = %s; source_round_id = 1 : nat64; source_close_id = null; kind = variant { PositionOpened }; '
           'beneficiary_pubkey = "%s"; basis_lamports = 1_000_000_000 : nat64; plan = opt variant { Simple } })' % (eid(sig), BOUND_PUBKEY))
    return call(step or "observeNativeEvent " + sig, stack["id"], "observeNativeEvent", arg)


def bind_native(stack, sig, step=None):
    return call(step or "bindAndReserve " + sig, stack["id"], "bindAndReserve", "(%s)" % eid(sig))


def obl(stack, sig):
    rep = call("getNativeObligation", stack["id"], "getNativeObligation", "(%s)" % eid(sig), record=False)
    v = opt(rep.val) if rep.ok else None
    if v is None:
        return None
    legs = []
    for l in v["legs"]:
        st, pl = V(l["status"])
        legs.append({"index": n(l["index"]), "amount": n(l["amount"]), "holder": l["holder"], "role": tag(l["role"]), "status": st,
                     "block": n(pl["block"]) if st == "Committed" else None, "attempts": n(l["attempts"]), "created_at_time": n(l["created_at_time"])})
    return {"status": tag(v["status"]), "beneficiary": opt(v["beneficiary"]), "reserved_at": n(opt(v["reserved_at"])) if opt(v["reserved_at"]) else None,
            "legs": legs, "raw": v}


def exec_all(stack, sig, label):
    o = obl(stack, sig)
    res = []
    for l in o["legs"]:
        rep = call("%s: executeNativeRewardLeg %s leg %d (%s)" % (label, sig, l["index"], l["role"]), stack["id"], "executeNativeRewardLeg",
                   "(%s, %d : nat)" % (eid(sig), l["index"]))
        res.append(tag(rep.val) if rep.ok else "ERR:" + rep.err[:200])
    return res


def exec_leg(stack, sig, idx, label):
    rep = call(label, stack["id"], "executeNativeRewardLeg", "(%s, %d : nat)" % (eid(sig), idx))
    return tag(rep.val) if rep.ok else "ERR:" + shorten(rep.err, 300)


def retry_leg(stack, sig, idx, label):
    rep = call(label, stack["id"], "retryNativeRewardLeg", "(%s, %d : nat)" % (eid(sig), idx))
    return tag(rep.val) if rep.ok else "ERR:" + shorten(rep.err, 300)


# ---- ledgers (real: get_transactions; latch: getMints) -------------------------------------
def norm_mint(block, m, kind="mint"):
    sub, memo, cat = opt(m["to"]["subaccount"]), opt(m["memo"]), opt(m["created_at_time"])
    memo_text = None
    if memo:
        try:
            memo_text = bytes(memo).decode("utf-8")
        except UnicodeDecodeError:
            memo_text = None
    return {"block": block, "kind": kind, "owner": m["to"]["owner"], "sub_hex": bytes(sub).hex() if sub else None, "amount": n(m["amount"]),
            "memo_hex": bytes(memo).hex() if memo else None, "memo_text": memo_text, "created_at_time": n(cat) if cat else None}


def real_len():
    rep = call("ledger length", LEDGER, "get_transactions", "(record { start = 0 : nat; length = 1 : nat })", record=False)
    return n(rep.val["log_length"])


def real_supply():
    return n(call("ledger supply", LEDGER, "icrc1_total_supply", record=False).val)


def real_mints_since(start):
    out, pos = [], start
    end = real_len()
    while pos < end:
        rep = call("ledger page", LEDGER, "get_transactions", "(record { start = %d : nat; length = 50 : nat })" % pos, record=False)
        txs = rep.val["transactions"]
        if not txs:
            break
        first = n(rep.val["first_index"])
        for i, t in enumerate(txs):
            if t["kind"] == "mint" and t["mint"]:
                out.append(norm_mint(first + i, t["mint"][0]))
            else:
                out.append({"block": first + i, "kind": t["kind"]})
        pos = first + len(txs)
    return out


def latch_mints(stack):
    rep = call("latch getMints", stack["latch"], "getMints", record=False)
    return [norm_mint(n(m["block"]), m) for m in rep.val]


def mints_of(stack, since):
    return real_mints_since(since) if stack["ledger"] == "real" else latch_mints(stack)


def gifts_in(stack, mints):
    chip = chip_hex(BOUND_PRINCIPAL)
    return [m for m in mints if m.get("kind") == "mint" and m["owner"] == stack["id"] and m["sub_hex"] == chip and m["amount"] == GIFT_NET]


def legacy_gifts(mints):
    return [m for m in mints if m.get("memo_text") and m["memo_text"].startswith("signup-")]


def short_mints(mints):
    return [{k: v for k, v in m.items() if k != "memo_hex" or True} for m in mints]


# ---------------------------------------------------------------- stacks
REAL = {"name": "shenanigans", "id": SHEN, "dir": ROOT, "ledger": "real", "label": "REAL committed wasm + REAL pp_ledger"}
VARIANT = {"name": "shenanigans_latch", "id": None, "dir": BUILD_DIR, "ledger": "latch", "latch": None, "label": "LATCH variant (PP ledger id patched to the latch stub)"}


def game_arg():
    return ('(vec { record { id = 0 : nat; player = principal "%s"; plan = variant { simple21Day }; amount = 1.0 : float64; startTime = 1 : int; '
            'isCompounding = false; isActive = true; lastUpdateTime = 1 : int; accumulatedEarnings = 0.0 : float64; totalWithdrawn = 0.0 : float64 } })' % BOUND_PRINCIPAL)


def fresh(stack, label):
    """Clean shenanigans (reinstall), bootstrapped with an EMPTY game source so nothing is grandfathered, then one game for the bound principal."""
    sid, stub, me = stack["id"], IDS["stub"], IDS["me"]
    ok, out = dfx_admin("%s: reinstall %s (clean state)" % (label, stack["name"]), "canister", "install", stack["name"], "--mode", "reinstall", "--yes", cwd=stack["dir"])
    if not ok:
        raise RuntimeError("reinstall failed: " + out)
    call(label + ": stub.setGames([])", stub, "setGames", "(vec {})")
    call(label + ": initialize(legacy games stub)", sid, "initialize", '(principal "%s")' % stub)
    call(label + ": stopObserver (only manual runObserverOnce ticks)", sid, "stopObserver")
    call(label + ": seedMigrationV2 (bootstrap; nothing to grandfather)", sid, "seedMigrationV2")
    call(label + ": stub.setGames([one game for the bound principal])", stub, "setGames", game_arg())
    call(label + ": setNativeBridgeAllowlist([operator])", sid, "setNativeBridgeAllowlist", '(vec { principal "%s" })' % me)
    call(label + ": appendNativePolicyFromMintConfig(24h)", sid, "appendNativePolicyFromMintConfig", "(86_400_000_000_000 : nat)")
    boot = call(label + ": isBootstrapped", sid, "isBootstrapped").val
    chk(label + "-setup", "%s: clean state, bootstrapped, no reservation yet, observer stopped" % label, boot is True and R(sid) is None, [boot, R(sid)])


def tick(stack, label):
    t0 = time.time()
    rep = call(label, stack["id"], "runObserverOnce")
    return rep, round(time.time() - t0, 2)


def ledger_stop(label):
    dfx_admin(label + ": stop pp_ledger", "canister", "stop", LEDGER)
    st = wait_until("pp_ledger Stopped", lambda: canister_status(LEDGER)[0] == "Stopped", 60)
    chk(label + "-ledger-stopped", "pp_ledger is Stopped", st, canister_status(LEDGER)[0])


def ledger_start(label):
    dfx_admin(label + ": start pp_ledger", "canister", "start", LEDGER)
    st = wait_until("pp_ledger Running", lambda: canister_status(LEDGER)[0] == "Running", 60)
    chk(label + "-ledger-running", "pp_ledger is Running again", st, canister_status(LEDGER)[0])


def all_log_lines(stack):
    ok, out = dfx_admin("canister logs", "canister", "logs", stack["id"], record=False)
    return [l for l in out.splitlines() if l.strip()]


def log_mark(stack):
    return len(all_log_lines(stack))


def logs_of(stack, label, needle=None, tail=40, since=0):
    lines = all_log_lines(stack)[since:]
    ok = True
    hits = [l for l in lines if (needle in l)] if needle else lines[-tail:]
    record_step({"t": now_iso(), "step": label + ": canister logs (filtered)", "canister": stack["name"], "canister_id": stack["id"], "dfx": "canister logs",
                 "ok": ok, "needle": needle, "lines": hits[-tail:]})
    return hits


def upgrade(stack, label, timeout=300):
    try:
        ok, out = dfx_admin("%s: install --mode upgrade --wasm-memory-persistence keep" % label, "canister", "install", stack["name"], "--mode", "upgrade",
                            "--wasm-memory-persistence", "keep", "--yes", cwd=stack["dir"], timeout=timeout)
    except subprocess.TimeoutExpired:
        record_step({"t": now_iso(), "step": label + ": upgrade TIMED OUT", "ok": False})
        return False, "timeout"
    # postupgrade re-arms the observer timer (first fire after 10 s); stop it again at once so only manual ticks run
    if ok:
        call(label + ": stopObserver (postupgrade re-armed it)", stack["id"], "stopObserver")
    return ok, out


def wait_ledger_n_blocks_stable():
    return real_len()


# ================================================================= PART A (real ledger)
def scenario_A1():
    """Stopped ledger -> Failed(legacy); running ledger -> legacy re-claims its own Failed gift and fails AGAIN (real ledger traps on the 70-byte memo); native takes the Failed gift; one gift."""
    st, sid = REAL, SHEN
    log("A1: legacy mint fails (ledger stopped) -> Failed; legacy retry re-claims and fails on the real ledger's memo limit; native takes the gift")
    fresh(st, "A1")
    LM = log_mark(st)
    L0, S0 = real_len(), real_supply()
    observe_native(st, "gA1")  # observed up front so the later bind is one call
    ledger_stop("A1")
    ing = call("A1: INGRESS analogue: icrc1_fee on the stopped pp_ledger (what a caller outside sees)", LEDGER, "icrc1_fee")
    obs("A1-ingress-stopped", "a call from outside to the STOPPED pp_ledger: ok=%s, error=%s" % (ing.ok, shorten(ing.err, 300)), {"ok": ing.ok, "error": shorten(ing.err, 600)})
    rep, secs = tick(st, "A1: runObserverOnce with pp_ledger STOPPED")
    obs("A1-tick-duration", "the observer tick call returned in %.2fs (ok=%s); a pending await would have kept it open" % (secs, rep.ok), {"ok": rep.ok, "seconds": secs})
    r1 = R(sid, "A1: getSignupGiftReservation after the failed tick")
    chk("A1-failed", "after the failed mint (ledger stopped) the shared reservation is Failed{owner = legacy}", r1 and r1[0] == "legacy" and r1[1] == "Failed", r1)
    lg = logs_of(st, "A1", "Signup-gift mint failed", since=LM)
    chk("A1-log", "the legacy path logged the gift-mint failure with the caught error text", len(lg) >= 1, lg[-1:] if lg else None)
    obs("A1-failure-classification-stopped", "legacy gift mint, pp_ledger STOPPED: the plain unbounded await completed with a caught reject whose text is in log_lines (the canister logs the message text only, not a reject code); mintInternal maps any caught Error to #Err(\"ppLedger call failed: <text>\")",
        {"log_lines": lg[-2:]})
    gl = logs_of(st, "A1", "Mint attempt", since=LM)
    if gl:
        obs("A1-game-mint-retry", "the game mint failed the same way and is re-presented next tick", {"log_lines": gl[-2:]})
    ledger_start("A1")
    chk("A1-no-mint", "the failed attempt minted nothing on the real ledger", real_len() == L0, [real_len(), L0])
    LM2 = log_mark(st)
    rep, secs = tick(st, "A1: runObserverOnce with pp_ledger RUNNING (legacy retry: re-claims its own Failed gift)")
    r2 = R(sid, "A1: getSignupGiftReservation after the retry tick")
    chk("A1-reclaimed", "the retry tick re-claimed the gift (Failed -> Reserved -> finalized again): the reservation timestamp advanced", r1 and r2 and r2[0] == "legacy" and r2[2] > r1[2], [r1, r2])
    lg2 = logs_of(st, "A1", "Signup-gift mint failed", since=LM2)
    memo_trap = [l for l in lg2 if "memo" in l]
    obs("A1-failure-classification-memo", "legacy gift mint, pp_ledger RUNNING: the REAL local pp_ledger rejected the legacy gift memo ('signup-' + principal text = %d bytes); the ledger TRAPPED and the caller saw a caught reject" % len(("signup-" + BOUND_PRINCIPAL).encode()),
        {"log_lines": lg2[-2:], "memo_bytes": len(("signup-" + BOUND_PRINCIPAL).encode())})
    chk("A1-memo-trap", "observed: the second legacy attempt failed on the real ledger's memo limit (so the legacy gift cannot commit on THIS local ledger); reservation is Failed{legacy} again", bool(memo_trap) and r2 and r2[1] == "Failed", [lg2[-1:], r2])
    ms = real_mints_since(L0)
    chk("A1-no-gift-yet", "no gift mint on the real ledger so far; the game mint (6-byte memo) did commit", len(gifts_in(st, ms)) == 0 and any(m.get("memo_text") == "game-0" for m in ms), [(m["block"], m["amount"], m.get("memo_text")) for m in ms])
    # native takes the Failed(legacy) gift
    rb = bind_native(st, "gA1")
    o = obl(st, "gA1")
    r3 = R(sid, "A1: reservation after the native bind")
    chk("A1-takeover", "native bind takes the Failed(legacy) gift: Reserved WITH a signupGift leg; reservation Reserved{native}", rb.ok and tag(rb.val) == "Reserved" and o and any(l["role"] == "signupGift" for l in o["legs"]) and r3 and r3[:2] == ("native", "Reserved"), [rb.val if rb.ok else rb.err, r3, [(l["role"], l["amount"]) for l in o["legs"]] if o else None])
    res = exec_all(st, "gA1", "A1")
    chk("A1-legs", "all four native legs commit on the real ledger", res == ["Committed"] * 4, res)
    r4 = R(sid, "A1: reservation after the native legs")
    chk("A1-committed-native", "reservation is Committed{native}", r4 and r4[:2] == ("native", "Committed"), r4)
    ms = real_mints_since(L0)
    g = gifts_in(st, ms)
    chk("A1-one-gift", "exactly ONE gift mint (net 4.5e10 to the bound principal's chip) on the real ledger, and it is the native leg's (32-byte memo, no 'signup-' mint)", len(g) == 1 and len(legacy_gifts(ms)) == 0, [(m["block"], m["amount"], m["memo_hex"][:16] if m["memo_hex"] else None) for m in g])
    chk("A1-supply", "supply rose by the legacy game mints (9e10 + 1e10) plus the four native legs (4.5e10 + 5e9 + 5.4e11 + 6e10)", real_supply() - S0 == 100_000_000_000 + 650_000_000_000, real_supply() - S0)
    before = canon(R(sid))
    ok, _ = upgrade(st, "A1 (Committed{native})")
    chk("A1-upgrade", "install --mode upgrade --wasm-memory-persistence keep succeeded and Committed{native} is byte-identical after it", ok and canon(R(sid, "A1: reservation after upgrade")) == before, [before, canon(R(sid))])
    obs("A1-winner", "NATIVE won the gift on the real ledger (the legacy attempts failed twice: stopped ledger, then the memo limit)", {"gift_blocks": [m["block"] for m in g]})
    record_blocks("A1", ms)


def scenario_A2():
    """Failed(legacy) at rest survives an upgrade -> native takes the Failed gift -> Reserved(native) survives an upgrade -> legacy skips -> native commits -> Committed(native) survives an upgrade."""
    st, sid = REAL, SHEN
    log("A2: legacy Failed -> native takeover -> legacy skips; durability at each boundary")
    fresh(st, "A2")
    L0, S0 = real_len(), real_supply()
    observe_native(st, "gA2")
    ledger_stop("A2")
    tick(st, "A2: runObserverOnce with pp_ledger STOPPED")
    r1 = R(sid, "A2: reservation after the failed tick")
    chk("A2-failed", "reservation is Failed{legacy}", r1 and r1[:2] == ("legacy", "Failed"), r1)
    ledger_start("A2")
    before = canon(R(sid))
    ok, _ = upgrade(st, "A2 (Failed at rest)")
    chk("A2-up1", "Failed{legacy} is byte-identical after an upgrade", ok and canon(R(sid, "A2: reservation after upgrade 1")) == before, [before, canon(R(sid))])
    # native binds FIRST after the failure (the game was already re-presented? no: ticks are manual, so the next legacy attempt has not happened)
    rb = bind_native(st, "gA2")
    o = obl(st, "gA2")
    r2 = R(sid, "A2: reservation after the native bind")
    has_gift = o and any(l["role"] == "signupGift" for l in o["legs"])
    chk("A2-takeover", "native bind takes the Failed(legacy) gift: Reserved WITH a signupGift leg, reservation Reserved{native}", rb.ok and tag(rb.val) == "Reserved" and has_gift and r2 and r2[:2] == ("native", "Reserved") and len(o["legs"]) == 4, [rb.val if rb.ok else rb.err, r2, [(l["role"], l["amount"]) for l in o["legs"]] if o else None])
    before_o, before_r = canon(obl(st, "gA2")["raw"]), canon(R(sid))
    ok, _ = upgrade(st, "A2 (Reserved{native} with Pending legs)")
    chk("A2-up2", "Reserved{native} and the obligation (Pending legs) are byte-identical after an upgrade", ok and canon(R(sid)) == before_r and canon(obl(st, "gA2")["raw"]) == before_o, None)
    at_native = R(sid)[2]
    rep, secs = tick(st, "A2: runObserverOnce (legacy must skip the gift: native holds Reserved)")
    r3 = R(sid, "A2: reservation after the skipping tick")
    chk("A2-skip", "legacy skipped the gift: reservation still Reserved{native} with the SAME timestamp", r3 and r3[:2] == ("native", "Reserved") and r3[2] == at_native, [r3, at_native])
    ms = real_mints_since(L0)
    chk("A2-no-legacy-gift", "the tick minted the game PP but NO legacy 'signup-' mint and no gift", len(legacy_gifts(ms)) == 0 and len(gifts_in(REAL, ms)) == 0, [(m["block"], m["amount"], m.get("memo_text")) for m in ms])
    res = exec_all(st, "gA2", "A2")
    chk("A2-legs", "all four native legs commit", res == ["Committed"] * 4, res)
    r4 = R(sid, "A2: reservation after the native legs")
    chk("A2-committed", "reservation is Committed{native}", r4 and r4[:2] == ("native", "Committed"), r4)
    before = canon(R(sid))
    ok, _ = upgrade(st, "A2 (Committed{native})")
    chk("A2-up3", "Committed{native} is byte-identical after an upgrade", ok and canon(R(sid)) == before, None)
    tick(st, "A2: runObserverOnce after the commit (legacy must stay out)")
    ms = real_mints_since(L0)
    g = gifts_in(st, ms)
    chk("A2-one-gift", "exactly ONE gift on the real ledger, and it is the native gift leg (no 'signup-' memo)", len(g) == 1 and len(legacy_gifts(ms)) == 0, [(m["block"], m["amount"], m["memo_hex"][:16] if m["memo_hex"] else None) for m in g])
    obs("A2-winner", "native won the gift (legacy mint had failed first)", {"gift_blocks": [m["block"] for m in g]})
    chk("A2-supply", "supply rose by the legacy game mints (9e10 + 1e10) plus the four native legs (4.5e10 + 5e9 + 5.4e11 + 6e10 = 6.5e11)", real_supply() - S0 == 100_000_000_000 + 650_000_000_000, real_supply() - S0)
    record_blocks("A2", ms)


def scenario_A3():
    """Native Reserved blocks legacy (including through a SubmittedUnknown gift leg), then everything completes with one gift."""
    st, sid = REAL, SHEN
    log("A3: native Reserved first -> legacy never attempts the gift, even while the native gift leg is SubmittedUnknown")
    fresh(st, "A3")
    LM = log_mark(st)
    L0 = real_len()
    observe_native(st, "gA3")
    rb = bind_native(st, "gA3")
    o = obl(st, "gA3")
    r1 = R(sid, "A3: reservation after the native bind")
    gift_idx = [l["index"] for l in o["legs"] if l["role"] == "signupGift"]
    chk("A3-reserved", "native bind first: Reserved{native} with a signupGift leg", rb.ok and tag(rb.val) == "Reserved" and r1 and r1[:2] == ("native", "Reserved") and len(gift_idx) == 1, [rb.val if rb.ok else rb.err, r1, gift_idx])
    at0 = r1[2]
    ledger_stop("A3")
    tick(st, "A3: runObserverOnce with pp_ledger STOPPED (legacy must not even try the gift)")
    r2 = R(sid, "A3: reservation after that tick")
    chk("A3-blocked", "legacy did not touch the reservation: Reserved{native}, same timestamp", r2 and r2 == r1, [r1, r2])
    chk("A3-no-attempt-log", "no 'Signup-gift mint failed' log line: the legacy path never attempted a gift mint", len(logs_of(st, "A3", "Signup-gift mint failed", since=LM)) == 0)
    s = exec_leg(st, "gA3", gift_idx[0], "A3: executeNativeRewardLeg gift leg with pp_ledger STOPPED")
    chk("A3-submitted-unknown", "native gift leg with the ledger stopped -> SubmittedUnknown (a trapped/rejected call leaves it unknown)", s == "SubmittedUnknown", s)
    obs("A3-native-await-classification", "native executeNativeRewardLeg with the ledger stopped: the caught reject leaves the leg #SubmittedUnknown (CallFailed), not Retryable and not Quarantined", {"status": s})
    tick(st, "A3: runObserverOnce again (native gift leg is SubmittedUnknown)")
    r3 = R(sid)
    chk("A3-blocked-2", "still Reserved{native} through SubmittedUnknown", r3 and r3[:2] == ("native", "Reserved") and r3[2] == at0, r3)
    ledger_start("A3")
    s2 = retry_leg(st, "gA3", gift_idx[0], "A3: retryNativeRewardLeg gift leg (identical frozen fields)")
    chk("A3-retry", "retry commits the gift leg", s2 == "Committed", s2)
    exec_all(st, "gA3", "A3")
    tick(st, "A3: runObserverOnce (game mint now succeeds; legacy stays out of the gift)")
    ms = real_mints_since(L0)
    chk("A3-one-gift", "exactly ONE gift on the real ledger (native); no legacy 'signup-' mint", len(gifts_in(st, ms)) == 1 and len(legacy_gifts(ms)) == 0, [(m["block"], m["amount"]) for m in gifts_in(st, ms)])
    chk("A3-committed", "reservation Committed{native}", (R(sid) or ("",))[:2] == ("native", "Committed"), R(sid))
    obs("A3-winner", "native won the gift (it reserved before the legacy observer looked)", {"gift_blocks": [m["block"] for m in gifts_in(st, ms)]})
    record_blocks("A3", ms)


# ================================================================= PART B (latch variant)
class Bg:
    """Run a dfx call in a thread so it can stay pending while the main thread interleaves."""

    def __init__(self, step, cid, method, arg="()"):
        self.step, self.cid, self.method, self.arg = step, cid, method, arg
        self.rep, self.t0, self.t1, self.iso0 = None, time.time(), None, now_iso()
        self.th = threading.Thread(target=self.run, daemon=True)
        self.th.start()

    def run(self):
        self.rep = dfx_call(self.cid, self.method, self.arg)
        self.t1 = time.time()
        record_step({"t": self.iso0, "step": self.step, "canister": label_of(self.cid), "canister_id": self.cid, "method": self.method, "caller": "operator (default dfx identity)",
                     "arg": self.arg, "ok": self.rep.ok, "reply": self.rep.val if self.rep.ok else None, "error": shorten(self.rep.err, 1800) if not self.rep.ok else None,
                     "seconds": round(self.t1 - self.t0, 2), "note": "returned when the held await resolved"})

    def alive(self):
        return self.th.is_alive()

    def join(self, timeout=180):
        self.th.join(timeout)
        return not self.th.is_alive()


def latch_call(stack, step, method, arg="()"):
    return call(step, stack["latch"], method, arg)


def latch_status(stack):
    rep = call("latch getStatus", stack["latch"], "getStatus", record=False)
    v = rep.val
    return {"mode": tag(v["mode"]), "gate": v["gate"], "heldNow": n(v["heldNow"]), "heldEver": n(v["heldEver"]), "mints": n(v["mints"]), "rejected": n(v["rejected"])}


def arm_latch(stack, label, mode):
    latch_call(stack, label + ": latch.resetAll", "resetAll")
    latch_call(stack, label + ": latch.setHold(%s, \"signup-\")" % mode, "setHold", '(variant { %s }, "signup-")' % mode)
    s = latch_status(stack)
    chk(label + "-armed", "latch armed in mode %s, nothing held yet" % mode, s["gate"] and s["mode"] == mode and s["heldNow"] == 0, s)


def start_held_tick(stack, label):
    """Start runObserverOnce in the background and wait until the legacy reservation is visible while the call is still pending."""
    bg = Bg(label + ": runObserverOnce (will stay pending while the gift mint is held)", stack["id"], "runObserverOnce")

    def seen():
        if not bg.alive():
            return "ended"
        r = R(stack["id"])
        s = latch_status(stack)
        if r and r[:2] == ("legacy", "Reserved") and s["heldNow"] == 1:
            return (r, s)
        return None

    got = wait_until("legacy Reserved + held mint", seen, 120, 0.3)
    return bg, got


def scenario_H1():
    """The R1 regression: native bindAndReserve completes INSIDE the legacy gift-mint window; resume; exactly one gift."""
    st, sid = VARIANT, VARIANT["id"]
    log("H1: held legacy gift mint (reply withheld AFTER the ledger committed it) + native bind inside the window")
    fresh(st, "H1")
    arm_latch(st, "H1", "commitThenHold")
    observe_native(st, "gH1")
    bg, got = start_held_tick(st, "H1")
    ok_seen = got not in (None, "ended")
    chk("H1-reserved-before-await", "legacy reservation {owner legacy, Reserved} is visible while the gift-mint await is still unresolved (tick call pending, latch holding 1 call)", ok_seen and bg.alive(), got)
    if not ok_seen:
        latch_call(st, "H1: latch.release (abort)", "release")
        bg.join()
        return
    r_in, s_in = got
    reserve_at = r_in[2]
    held_mints = latch_mints(st)
    chk("H1-ledger-committed", "commitThenHold: the stub ledger has ALREADY committed the legacy gift mint (reply in flight): 1 gift mint recorded", len(gifts_in(st, held_mints)) == 1 and legacy_gifts(held_mints)[0]["memo_text"] == "signup-" + BOUND_PRINCIPAL, [(m["block"], m["amount"], m.get("memo_text")) for m in held_mints])
    rb = bind_native(st, "gH1", "H1: bindAndReserve INSIDE the window (real SIWS lookup, applyBind after it)")
    still = bg.alive()
    o = obl(st, "gH1")
    r_after_bind = R(sid, "H1: reservation after the native bind (tick still pending)")
    chk("H1-bind-in-window", "the native bind returned Reserved while the legacy tick call was STILL pending", rb.ok and tag(rb.val) == "Reserved" and still, [rb.val if rb.ok else rb.err, still])
    chk("H1-no-native-gift", "the native obligation has NO signupGift leg (player chain only)", o and not any(l["role"] == "signupGift" for l in o["legs"]) and len(o["legs"]) == 2, [(l["role"], l["amount"]) for l in o["legs"]] if o else None)
    chk("H1-reservation-untouched", "the legacy reservation was not disturbed by the bind: still Reserved{legacy}, same timestamp", r_after_bind == r_in, [r_in, r_after_bind])
    bind_at = o["reserved_at"]
    exec_all(st, "gH1", "H1 (inside the window)")
    still2 = bg.alive()
    chk("H1-legs-in-window", "the native legs executed while the legacy await was still pending", still2, still2)
    latch_call(st, "H1: latch.release (resume the legacy await)", "release")
    done = bg.join(180)
    chk("H1-tick-resumed", "the legacy tick resumed and returned after the release", done and bg.rep is not None and bg.rep.ok, bg.rep.err if bg.rep and not bg.rep.ok else None)
    r_end = R(sid, "H1: reservation after the resume")
    chk("H1-committed-legacy", "the resumed legacy path finalized: Committed{owner legacy}", r_end and r_end[:2] == ("legacy", "Committed"), r_end)
    commit_at = r_end[2] if r_end else None
    chk("H1-window-timestamps", "IC timestamps prove the bind was strictly inside the window: legacy reserve < native bind < legacy commit", reserve_at < bind_at < commit_at, {"legacy_reserved_at": reserve_at, "native_bind_reserved_at": bind_at, "legacy_committed_at": commit_at})
    ms = latch_mints(st)
    g = gifts_in(st, ms)
    chk("H1-one-gift", "exactly ONE gift mint recorded by the ledger stub after everything resumed (the legacy one); the native legs minted no gift", len(g) == 1 and legacy_gifts(ms)[0]["block"] == g[0]["block"], [(m["block"], m["amount"], m.get("memo_text")) for m in g])
    obs("H1-winner", "LEGACY won the gift; the native bind that landed inside its window correctly took none", {"timestamps": {"legacy_reserved_at": reserve_at, "native_bind_at": bind_at, "legacy_committed_at": commit_at}})
    record_blocks("H1", ms, "latch_ledger_stub")


def scenario_H3():
    """Held gift mint REJECTED (synthetic) while a native bind sits inside the window; a later native event takes the Failed gift."""
    st, sid = VARIANT, VARIANT["id"]
    log("H3: held legacy gift mint ends in a reject; native bind inside the window got no gift; a later native event takes the Failed gift")
    fresh(st, "H3")
    LM = log_mark(st)
    arm_latch(st, "H3", "holdThenReject")
    observe_native(st, "gH3a")
    observe_native(st, "gH3b")
    bg, got = start_held_tick(st, "H3")
    ok_seen = got not in (None, "ended")
    chk("H3-reserved-before-await", "legacy reservation Reserved{legacy} visible while the gift-mint await is unresolved", ok_seen and bg.alive(), got)
    if not ok_seen:
        latch_call(st, "H3: latch.release (abort)", "release")
        bg.join()
        return
    rb = bind_native(st, "gH3a", "H3: bindAndReserve gH3a INSIDE the window")
    oa = obl(st, "gH3a")
    chk("H3-inside-no-gift", "bind inside the window: Reserved without a gift leg", rb.ok and tag(rb.val) == "Reserved" and oa and not any(l["role"] == "signupGift" for l in oa["legs"]), [rb.val if rb.ok else rb.err])
    chk("H3-nothing-minted", "holdThenReject: the stub ledger recorded no gift while held", len(gifts_in(st, latch_mints(st))) == 0)
    latch_call(st, "H3: latch.release (the held call is REJECTED with a synthetic reject)", "release")
    bg.join(180)
    r1 = R(sid, "H3: reservation after the rejected mint")
    chk("H3-failed", "the rejected await was caught: reservation Failed{owner legacy}", r1 and r1[:2] == ("legacy", "Failed"), r1)
    lg = logs_of(st, "H3", "Signup-gift mint failed", since=LM)
    obs("H3-failure-classification", "legacy gift mint whose await was REJECTED (synthetic reject from the latch stub, not a real ledger error): caught -> #Err(\"ppLedger call failed: <reject text>\") -> Failed", {"log_lines": lg[-1:]})
    game_minted = [m for m in latch_mints(st) if m.get("memo_text") == "game-0"]
    chk("H3-game-advanced", "the same tick went on to mint the GAME PP (memo 'game-0' does not match the latch), so the game cursor advanced", len(game_minted) == 1, [(m["block"], m["amount"]) for m in game_minted])
    obs("H3-cursor", "because the game mint succeeded, the game is NOT re-presented: the Failed(legacy) gift gets no automatic legacy retry until that player's NEXT legacy game; a native bind can take it meanwhile", {"reservation": r1, "game_mints": [(m["block"], m["amount"]) for m in game_minted]})
    rb2 = bind_native(st, "gH3b", "H3: bindAndReserve gH3b AFTER the failure")
    ob = obl(st, "gH3b")
    r2 = R(sid, "H3: reservation after the second native bind")
    chk("H3-takeover", "the later native event takes the Failed(legacy) gift: Reserved WITH a signupGift leg; reservation Reserved{native}", rb2.ok and tag(rb2.val) == "Reserved" and ob and any(l["role"] == "signupGift" for l in ob["legs"]) and r2 and r2[:2] == ("native", "Reserved"), [r2, [(l["role"], l["amount"]) for l in ob["legs"]]])
    exec_all(st, "gH3b", "H3")
    exec_all(st, "gH3a", "H3")
    r3 = R(sid, "H3: reservation after the native legs")
    chk("H3-committed", "Committed{native}", r3 and r3[:2] == ("native", "Committed"), r3)
    ms = latch_mints(st)
    g = gifts_in(st, ms)
    chk("H3-one-gift", "exactly ONE gift recorded (native); no legacy 'signup-' mint ever committed", len(g) == 1 and len(legacy_gifts(ms)) == 0, [(m["block"], m["amount"], m.get("memo_text")) for m in g])
    obs("H3-winner", "NATIVE won the gift after the legacy mint was rejected; the first native event (inside the window) correctly has none and cannot get one later", {})
    record_blocks("H3", ms, "latch_ledger_stub")


def scenario_H5():
    """Legacy mint fails (stub rejects every call, like a stopped ledger) -> Failed; the whole tick is re-presented; legacy retry wins."""
    st, sid = VARIANT, VARIANT["id"]
    log("H5: legacy mint rejected (stub fail-all) -> Failed -> legacy retry wins on the next tick; native then gets no gift")
    fresh(st, "H5")
    LM = log_mark(st)
    latch_call(st, "H5: latch.resetAll", "resetAll")
    latch_call(st, "H5: latch.setFailAll(true) (every icrc1_transfer rejects immediately, like a stopped ledger)", "setFailAll", "(true)")
    observe_native(st, "gH5")
    rep, secs = tick(st, "H5: runObserverOnce with the stub failing every call")
    r1 = R(sid, "H5: reservation after the failed tick")
    chk("H5-failed", "reservation is Failed{legacy} after the rejected gift mint", r1 and r1[:2] == ("legacy", "Failed"), r1)
    lg = logs_of(st, "H5", "Signup-gift mint failed", since=LM)
    obs("H5-failure-classification", "legacy gift mint rejected immediately by the stub: caught -> #Err(\"ppLedger call failed: <reject text>\") -> Failed (synthetic reject)", {"log_lines": lg[-1:]})
    latch_call(st, "H5: latch.setFailAll(false)", "setFailAll", "(false)")
    rep, secs = tick(st, "H5: runObserverOnce (the game was re-presented because its mint failed too; legacy re-claims its Failed gift)")
    r2 = R(sid, "H5: reservation after the retry tick")
    chk("H5-retry-wins", "the legacy retry committed the gift: Committed{owner = legacy}", r2 and r2[:2] == ("legacy", "Committed") and r2[2] > r1[2], [r1, r2])
    ms = latch_mints(st)
    g = gifts_in(st, ms)
    chk("H5-one-gift", "exactly ONE gift recorded and it is the legacy 'signup-' mint", len(g) == 1 and len(legacy_gifts(ms)) == 1 and legacy_gifts(ms)[0]["block"] == g[0]["block"], [(m["block"], m["amount"], m.get("memo_text")) for m in g])
    rb = bind_native(st, "gH5")
    o = obl(st, "gH5")
    chk("H5-native-no-gift", "a native bind afterwards reserves WITHOUT a gift leg", rb.ok and tag(rb.val) == "Reserved" and o and not any(l["role"] == "signupGift" for l in o["legs"]), [rb.val if rb.ok else rb.err])
    exec_all(st, "gH5", "H5")
    chk("H5-still-one", "still exactly one gift after the native legs", len(gifts_in(st, latch_mints(st))) == 1)
    obs("H5-winner", "LEGACY won (retry on the next tick, as designed)", {})
    record_blocks("H5", latch_mints(st), "latch_ledger_stub")


def scenario_H4(mode):
    """`install --mode upgrade` (same module) while the legacy gift-mint await is HELD: what survives and what happens when it resolves."""
    st, sid = VARIANT, VARIANT["id"]
    lab = "H4-" + mode
    log("%s: install --mode upgrade while the legacy await is HELD (open call context)" % lab)
    fresh(st, lab)
    arm_latch(st, lab, mode)
    observe_native(st, "g" + lab)
    bg, got = start_held_tick(st, lab)
    ok_seen = got not in (None, "ended")
    chk(lab + "-reserved-before-await", "Reserved{legacy} visible with the mint held", ok_seen and bg.alive(), got)
    if not ok_seen:
        latch_call(st, lab + ": latch.release (abort)", "release")
        bg.join()
        return
    r_before = canon(R(sid))
    ok, out = upgrade(st, lab + " (open call context!)", timeout=240)
    obs(lab + "-upgrade-with-open-call", "install --mode upgrade --wasm-memory-persistence keep with an OPEN call context (same module): ok=%s" % ok, {"ok": ok, "dfx_output": shorten(out, 600)})
    chk(lab + "-upgrade-ok", "the upgrade with an open call context was accepted", ok, out if not ok else None)
    if not ok:
        latch_call(st, lab + ": latch.release (abort)", "release")
        bg.join()
        return
    r_up = canon(R(sid, lab + ": reservation after the upgrade"))
    chk(lab + "-durable", "the legacy reservation (Reserved{legacy}) is byte-identical after the upgrade, before the await resolves", r_up == r_before, [r_before, r_up])
    pending_after_upgrade = not bg.join(8)
    obs(lab + "-ingress-after-upgrade", "8 s after the upgrade the runObserverOnce ingress call was %s" % ("STILL PENDING (the call context survived the upgrade)" if pending_after_upgrade else "already finished"), {"still_pending": pending_after_upgrade, "reply_ok": bg.rep.ok if bg.rep else None, "error": shorten(bg.rep.err, 400) if bg.rep and not bg.rep.ok else None})
    latch_call(st, lab + ": latch.release", "release")
    done = bg.join(120)
    obs(lab + "-ingress-after-release", "after the release the runObserverOnce call finished=%s ok=%s" % (done, bg.rep.ok if bg.rep else None), {"finished": done, "reply_ok": bg.rep.ok if bg.rep else None, "error": shorten(bg.rep.err, 500) if bg.rep and not bg.rep.ok else None})
    r_rel = R(sid, lab + ": reservation after the release")
    want = "Failed" if mode == "holdThenReject" else "Committed"
    chk(lab + "-continuation-ran", "the held await's continuation ran in the UPGRADED instance when the reply arrived: reservation is %s{legacy}" % want, r_rel and r_rel[:2] == ("legacy", want), r_rel)
    obs(lab + "-continuation-survived-upgrade", "observed: a Motoko await that was open across `install --mode upgrade --wasm-memory-persistence keep` (to the identical wasm) resumed and ran its continuation (finalize) afterwards; NOT tested: upgrading to a different module version with an await in flight", {"reservation": r_rel, "latch": latch_status(st)})
    if mode == "holdThenReject":
        # the tick went on to mint the game PP, so there is no automatic legacy retry: a native event takes the Failed gift
        rb = bind_native(st, "g" + lab)
        o = obl(st, "g" + lab)
        chk(lab + "-takeover", "after the rejected+resumed mint a native bind takes the Failed(legacy) gift", rb.ok and o and any(l["role"] == "signupGift" for l in o["legs"]), [rb.val if rb.ok else rb.err, R(sid)])
        exec_all(st, "g" + lab, lab)
    else:
        rb = bind_native(st, "g" + lab)
        o = obl(st, "g" + lab)
        chk(lab + "-native-no-gift", "after the resumed legacy commit a native bind gets no gift leg", rb.ok and o and not any(l["role"] == "signupGift" for l in o["legs"]), [rb.val if rb.ok else rb.err])
        exec_all(st, "g" + lab, lab)
    ms = latch_mints(st)
    g = gifts_in(st, ms)
    chk(lab + "-one-gift", "%s + upgrade mid-await: exactly ONE gift recorded by the ledger stub" % mode, len(g) == 1, [(m["block"], m["amount"], m.get("memo_text")) for m in g])
    obs(lab + "-winner", "%s won the gift" % ("NATIVE" if mode == "holdThenReject" else "LEGACY"), {"gift_blocks": [m["block"] for m in g]})
    record_blocks(lab, ms, "latch_ledger_stub")


# ---------------------------------------------------------------- receipt plumbing
BLOCKS = {}


def record_blocks(label, mints, src="pp_ledger (real)"):
    BLOCKS[label] = {"source": src, "mints": [{k: v for k, v in m.items()} for m in mints]}


def run_scenario(fn, *a):
    name = fn.__name__ + ("(%s)" % ",".join(map(str, a)) if a else "")
    try:
        fn(*a)
    except Exception as e:  # noqa
        import traceback
        tb = traceback.format_exc()
        chk("ERR-" + name, "%s raised %s" % (name, e), False, shorten(tb, 900))
        log(tb)


# ---------------------------------------------------------------- infra
def prepare_infra():
    me = subprocess.run(["dfx", "identity", "get-principal"], capture_output=True, text=True).stdout.strip()
    IDS["me"] = me
    # 0. preflight (read-only)
    rep = call("preflight: pp_ledger minting account", LEDGER, "icrc1_minting_account")
    chk("ENV-minter", "pp_ledger's minting account is the shenanigans canister (no subaccount)", rep.ok and opt(rep.val) and opt(rep.val)["owner"] == SHEN and not opt(opt(rep.val)["subaccount"]), rep.val if rep.ok else rep.err)
    rep = call("preflight: siws get_principal(bound pubkey)", SIWS, "get_principal", '("%s")' % BOUND_PUBKEY)
    chk("ENV-siws", "siws_provider resolves the bound owner (Bow1CG... -> m7ydg-...)", rep.ok and "Ok" in rep.val, rep.val if rep.ok else rep.err)
    try:
        chk("ENV-principal", "the bound principal text decodes (chip subaccount helper)", len(chip_hex(BOUND_PRINCIPAL)) == 64, chip_hex(BOUND_PRINCIPAL))
    except Exception as e:  # noqa
        chk("ENV-principal", "principal decode", False, str(e))
    # 1. games stub + latch ledger (their own dfx project)
    for name in ("legacy_games_stub", "latch_ledger"):
        okc, outc = dfx_admin("create " + name, "canister", "id", name, cwd=STUBDIR, record=False)
        if not okc:
            dfx_admin("create " + name, "canister", "create", name, cwd=STUBDIR)
        IDS[name] = subprocess.run(["dfx", "canister", "id", name, "--network", "local"], cwd=STUBDIR, capture_output=True, text=True).stdout.strip()
    ok1, _ = dfx_admin("build stubs", "build", cwd=STUBDIR)
    chk("INFRA-stubs-build", "stub canisters build", ok1)
    dfx_admin("install legacy_games_stub", "canister", "install", "legacy_games_stub", "--mode", "reinstall", "--yes", cwd=STUBDIR)
    dfx_admin("install latch_ledger", "canister", "install", "latch_ledger", "--mode", "reinstall", "--yes", cwd=STUBDIR)
    IDS["stub"] = IDS["legacy_games_stub"]
    VARIANT["latch"] = IDS["latch_ledger"]
    # 2. real shenanigans build
    ok2, out2 = (True, "") if SKIP_BUILD else dfx_admin("build real shenanigans", "build", "shenanigans", cwd=ROOT)
    chk("INFRA-real-build", "dfx build shenanigans (committed sources)%s" % (" (skipped, artifacts reused)" if SKIP_BUILD else ""), ok2, shorten(out2, 300))
    # 3. latch variant: same sources, one constant patched
    prepare_variant()


def prepare_variant():
    os.makedirs(BUILD_DIR, exist_ok=True)
    for sub in ("shenanigans", "observatory"):
        shutil.rmtree(os.path.join(BUILD_DIR, sub), ignore_errors=True)
        shutil.copytree(os.path.join(ROOT, sub), os.path.join(BUILD_DIR, sub), ignore=shutil.ignore_patterns("test", "scripts", "*.md", ".dfx"))
    pp = os.path.join(BUILD_DIR, "shenanigans", "PpLedger.mo")
    orig = open(os.path.join(ROOT, "shenanigans", "PpLedger.mo")).read()
    needle = 'public let PP_LEDGER_CANISTER_ID : Text = "%s";' % LEDGER
    assert needle in orig, "PpLedger.mo constant not found"
    patched = orig.replace(needle, 'public let PP_LEDGER_CANISTER_ID : Text = "%s";' % VARIANT["latch"])
    open(pp, "w").write(patched)
    diff = [l for l in (orig.splitlines()) if l not in patched.splitlines()]
    json.dump({"version": 1, "canisters": {"shenanigans_latch": {"type": "motoko", "main": "shenanigans/main.mo"}}}, open(os.path.join(BUILD_DIR, "dfx.json"), "w"))
    same_main = sha(os.path.join(ROOT, "shenanigans", "main.mo")) == sha(os.path.join(BUILD_DIR, "shenanigans", "main.mo"))
    same_nr = sha(os.path.join(ROOT, "shenanigans", "NativeRewards.mo")) == sha(os.path.join(BUILD_DIR, "shenanigans", "NativeRewards.mo"))
    same_others = all(sha(os.path.join(ROOT, "shenanigans", f)) == sha(os.path.join(BUILD_DIR, "shenanigans", f)) for f in ("Subaccount.mo", "Reginald.mo", "icrc21.mo"))
    chk("INFRA-variant-identical", "latch variant: main.mo, NativeRewards.mo, Subaccount.mo, Reginald.mo, icrc21.mo byte-identical to the committed sources; PpLedger.mo differs by exactly one line (the ledger id)", same_main and same_nr and same_others and len(diff) == 1, {"changed_line_original": diff, "sha256_PpLedger_original": hashlib.sha256(orig.encode()).hexdigest(), "sha256_PpLedger_patched": hashlib.sha256(patched.encode()).hexdigest(), "sha256_main_mo": sha(os.path.join(ROOT, "shenanigans", "main.mo")), "sha256_NativeRewards_mo": sha(os.path.join(ROOT, "shenanigans", "NativeRewards.mo"))})
    okc, _ = dfx_admin("variant canister id", "canister", "id", "shenanigans_latch", cwd=BUILD_DIR, record=False)
    if not okc:
        dfx_admin("create shenanigans_latch", "canister", "create", "shenanigans_latch", cwd=BUILD_DIR)
    VARIANT["id"] = subprocess.run(["dfx", "canister", "id", "shenanigans_latch", "--network", "local"], cwd=BUILD_DIR, capture_output=True, text=True).stdout.strip()
    NAMES[VARIANT["id"]] = "shenanigans_latch"
    NAMES[VARIANT["latch"]] = "latch_ledger"
    NAMES[IDS["stub"]] = "legacy_games_stub"
    ok, out = (True, "") if SKIP_BUILD else dfx_admin("build shenanigans_latch", "build", "shenanigans_latch", cwd=BUILD_DIR)
    chk("INFRA-variant-build", "latch variant builds%s" % (" (skipped, artifacts reused)" if SKIP_BUILD else ""), ok, shorten(out, 300))


def restore():
    """Leave the shared replica usable whatever happened above."""
    try:
        st = canister_status(LEDGER)[0]
        if st != "Running":
            dfx_admin("RESTORE: start pp_ledger", "canister", "start", LEDGER)
        if VARIANT.get("latch"):
            dfx_call(VARIANT["latch"], "release", "()")
        for s in (REAL, VARIANT):
            if s.get("id"):
                dfx_call(s["id"], "stopObserver", "()")
    except Exception as e:  # noqa
        log("restore error: %s" % e)


def finish():
    restore()
    ledger_final = canister_status(LEDGER)
    mod_hashes = {label_of(c): canister_status(c) for c in (LEDGER, SIWS, SHEN) if c}
    receipt = {
        "receipt": "IMPL-49h: actor-level signup-gift interleave / failure / takeover / durability regression (Codex R1), local dfx replica",
        "date": "2026-10-15", "started": now_iso(), "network": "local dfx replica 127.0.0.1:4943 only; nothing public",
        "head_note": "run against the working tree at the committed 883e044 shenanigans sources (git HEAD d43a462 at run time)",
        "design": {
            "legacy_game_source": "legacy_games_stub (this directory): returns exactly one qualifying game (1.0 ICP-denominated, simple21Day) naming the SIWS-bound principal m7ydg-... so the real legacy observer runs its real signup-gift block for the same principal the native path binds (Bow1CG... -> m7ydg-...)",
            "hold": "PART B: a latch ledger stub at a different canister id and a build of the same sources whose only change is the PP ledger id constant (hashes in the INFRA-variant-identical check). Stopping the REAL pp_ledger does NOT hold the await open (observed: fail-fast reject), so the real ledger is used for the failure/ordering/durability scenarios of PART A and the stub only for the held-await window",
            "gift_counting": "PART A counts mints on the REAL pp_ledger (blocks, supply, balances in the receipt). PART B counts the latch stub's mint records, which are not the real ledger's",
            "reinstalls": "shenanigans (j56tm-...) and shenanigans_latch were reinstalled once per scenario; pp_ledger and siws_provider were only called, and pp_ledger was stopped/started briefly (restored by a trap)",
            "operator": IDS.get("me"),
            "bridge_state_note": "the reinstall of shenanigans wiped the bridge worker's COOL-1 leftovers on j56tm (two Reserved obligations); native_sol_bridge itself was not touched",
        },
        "canisters": {"ids": {**IDS, "shenanigans_latch": VARIANT.get("id")}, "module_hash_and_status_at_end": mod_hashes},
        "steps": STEPS, "checks": CHECKS, "observations": OBS, "ledger_blocks": BLOCKS,
        "summary": {"checks": len(CHECKS), "passed": sum(1 for c in CHECKS if c["pass"]), "failed": [c["id"] for c in CHECKS if not c["pass"]]},
        "scenario_map_to_native-rewards.md_G1-G6": {
            "G1 held await + native bind inside the window": "H1 (latch stub; deterministic hold; IC timestamps)",
            "G2 legacy mint fails, native takes the Failed gift": "A1 and A2 (REAL pp_ledger, committed wasm) and H3 (stub, rejected held await)",
            "G3 legacy retry wins after a failed mint": "H5 (stub). On the real local ledger the retry re-claims correctly (A1-reclaimed) but cannot commit: 32-byte memo limit",
            "G4 native Reserved blocks legacy": "A2-skip, A3-blocked (REAL ledger)",
            "G5 native gift leg SubmittedUnknown still blocks legacy": "A3-blocked-2 (REAL ledger, stopped)",
            "G6 durability across upgrade": "A1/A2 at rest in Failed/Reserved/Committed (REAL stack) and H4 with an in-flight await (stub)",
        },
        "proven": [
            "REAL stack (committed shenanigans wasm, real pp_ledger, real siws_provider): failed legacy gift mint with the ledger stopped -> Failed{legacy}; the retry tick re-claims its own Failed gift (timestamp advances) and fails again on the real ledger's memo limit; the native bind then takes the Failed gift and exactly ONE gift lands on the real ledger (blocks/supply recorded)",
            "REAL stack: Failed{legacy} -> native takeover -> legacy tick skips (reservation and timestamp untouched) -> native legs commit -> Committed{native}; each of Failed, Reserved{native} with Pending legs and Committed survives `install --mode upgrade --wasm-memory-persistence keep` byte-identical",
            "REAL stack: native Reserved first -> the legacy path never attempts the gift (no failure log line), also while the native gift leg is SubmittedUnknown (ledger stopped); retry commits it; exactly one gift",
            "LATCH variant: the legacy reservation was observed Reserved{legacy} while its gift-mint await was unresolved (tick call pending, latch holding the reply after the stub had committed the mint); a native bindAndReserve (real SIWS lookup) and its leg executions completed inside that window and took no gift; after the release exactly one gift was recorded; IC timestamps order legacy-reserve < native-bind < legacy-commit",
            "LATCH variant: rejected held await -> Failed{legacy}, the same tick still mints the game PP (cursor advances), a later native bind takes the Failed gift, one gift; fail-all then recovery -> legacy retry wins, one gift, native gets none; upgrade with the await in flight (same module) keeps the reservation byte-identical, the open await resumes after the upgrade and finalizes, one gift in both reject and commit modes",
        ],
        "not_proven": [
            "PART B's gift count is the latch stub's, not the real pp_ledger's; the real ledger answers within about one round and cannot be held open from outside",
            "the held-await scenarios run a build whose only difference is the ledger id constant (the hashes are recorded); they are not the byte-identical committed wasm",
            "no SYS_UNKNOWN / best-effort-call behaviour was observed or is claimed: the awaits are plain unbounded Motoko awaits and every failure seen is a caught reject",
            "a caught reject AFTER an already-committed real-ledger mint was not observed (no such case was produced)",
            "an upgrade to a DIFFERENT module version with an await in flight was not tested (H4 upgrades to the identical wasm; the open await resumed and ran its continuation, which is only evidence for that case)",
            "the legacy gift mint can never commit on THIS local pp_ledger (32-byte memo limit vs the 70-byte 'signup-<principal>' memo); mainnet pp_ledger's limit was not read, so no claim is made about it. Legacy-commit paths are therefore demonstrated only against the latch stub, which does not enforce a memo limit",
            "the 3,190-interleaving enumeration is a pure model and is not evidence for this wiring",
            "real timer re-arm, multi-subnet message ordering and cycle behaviour are out of scope",
        ],
        "finished": now_iso(), "ledger_status_at_end": ledger_final[0],
    }
    with open(RECEIPT_PATH, "w") as f:
        json.dump(receipt, f, indent=1)
    log("receipt written: %s  (%d checks, %d failed)" % (RECEIPT_PATH, len(CHECKS), len(FAILED)))
    return 0 if not FAILED else 1


def main():
    prepare_infra()
    only = [a for a in sys.argv[1:] if not a.startswith("-")]

    def want(x):
        return not only or x in only

    if want("A1"): run_scenario(scenario_A1)
    if want("A2"): run_scenario(scenario_A2)
    if want("A3"): run_scenario(scenario_A3)
    if want("H1"): run_scenario(scenario_H1)
    if want("H3"): run_scenario(scenario_H3)
    if want("H5"): run_scenario(scenario_H5)
    if want("H4a"): run_scenario(scenario_H4, "holdThenReject")
    if want("H4b"): run_scenario(scenario_H4, "commitThenHold")
    return finish()


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        assert len(chip_hex(BOUND_PRINCIPAL)) == 64
        assert principal_bytes("aaaaa-aa") == b""
        print("selftest ok", chip_hex(BOUND_PRINCIPAL))
        sys.exit(0)
    try:
        rc = main()
    except KeyboardInterrupt:
        rc = 130
    finally:
        restore()
    sys.exit(rc)
