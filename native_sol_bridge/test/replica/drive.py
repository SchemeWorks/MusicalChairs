#!/usr/bin/env python3
"""IMPL-47d / IMPL-49b scenario driver (called by run.sh; stdlib only; local replica only).

v2 (IMPL-49b, review R2/R3): initial reward execution goes through the bridge's permissionless
`executeReservedLeg`, called by a THROWAWAY NON-ADMIN dfx identity (mc-nonadmin: not a controller,
not an admin, not a relayer, not on any allowlist); the admin path on Shenanigans is kept as ONE
fallback check (E2). History checks split the public verified page (`getRecentEvents`) from the
operator intake page (`getIntakeEvents`). Receipt: receipt-2026-10-14-v2.json (v1 is kept).

Runs the bridge -> Shenanigans -> PP ledger path end to end against the canisters run.sh just
(re)installed, asserts as it goes, and writes receipt-2026-10-14.json. Every `dfx` call here is
`--network local`. Exit status 0 only if every check passed.
"""
import base64, datetime, hashlib, json, os, subprocess, sys, threading, time, zlib

E = os.environ
ROOT, HERE = E["MC_REPLICA_ROOT"], E["MC_REPLICA_HERE"]
BRIDGE, ME = E["MC_BRIDGE"], E["MC_ME"]
LEDGER, SIWS, SHEN, STUB = E["MC_LEDGER"], E["MC_SIWS"], E["MC_SHEN"], E["MC_STUB"]
BOUND_PUBKEY, BOUND_PRINCIPAL = E["MC_BOUND_PUBKEY"], E["MC_BOUND_PRINCIPAL"]
RECEIPT_PATH = os.path.join(HERE, "receipt-2026-10-14-v2.json")
NONADMIN, NONADMIN_PRINCIPAL = E["MC_NONADMIN_IDENTITY"], E["MC_NONADMIN_PRINCIPAL"]
FIXDIR = os.path.join(ROOT, "solana", "fixtures")
PROGRAM = "7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7"
AUTHORITY = "BuzaujH9dKaTPZGcLmRe3SA1DNvHJGN9GAGrGVAzwZd6"
DOMAIN = b"musical-chairs-localnet-dev!!!!!"
HOUR_NS = 3600 * 10**9
NAMES = {BRIDGE: "native_sol_bridge", SHEN: "shenanigans", LEDGER: "pp_ledger", SIWS: "siws_provider", STUB: "sol_rpc_stub"}

STEPS, CHECKS, UPGRADES, FINDINGS, MODES, BLOCKS, BALANCES = [], [], [], [], [], {}, []
NOT_PROVEN = []
FAILED = []
T_START = time.time()


def now_iso():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def log(msg):
    print("[%6.1fs] %s" % (time.time() - T_START, msg), flush=True)


# ---------------------------------------------------------------- numbers / candid json
def n(x):
    if isinstance(x, int):
        return x
    return int(str(x).replace("_", ""))


def V(x):
    """single-key variant dict -> (tag, payload)"""
    assert isinstance(x, dict) and len(x) == 1, x
    k = next(iter(x))
    return k, x[k]


def tag(x):
    return V(x)[0]


def opt(x):
    return x[0] if x else None


def blob(bs):
    return 'blob "' + "".join("\\%02x" % b for b in bs) + '"'


# ---------------------------------------------------------------- dfx
class Reply:
    def __init__(self, ok, val, err):
        self.ok, self.val, self.err = ok, val, err


def dfx_call(cid, method, arg="()", identity=None, timeout=300):
    cmd = ["dfx"] + (["--identity", identity] if identity else [])
    cmd += ["canister", "call", cid, method, arg, "--network", "local", "--output", "json"]
    r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=timeout)
    if r.returncode == 0:
        out = r.stdout.strip()
        return Reply(True, json.loads(out) if out else None, "")
    return Reply(False, None, (r.stderr or r.stdout).strip())


def shorten(s, k=600):
    return s if len(s) <= k else s[:k] + "...(%d chars)" % len(s)


def caller_label(identity):
    if identity is None:
        return "operator(dfx identity; controller+admin+relayer)"
    if identity == NONADMIN:
        return "%s(throwaway non-admin: not controller, not admin, not relayer)" % NONADMIN
    return identity


def call(step, cid, method, arg="()", identity=None, record=True):
    rep = dfx_call(cid, method, arg, identity)
    if record:
        STEPS.append({
            "n": len(STEPS) + 1, "t": now_iso(), "step": step, "canister": NAMES.get(cid, cid), "canister_id": cid,
            "method": method, "caller": caller_label(identity), "arg": shorten(arg),
            "ok": rep.ok, "reply": rep.val if rep.ok else None, "error": shorten(rep.err, 1500) if not rep.ok else None,
        })
    return rep


def dfx_admin(step, *args, record=True):
    """non-call dfx subcommands (stop/start/install/snapshot/info/status)"""
    cmd = ["dfx"] + list(args) + ["--network", "local"]
    r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=600)
    out = (r.stdout + "\n" + r.stderr).strip()
    if record:
        STEPS.append({"n": len(STEPS) + 1, "t": now_iso(), "step": step, "dfx": " ".join(args), "ok": r.returncode == 0, "output": shorten(out, 1200)})
    return r.returncode == 0, out


def module_hash(cid):
    ok, out = dfx_admin("module hash", "canister", "info", cid, record=False)
    for line in out.splitlines():
        if line.startswith("Module hash:"):
            return line.split(":", 1)[1].strip()
    return None


def chk(cid, desc, cond, detail=None):
    cond = bool(cond)
    CHECKS.append({"id": cid, "check": desc, "pass": cond, "detail": detail})
    if not cond:
        FAILED.append(cid)
    log("%s %s: %s%s" % ("PASS" if cond else "FAIL", cid, desc, "" if cond or detail is None else "  [" + shorten(str(detail), 400) + "]"))
    return cond


def sleep(sec, why):
    log("waiting %ds (%s)" % (sec, why))
    time.sleep(sec)


# ---------------------------------------------------------------- fixtures (independent Python oracle)
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def b58dec(s):
    v = 0
    for c in s:
        v = v * 58 + B58.index(c)
    b = v.to_bytes((v.bit_length() + 7) // 8, "big") if v else b""
    return b"\x00" * (len(s) - len(s.lstrip("1"))) + b


def b58enc(b):
    v, out = int.from_bytes(b, "big"), ""
    while v:
        v, r = divmod(v, 58)
        out = B58[r] + out
    return "1" * (len(b) - len(b.lstrip(b"\x00"))) + out


TAG = bytes.fromhex("e445a52e51cb9a1d")
DISC = {e["discriminator_hex"]: e["name"] for e in json.load(open(os.path.join(FIXDIR, "event-encodings.json")))}
LAYOUT = {
    "PositionOpened": ["u8", "pk:owner", "u64:nonce", "u64:round", "opt:close", "u8:plan", "u64:gross", "u64:load", "u32:cfg"],
    "SeriesAFunded": ["u8", "pk:owner", "u64:round", "opt:close", "u64:gross", "u128:ent"],
    "PromotionApplied": ["u8", "u64:round", "opt:close", "pk:winner", "u64:loss", "u128:ent"],
}


def decode_body(name, body):
    o, out = 0, {}
    for item in LAYOUT[name]:
        t, _, f = item.partition(":")
        if t == "u8": v, o = body[o], o + 1
        elif t == "u32": v, o = int.from_bytes(body[o:o + 4], "little"), o + 4
        elif t == "u64": v, o = int.from_bytes(body[o:o + 8], "little"), o + 8
        elif t == "u128": v, o = int.from_bytes(body[o:o + 16], "little"), o + 16
        elif t == "pk": v, o = b58enc(body[o:o + 32]), o + 32
        elif t == "opt":
            if body[o] == 0: v, o = None, o + 1
            else: v, o = int.from_bytes(body[o + 1:o + 9], "little"), o + 9
        if f: out[f] = v
    assert o == len(body), (name, o, len(body))
    return out


def fixture_event(fixture):
    """The (single) forwarded-kind event of a captured transaction, decoded from the raw bytes."""
    r = json.load(open(os.path.join(FIXDIR, "rpc", fixture + ".json")))["result"]
    sig = r["transaction"]["signatures"][0]
    for g in r["meta"]["innerInstructions"]:
        for j, x in enumerate(g["instructions"]):
            d = b58dec(x["data"])
            if d[:8] == TAG and DISC.get(d[8:16].hex()) in LAYOUT:
                name = DISC[d[8:16].hex()]
                f = decode_body(name, d[16:])
                ev = {"fixture": fixture, "sig": sig, "ix": g["index"], "inner": j, "name": name, "round": f["round"], "close": f["close"]}
                if name == "PositionOpened":
                    ev.update(kind="PositionOpened", beneficiary=f["owner"], basis=f["gross"], plan=["Simple", "Compound15", "Compound30"][f["plan"]])
                elif name == "SeriesAFunded":
                    ev.update(kind="SeriesAFunded", beneficiary=f["owner"], basis=f["gross"], plan=None)
                else:
                    ev.update(kind="PromotionApplied", beneficiary=f["winner"], basis=f["loss"], plan=None)
                return ev
    raise RuntimeError("no forwarded-kind event in " + fixture)


def eid_arg(ev, **o):
    return ("record { cluster_domain = %s; program_id = \"%s\"; signature = \"%s\"; instruction_index = %d : nat; "
            "inner_instruction_index = %d : nat; event_schema_version = %d : nat }") % (
        blob(o.get("domain", DOMAIN)), o.get("program", PROGRAM), ev["sig"], ev["ix"], ev["inner"], o.get("schema", 1))


def desc_arg(ev, **o):
    close = "opt (%d : nat64)" % ev["close"] if ev["close"] is not None else "null"
    plan = "opt variant { %s }" % ev["plan"] if ev["plan"] else "null"
    return ("record { event_id = %s; source_round_id = %d : nat64; source_close_id = %s; kind = variant { %s }; "
            "beneficiary_pubkey = \"%s\"; basis_lamports = %d : nat64; plan = %s }") % (
        eid_arg(ev, **o), ev["round"], close, ev["kind"], ev["beneficiary"], o.get("basis", ev["basis"]), plan)


# ---------------------------------------------------------------- reading state
def obl(ev, step=None):
    rep = call(step or "getNativeObligation", SHEN, "getNativeObligation", "(%s)" % eid_arg(ev), record=step is not None)
    v = opt(rep.val)
    if v is None:
        return None
    legs = []
    for l in v["legs"]:
        st, pl = V(l["status"])
        legs.append({
            "index": n(l["index"]), "amount": n(l["amount"]), "holder": l["holder"], "role": tag(l["role"]), "status": st,
            "block": n(pl["block"]) if st == "Committed" else None, "attempts": n(l["attempts"]), "memo": l["memo"],
            "created_at_time": n(l["created_at_time"]), "owner": l["recipient"]["owner"], "sub": opt(l["recipient"]["subaccount"]),
            "status_payload": pl,
        })
    return {"status": tag(v["status"]), "policy_version": n(v["policy_version"]), "beneficiary": opt(v["beneficiary"]), "legs": legs, "raw": v}


def canon(x):
    return json.dumps(x, sort_keys=True)


def bal(owner, sub):
    s = "opt %s" % blob(sub) if sub else "null"
    return n(call("balance", LEDGER, "icrc1_balance_of", '(record { owner = principal "%s"; subaccount = %s })' % (owner, s), record=False).val)


def ledger_len():
    return n(call("ledger log length", LEDGER, "get_transactions", "(record { start = 0 : nat; length = 1 : nat })", record=False).val["log_length"])


def supply():
    return n(call("ledger supply", LEDGER, "icrc1_total_supply", record=False).val)


def tx_at(block):
    rep = call("ledger tx", LEDGER, "get_transactions", "(record { start = %d : nat; length = 1 : nat })" % block, record=False)
    t = rep.val["transactions"]
    return t[0] if t else None


def bridge_summary(rep):
    """ok-record of a bridge reply -> (status tag, status payload, binding tag, cursor, seq); or ('ERR', err variant)"""
    if not rep.ok:
        return ("TRAP", rep.err)
    t, p = V(rep.val)
    if t == "err":
        return ("ERR", p)
    st, pl = V(p["status"])
    return ("OK", st, pl, tag(p["binding"]), n(p["cursor"]), n(p["seq"]))


def retry_cooldown(step, cid, method, arg, max_wait=150, identity=None):
    """Call a bridge method; on `err.Cooldown` record that first reply, wait, retry. Records the final reply too.
    Returns (final Reply, first Cooldown Reply or None)."""
    t0, first = time.time(), None
    while True:
        rep = call(step, cid, method, arg, identity=identity, record=False)
        if rep.ok:
            t, p = V(rep.val)
            if t == "err" and isinstance(p, dict) and "Cooldown" in p:
                if first is None:
                    first = rep
                    log("%s: Cooldown reply (as designed), waiting it out" % step)
                    STEPS.append({"n": len(STEPS) + 1, "t": now_iso(), "step": step + " [first attempt: Cooldown]", "canister": NAMES.get(cid, cid), "canister_id": cid,
                                  "method": method, "caller": caller_label(identity), "arg": shorten(arg), "ok": True, "reply": rep.val, "error": None})
                if time.time() - t0 > max_wait:
                    return rep, first
                time.sleep(5)
                continue
        STEPS.append({"n": len(STEPS) + 1, "t": now_iso(), "step": step, "canister": NAMES.get(cid, cid), "canister_id": cid, "method": method,
                      "caller": caller_label(identity), "arg": shorten(arg), "ok": rep.ok, "reply": rep.val if rep.ok else None, "error": shorten(rep.err, 1500) if not rep.ok else None})
        return rep, first


# COOL-1 oracle. The IMPL-47d v1 / first v2 draft asserted the OLD behaviour (a bind straight after a
# successful forward answered err Cooldown). ca00bbc made a successful forward clear Entry.next_attempt_at
# and IMPL-49b made a successful bind clear it too, so the correct expectation is the opposite.
COOL1_STALE_TEXT = "releasePending right after a forward is refused with Cooldown: the bind step reads the next_attempt_at that verifyAndForward set"
COOL1_TEXT = ("releasePending immediately after a successful verifyAndForward is NOT throttled: a forward that lands in Forwarded clears "
              "Entry.next_attempt_at (ca00bbc), so the bind answers ok with Shenanigans' own answer (Unbound for an unbound owner, "
              "Reserved/AlreadyReserved for a bound one) and never err Cooldown")


def cool1_ok(rc):
    if not rc.ok:
        return False
    t, p = V(rc.val)
    return t == "ok" and p["shenanigans"] in ([{"Unbound": None}], [{"Reserved": None}], [{"AlreadyReserved": None}])


def exp_legs(basis, rate, gift_pp=None, bps=1000):
    base = basis * rate * 10**8 // 10**9
    cas = base * bps // 10000
    out = []
    if gift_pp:
        gb = gift_pp * 10**8
        gc = gb * bps // 10000
        out += [("signupGift", gb - gc, "player"), ("residual", gc, "house")]
    out += [("player", base - cas, "player"), ("residual", cas, "house")]
    return out


def exec_leg(ev, idx, step, identity="__nonadmin__", wait=True):
    """Bridge-driven execution of ONE frozen leg, by default as the throwaway non-admin identity."""
    ident = NONADMIN if identity == "__nonadmin__" else identity
    arg = "(%s, %d : nat)" % (eid_arg(ev), idx)
    if wait:
        rep, _ = retry_cooldown(step, BRIDGE, "executeReservedLeg", arg, identity=ident)
        return rep
    return call(step, BRIDGE, "executeReservedLeg", arg, identity=ident)


def exec_summary(rep):
    """executeReservedLeg reply -> ("ok", status tag, status payload, from_record, LegRecord) | ("err", tag, payload) | ("trap", text)"""
    if not rep.ok:
        return ("trap", rep.err)
    t, p = V(rep.val)
    if t == "err":
        et, ep = V(p)
        return ("err", et, ep)
    st, pl = V(p["status"])
    return ("ok", st, pl, p["from_record"], p["leg"])


def leg_records(ev):
    return call("bridge getLegStatuses", BRIDGE, "getLegStatuses", "(%s)" % eid_arg(ev), record=False).val


def history_pages(label, want_verified, want_intake, desc):
    v = call("bridge getRecentEvents (public verified history)", BRIDGE, "getRecentEvents", "(null, 100 : nat)").val
    i = call("bridge getIntakeEvents (operator/audit intake)", BRIDGE, "getIntakeEvents", "(null, 100 : nat)").val
    vs, is_ = [n(e["seq"]) for e in v], [n(e["seq"]) for e in i]
    vst, ist = {n(e["seq"]): tag(e["status"]) for e in v}, {n(e["seq"]): tag(e["status"]) for e in i}
    chk(label + "-verified", "public history %s: %s" % (desc, want_verified), vs == want_verified and all(x in ("Forwarded", "Quarantined") for x in vst.values()), {"seqs": vs, "statuses": vst})
    chk(label + "-intake", "operator intake page %s: %s" % (desc, want_intake), is_ == want_intake and all(x not in ("Forwarded", "Quarantined") for x in ist.values()), {"seqs": is_, "statuses": ist})
    chk(label + "-partition", "the two pages are disjoint (an Invalid or unacknowledged entry is never listed as verified delivery)", not (set(vs) & set(is_)))
    return vst, ist


def group_by_recipient(legs):
    g = {}
    for l in legs:
        g.setdefault((l["owner"], json.dumps(l["sub"])), {"owner": l["owner"], "sub": l["sub"], "holder": l["holder"], "amount": 0})
        g[(l["owner"], json.dumps(l["sub"]))]["amount"] += l["amount"]
    return list(g.values())


def snapshot_balances(label, legs):
    return {(r["owner"], json.dumps(r["sub"])): bal(r["owner"], r["sub"]) for r in group_by_recipient(legs)}


def check_balances(label, legs, before, expect_mult=1):
    ok = True
    for r in group_by_recipient(legs):
        k = (r["owner"], json.dumps(r["sub"]))
        after = bal(r["owner"], r["sub"])
        want = r["amount"] * expect_mult
        BALANCES.append({"label": label, "holder": r["holder"], "owner": r["owner"], "subaccount_hex": bytes(r["sub"]).hex() if r["sub"] else None,
                         "before": before[k], "after": after, "delta": after - before[k], "expected_delta": want})
        ok &= (after - before[k] == want)
    return ok


def record_block(block):
    tx = tx_at(block)
    BLOCKS[str(block)] = tx
    return tx


def mint_matches(leg, block):
    tx = record_block(block)
    if not tx or tx["kind"] != "mint" or not tx["mint"]:
        return False
    m = tx["mint"][0]
    return (m["to"]["owner"] == leg["owner"] and (opt(m["to"]["subaccount"]) == leg["sub"]) and n(m["amount"]) == leg["amount"]
            and opt(m["memo"]) == leg["memo"] and n(opt(m["created_at_time"])) == leg["created_at_time"])


# ================================================================ scenario
def main():
    receipt = {"receipt": "IMPL-49b v2: bridge-driven frozen-leg execution by a non-admin, verified vs intake history; bridge -> Shenanigans -> PP ledger on the local replica", "date": "2026-10-14",
               "supersedes": "receipt-2026-10-14.json (IMPL-47d v1: every initial leg was executed by the admin identity)",
               "started": now_iso(), "network": "local dfx replica 127.0.0.1:4943 (pocket-ic); nothing public"}

    # ---- 0. environment -------------------------------------------------------------------
    log("0. environment")
    ids = {"pp_ledger": LEDGER, "siws_provider": SIWS, "shenanigans": SHEN, "native_sol_bridge": BRIDGE, "sol_rpc_stub": STUB}
    receipt["canisters"] = {k: {"id": v, "module_hash_at_start": module_hash(v)} for k, v in ids.items()}
    receipt["operator_principal"] = ME
    ok, ver = dfx_admin("dfx version", "--version", record=False)
    receipt["dfx_version"] = ver.splitlines()[0] if ver else None
    mq = call("icrc1_minting_account", LEDGER, "icrc1_minting_account")
    chk("ENV-1", "pp_ledger minting account is shenanigans' mainnet principal (no subaccount)", opt(mq.val)["owner"] == SHEN and opt(opt(mq.val)["subaccount"]) is None, mq.val)
    sp = call("siws get_principal(bound pubkey)", SIWS, "get_principal", '("%s")' % BOUND_PUBKEY)
    chk("ENV-2", "real siws_provider resolves Bow1CG... (%s)" % BOUND_PRINCIPAL, "Ok" in sp.val, sp.val if "Ok" not in sp.val else None)
    chk("ENV-3", "an unbound pubkey does not resolve", "Err" in call("siws get_principal(unbound)", SIWS, "get_principal", '("4Nd1mBQtrMJVYVfKGEfnrXz1pZzU3x9Jhc1sYQhQ6Wq7")').val)
    stat = lambda cid: dfx_admin("canister status " + NAMES[cid], "canister", "status", cid, record=False)[1]
    receipt["bridge_status_at_start"] = [l for l in stat(BRIDGE).splitlines() if "Balance" in l or "Memory Size" in l]
    cfg = call("bridge getConfig", BRIDGE, "getConfig")
    chk("ENV-4", "bridge configured (program, domain, authority, shenanigans)", opt(cfg.val) and opt(cfg.val)["program_id"] == PROGRAM and opt(cfg.val)["event_authority"] == AUTHORITY and opt(cfg.val)["shenanigans"] == SHEN, cfg.val)
    pol = call("getNativePolicyTable", SHEN, "getNativePolicyTable")
    chk("ENV-5", "policy v1 pinned rates 6000/12000/18000/120000, gift 500, cascade 1000/5000", len(pol.val) == 1 and all(n(pol.val[0][k]) == v for k, v in {"simple21DayPpPerSol": 6000, "compounding15DayPpPerSol": 12000, "compounding30DayPpPerSol": 18000, "backerPpPerSol": 120000, "signupGiftPp": 500, "cascadeInitialBps": 1000, "cascadePassthroughBps": 5000}.items()), pol.val)
    al = call("getNativeBridgeAllowlist", SHEN, "getNativeBridgeAllowlist")
    chk("ENV-6", "shenanigans bridge allowlist = [bridge]", al.val == [BRIDGE], al.val)
    chk("ENV-7", "stub holds the 29 fixtures", n(call("stub fixtureCount", STUB, "fixtureCount").val) == 29)
    call("stub setMode Agree", STUB, "setMode", "(variant { Agree })")
    call("stub clearCallLog", STUB, "clearCallLog")

    # the throwaway identity must be UNPRIVILEGED everywhere or the R2 proof means nothing
    privileged = []
    for nm, cid in (("native_sol_bridge", BRIDGE), ("shenanigans", SHEN), ("pp_ledger", LEDGER)):
        okc, info = dfx_admin("canister info " + nm, "canister", "info", cid, record=False)
        if NONADMIN_PRINCIPAL in info:
            privileged.append(nm)
    rl = call("bridge getRelayers", BRIDGE, "getRelayers").val
    chk("ENV-8", "mc-nonadmin (%s) is not a controller of the bridge, Shenanigans or the ledger, not a relayer, not on the Shenanigans bridge allowlist, and is not the operator identity" % NONADMIN_PRINCIPAL,
        not privileged and NONADMIN_PRINCIPAL not in rl and NONADMIN_PRINCIPAL not in al.val and NONADMIN_PRINCIPAL != ME, {"controller_of": privileged, "relayers": rl, "allowlist": al.val})
    receipt["nonadmin_identity"] = {"name": NONADMIN, "principal": NONADMIN_PRINCIPAL, "role": "throwaway local dfx identity used for every initial leg execution and for verifyAndForward/releasePending in v2"}
    L0, S0 = ledger_len(), supply()
    receipt["ledger_baseline"] = {"log_length": L0, "total_supply": S0}

    # ---- events (derived from the raw fixture bytes by the Python oracle) --------------------
    E1, E2, E3, E4 = (fixture_event(f) for f in ("closing-012", "closing-009", "flows-007", "flows-009"))
    chk("EV-1", "closing-012 is PositionOpened(Simple) for Bow1CG... round 2", E1["kind"] == "PositionOpened" and E1["beneficiary"] == BOUND_PUBKEY and E1["plan"] == "Simple" and E1["round"] == 2 and E1["basis"] == 100_000_000, E1)
    chk("EV-2", "closing-009 is PromotionApplied(close 1) with winner Bow1CG...", E2["kind"] == "PromotionApplied" and E2["beneficiary"] == BOUND_PUBKEY and E2["close"] == 1 and E2["basis"] == 907_500_000, E2)
    chk("EV-3", "flows-007 is PositionOpened for a burner owner that is not SIWS-bound", E3["kind"] == "PositionOpened" and E3["beneficiary"] != BOUND_PUBKEY, E3)
    chk("EV-4", "flows-009 is SeriesAFunded for a burner owner", E4["kind"] == "SeriesAFunded" and E4["beneficiary"] != BOUND_PUBKEY, E4)
    receipt["events"] = {"E1_open_bound": E1, "E2_promotion_bound": E2, "E3_open_unbound": E3, "E4_seriesA_unbound": E4}

    # ---- 1. submit -------------------------------------------------------------------------
    log("1. submit descriptors (relayer = operator identity)")
    def submit(ev, step, **o):
        return call(step, BRIDGE, "submitEventDescriptor", "(%s)" % desc_arg(ev, **o))
    r1 = submit(E1, "submit E1")
    r2 = submit(E2, "submit E2")
    r4 = submit(E4, "submit E4")
    E3f = dict(E3)
    r3f = submit(E3, "submit E3 with a FORGED basis (+1 lamport)", basis=E3["basis"] + 1)
    seqs = [n(V(V(r.val)[1])[1]["seq"]) for r in (r1, r2, r4, r3f)]
    chk("SUB-1", "four descriptors accepted with seq 1,2,3,4", seqs == [1, 2, 3, 4], seqs)
    dup = submit(E1, "re-submit E1 (identical)")
    chk("SUB-2", "re-submitting the same event id is a Duplicate that changes nothing", tag(V(dup.val)[1]) == "Duplicate" and V(V(dup.val)[1])[1]["same_descriptor"] is True, dup.val)
    badc = call("submit with wrong cluster_domain", BRIDGE, "submitEventDescriptor", "(%s)" % desc_arg(E1, domain=b"musical-chairs-devnet-domain-v1!"))
    chk("SUB-3", "descriptor for another cluster is refused without being stored", tag(V(badc.val)[1]) == "Rejected", badc.val)
    anon = call("submit as anonymous", BRIDGE, "submitEventDescriptor", "(%s)" % desc_arg(E1), identity="anonymous")
    chk("SUB-4", "non-relayer caller gets NotRelayer", V(anon.val)[1] == {"NotRelayer": None} if anon.ok else False, anon.val if anon.ok else anon.err)

    # ---- 2. verify + forward (mode Agree) ---------------------------------------------------
    log("2. verifyAndForward with two agreeing providers")
    def vaf(ev, step, record=True, identity=None):
        return call(step, BRIDGE, "verifyAndForward", "(%s)" % eid_arg(ev), identity=identity, record=record)
    keyE = {}
    for name, ev in (("E1", E1), ("E2", E2)):
        rep = vaf(ev, "verifyAndForward %s (%s)" % (name, ev["fixture"]))
        s = bridge_summary(rep)
        ok = s[0] == "OK" and s[1] == "Forwarded" and s[2]["already_observed"] is False
        keyE[name] = s[2]["obligation_id"] if ok else None
        chk("FWD-%s" % name, "%s verified against both providers and forwarded: Shenanigans #Observed" % name, ok, rep.val if rep.ok else rep.err)
    k1 = call("nativeEventKey E1", SHEN, "nativeEventKey", "(%s)" % eid_arg(E1)).val
    chk("FWD-key", "the obligation id the bridge stored is Shenanigans' canonical event key", keyE["E1"] == k1, [keyE["E1"], k1])
    cur = n(call("getCursor", BRIDGE, "getCursor").val)
    chk("CUR-1", "cursor == 2 after E1 and E2 are Observed", cur == 2, cur)
    slog = call("stub getCallLog", STUB, "getCallLog").val
    mine = [c for c in slog if c["signature"] in (E1["sig"], E2["sig"])]
    chk("RPC-1", "4 getTransaction calls: one per provider per event (A and B)", len(mine) == 4 and sorted(c["provider"] for c in mine) == ["A", "A", "B", "B"], [(c["provider"], c["served"]) for c in mine])
    chk("RPC-2", "every call asked for finalized / maxSupportedTransactionVersion 0 / base64 and attached 25_000_000_000 cycles",
        all(tag(c["commitment"][0]) == "finalized" if c["commitment"] else False for c in mine) and all(opt(c["maxSupportedTransactionVersion"]) == 0 or n(opt(c["maxSupportedTransactionVersion"])) == 0 for c in mine)
        and all(tag(c["encoding"][0]) == "base64" for c in mine) and all(n(c["cycles_available"]) == 25_000_000_000 for c in mine), mine[:2])

    # policy v2 appended AFTER E1/E2 are observed (their obligations pinned v1); E3 is observed under v2
    p2 = call("appendNativePolicyVersion v2 (different rates)", SHEN, "appendNativePolicyVersion",
              "(record { simple21DayPpPerSol = 7_000 : nat; compounding15DayPpPerSol = 13_000 : nat; compounding30DayPpPerSol = 19_000 : nat; backerPpPerSol = 130_000 : nat; signupGiftPp = 300 : nat; cascadeInitialBps = 1_000 : nat; cascadePassthroughBps = 5_000 : nat; applyEffects = true; dedupWindowNs = 86_400_000_000_000 : nat })")
    chk("POL-1", "policy v2 appended", n(p2.val) == 2, p2.val)

    # E4 under DisagreeB: stays Unverified, cursor does not move
    call("stub setMode DisagreeB", STUB, "setMode", "(variant { DisagreeB })")
    t_e4 = time.time()
    rep = vaf(E4, "verifyAndForward E4 [mode DisagreeB]")
    s = bridge_summary(rep)
    MODES.append({"mode": "DisagreeB", "event": "E4 flows-009", "reply_status": s[1] if s[0] == "OK" else s, "reason": s[2] if s[0] == "OK" else None})
    chk("MODE-DisagreeB", "provider B returns a different slot: E4 stays Unverified(Disagreement), nothing forwarded", s[0] == "OK" and s[1] == "Unverified" and tag(s[2]["reason"]) == "Disagreement", rep.val if rep.ok else rep.err)
    call("stub setMode Agree", STUB, "setMode", "(variant { Agree })")
    # E3f: forged descriptor is proven false; E3 corrected goes in as a new entry
    rep = call("verifyAndForward E3 (forged-basis entry, seq 4)", BRIDGE, "verifyAndForward", "(%s)" % eid_arg(E3))
    s = bridge_summary(rep)
    chk("FWD-forged", "forged basis is proven false from the agreed finalized observation: Invalid(BasisMismatch), a durable record", s[0] == "OK" and s[1] == "Invalid" and tag(s[2]["reason"]) == "BasisMismatch", rep.val if rep.ok else rep.err)
    r3 = submit(E3, "submit E3 (corrected descriptor, same event id)")
    chk("SUB-5", "the corrected descriptor is accepted as a NEW entry (seq 5) because the old one is Invalid", tag(V(r3.val)[1]) == "Submitted" and n(V(V(r3.val)[1])[1]["seq"]) == 5, r3.val)
    rep = vaf(E3, "verifyAndForward E3 (corrected)")
    s = bridge_summary(rep)
    chk("FWD-E3", "E3 verified and forwarded (Observed under policy v2)", s[0] == "OK" and s[1] == "Forwarded", rep.val if rep.ok else rep.err)
    rc = call("releasePending E3 immediately after its verifyAndForward", BRIDGE, "releasePending", "(%s)" % eid_arg(E3))
    chk("COOL-1", COOL1_TEXT, cool1_ok(rc), rc.val if rc.ok else rc.err)
    FINDINGS.append("RESOLVED (IMPL-47d finding 1): the bind step used to inherit verifyAndForward's 30 s attempt gate (Outbox.beginBind reads Entry.next_attempt_at), so a just-forwarded event answered err Cooldown for up to 30 s. ca00bbc clears the gate when a forward lands in Forwarded and IMPL-49b also clears it on a successful bind; COOL-1 asserts the immediate bind is not throttled.")
    cur = n(call("getCursor", BRIDGE, "getCursor").val)
    pend = call("getPending(after=cursor)", BRIDGE, "getPending", "(%d : nat, 10 : nat)" % cur).val
    chk("CUR-2", "cursor stays at 2: E4 (seq 3, Unverified) blocks it although seq 4 is Invalid and seq 5 is Forwarded", cur == 2 and [n(p["seq"]) for p in pend] == [3], {"cursor": cur, "pending": [n(p["seq"]) for p in pend]})
    for nm, ev, want in (("E1", E1, 1), ("E2", E2, 1), ("E3", E3, 2)):
        o = obl(ev)
        chk("POL-pin-" + nm, "%s obligation pinned policy v%d at observation" % (nm, want), o and o["policy_version"] == want and o["status"] == "Observed", o and (o["policy_version"], o["status"]))

    # ---- 2b. live negative controls for executeReservedLeg and the verified/intake history pages ----
    log("2b. executeReservedLeg refusals (valid canister, wrong state) and history pages")
    def refused(ev, idx, want, step, identity="__nonadmin__"):
        es = exec_summary(exec_leg(ev, idx, step, identity=identity, wait=False))
        return es[0] == "err" and es[1] == want, es
    ok_, es = refused(E3, 0, "NotBound", "executeReservedLeg E3 leg 0 [E3 is Forwarded but UNBOUND]")
    chk("NEG-1", "negative control (live): E3 is Forwarded but has no SIWS binding -> err NotBound; nothing reached Shenanigans", ok_, es)
    ok_, es = refused(E1, 0, "NotBound", "executeReservedLeg E1 leg 0 [E1 is Forwarded, not yet bound]")
    chk("NEG-2", "negative control (live): E1 is Forwarded but not yet bound -> err NotBound", ok_, es)
    ok_, es = refused(E4, 0, "NotForwarded", "executeReservedLeg E4 leg 0 [E4 is Unverified]")
    chk("NEG-3", "negative control (live): E4 is Unverified (not forwarded) -> err NotForwarded", ok_, es)
    sig_x = dict(E1); sig_x["sig"] = "1" * 64
    ok_, es = refused(sig_x, 0, "UnknownEvent", "executeReservedLeg for an event the bridge never saw")
    chk("NEG-4", "unknown event -> err UnknownEvent", ok_, es)
    ok_, es = refused(E1, 64, "BadLegIndex", "executeReservedLeg E1 leg 64")
    chk("NEG-5", "leg index 64 (> MAX_LEG_INDEX 63) -> err BadLegIndex", ok_, es)
    ok_, es = refused(E1, 0, "Anonymous", "executeReservedLeg as anonymous", identity="anonymous")
    chk("NEG-6", "the anonymous principal is refused -> err Anonymous", ok_, es)
    chk("NEG-7", "none of the refusals recorded a leg or touched Shenanigans (no leg records for E1/E3/E4; E1 and E3 still Observed with no legs)",
        all(leg_records(e) == [] for e in (E1, E3, E4)) and obl(E1)["status"] == "Observed" and obl(E1)["legs"] == [] and obl(E3)["legs"] == [])
    history_pages("PAGES-1", [5, 2, 1], [4, 3], "after round 1 (E1, E2, E3 Forwarded; E4 Unverified; the forged E3 entry Invalid)")
    chk("PAGES-1-paging", "paging by `before` is unchanged: getRecentEvents(before = 5) -> [2, 1]; getIntakeEvents(before = 4) -> [3]",
        [n(e["seq"]) for e in call("bridge getRecentEvents(before 5)", BRIDGE, "getRecentEvents", "(opt (5 : nat), 100 : nat)", record=False).val] == [2, 1]
        and [n(e["seq"]) for e in call("bridge getIntakeEvents(before 4)", BRIDGE, "getIntakeEvents", "(opt (4 : nat), 100 : nat)", record=False).val] == [3])

    # ---- 3. bridge upgrade preserves state --------------------------------------------------
    log("3. bridge upgrade")
    def bridge_state():
        return canon({"cursor": call("c", BRIDGE, "getCursor", record=False).val,
                      "recent": call("r", BRIDGE, "getRecentEvents", "(null, 100 : nat)", record=False).val,
                      "pending": call("p", BRIDGE, "getPending", "(0 : nat, 100 : nat)", record=False).val,
                      "config": call("g", BRIDGE, "getConfig", record=False).val,
                      "relayers": call("y", BRIDGE, "getRelayers", record=False).val})
    before = bridge_state(); h0 = module_hash(BRIDGE)
    ok, out = dfx_admin("upgrade native_sol_bridge (same wasm)", "canister", "install", "native_sol_bridge", "--mode", "upgrade", "--wasm-memory-persistence", "keep")
    after = bridge_state(); h1 = module_hash(BRIDGE)
    UPGRADES.append({"point": "bridge: after round 1 (3 Forwarded, 1 Unverified gap, 1 Invalid)", "canister": "native_sol_bridge", "module_hash_before": h0, "module_hash_after": h1, "install_ok": ok, "state_identical": before == after, "compared": "getCursor, getRecentEvents(100), getPending(0), getConfig, getRelayers"})
    chk("UPG-bridge", "bridge upgrade (keep wasm memory) preserves outbox, cursor, config and relayers byte for byte", ok and before == after)

    # ---- 4. E1: bind through the real siws_provider, pinned policy, idempotent binding ------------
    log("4. E1: releasePending -> bindAndReserve through the real siws_provider")
    rep, first = retry_cooldown("releasePending E1", BRIDGE, "releasePending", "(%s)" % eid_arg(E1))

    t, p = V(rep.val)
    chk("BIND-E1", "releasePending(E1): Shenanigans #Reserved via the real siws_provider; bridge binding Bound", t == "ok" and p["shenanigans"] == [{"Reserved": None}] and tag(p["binding"]) == "Bound", rep.val)
    o1 = obl(E1, "getNativeObligation E1 (after bind)")
    chk("BIND-E1-beneficiary", "obligation beneficiary is the siws principal of Bow1CG...", o1["beneficiary"] == BOUND_PRINCIPAL, o1["beneficiary"])
    want = exp_legs(E1["basis"], 6000, gift_pp=500)
    got = [(l["role"], l["amount"]) for l in o1["legs"]]
    chk("LEG-E1", "4 legs under pinned policy v1 (NOT v2): gift 45e9, gift-cascade residual 5e9, player 54e9, cascade residual 6e9",
        o1["status"] == "Reserved" and o1["policy_version"] == 1 and got == [(r, a) for r, a, _ in want], {"got": got, "want": want})
    chk("LEG-E1-holders", "player/gift legs go to the bound principal's chip subaccount, residuals to the house (= admin)", [l["holder"] for l in o1["legs"]] == [BOUND_PRINCIPAL, ME, BOUND_PRINCIPAL, ME], [l["holder"] for l in o1["legs"]])
    chk("LEG-E1-frozen", "every leg has a 32-byte memo, one shared created_at_time, attempts 0, status Pending", all(len(l["memo"]) == 32 and l["status"] == "Pending" and l["attempts"] == 0 for l in o1["legs"]) and len({l["created_at_time"] for l in o1["legs"]}) == 1)
    rep = call("releasePending E1 (second)", BRIDGE, "releasePending", "(%s)" % eid_arg(E1))
    chk("BIND-E1-idem-bridge", "second releasePending returns the stored binding without calling Shenanigans", V(rep.val)[0] == "ok" and V(rep.val)[1]["shenanigans"] == [], rep.val)
    rep = call("bindAndReserve E1 directly (operator)", SHEN, "bindAndReserve", "(%s)" % eid_arg(E1))
    chk("BIND-E1-idem-shen", "direct second bindAndReserve -> AlreadyReserved", rep.val == {"AlreadyReserved": None}, rep.val)
    snapE1 = canon(o1["raw"])
    bal1 = snapshot_balances("E1", o1["legs"])
    gr = call("getSignupGiftReservation(bound principal) after E1 bind (IMPL-49a)", SHEN, "getSignupGiftReservation", '(principal "%s")' % BOUND_PRINCIPAL).val
    chk("GIFT-1", "the shared signup-gift reservation of the bound principal is Reserved by the native path (taken at bind, before any ledger await)", bool(gr) and gr[0]["owner"] == {"native": None} and gr[0]["state"] == {"Reserved": None}, gr)
    es = exec_summary(exec_leg(E1, 9, "executeReservedLeg E1 leg 9 (Shenanigans has only 4 legs)", wait=True))
    chk("NEG-8", "an index Shenanigans has no leg for: err CallFailed carrying Shenanigans' trap text ('unknown native reward leg'); the bridge records NO leg for it (garbage indices cannot grow the store)",
        es[0] == "err" and es[1] == "CallFailed" and "unknown native reward leg" in es[2] and leg_records(E1) == [], es)

    # ---- 5. upgrade #1 between bind and execute ---------------------------------------------
    def shen_state():
        return canon({"E1": call("a", SHEN, "getNativeObligation", "(%s)" % eid_arg(E1), record=False).val,
                      "E2": call("b", SHEN, "getNativeObligation", "(%s)" % eid_arg(E2), record=False).val,
                      "E3": call("c", SHEN, "getNativeObligation", "(%s)" % eid_arg(E3), record=False).val,
                      "obsE3": call("d", SHEN, "getNativeObservation", "(%s)" % eid_arg(E3), record=False).val,
                      "policy": call("e", SHEN, "getNativePolicyTable", record=False).val,
                      "allow": call("f", SHEN, "getNativeBridgeAllowlist", record=False).val,
                      "gift": call("g", SHEN, "getSignupGiftReservation", '(principal "%s")' % BOUND_PRINCIPAL, record=False).val})
    def upgrade_shen(point):
        before = shen_state(); h0 = module_hash(SHEN)
        ok, out = dfx_admin("upgrade shenanigans (%s)" % point, "canister", "install", "shenanigans", "--mode", "upgrade", "--wasm-memory-persistence", "keep")
        after = shen_state(); h1 = module_hash(SHEN)
        rs = call("shenanigans getObserverStatus after upgrade", SHEN, "getObserverStatus")
        call("shenanigans stopObserver (postupgrade re-arms the legacy timer)", SHEN, "stopObserver")
        UPGRADES.append({"point": point, "canister": "shenanigans", "module_hash_before": h0, "module_hash_after": h1, "install_ok": ok, "state_identical": before == after,
                         "compared": "getNativeObligation E1/E2/E3, getNativeObservation E3, getNativePolicyTable, getNativeBridgeAllowlist, getSignupGiftReservation(bound principal)",
                         "observer_running_right_after_upgrade": rs.val.get("running") if rs.ok else None})
        return ok and before == after
    chk("UPG-1", "upgrade between bind and execute: Reserved legs, Observed obligations, policy table and allowlist identical", upgrade_shen("shenanigans: E1 Reserved/Pending, E2 + E3 Observed"))

    # ---- 6. B3: ledger down => SubmittedUnknown, upgrade, then identical retry (all bridge-driven) --
    log("6. E1 leg 0 executed through the bridge by the NON-ADMIN identity with pp_ledger stopped (SubmittedUnknown boundary)")
    leg0 = o1["legs"][0]
    ok, out = dfx_admin("stop pp_ledger (a few seconds)", "canister", "stop", LEDGER)
    rep = exec_leg(E1, 0, "executeReservedLeg E1 leg 0 [ledger stopped] (non-admin)")
    es = exec_summary(rep)
    oU = obl(E1)
    chk("SU-1", "bridge-driven leg 0 with the ledger down: ok, status SubmittedUnknown (the outcome of a call to a stopped ledger is unknown), from_record = false",
        es[0] == "ok" and es[1] == "SubmittedUnknown" and es[3] is False, rep.val if rep.ok else rep.err)
    lr = leg_records(E1)
    chk("SU-1b", "the bridge recorded the answer: getLegStatuses(E1) = [leg 0: SubmittedUnknown, calls 1, no error]",
        len(lr) == 1 and n(lr[0]["index"]) == 0 and opt(lr[0]["status"]) == {"SubmittedUnknown": None} and n(lr[0]["calls"]) == 1 and lr[0]["last_error"] == [], lr)
    chk("SU-2", "SubmittedUnknown persisted before the await: attempts = 1, frozen memo/created_at_time unchanged", oU["legs"][0]["status"] == "SubmittedUnknown" and oU["legs"][0]["attempts"] == 1 and oU["legs"][0]["memo"] == leg0["memo"] and oU["legs"][0]["created_at_time"] == leg0["created_at_time"], oU["legs"][0]["status_payload"])
    chk("UPG-1b", "upgrade while a leg is SubmittedUnknown: state identical", upgrade_shen("shenanigans: E1 leg 0 SubmittedUnknown (ledger stopped)"))
    ok, out = dfx_admin("start pp_ledger", "canister", "start", LEDGER)
    time.sleep(2)
    chk("SU-3", "no mint happened while the ledger was down", ledger_len() == L0 and bal(leg0["owner"], leg0["sub"]) == bal1[(leg0["owner"], json.dumps(leg0["sub"]))])
    rep = exec_leg(E1, 0, "executeReservedLeg E1 leg 0 again (non-admin): Shenanigans re-presents the identical frozen fields")
    es = exec_summary(rep)
    oR = obl(E1)
    chk("SU-4", "the bridge-driven retry commits the SubmittedUnknown leg with the identical memo and created_at_time (Shenanigans attempts = 2)", es[0] == "ok" and es[1] == "Committed" and es[3] is False and oR["legs"][0]["attempts"] == 2 and oR["legs"][0]["memo"] == leg0["memo"] and oR["legs"][0]["created_at_time"] == leg0["created_at_time"], rep.val if rep.ok else rep.err)
    blk0 = oR["legs"][0]["block"]
    chk("SU-5", "ledger block %s is an exact mint (kind, to, amount, memo, created_at_time) of leg 0" % blk0, mint_matches(oR["legs"][0], blk0))
    gr = call("getSignupGiftReservation(bound principal) after the gift leg committed", SHEN, "getSignupGiftReservation", '(principal "%s")' % BOUND_PRINCIPAL).val
    chk("GIFT-2", "the gift leg (leg 0) committed, so the shared reservation is now Committed (owner native)", bool(gr) and gr[0]["owner"] == {"native": None} and gr[0]["state"] == {"Committed": None}, gr)
    lr = leg_records(E1)
    chk("SU-6", "bridge record for leg 0 is now Committed{%s} with calls = 2" % blk0, len(lr) == 1 and opt(lr[0]["status"]) == {"Committed": {"block": str(blk0)}} and n(lr[0]["calls"]) == 2, lr)

    # ---- 7. execute the remaining legs (bridge-driven, non-admin) ---------------------------------
    log("7. execute legs 1..3 through the bridge as the non-admin identity")
    for i in (1, 2, 3):
        rep = exec_leg(E1, i, "executeReservedLeg E1 leg %d (non-admin)" % i)
        es = exec_summary(rep)
        chk("EXE-E1-%d" % i, "leg %d -> ok, Committed, from_record = false (a real call to Shenanigans)" % i, es[0] == "ok" and es[1] == "Committed" and es[3] is False, rep.val if rep.ok else rep.err)
    o1 = obl(E1, "getNativeObligation E1 (all executed)")
    blocks = [l["block"] for l in o1["legs"]]
    chk("EXE-E1-status", "obligation Completed, every leg Committed with distinct blocks %s" % blocks, o1["status"] == "Completed" and all(l["status"] == "Committed" for l in o1["legs"]) and len(set(blocks)) == 4, blocks)
    chk("EXE-E1-mints", "each block is an exact mint of its leg (kind, to, amount, memo, created_at_time)", all(mint_matches(l, l["block"]) for l in o1["legs"]))
    chk("EXE-E1-balances", "recipient chip balances rose by exactly the leg amounts (Bow1 chip +99e9, house chip +11e9)", check_balances("E1", o1["legs"], bal1))
    chk("EXE-E1-supply", "ledger supply rose by exactly the four legs; log grew by four blocks", supply() == S0 + sum(l["amount"] for l in o1["legs"]) and ledger_len() == L0 + 4, [supply() - S0, ledger_len() - L0])
    lr = leg_records(E1)
    chk("EXE-E1-records", "getLegStatuses(E1) lists 4 legs, ascending, each Committed at the block Shenanigans reports", [n(r["index"]) for r in lr] == [0, 1, 2, 3] and all(opt(r["status"]) == {"Committed": {"block": str(l["block"])}} for r, l in zip(lr, o1["legs"])), lr)
    alw = call("getNativeBridgeAllowlist", SHEN, "getNativeBridgeAllowlist").val
    chk("EXE-E1-allowlist", "the Shenanigans observation allowlist is still exactly [bridge]: the non-admin principal and nobody else was added to make this work", alw == [BRIDGE], alw)
    balE1 = {k: bal(r["owner"], r["sub"]) for k, r in ((json.dumps([r["owner"], r["sub"]]), r) for r in group_by_recipient(o1["legs"]))}
    Lh, Sh = ledger_len(), supply()

    # ---- 8. idempotency ---------------------------------------------------------------------
    log("8. idempotency: a second executeReservedLeg / retry of Committed legs")
    calls_before = {n(r["index"]): n(r["calls"]) for r in lr}
    att_before = {l["index"]: l["attempts"] for l in o1["legs"]}
    same = True
    from_rec = True
    for l in o1["legs"]:
        a = exec_leg(E1, l["index"], "executeReservedLeg E1 leg %d AGAIN (non-admin)" % l["index"], wait=False)
        es = exec_summary(a)
        same &= es[0] == "ok" and es[1] == "Committed" and es[2] == {"block": str(l["block"])}
        from_rec &= es[0] == "ok" and es[3] is True
        b = call("retryNativeRewardLeg E1 leg %d AGAIN (non-admin; public)" % l["index"], SHEN, "retryNativeRewardLeg", "(%s, %d : nat)" % (eid_arg(E1), l["index"]), identity=NONADMIN)
        same &= b.ok and b.val == {"Committed": {"block": str(l["block"])}}
    chk("IDEM-1", "a second executeReservedLeg for every Committed leg answers from the bridge's record (from_record = true) with no Cooldown error even back to back; public retryNativeRewardLeg also returns the stored Committed block", same and from_rec)
    chk("IDEM-1b", "no second call: the bridge's per-leg call counts and Shenanigans' per-leg attempts are unchanged", {n(r["index"]): n(r["calls"]) for r in leg_records(E1)} == calls_before and {l["index"]: l["attempts"] for l in obl(E1)["legs"]} == att_before, [calls_before, att_before])
    chk("IDEM-2", "no second mint: ledger log length, total supply and recipient balances unchanged", ledger_len() == Lh and supply() == Sh and all(bal(r["owner"], r["sub"]) == balE1[json.dumps([r["owner"], r["sub"]])] for r in group_by_recipient(o1["legs"])))
    calls_before_rpc = n(call("stub getCallCount", STUB, "getCallCount", record=False).val)
    rep = vaf(E1, "verifyAndForward E1 AGAIN (non-admin)", identity=NONADMIN)
    sA = bridge_summary(rep)
    calls_after_rpc = n(call("stub getCallCount", STUB, "getCallCount", record=False).val)
    chk("IDEM-3", "second verifyAndForward returns the stored Forwarded status (no new RPC call, no new Shenanigans call)", sA[0] == "OK" and sA[1] == "Forwarded" and sA[2]["already_observed"] is False and calls_before_rpc == calls_after_rpc, [calls_before_rpc, calls_after_rpc, rep.val])

    # ---- 9. upgrade #2 between execute and reconcile; reconcile guard -------------------------
    chk("UPG-2", "upgrade between execute and reconcile: Completed obligation, blocks and memos identical", upgrade_shen("shenanigans: E1 Completed (all legs Committed)"))
    allsame = True
    for l in o1["legs"]:
        rep = call("reconcileLegFromLedger E1 leg %d block %d" % (l["index"], l["block"]), SHEN, "reconcileLegFromLedger", "(%s, %d : nat, %d : nat)" % (eid_arg(E1), l["index"], l["block"]))
        allsame &= rep.ok and rep.val == {"Committed": {"block": str(l["block"])}}
    chk("REC-E1", "reconcile of already-Committed legs with their returned block indices returns Committed (no-op by design: no ledger call, no state change)", allsame and ledger_len() == Lh and canon(obl(E1)["raw"]) == canon(o1["raw"]))

    # ---- 10. E2 promotion + rollback => real reconcile --------------------------------------
    log("10. E2 (PromotionApplied): bind, then snapshot rollback to exercise reconcileLegFromLedger for real")
    rep, first = retry_cooldown("releasePending E2", BRIDGE, "releasePending", "(%s)" % eid_arg(E2))
    t, p = V(rep.val)
    chk("BIND-E2", "releasePending(E2) -> Reserved (the beneficiary is the event's winner Bow1CG..., not any signer)", t == "ok" and p["shenanigans"] == [{"Reserved": None}], rep.val)
    o2 = obl(E2, "getNativeObligation E2 (after bind)")
    want2 = exp_legs(E2["basis"], 120000, gift_pp=None)
    got2 = [(l["role"], l["amount"]) for l in o2["legs"]]
    chk("LEG-E2", "2 legs under pinned v1 backer rate, NO signup gift (marker already claimed by E1): player 9_801e9, residual 1_089e9", o2["policy_version"] == 1 and got2 == [(r, a) for r, a, _ in want2] and not any(l["role"] == "signupGift" for l in o2["legs"]), {"got": got2, "want": want2})
    bal2 = snapshot_balances("E2", o2["legs"]); L2, S2 = ledger_len(), supply()
    ok, out = dfx_admin("stop shenanigans for snapshot", "canister", "stop", SHEN)
    ok2, out2 = dfx_admin("snapshot create shenanigans", "canister", "snapshot", "create", SHEN)
    snap = out2.split("Snapshot ID:")[1].split()[0] if "Snapshot ID:" in out2 else None
    dfx_admin("start shenanigans", "canister", "start", SHEN)
    chk("B7-0", "snapshot of shenanigans taken (E2 Reserved, all legs Pending)", ok and ok2 and snap, out2)
    e2a = [call("FALLBACK admin path: executeNativeRewardLeg E2 leg %d (operator is Shenanigans admin; the bridge is not involved)" % i, SHEN, "executeNativeRewardLeg", "(%s, %d : nat)" % (eid_arg(E2), i)) for i in (0, 1)]
    oE2 = obl(E2)
    chk("B7-1", "FALLBACK check: the admin path on Shenanigans still executes both E2 legs without the bridge (bridge leg records stay empty)", all(r.ok and tag(r.val) == "Committed" for r in e2a) and oE2["status"] == "Completed" and leg_records(E2) == [], [r.val for r in e2a])
    blk = [l["block"] for l in oE2["legs"]]
    chk("B7-2", "E2 balances rose by exactly the legs", check_balances("E2 before rollback", oE2["legs"], bal2))
    Lm, Sm = ledger_len(), supply()
    dfx_admin("stop shenanigans for rollback", "canister", "stop", SHEN)
    okl, outl = dfx_admin("snapshot load shenanigans (state rolls back; the ledger keeps the mints)", "canister", "snapshot", "load", SHEN, snap)
    dfx_admin("start shenanigans", "canister", "start", SHEN)
    call("shenanigans stopObserver (after snapshot load)", SHEN, "stopObserver")
    oRB = obl(E2, "getNativeObligation E2 (after rollback)")
    chk("B7-3", "after the rollback E2's legs are Pending/attempts 0 again while the ledger still holds both mints (blocks %s)" % blk, okl and all(l["status"] == "Pending" and l["attempts"] == 0 for l in oRB["legs"]) and ledger_len() == Lm and supply() == Sm and canon(obl(E1)["raw"]) == canon(o1["raw"]), [l["status"] for l in oRB["legs"]])
    # negative reconcile checks (each must trap and change nothing)
    def must_trap(label, step, args, needle, identity=None):
        before = canon(obl(E2)["raw"])
        rep = call(step, SHEN, "reconcileLegFromLedger", args, identity=identity)
        return (not rep.ok) and needle in rep.err and canon(obl(E2)["raw"]) == before, (rep.err if not rep.ok else rep.val)
    ok_, info = must_trap("N1", "reconcile E2 leg 1 against leg 0's mint block", "(%s, 1 : nat, %d : nat)" % (eid_arg(E2), blk[0]), "differs from the frozen leg")
    chk("REC-neg-1", "reconcile of leg 1 against the block of leg 0's mint is rejected (exact-field match), state unchanged", ok_, info)
    ok_, info = must_trap("N2", "reconcile E2 leg 0 against a block past the log", "(%s, 0 : nat, %d : nat)" % (eid_arg(E2), ledger_len() + 1000), "no transaction at block")
    chk("REC-neg-2", "reconcile against a block that does not exist is rejected, state unchanged", ok_, info)
    ok_, info = must_trap("N3", "reconcile E2 leg 0 against block 0 (the initial-balance mint to another account)", "(%s, 0 : nat, 0 : nat)" % eid_arg(E2), "differs from the frozen leg")
    chk("REC-neg-3", "reconcile against an unrelated real mint (block 0) is rejected, state unchanged", ok_, info)
    ok_, info = must_trap("N4", "reconcile as anonymous", "(%s, 0 : nat, %d : nat)" % (eid_arg(E2), blk[0]), "Authentication required", identity="anonymous")
    chk("REC-neg-4", "anonymous reconcile is refused", ok_, info)
    rep = call("reconcileLegFromLedger E2 leg 0 block %d (real receipt)" % blk[0], SHEN, "reconcileLegFromLedger", "(%s, 0 : nat, %d : nat)" % (eid_arg(E2), blk[0]))
    chk("REC-pos", "reconcile of the rolled-back leg 0 against its real mint: Committed{block %d} (get_transactions + archives + icrc1_minting_account decoded against the real ledger)" % blk[0], rep.ok and rep.val == {"Committed": {"block": str(blk[0])}}, rep.val if rep.ok else rep.err)
    rep = exec_leg(E2, 0, "executeReservedLeg E2 leg 0 (non-admin) after the direct reconcile")
    es = exec_summary(rep)
    chk("REC-pos-bridge", "the bridge catches up: Shenanigans reports the reconciled leg 0 as Committed{%d}; the bridge records it (from_record = false)" % blk[0], es[0] == "ok" and es[1] == "Committed" and es[2] == {"block": str(blk[0])} and es[3] is False, rep.val if rep.ok else rep.err)
    rep = exec_leg(E2, 1, "executeReservedLeg E2 leg 1 (non-admin; the ledger answers Duplicate)")
    es = exec_summary(rep)
    oD = obl(E2)
    chk("REC-dup", "bridge-driven re-presentation of the identical frozen fields of the rolled-back leg 1: the ledger answers Duplicate{duplicate_of}; Shenanigans commits the ORIGINAL block %d and the bridge records it" % blk[1], es[0] == "ok" and es[1] == "Committed" and es[2] == {"block": str(blk[1])} and oD["legs"][1]["attempts"] == 1, rep.val if rep.ok else rep.err)
    chk("REC-nomint", "no second mint: log length and total supply unchanged since before the rollback; E2 obligation Completed", ledger_len() == Lm and supply() == Sm and oD["status"] == "Completed", [ledger_len() - Lm, supply() - Sm, oD["status"]])
    chk("REC-balances", "E2 recipients hold the legs exactly once", check_balances("E2 after rollback+reconcile+duplicate", oD["legs"], bal2))
    dfx_admin("snapshot delete shenanigans", "canister", "snapshot", "delete", SHEN, snap)

    # ---- 11. E3: unbound burner stays pubkey-pending ----------------------------------------
    log("11. E3 (burner owner, no SIWS binding)")
    rep, first = retry_cooldown("releasePending E3", BRIDGE, "releasePending", "(%s)" % eid_arg(E3))
    t, p = V(rep.val)
    chk("UNB-1", "releasePending(E3) -> Shenanigans #Unbound; bridge binding stays Unbound", t == "ok" and p["shenanigans"] == [{"Unbound": None}] and tag(p["binding"]) == "Unbound", rep.val)
    ub = call("bridge getUnbound(E3 owner)", BRIDGE, "getUnbound", '("%s")' % E3["beneficiary"]).val
    chk("UNB-2", "the event stays pubkey-pending in the bridge (getUnbound lists it)", len(ub) == 1 and n(ub[0]["seq"]) == 5, [n(u["seq"]) for u in ub])
    o3 = obl(E3, "getNativeObligation E3")
    chk("UNB-3", "Shenanigans keeps it Observed: beneficiary null, no legs, nothing minted, policy v2", o3["status"] == "Observed" and o3["beneficiary"] is None and o3["legs"] == [] and o3["policy_version"] == 2, o3["status"])
    ev3 = opt(call("bridge getEvent E3", BRIDGE, "getEvent", "(%s)" % eid_arg(E3)).val)
    chk("UNB-4", "bridge entry for E3 is Forwarded (done for the cursor) with binding Unbound: observation does not wait for a SIWS binding", ev3 and tag(ev3["status"]) == "Forwarded" and tag(ev3["binding"]) == "Unbound", ev3 and (tag(ev3["status"]), tag(ev3["binding"])))
    sleep(11, "bind cooldown")
    rep = call("releasePending E3 (again)", BRIDGE, "releasePending", "(%s)" % eid_arg(E3))
    chk("UNB-5", "still Unbound on a second attempt; still listed", V(rep.val)[0] == "ok" and V(rep.val)[1]["shenanigans"] == [{"Unbound": None}] and len(call("getUnbound", BRIDGE, "getUnbound", '("%s")' % E3["beneficiary"], record=False).val) == 1, rep.val)

    # ---- 12. E4: gap modes ------------------------------------------------------------------
    log("12. E4 failure modes: each leaves the event open and the cursor behind it")
    def e4_attempt(mode, expect_reason, label):
        call("stub setMode " + mode, STUB, "setMode", "(variant { %s })" % mode)
        rep, first = retry_cooldown("verifyAndForward E4 [mode %s]" % mode, BRIDGE, "verifyAndForward", "(%s)" % eid_arg(E4))
        s = bridge_summary(rep)
        cur = n(call("getCursor", BRIDGE, "getCursor", record=False).val)
        pend = call("getPending", BRIDGE, "getPending", "(2 : nat, 10 : nat)", record=False).val
        reason = tag(s[2]["reason"]) if s[0] == "OK" and s[1] == "Unverified" else None
        MODES.append({"mode": mode, "event": "E4 flows-009", "status": s[1] if s[0] == "OK" else s, "reason": s[2]["reason"] if s[0] == "OK" and s[1] == "Unverified" else None, "attempts": s[2].get("attempts") if s[0] == "OK" and s[1] == "Unverified" else None, "cursor": cur})
        chk("MODE-" + mode, label, reason == expect_reason and cur == 2 and [n(p["seq"]) for p in pend] == [3], {"reason": reason, "cursor": cur, "pending": [n(p["seq"]) for p in pend], "reply": rep.val if rep.ok else rep.err})
        return s
    e4_attempt("DisagreeBNone", "NotFound", "provider B has no such transaction: Unverified(NotFound), cursor stays 2")
    s = e4_attempt("UnsupportedVersion", "UnsupportedTransactionVersion", "both providers answer -32015: Unverified(UnsupportedTransactionVersion) = a GAP, cursor stays 2")
    chk("MODE-gap-payload", "the gap reason carries no version (JSON-RPC -32015 gives none)", s[0] == "OK" and s[2]["reason"]["UnsupportedTransactionVersion"] == [], s[2]["reason"] if s[0] == "OK" else s)
    e4_attempt("Inconsistent", "Inconsistent", "a provider's own sources disagree (Multi*Result Inconsistent): Unverified(Inconsistent), cursor stays 2")
    call("stub setMode Agree", STUB, "setMode", "(variant { Agree })")

    # forward fails because Shenanigans is stopped: verification succeeds, the forward is retained for retry (not skipped, cursor does not move)
    ok_s, _ = dfx_admin("stop shenanigans (forward-failure test)", "canister", "stop", SHEN)
    rep, first = retry_cooldown("verifyAndForward E4 [mode Agree, Shenanigans STOPPED]", BRIDGE, "verifyAndForward", "(%s)" % eid_arg(E4))
    dfx_admin("start shenanigans", "canister", "start", SHEN)
    s = bridge_summary(rep)
    cur = n(call("getCursor", BRIDGE, "getCursor", record=False).val)
    chk("FWD-retry", "both providers agree (E4 Verified) but the forward call to a stopped Shenanigans fails: status Verified{retries = 1, last_reason}, cursor stays 2 (a failed call is not an acknowledgement)",
        ok_s and s[0] == "OK" and s[1] == "Verified" and n(s[2]["retries"]) == 1 and bool(s[2]["last_reason"]) and cur == 2, rep.val if rep.ok else rep.err)
    MODES.append({"mode": "Agree + Shenanigans stopped", "event": "E4 flows-009", "status": s[1] if s[0] == "OK" else s, "payload": s[2] if s[0] == "OK" else None, "cursor": cur})
    vst_, ist_ = history_pages("PAGES-1b", [5, 2, 1], [4, 3], "while E4 is Verified-but-unacknowledged (the forward to a stopped Shenanigans failed)")
    chk("PAGES-1b-status", "E4 (seq 3) is listed in the intake page as Verified (unacknowledged) and is NOT in the public history; Invalid (seq 4) stays intake-only", ist_.get(3) == "Verified" and 3 not in vst_ and ist_.get(4) == "Invalid", {"intake": ist_, "verified": vst_})
    call("shenanigans stopObserver (after start)", SHEN, "stopObserver")
    stub_calls_before = n(call("stub getCallCount", STUB, "getCallCount", record=False).val)

    # lost-reply path: Shenanigans already holds E4 (observed directly by an allowlisted caller) -> bridge gets AlreadyObserved
    call("setNativeBridgeAllowlist [bridge, operator] (test only)", SHEN, "setNativeBridgeAllowlist", '(vec { principal "%s"; principal "%s" })' % (BRIDGE, ME))
    d4 = desc_arg(E4)
    pre = call("observeNativeEvent E4 directly (simulates a forward whose reply was lost)", SHEN, "observeNativeEvent", "(%s)" % d4)
    chk("OBS-1", "direct observeNativeEvent(E4) -> Observed(key)", pre.ok and tag(pre.val) == "Observed", pre.val if pre.ok else pre.err)
    again = call("observeNativeEvent E4 directly AGAIN", SHEN, "observeNativeEvent", "(%s)" % d4)
    chk("OBS-2", "idempotent on the event id: second observe -> AlreadyObserved(same key)", again.ok and tag(again.val) == "AlreadyObserved" and V(again.val)[1] == V(pre.val)[1], again.val if again.ok else again.err)
    chg = call("observeNativeEvent E4 with a different basis", SHEN, "observeNativeEvent", "(%s)" % desc_arg(E4, basis=E4["basis"] + 1))
    chk("OBS-3", "same event id with different fields -> Rejected(permanent=true)", chg.ok and tag(chg.val) == "Rejected" and V(chg.val)[1]["permanent"] is True, chg.val if chg.ok else chg.err)
    call("setNativeBridgeAllowlist [bridge] (restored)", SHEN, "setNativeBridgeAllowlist", '(vec { principal "%s" })' % BRIDGE)
    rep, first = retry_cooldown("verifyAndForward E4 [mode Agree, Shenanigans already holds it]", BRIDGE, "verifyAndForward", "(%s)" % eid_arg(E4))
    s = bridge_summary(rep)
    chk("OBS-4", "bridge forward answered AlreadyObserved by Shenanigans -> Forwarded{already_observed = true}", s[0] == "OK" and s[1] == "Forwarded" and s[2]["already_observed"] is True, rep.val if rep.ok else rep.err)
    chk("OBS-5", "the retry of an already-Verified entry re-used its stored verification: no new getTransaction call reached the stub", n(call("stub getCallCount", STUB, "getCallCount", record=False).val) == stub_calls_before)
    history_pages("PAGES-2", [5, 3, 2, 1], [4], "after E4 is delivered (AlreadyObserved): the gap entry moved from intake to verified history")
    cur = n(call("getCursor", BRIDGE, "getCursor").val)
    chk("CUR-3", "only now does the cursor pass the gap: cursor == 5 (seq 1,2,3 Forwarded; 4 Invalid; 5 Forwarded)", cur == 5 and call("getPending", BRIDGE, "getPending", "(0 : nat, 10 : nat)", record=False).val == [], cur)

    # ---- 12b. Quarantined: a durable permanent Shenanigans rejection is verified-history material -------
    log("12b. E6 (synthetic re-signed copy of flows-009): Shenanigans permanently rejects it -> Quarantined")
    cl = call("stub cloneFixture(flows-009 -> synthetic copy with another signature)", STUB, "cloneFixture", '("%s", 102 : nat8)' % E4["sig"])
    newsig = opt(cl.val) if cl.ok else None
    E6 = dict(E4)
    E6["sig"], E6["fixture"] = newsig, "synthetic clone of flows-009 (NOT a captured transaction)"
    receipt["events"]["E6_synthetic_clone_of_flows-009"] = E6
    r6 = submit(E6, "submit E6 (synthetic clone)")
    chk("Q-1", "E6 accepted as seq 6", newsig and tag(V(r6.val)[1]) == "Submitted" and n(V(V(r6.val)[1])[1]["seq"]) == 6, r6.val)
    call("setNativeBridgeAllowlist [bridge, operator] (test only)", SHEN, "setNativeBridgeAllowlist", '(vec { principal "%s"; principal "%s" })' % (BRIDGE, ME))
    pre6 = call("observeNativeEvent E6 directly with a DIFFERENT basis (so the real descriptor conflicts)", SHEN, "observeNativeEvent", "(%s)" % desc_arg(E6, basis=E6["basis"] + 1))
    call("setNativeBridgeAllowlist [bridge] (restored)", SHEN, "setNativeBridgeAllowlist", '(vec { principal "%s" })' % BRIDGE)
    rep = vaf(E6, "verifyAndForward E6 (non-admin)", identity=NONADMIN)
    s6 = bridge_summary(rep)
    chk("Q-2", "Shenanigans answers Rejected(permanent=true, 'event id re-presented with different fields'): the bridge stores a durable Quarantined record carrying the reason",
        pre6.ok and s6[0] == "OK" and s6[1] == "Quarantined" and "re-presented with different fields" in s6[2]["reason"], rep.val if rep.ok else rep.err)
    cur = n(call("getCursor", BRIDGE, "getCursor").val)
    chk("Q-3", "the cursor passes the durable quarantine record: cursor == 6", cur == 6, cur)
    vst, ist = history_pages("PAGES-3", [6, 5, 3, 2, 1], [4], "with a Quarantined entry")
    chk("Q-4", "the Quarantined entry is returned by the public verified history WITH its status; the Invalid entry (seq 4) is only in the intake page", vst.get(6) == "Quarantined" and ist.get(4) == "Invalid" and 4 not in vst)
    es = exec_summary(exec_leg(E6, 0, "executeReservedLeg E6 leg 0 [E6 is Quarantined]", wait=False))
    chk("Q-5", "negative control (live): a Quarantined entry has nothing reserved -> executeReservedLeg answers err NotForwarded", es[0] == "err" and es[1] == "NotForwarded", es)

    # ---- 13. authorization -------------------------------------------------------------------
    log("13. authorization")
    r = call("observeNativeEvent as operator (not allowlisted)", SHEN, "observeNativeEvent", "(%s)" % d4)
    chk("AUTH-1", "observeNativeEvent from a non-bridge caller traps 'Unauthorized: native bridge only'", (not r.ok) and "Unauthorized: native bridge only" in r.err, r.err[:200])
    r = call("executeNativeRewardLeg as anonymous", SHEN, "executeNativeRewardLeg", "(%s, 0 : nat)" % eid_arg(E1), identity="anonymous")
    chk("AUTH-2", "executeNativeRewardLeg from anonymous traps Unauthorized", (not r.ok) and "Unauthorized: native bridge or admin only" in r.err, r.err[:200])
    r = call("bindAndReserve as anonymous", SHEN, "bindAndReserve", "(%s)" % eid_arg(E3), identity="anonymous")
    chk("AUTH-3", "bindAndReserve from anonymous traps 'Authentication required'", (not r.ok) and "Authentication required" in r.err, r.err[:200])
    r = call("executeNativeRewardLeg DIRECTLY on Shenanigans as the non-admin identity", SHEN, "executeNativeRewardLeg", "(%s, 0 : nat)" % eid_arg(E1), identity=NONADMIN)
    chk("AUTH-5", "the non-admin identity cannot execute on Shenanigans itself (traps Unauthorized): it can only reach the legs through the bridge's frozen-leg entry point", (not r.ok) and "Unauthorized" in r.err, r.err[:200])
    r = call("observeNativeEvent DIRECTLY on Shenanigans as the non-admin identity", SHEN, "observeNativeEvent", "(%s)" % d4, identity=NONADMIN)
    chk("AUTH-5b", "...nor observe events there", (not r.ok) and "Unauthorized" in r.err, r.err[:200])
    alw = call("getNativeBridgeAllowlist (final)", SHEN, "getNativeBridgeAllowlist").val
    chk("AUTH-6", "after the whole run the Shenanigans observation allowlist is again exactly [bridge]", alw == [BRIDGE], alw)
    r = call("bridge setConfig as anonymous", BRIDGE, "setRelayers", "(vec {})", identity="anonymous")
    chk("AUTH-4", "bridge setRelayers from anonymous: controllers only", r.ok and r.val == {"err": "controllers only"}, r.val if r.ok else r.err)

    # ---- 14. ledger probes (g) ---------------------------------------------------------------
    log("14. ledger error probes")
    probes = {}
    me_acct = 'record { owner = principal "%s"; subaccount = null }' % ME
    def xfer(step, memo_len=None, created=None):
        memo = "opt %s" % blob(bytes(range(1, memo_len + 1))) if memo_len else "null"
        ct = "opt (%d : nat64)" % created if created else "null"
        rep = call(step, LEDGER, "icrc1_transfer", "(record { to = %s; fee = null; memo = %s; from_subaccount = null; created_at_time = %s; amount = 1 : nat })" % (me_acct, memo, ct))
        probes[step] = rep.val if rep.ok else rep.err
        return rep
    r33 = xfer("icrc1_transfer from operator with a 33-byte memo", 33)
    r64 = xfer("icrc1_transfer from operator with a 64-byte memo", 64)
    r32 = xfer("icrc1_transfer from operator with a 32-byte memo (control)", 32)
    ref = o1["legs"][0]["created_at_time"]
    rold = xfer("icrc1_transfer with created_at_time 25 h in the past", None, ref - 25 * HOUR_NS)
    rfut = xfer("icrc1_transfer with created_at_time 2 h in the future", None, ref + 2 * HOUR_NS)
    chk("MEMO-1", "the local ledger answers an over-long memo (33 and 64 bytes, limit 32) with a canister TRAP (reject code CanisterError / IC0503, text 'the memo field size of N bytes is above the allowed limit of 32 bytes'), NOT an Err(GenericError) variant",
        (not r33.ok) and "CanisterError" in r33.err and "the memo field size of 33 bytes is above the allowed limit of 32 bytes" in r33.err and (not r64.ok) and "the memo field size of 64 bytes is above the allowed limit of 32 bytes" in r64.err, r33.err[:400] if not r33.ok else r33.val)
    FINDINGS.append("pp_ledger rejects an over-long memo with a TRAP, not an Err variant: reject code CanisterError (IC0503), message 'the memo field size of 33 bytes is above the allowed limit of 32 bytes'. Shenanigans' memoRejected(GenericError.message) heuristic (NativeRewards.mo) therefore never fires against this ledger: a trap reaches nativeSubmitLeg's catch as #CallFailed and the leg would stay SubmittedUnknown (identical retry traps again) until the dedup window passes and it quarantines as reconcile. Not reachable today (memos are always the 32-byte sha256), so informational.")
    chk("MEMO-2", "a 32-byte memo passes the memo check (control: fails only for balance -> InsufficientFunds)", r32.ok and r32.val == {"Err": {"InsufficientFunds": {"balance": "0"}}}, r32.val if r32.ok else r32.err)
    chk("MEMO-3", "created_at_time 25 h old -> Err(TooOld); 2 h ahead -> Err(CreatedInFuture{ledger_time}) (the shapes shenanigans' outcome mapping consumes)", rold.ok and rold.val == {"Err": {"TooOld": None}} and rfut.ok and tag(V(rfut.val)[1]) == "CreatedInFuture", [rold.val if rold.ok else rold.err, rfut.val if rfut.ok else rfut.err])
    receipt["ledger_error_probes"] = {"over_long_memo_33_bytes": r33.val if r33.ok else r33.err, "over_long_memo_64_bytes": r64.val if r64.ok else r64.err, "memo_32_bytes_control": r32.val if r32.ok else r32.err,
                                       "created_at_time_25h_old": rold.val if rold.ok else rold.err, "created_at_time_2h_future": rfut.val if rfut.ok else rfut.err}

    # ---- 15. phase 2 ---------------------------------------------------------------------------
    write_receipt(dict(receipt))  # checkpoint: phase 1 is complete even if phase 2 is interrupted
    phase2(receipt)

    # ---- 16. finish --------------------------------------------------------------------------
    log("16. final state")
    call("bridge getRecentEvents (final)", BRIDGE, "getRecentEvents", "(null, 20 : nat)")
    final_log = call("stub getCallLog (final)", STUB, "getCallLog").val
    receipt["stub_call_log_summary"] = {"calls": len(final_log), "by_provider": {p: sum(1 for c in final_log if c["provider"] == p) for p in ("A", "B")},
                                         "cycles_available_per_call": sorted({n(c["cycles_available"]) for c in final_log}), "served": sorted({c["served"] for c in final_log})}
    receipt["bridge_status_at_end"] = [l for l in stat(BRIDGE).splitlines() if "Balance" in l or "Memory Size" in l]
    receipt["module_hashes_at_end"] = {k: module_hash(v) for k, v in ids.items()}
    receipt["ledger_final"] = {"log_length": ledger_len(), "total_supply": supply(), "minted_by_this_run": supply() - S0}
    write_receipt(receipt)
    return receipt


# ================================================================ phase 2: concurrency + dedup window (fresh Shenanigans state)
def synth_event(label, i):
    """A synthetic observation fed DIRECTLY to Shenanigans by an allowlisted test caller (not via the bridge)."""
    return {"fixture": "synthetic:" + label, "sig": "phase2-%s" % label, "ix": 1, "inner": i, "name": "PositionOpened", "round": 7, "close": None,
            "kind": "PositionOpened", "beneficiary": BOUND_PUBKEY, "basis": 100_000_000, "plan": "Simple"}


def par(fns):
    out = [None] * len(fns)
    def run(i):
        out[i] = fns[i]()
    th = [threading.Thread(target=run, args=(i,)) for i in range(len(fns))]
    for t in th: t.start()
    for t in th: t.join()
    return out


def phase2(receipt):
    log("PHASE 2: fresh Shenanigans (reinstall) for concurrency and the dedup-window boundary")
    skip_window = E.get("MC_SKIP_WINDOW") == "1"
    ok, out = dfx_admin("phase 2: reinstall shenanigans (clears the signup markers)", "canister", "install", "shenanigans", "--mode", "reinstall", "--yes")
    call("phase2 initialize", SHEN, "initialize", '(principal "%s")' % ME)
    call("phase2 stopObserver", SHEN, "stopObserver")
    call("phase2 allowlist [bridge, operator]", SHEN, "setNativeBridgeAllowlist", '(vec { principal "%s"; principal "%s" })' % (BRIDGE, ME))
    call("phase2 policy v1 (24 h window)", SHEN, "appendNativePolicyFromMintConfig", "(86_400_000_000_000 : nat)")
    C1, C2, C3 = synth_event("C1", 0), synth_event("C2", 1), synth_event("C3", 2)
    for ev in (C1, C2, C3):
        r = call("phase2 observe %s" % ev["sig"], SHEN, "observeNativeEvent", "(%s)" % desc_arg(ev))
        chk("P2-obs-" + ev["sig"], "observed", r.ok and tag(r.val) == "Observed", r.val if r.ok else r.err)
    L, S = ledger_len(), supply()

    # --- two simultaneous FIRST events for one principal => exactly one signup gift
    rs = par([lambda: call("phase2 bindAndReserve C1 (concurrent)", SHEN, "bindAndReserve", "(%s)" % eid_arg(C1)),
              lambda: call("phase2 bindAndReserve C2 (concurrent)", SHEN, "bindAndReserve", "(%s)" % eid_arg(C2))])
    oc1, oc2 = obl(C1, "phase2 getNativeObligation C1"), obl(C2, "phase2 getNativeObligation C2")
    gifts = sum(1 for o in (oc1, oc2) for l in o["legs"] if l["role"] == "signupGift")
    chk("P2-gift", "two simultaneous first events for one principal: both Reserved and exactly ONE signupGift leg across both obligations", all(r.ok and r.val == {"Reserved": None} for r in rs) and gifts == 1, [r.val if r.ok else r.err for r in rs] + [gifts])
    chk("P2-gift-amount", "the gift leg is 45e9 under policy v1 (500 PP less the 10% cascade)", sorted(l["amount"] for o in (oc1, oc2) for l in o["legs"] if l["role"] == "signupGift") == [45_000_000_000])
    # --- two simultaneous binders of ONE event
    rs = par([lambda: call("phase2 bindAndReserve C3 (concurrent)", SHEN, "bindAndReserve", "(%s)" % eid_arg(C3)),
              lambda: call("phase2 bindAndReserve C3 (concurrent)", SHEN, "bindAndReserve", "(%s)" % eid_arg(C3))])
    vals = sorted(json.dumps(r.val) if r.ok else r.err for r in rs)
    oc3 = obl(C3, "phase2 getNativeObligation C3")
    chk("P2-2binders", "two simultaneous binders of one event: one Reserved, one AlreadyReserved; one set of legs (player + residual, no second gift)",
        vals == sorted([json.dumps({"Reserved": None}), json.dumps({"AlreadyReserved": None})]) and [l["role"] for l in oc3["legs"]] == ["player", "residual"], vals)
    # --- two simultaneous executors of ONE leg
    b3 = snapshot_balances("C3", oc3["legs"])
    rs = par([lambda: call("phase2 executeNativeRewardLeg C3 leg 0 (concurrent)", SHEN, "executeNativeRewardLeg", "(%s, 0 : nat)" % eid_arg(C3)),
              lambda: call("phase2 executeNativeRewardLeg C3 leg 0 (concurrent)", SHEN, "executeNativeRewardLeg", "(%s, 0 : nat)" % eid_arg(C3))])
    oc3 = obl(C3, "phase2 getNativeObligation C3 after concurrent execute")
    leg = oc3["legs"][0]
    one_mint = bal(leg["owner"], leg["sub"]) - b3[(leg["owner"], json.dumps(leg["sub"]))] == leg["amount"] and ledger_len() == L + 1
    chk("P2-2exec", "two simultaneous executors of one leg: no double mint (balance +leg once, one new block); final status Committed at that block; replies %s" % [r.val if r.ok else r.err[:80] for r in rs],
        one_mint and leg["status"] == "Committed", {"attempts": leg["attempts"], "replies": [r.val if r.ok else r.err[:200] for r in rs]})
    receipt["phase2_concurrency_note"] = "Both CLI calls are issued from two threads within milliseconds; whether the replica interleaved the awaits is not observable from outside (attempts on the leg: %s). The invariants asserted hold for any interleaving." % leg["attempts"]
    call("phase2 executeNativeRewardLeg C3 leg 1", SHEN, "executeNativeRewardLeg", "(%s, 1 : nat)" % eid_arg(C3))

    if skip_window:
        NOT_PROVEN.append("Dedup-window quarantine was skipped (MC_SKIP_WINDOW=1).")
        call("phase2 allowlist restored to [bridge]", SHEN, "setNativeBridgeAllowlist", '(vec { principal "%s" })' % BRIDGE)
        return
    # --- dedup window: 660 s policy (window - 5 min safety margin = legs expire 6 min after binding)
    pv = call("phase2 policy v2 (660 s window)", SHEN, "appendNativePolicyFromMintConfig", "(660_000_000_000 : nat)")
    chk("P2-pol", "policy v2 with a 660 s window appended", n(pv.val) == 2, pv.val)
    W1, W2, W3 = synth_event("W1", 3), synth_event("W2", 4), synth_event("W3", 5)
    for ev in (W1, W2, W3):
        call("phase2 observe %s" % ev["sig"], SHEN, "observeNativeEvent", "(%s)" % desc_arg(ev))
        r = call("phase2 bindAndReserve %s" % ev["sig"], SHEN, "bindAndReserve", "(%s)" % eid_arg(ev))
        chk("P2-bind-" + ev["sig"], "bound under the 660 s policy", r.ok and r.val == {"Reserved": None}, r.val if r.ok else r.err)
    ow = {ev["sig"]: obl(ev, "phase2 getNativeObligation %s" % ev["sig"]) for ev in (W1, W2, W3)}
    t_bind = time.time()
    chk("P2-bind-policy", "W1..W3 pinned policy v2; no gift (marker claimed by C1/C2); two legs each", all(o["policy_version"] == 2 and [l["role"] for l in o["legs"]] == ["player", "residual"] for o in ow.values()))
    # W2: leg 0 executes inside the window and commits (B6)
    r = call("phase2 execute W2 leg 0 (inside the window)", SHEN, "executeNativeRewardLeg", "(%s, 0 : nat)" % eid_arg(W2))
    chk("P2-W2-in", "W2 leg 0 inside the window -> Committed", r.ok and tag(r.val) == "Committed", r.val if r.ok else r.err)
    # W3: mint both legs, then roll Shenanigans back (the ledger keeps the mints)
    dfx_admin("phase2 stop shenanigans", "canister", "stop", SHEN)
    # (snapshot must predate W3's mints but follow W2's commit)
    ok2, out2 = dfx_admin("phase2 snapshot create shenanigans", "canister", "snapshot", "create", SHEN)
    snap = out2.split("Snapshot ID:")[1].split()[0] if "Snapshot ID:" in out2 else None
    dfx_admin("phase2 start shenanigans", "canister", "start", SHEN)
    w3 = [call("phase2 execute W3 leg %d" % i, SHEN, "executeNativeRewardLeg", "(%s, %d : nat)" % (eid_arg(W3), i)) for i in (0, 1)]
    w3b = [l["block"] for l in obl(W3)["legs"]]
    dfx_admin("phase2 stop shenanigans", "canister", "stop", SHEN)
    dfx_admin("phase2 snapshot load shenanigans", "canister", "snapshot", "load", SHEN, snap)
    dfx_admin("phase2 start shenanigans", "canister", "start", SHEN)
    call("phase2 stopObserver", SHEN, "stopObserver")
    dfx_admin("phase2 snapshot delete", "canister", "snapshot", "delete", SHEN, snap)
    oW3 = obl(W3)
    chk("P2-W3-rb", "W3: minted twice-over legs %s, then rolled back: legs Pending again, ledger still holds the mints" % w3b, ok2 and snap and all(r.ok and tag(r.val) == "Committed" for r in w3) and all(l["status"] == "Pending" for l in oW3["legs"]) and obl(W2)["legs"][0]["status"] == "Committed")
    LW, SW = ledger_len(), supply()
    wait = int(660 - 300 + 8 - (time.time() - t_bind))
    if wait > 0:
        sleep(wait, "until the legs' re-presentation window (660 s - 5 min margin) has passed")
    # W1: both legs past the window -> Quarantined(reconcile), NO transfer attempted
    q = [call("phase2 execute W1 leg %d (past the window)" % i, SHEN, "executeNativeRewardLeg", "(%s, %d : nat)" % (eid_arg(W1), i)) for i in (0, 1)]
    oW1 = obl(W1, "phase2 getNativeObligation W1 (quarantined)")
    chk("P2-W1-q", "W1 legs past the window -> Quarantined{recoverable = true}, reason names reconcile; no icrc1_transfer was made (log and supply unchanged)",
        all(r.ok and tag(r.val) == "Quarantined" and V(r.val)[1]["recoverable"] is True and "reconcile" in V(r.val)[1]["reason"].lower() for r in q) and ledger_len() == LW and supply() == SW and oW1["status"] == "PartiallyQuarantined",
        [r.val if r.ok else r.err for r in q])
    r1 = call("phase2 retry W1 leg 0", SHEN, "retryNativeRewardLeg", "(%s, 0 : nat)" % eid_arg(W1))
    chk("P2-W1-retry", "retry of a window-expired leg returns the same Quarantined status and mints nothing", r1.ok and r1.val == q[0].val and ledger_len() == LW)
    # W2: leg 1 past the window -> PartiallyQuarantined with leg 0 Committed
    r = call("phase2 execute W2 leg 1 (past the window)", SHEN, "executeNativeRewardLeg", "(%s, 1 : nat)" % eid_arg(W2))
    oW2 = obl(W2, "phase2 getNativeObligation W2")
    chk("P2-W2-part", "W2: leg 0 Committed, leg 1 Quarantined -> obligation PartiallyQuarantined", oW2["legs"][0]["status"] == "Committed" and oW2["legs"][1]["status"] == "Quarantined" and oW2["status"] == "PartiallyQuarantined", [l["status"] for l in oW2["legs"]])
    # W3: window expired AND the ledger holds the mints: execute quarantines, reconcile with the exact block recovers
    r = call("phase2 execute W3 leg 0 (past the window; ledger already holds the mint)", SHEN, "executeNativeRewardLeg", "(%s, 0 : nat)" % eid_arg(W3))
    chk("P2-W3-q", "W3 leg 0 past the window -> Quarantined (it is NOT re-presented, so no Duplicate probing past the window)", r.ok and tag(r.val) == "Quarantined", r.val if r.ok else r.err)
    rec = [call("phase2 reconcileLegFromLedger W3 leg %d block %d" % (i, w3b[i]), SHEN, "reconcileLegFromLedger", "(%s, %d : nat, %d : nat)" % (eid_arg(W3), i, w3b[i])) for i in (0, 1)]
    oW3 = obl(W3, "phase2 getNativeObligation W3 (after reconcile)")
    chk("P2-W3-rec", "reconcileLegFromLedger recovers both quarantined legs from the real ledger blocks %s: obligation Completed, no new mint" % w3b,
        all(r.ok and tag(r.val) == "Committed" for r in rec) and oW3["status"] == "Completed" and ledger_len() == LW and supply() == SW, [r.val if r.ok else r.err for r in rec])
    # a quarantined leg cannot be 'recovered' with someone else's block
    bad = call("phase2 reconcile W1 leg 0 with W3's block", SHEN, "reconcileLegFromLedger", "(%s, 0 : nat, %d : nat)" % (eid_arg(W1), w3b[0]))
    chk("P2-W1-badrec", "W1's quarantined leg cannot be committed with another leg's mint block", (not bad.ok) and "differs from the frozen leg" in bad.err and obl(W1)["legs"][0]["status"] == "Quarantined", bad.err[:200] if not bad.ok else bad.val)
    call("phase2 allowlist restored to [bridge]", SHEN, "setNativeBridgeAllowlist", '(vec { principal "%s" })' % BRIDGE)
    receipt["final_state_note"] = "Phase 2 reinstalls Shenanigans to get fresh signup markers, so after this run the bridge outbox still lists phase-1 events as Forwarded while Shenanigans holds only the phase-2 synthetic obligations. run.sh reinstalls all three canisters at its next start."


def write_receipt(receipt):
    receipt.update({
        "finished": now_iso(), "elapsed_seconds": round(time.time() - T_START),
        "result": {"checks": len(CHECKS), "passed": sum(1 for c in CHECKS if c["pass"]), "failed": [c["id"] for c in CHECKS if not c["pass"]]},
        "checks": CHECKS, "upgrades": UPGRADES, "mode_results": MODES, "balances_before_after": BALANCES,
        "ledger_blocks": BLOCKS, "findings": FINDINGS, "not_proven": NOT_PROVEN, "calls": STEPS,
    })
    with open(RECEIPT_PATH, "w") as f:
        json.dump(receipt, f, indent=1, sort_keys=False)
    log("receipt written: %s (%d checks, %d failed)" % (RECEIPT_PATH, len(CHECKS), len(FAILED)))


NOT_PROVEN.extend([
    "Real SOL RPC canister behaviour: the stub implements only getTransaction/getSignaturesForAddress with fixed reply shapes; real cycle charges, real provider consensus, HTTP-outcall size limits and the pinned wasm's Candid encoding are not exercised (the Motoko types were only checked with didc against the pinned .did). The stub refunds all attached cycles.",
    "reconcileLegFromLedger's ARCHIVE path: the local ledger archived nothing (archives() is empty), so selectArchive / an archive's get_transactions were not driven; only the ledger-direct get_transactions path was.",
    "A reconcile against a real `transfer` or `burn` block: the local ledger has no funded non-minting account to create one (kind checks are covered only by shenanigans' pure tests).",
    "Wrong-deployment guard (worker B plan section 4: shenanigans at a non-pinned id next to a ledger whose minter is j56tm...): the replica dfx.json is stub-only and the root dfx.json was not changed, so a second shenanigans instance was not deployed. The Retryable (BadFee / InsufficientFunds) path through Shenanigans and the minting-account-mismatch rejection were not driven.",
    "No automatic caller: executeReservedLeg is one explicit permissionless call per frozen leg (by design); no keeper or timer drives it, and the bridge's LegRecord can lag Shenanigans (for example after a direct admin execute) until the next executeReservedLeg. A Committed record is trusted over a later Shenanigans snapshot rollback.",
    "The live Quarantined history row uses E6, a SYNTHETIC copy of captured flows-009 with its 64 signature bytes replaced by the stub (not a captured transaction; the bridge does not verify signatures). A real permanent Shenanigans rejection path was exercised, but not for a genuinely distinct on-chain event.",
    "Phase 2 (concurrency / dedup window) still executes with the admin identity on synthetic Shenanigans-only observations: they have no bridge outbox entry, so executeReservedLeg (which requires Forwarded + Bound) cannot apply.",
    "Whether the two concurrent CLI calls in phase 2 actually interleaved their awaits inside the canister is not observable from outside; the asserted invariants hold for any interleaving, but interleaving itself is not proven.",
    "Lost reply AFTER a successful forward (the bridge trapping between Shenanigans' reply and applyForward) was simulated by observing the event directly first, not by crashing the bridge mid-call.",
    "Fixture transaction authenticity: the bridge does not verify ed25519 signatures (spec: two RPC providers are the trust boundary); the generator verified every fixture signature offline.",
    "Mainnet behaviour of any kind; the SOL RPC module/ledger/siws wasm are the local pins named in dfx.json, but nothing was run on a public network.",
])

def supplement():
    """Focused COOL-1 supplement: only the corrected scenario, against the replica state the v2 run left (nothing is reinstalled,
    nothing is executed on the ledger). Writes receipt-2026-10-14-v2-cool1-supplement.json."""
    path = os.path.join(HERE, "receipt-2026-10-14-v2-cool1-supplement.json")
    v2 = os.path.join(HERE, "receipt-2026-10-14-v2.json")
    out = {"receipt": "COOL-1 supplement to receipt-2026-10-14-v2.json (IMPL-49b)", "started": now_iso(), "network": "local dfx replica 127.0.0.1:4943; nothing public",
           "why": "The v2 run's COOL-1 asserted the OLD post-forward bind cooldown and failed against the accepted repair (ca00bbc + bind clears the gate). This supplement runs only the corrected scenario.",
           "v2_receipt": {"file": "receipt-2026-10-14-v2.json", "sha256": hashlib.sha256(open(v2, "rb").read()).hexdigest(), "left_unmodified": True},
           "stale_assertion_in_v2_receipt": COOL1_STALE_TEXT, "corrected_assertion": COOL1_TEXT,
           "other_cases": "The other 153 passing checks of the v2 run (154 total; 133 of them were in the first-phase checkpoint) are unaffected by this supplement and remain valid at their pins; nothing else was re-run.",
           "operator_principal": ME, "nonadmin_identity": {"name": NONADMIN, "principal": NONADMIN_PRINCIPAL}}
    out["canisters"] = {NAMES[c]: {"id": c, "module_hash": module_hash(c)} for c in (BRIDGE, SHEN, STUB)}
    mode = call("stub getMode", STUB, "getMode").val
    if mode != {"Agree": None}:
        call("stub setMode Agree (setup)", STUB, "setMode", "(variant { Agree })")
    out["stub_mode_before"] = mode
    prev = os.path.join(HERE, "receipt-2026-10-14-v2-cool1-supplement-attempt1.json")
    if os.path.exists(prev):
        out["previous_attempt"] = {"file": "receipt-2026-10-14-v2-cool1-supplement-attempt1.json", "sha256": hashlib.sha256(open(prev, "rb").read()).hexdigest(),
                                   "outcome": "2 checks, 2 failed: a bug in the supplement's own post-condition (it compared Shenanigans' one-element answer list to a list of bare variants). The raw replies in that file already showed the correct behaviour (Unbound/Unbound, Bound/Reserved, next_attempt_at 0, 3.4 s after the forward). Kept for honesty; this file is the corrected rerun with fresh synthetic events (tags 115/116, so the forwards and binds are genuinely new)."}
    scen = []
    for label, fixture, tagbyte, want in (("unbound owner (clone of flows-007, burner owner)", "flows-007", 115, [[{"Unbound": None}]]),
                                          ("bound owner (clone of closing-012, Bow1CG...)", "closing-012", 116, [[{"Reserved": None}], [{"AlreadyReserved": None}]])):
        base = fixture_event(fixture)
        cl = call("stub cloneFixture(%s -> synthetic copy with another signature)" % fixture, STUB, "cloneFixture", '("%s", %d : nat8)' % (base["sig"], tagbyte))
        newsig = opt(cl.val) if cl.ok else None
        ev = dict(base)
        ev["sig"], ev["fixture"] = newsig, "synthetic clone of %s (NOT a captured transaction)" % fixture
        sub = call("submit %s" % label, BRIDGE, "submitEventDescriptor", "(%s)" % desc_arg(ev))
        fw = call("verifyAndForward %s (non-admin)" % label, BRIDGE, "verifyAndForward", "(%s)" % eid_arg(ev), identity=NONADMIN)
        t_fw = time.time()
        fs = bridge_summary(fw)
        entry = opt(call("bridge getEvent %s" % label, BRIDGE, "getEvent", "(%s)" % eid_arg(ev)).val)
        gate = n(entry["next_attempt_at"]) if entry else None
        rc = call("releasePending %s IMMEDIATELY after the forward (non-admin)" % label, BRIDGE, "releasePending", "(%s)" % eid_arg(ev), identity=NONADMIN)
        gap = round(time.time() - t_fw, 2)
        okc = cool1_ok(rc) and rc.val and V(rc.val)[1]["shenanigans"] in want
        chk("COOL-1/" + fixture, COOL1_TEXT + " [%s; %.1f s after the forward]" % (label, gap), bool(newsig) and tag(V(sub.val)[1]) == "Submitted" and fs[0] == "OK" and fs[1] == "Forwarded" and gate == 0 and okc, rc.val if rc.ok else rc.err)
        scen.append({"scenario": label, "synthetic_signature": newsig, "forward_status": fs[1] if fs[0] == "OK" else fs, "entry_next_attempt_at_after_forward": gate,
                     "seconds_between_forward_reply_and_bind_call": gap, "releasePending_reply": rc.val if rc.ok else rc.err, "expected_shenanigans_answers": want})
    out["scenarios"] = scen
    out["left_behind"] = "Two synthetic bridge entries (Forwarded). The bound-owner one is Reserved in Shenanigans with two Pending legs (no gift: the shared reservation is held); no leg was executed and nothing was minted."
    out.update({"finished": now_iso(), "result": {"checks": len(CHECKS), "passed": sum(1 for c in CHECKS if c["pass"]), "failed": [c["id"] for c in CHECKS if not c["pass"]]},
                "checks": CHECKS, "calls": STEPS})
    with open(path, "w") as f:
        json.dump(out, f, indent=1)
    log("supplement written: %s (%d checks, %d failed)" % (path, len(CHECKS), len(FAILED)))
    return 1 if FAILED else 0


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--cool1-supplement":
        sys.exit(supplement())
    try:
        rc = main()
    finally:
        # best effort: leave the shared ledger and shenanigans running
        for cid in (LEDGER, SHEN):
            subprocess.run(["dfx", "canister", "start", cid, "--network", "local"], cwd=ROOT, capture_output=True, text=True)
    sys.exit(1 if FAILED else 0)
