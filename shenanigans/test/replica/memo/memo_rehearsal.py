#!/usr/bin/env python3
"""PP-ledger memo fix: local-replica rehearsal (standard library only).

LOOPBACK ONLY. Every dfx call carries `--network local` (127.0.0.1:4943). Called by memo-rehearsal.sh, which
refuses any other network and installs the restore-on-exit trap.

What it does (the receipt records every call/reply, the ledger blocks, balances before/after):

  PHASE 1  PRE-FIX build (main@ee9a402 source, built before the edit): reinstalled at the pinned id
           j56tm-... next to the REAL pp_ledger (5xv2o-..., icrc1:max_memo_length = 32). A stub game source
           gives the real observer one qualifying game (-> signup gift, 70-byte memo) and two backers
           (-> backer-delta mints, 79 / 80-byte memos). Observed: the ledger traps (IC0503), the gift is
           lost (Debug.print only), the two backers land in missedBackerMints after 10 ticks, and the
           short-memo mints (game-N, cascade-residual-game-N, admin-mint) commit.
  PHASE 2  `dfx canister install --mode upgrade --wasm-memory-persistence keep` to the FIXED build. The stuck
           state (missed entries, cursors) must survive and the Candid interface must be unchanged.
  PHASE 3  FIXED build: a NEW signup gift and a NEW backer mint commit on the FIRST try with 32-byte
           memos equal to sha256(event id); their cascade legs (also long ids) commit; short memos are
           byte-identical to phase 1; the earlier failed entries stay recorded exactly as they were
           (this change does NOT re-credit anyone); a second observer tick mints nothing.
  PHASE 4  OBSERVATION ONLY (no new code): what the UNCHANGED observer then does with the earlier failures
           (P1 plays again -> its lost gift commits under the original event id; B1 tops up -> only the new
           delta mints; B1/B2 stay in missedBackerMints). Evidence for recovery-evidence.md.

Not proven here (also written into the receipt): behaviour on mainnet, the other short-memo call sites
(chip deposit, cash-out, spell casts, karma) beyond the static + unit tests, an upline-ranked cascade leg
(`cascade-A<rank>-...`; the rehearsal's upline is the inactive house, so the legs are cascade-residual-...).
"""
import base64, binascii, datetime, hashlib, json, os, subprocess, sys, time

E = os.environ
HERE = E.get("MC_MEMO_HERE") or os.path.dirname(os.path.abspath(__file__))
ROOT = E.get("MC_MEMO_ROOT") or os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
ART = os.path.join(HERE, "artifacts")
STUBDIR = os.path.join(HERE, "stub")
RECEIPT = E.get("MC_MEMO_RECEIPT") or os.path.join(HERE, "receipt-2026-10-23.json")
PREFIX_WASM = E.get("MC_MEMO_PREFIX_WASM") or os.path.join(ART, "shenanigans-prefix-main-ee9a402.wasm")
FIXED_WASM = os.path.join(ART, "shenanigans-fixed.wasm")
SKIP_BUILD = bool(E.get("MC_MEMO_SKIP_BUILD"))
SALT = E.get("MC_MEMO_SALT") or str(int(time.time()))

LEDGER = "5xv2o-iiaaa-aaaac-qeclq-cai"
SHEN = "j56tm-oaaaa-aaaac-qf34q-cai"
GIFT_NET = 45_000_000_000        # 500 PP gift less the 10% cascade, in 1e-8 PP units
PP_PER_ICP_SIMPLE = 1000         # mintConfig.simple21DayPpPerIcp
PP_PER_ICP_BACKER = 4000         # mintConfig.backerPpPerIcp
CASCADE_BPS = 1000

STEPS, CHECKS, OBS, FAILED = [], [], [], []
T0 = time.time()


def now_iso():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def log(msg):
    print("[%6.1fs] %s" % (time.time() - T0, msg), flush=True)


def shorten(s, k=900):
    s = str(s)
    return s if len(s) <= k else s[:k] + "...(%d chars)" % len(s)


def nat(x):
    return x if isinstance(x, int) else int(str(x).replace("_", ""))


def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def sha256_file(p):
    return sha256_hex(open(p, "rb").read())


# ------------------------------------------------------------------ principals / candid helpers
def principal_text(b):
    raw = binascii.crc32(b).to_bytes(4, "big") + b
    s = base64.b32encode(raw).decode().lower().rstrip("=")
    return "-".join(s[i:i + 5] for i in range(0, len(s), 5))


def principal_bytes(text):
    s = text.replace("-", "").upper()
    s += "=" * (-len(s) % 8)
    raw = base64.b32decode(s)
    assert binascii.crc32(raw[4:]).to_bytes(4, "big") == raw[:4], "bad principal checksum"
    return raw[4:]


def fresh_principal(label):
    """29-byte self-authenticating-shaped principal => 63-char text, like a real user principal."""
    return principal_text(hashlib.sha256(("memo-rehearsal|%s|%s" % (SALT, label)).encode()).digest()[:28] + b"\x02")


def chip_sub(text):
    b = principal_bytes(text)
    return b + bytes(32 - len(b))


def blob_lit(b):
    return 'blob "' + "".join("\\%02x" % x for x in b) + '"'


def v(x):
    assert isinstance(x, dict) and len(x) == 1, x
    k = next(iter(x))
    return k, x[k]


def icrc3_value(x):
    k, y = v(x)
    if k in ("Nat", "Nat64", "Int", "Int64"):
        return nat(y)
    if k == "Text":
        return y
    if k == "Blob":
        return bytes(y) if isinstance(y, list) else bytes.fromhex(y)
    if k == "Array":
        return [icrc3_value(i) for i in y]
    if k == "Map":
        return {m["0"]: icrc3_value(m["1"]) for m in y}
    raise ValueError(k)


# ------------------------------------------------------------------ dfx
class Reply:
    def __init__(self, ok, val, err):
        self.ok, self.val, self.err = ok, val, err


def run(cmd, cwd=None, timeout=1500):
    return subprocess.run(cmd, cwd=cwd or ROOT, capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)


def step(entry):
    entry["n"] = len(STEPS) + 1
    entry["t"] = now_iso()
    STEPS.append(entry)


def call(label, cid, method, arg="()", query=False, record=True):
    cmd = ["dfx", "canister", "call", cid, method, arg, "--network", "local", "--output", "json"] + (["--query"] if query else [])
    t0 = time.time()
    r = run(cmd)
    ok = r.returncode == 0
    out = r.stdout.strip()
    try:
        val = json.loads(out) if (ok and out) else None
    except ValueError:
        val = out
    if record:
        step({"step": label, "canister": cid, "method": method, "arg": shorten(arg, 600), "ok": ok,
              "reply": val if ok else None, "error": shorten((r.stderr or r.stdout).strip(), 1500) if not ok else None,
              "seconds": round(time.time() - t0, 2)})
    return Reply(ok, val, (r.stderr or r.stdout).strip() if not ok else "")


def dfx_admin(label, args, cwd=None, record=True):
    t0 = time.time()
    r = run(["dfx"] + args + ["--network", "local"], cwd=cwd)
    out = (r.stdout + "\n" + r.stderr).strip()
    if record:
        step({"step": label, "dfx": " ".join(args), "cwd": cwd or ROOT, "ok": r.returncode == 0,
              "output": shorten(out, 1500), "seconds": round(time.time() - t0, 2)})
    return r.returncode == 0, out


def status_of(cid):
    ok, out = dfx_admin("status " + cid, ["canister", "status", cid], record=False)
    st = mh = None
    for line in out.splitlines():
        if line.startswith("Status:"):
            st = line.split(":", 1)[1].strip()
        if line.startswith("Module hash:"):
            mh = line.split(":", 1)[1].strip()
    return st, mh


def logs_of(cid):
    ok, out = dfx_admin("logs", ["canister", "logs", cid], record=False)
    return out.splitlines() if ok else []


def check(cid, desc, cond, detail=None):
    cond = bool(cond)
    CHECKS.append({"id": cid, "check": desc, "pass": cond, "detail": detail})
    if not cond:
        FAILED.append(cid)
    log("%s %s: %s%s" % ("PASS" if cond else "FAIL", cid, desc, "" if cond or detail is None else "  [" + shorten(detail, 500) + "]"))
    return cond


def observe(oid, text, data=None):
    OBS.append({"id": oid, "observed": text, "data": data})
    log("OBSERVED %s: %s" % (oid, shorten(text, 600)))


# ------------------------------------------------------------------ ledger reads
def ledger_chain_length():
    r = call("ledger tip", LEDGER, "get_blocks", "(record { start = 0 : nat; length = 0 : nat })", query=True, record=False)
    return nat(r.val["chain_length"])


def ledger_blocks(start):
    out, cur = [], start
    end = ledger_chain_length()
    while cur < end:
        r = call("ledger blocks", LEDGER, "get_blocks", "(record { start = %d : nat; length = 100 : nat })" % cur, query=True, record=False)
        assert r.ok, r.err
        first = nat(r.val["first_index"])
        assert first == cur, (first, cur)
        for i, b in enumerate(r.val["blocks"]):
            out.append(decode_block(first + i, icrc3_value(b)))
        cur += len(r.val["blocks"])
        if not r.val["blocks"]:
            break
    return out


def acct(a):
    if not isinstance(a, list) or not a:
        return None
    owner = principal_text(a[0])
    sub = a[1].hex() if len(a) > 1 else None
    return {"owner": owner, "subaccount": sub}


def decode_block(idx, blk):
    tx = blk.get("tx", {})
    memo = tx.get("memo")
    txt = None
    if memo is not None:
        try:
            t = memo.decode("utf-8")
            txt = t if t.isprintable() else None
        except UnicodeDecodeError:
            txt = None
    return {"block": idx, "op": tx.get("op", blk.get("btype")), "amount": tx.get("amt"), "to": acct(tx.get("to")),
            "from": acct(tx.get("from")), "memo_len": None if memo is None else len(memo),
            "memo_hex": None if memo is None else memo.hex(), "memo_text": txt}


def chip_balance(text):
    arg = '(record { owner = principal "%s"; subaccount = opt %s })' % (SHEN, blob_lit(chip_sub(text)))
    r = call("chip balance", LEDGER, "icrc1_balance_of", arg, query=True, record=False)
    assert r.ok, r.err
    return nat(r.val)


def house_balance(house):
    return chip_balance(house)


def blocks_with_memo_prefix(blocks, prefix):
    return [b for b in blocks if b["memo_text"] is not None and b["memo_text"].startswith(prefix)]


def block_by_memo_hex(blocks, hexmemo):
    return [b for b in blocks if b["memo_hex"] == hexmemo]


# ------------------------------------------------------------------ stub game source
def game_rec(gid, player, amount):
    return ('record { id = %d : nat; player = principal "%s"; plan = variant { simple21Day }; amount = %r : float64; '
            'startTime = 0 : int; isCompounding = false; isActive = true; lastUpdateTime = 0 : int; '
            'accumulatedEarnings = 0.0 : float64; totalWithdrawn = 0.0 : float64 }' % (gid, player, amount))


def backer_rec(owner, amount):
    return ('record { owner = principal "%s"; amount = %r : float64; entitlement = 0.0 : float64; startTime = 0 : int; '
            'isActive = true; backerType = variant { seriesA }; firstDepositDate = null }' % (owner, amount))


def set_games(stub, recs):
    r = call("stub.setGames", stub, "setGames", "(vec { %s })" % "; ".join(recs))
    assert r.ok, r.err


def set_backers(stub, recs):
    r = call("stub.setBackers", stub, "setBackers", "(vec { %s })" % "; ".join(recs))
    assert r.ok, r.err


def observer_tick(label):
    r = call(label, SHEN, "runObserverOnce")
    assert r.ok, r.err
    return r


def missed_backers():
    r = call("getMissedBackerMints", SHEN, "getMissedBackerMints", query=True)
    assert r.ok, r.err
    return sorted([(e["0"], e["1"]) for e in r.val])


def observer_status():
    r = call("getObserverStatus", SHEN, "getObserverStatus", query=True)
    assert r.ok, r.err
    return r.val


def ftext(x):
    """Float.toText as the Shenanigans wasm renders it inside backer event ids: fixed 6 decimals (observed in
    this rehearsal: 5.0 -> "5.000000"; it is also why the live ids are 79 / 80 bytes = 8 / 9-char amounts)."""
    return "%.6f" % x


def mint_units(amount_icp, per_icp):
    base = int(amount_icp * 100_000_000.0) * per_icp  # mirrors icpFloatToPpUnits (truncating Float.toInt)
    cascade = base * CASCADE_BPS // 10_000
    return base, cascade, base - cascade


# ------------------------------------------------------------------ main
def main():
    log("memo rehearsal start (salt=%s)" % SALT)
    for p in (PREFIX_WASM,):
        if not os.path.exists(p):
            sys.exit("missing pre-fix wasm: %s (build it from main@ee9a402 or set MC_MEMO_PREFIX_WASM)" % p)

    # ---- preflight
    if run(["dfx", "ping", "local"]).returncode != 0:
        sys.exit("no replica on 127.0.0.1:4943")
    admin = run(["dfx", "identity", "get-principal"]).stdout.strip()
    meta = call("pp_ledger.icrc1_metadata", LEDGER, "icrc1_metadata", query=True)
    memo_lim = None
    for e in meta.val:
        if e["0"] == "icrc1:max_memo_length":
            memo_lim = nat(v(e["1"])[1])
    minting = call("pp_ledger.icrc1_minting_account", LEDGER, "icrc1_minting_account", query=True)
    check("PRE-1", "real pp_ledger advertises icrc1:max_memo_length = 32", memo_lim == 32, memo_lim)
    check("PRE-2", "pp_ledger minting account is the Shenanigans canister id (every shenanigans transfer from null subaccount is a mint)",
          minting.val and minting.val[0]["owner"] == SHEN and not minting.val[0].get("subaccount"), minting.val)
    ledger_status, ledger_hash = status_of(LEDGER)
    observe("OBS-env", "replica state at start", {"pp_ledger_status": ledger_status, "pp_ledger_module_hash": ledger_hash,
                                                    "ledger_chain_length": ledger_chain_length(), "admin_principal": admin,
                                                    "total_supply": call("pp_ledger.icrc1_total_supply", LEDGER, "icrc1_total_supply", query=True).val})

    # ---- build the fixed wasm from the worktree (the pre-fix wasm was built BEFORE the edit and is kept in artifacts/)
    if not SKIP_BUILD:
        ids_path = os.path.join(ROOT, ".dfx", "local", "canister_ids.json")
        os.makedirs(os.path.dirname(ids_path), exist_ok=True)
        ids = json.load(open(ids_path)) if os.path.exists(ids_path) else {}
        ids.setdefault("shenanigans", {})["local"] = SHEN
        ids.setdefault("pp_ledger", {})["local"] = LEDGER
        json.dump(ids, open(ids_path, "w"), indent=2)
        ok, out = dfx_admin("dfx build shenanigans (fixed)", ["build", "shenanigans"])
        assert ok, out
        built = os.path.join(ROOT, ".dfx", "local", "canisters", "shenanigans", "shenanigans.wasm")
        os.makedirs(ART, exist_ok=True)
        with open(built, "rb") as f, open(FIXED_WASM, "wb") as g:
            g.write(f.read())
    pre_sha, fix_sha = sha256_file(PREFIX_WASM), sha256_file(FIXED_WASM)
    observe("OBS-wasm", "wasm artifacts", {"prefix_wasm": os.path.relpath(PREFIX_WASM, ROOT), "prefix_sha256": pre_sha,
                                           "fixed_wasm": os.path.relpath(FIXED_WASM, ROOT), "fixed_sha256": fix_sha})
    check("PRE-3", "pre-fix and fixed wasm differ", pre_sha != fix_sha)

    # ---- stub
    ok, out = dfx_admin("deploy memo_games_stub", ["deploy", "memo_games_stub"], cwd=STUBDIR)
    assert ok, out
    stub = run(["dfx", "canister", "id", "memo_games_stub", "--network", "local"], cwd=STUBDIR).stdout.strip()
    observe("OBS-stub", "ponzi_math stand-in canister", {"id": stub})

    # fresh principals for this run (per-run salt keeps ledger balances from earlier runs out of the picture)
    P1, B1, B2 = fresh_principal("player-pre-gift"), fresh_principal("backer-pre-8"), fresh_principal("backer-pre-9")
    P2, B3, PS = fresh_principal("player-post-gift"), fresh_principal("backer-post"), fresh_principal("short-memo")
    A1, A2, A3 = 2.345678, 12.345678, 3.456789
    check("PRE-4", "test principals are 63 chars", all(len(p) == 63 for p in (P1, B1, B2, P2, B3, PS)), [len(p) for p in (P1, B1, B2, P2, B3, PS)])

    # ================================================================= PHASE 1: pre-fix
    log("PHASE 1: reinstall PRE-FIX build at %s" % SHEN)
    ok, out = dfx_admin("install pre-fix (reinstall)", ["canister", "install", SHEN, "--mode", "reinstall", "--yes", "--wasm", PREFIX_WASM])
    assert ok, out
    st, mh = status_of(SHEN)
    check("P1-0", "installed module hash == sha256(pre-fix wasm)", mh and mh.lower().replace("0x", "") == pre_sha, (mh, pre_sha))
    for label, cid, m, a in (("initialize", SHEN, "initialize", '(principal "%s")' % stub), ("seedMigrationV2", SHEN, "seedMigrationV2", "()"),
                             ("stopObserver", SHEN, "stopObserver", "()")):
        r = call("PRE-FIX " + label, cid, m, a)
        assert r.ok, r.err
    iface_pre = run(["dfx", "canister", "metadata", SHEN, "candid:service", "--network", "local"]).stdout
    head0 = ledger_chain_length()

    r = call("PRE-FIX adminMint(short memo site)", SHEN, "adminMint", '(principal "%s", 1 : nat)' % PS)
    assert r.ok and "Ok" in r.val, r
    set_games(stub, [game_rec(0, P1, 1.0)])
    set_backers(stub, [backer_rec(B1, A1), backer_rec(B2, A2)])
    id_gift1 = "signup-" + P1
    id_b1 = "backer-%s-%s" % (B1, ftext(A1))
    id_b2 = "backer-%s-%s" % (B2, ftext(A2))
    check("P1-1", "pre-fix event-id lengths reproduce the live shapes (gift 70 B; backers 79 / 80 B)",
          (len(id_gift1), len(id_b1), len(id_b2)) == (70, 79, 80), (len(id_gift1), len(id_b1), len(id_b2)))

    observer_tick("PRE-FIX observer tick 1")
    logs1 = [l for l in logs_of(SHEN) if P1 in l or B1 in l or B2 in l]
    observe("OBS-pre-logs-tick1", "canister log lines after tick 1 (Debug.print is the only record of a failed signup gift)", logs1)
    check("P1-2", "pre-fix: signup-gift mint failed with the ledger's memo trap (IC0503 / 32-byte limit)",
          any("Signup-gift mint failed for " + id_gift1 in l and "IC0503" in l and "70 bytes" in l and "32 bytes" in l for l in logs1), logs1[:3])
    for i in range(2, 11):
        observer_tick("PRE-FIX observer tick %d" % i)
    miss_pre = missed_backers()
    st_pre = observer_status()
    check("P1-3", "pre-fix: after %d ticks both backers are in missedBackerMints with the IC0503 memo-size error" % 10,
          len(miss_pre) == 2 and {m[0] for m in miss_pre} == {B1, B2}
          and all("IC0503" in m[1] and "32 bytes" in m[1] for m in miss_pre)
          and any("79 bytes" in m[1] for m in miss_pre) and any("80 bytes" in m[1] for m in miss_pre), miss_pre)
    observe("OBS-pre-missed", "missedBackerMints (owner, error text)", miss_pre)
    blocks_pre = ledger_blocks(head0)
    bal_pre = {"P1_gift_player": chip_balance(P1), "B1": chip_balance(B1), "B2": chip_balance(B2), "PS_short": chip_balance(PS)}
    base1, casc1, net1 = mint_units(1.0, PP_PER_ICP_SIMPLE)
    check("P1-4", "pre-fix: P1 holds only its game mint (no 500 PP gift), B1/B2 hold nothing",
          bal_pre["P1_gift_player"] == net1 and bal_pre["B1"] == 0 and bal_pre["B2"] == 0, bal_pre)
    check("P1-5", "pre-fix: no ledger block carries a signup-/backer-/cascade-*signup/backer memo (nothing committed for the long ids)",
          not [b for b in blocks_pre if b["memo_text"] and (b["memo_text"].startswith(("signup-", "backer-")) or "signup-" in b["memo_text"] or "backer-" in b["memo_text"])]
          and not [b for b in blocks_pre if b["memo_len"] and b["memo_len"] > 32], [b["memo_text"] for b in blocks_pre])
    pre_short = {b["memo_text"]: b for b in blocks_pre if b["memo_text"]}
    check("P1-6", "pre-fix: short memos commit as raw UTF-8 (admin-mint, game-0, cascade-residual-game-0)",
          all(k in pre_short and pre_short[k]["memo_hex"] == k.encode().hex() for k in ("admin-mint", "game-0", "cascade-residual-game-0")), list(pre_short))
    gift_status_pre = {"gameIdCursor": nat(st_pre["gameIdCursor"]), "backerSeenCount": nat(st_pre["backerSeenCount"]),
                       "missedBackerMintsCount": nat(st_pre["missedBackerMintsCount"])}

    # ================================================================= PHASE 2: upgrade
    log("PHASE 2: upgrade to FIXED build with --wasm-memory-persistence keep")
    ok, out = dfx_admin("install fixed (upgrade, keep)", ["canister", "install", SHEN, "--mode", "upgrade", "--wasm-memory-persistence", "keep", "--yes", "--wasm", FIXED_WASM])
    assert ok, out
    st, mh = status_of(SHEN)
    check("P2-1", "installed module hash == sha256(fixed wasm) and the canister is running", mh and mh.lower().replace("0x", "") == fix_sha and st == "Running", (st, mh))
    miss_mid = missed_backers()
    st_mid = observer_status()
    check("P2-2", "stuck state survived the upgrade: same missedBackerMints entries, cursors and seen-count",
          miss_mid == miss_pre and nat(st_mid["gameIdCursor"]) == gift_status_pre["gameIdCursor"]
          and nat(st_mid["backerSeenCount"]) == gift_status_pre["backerSeenCount"], miss_mid)
    iface_post = run(["dfx", "canister", "metadata", SHEN, "candid:service", "--network", "local"]).stdout
    check("P2-3", "Candid service interface is unchanged by the fix", iface_pre == iface_post and len(iface_pre) > 1000, (len(iface_pre), len(iface_post)))
    r = call("FIXED stopObserver (no timer survives an upgrade; belt and braces)", SHEN, "stopObserver")
    assert r.ok, r.err
    bal_mid = {"P1_gift_player": chip_balance(P1), "B1": chip_balance(B1), "B2": chip_balance(B2), "PS_short": chip_balance(PS)}
    check("P2-4", "balances unchanged across the upgrade", bal_mid == bal_pre, bal_mid)

    # ================================================================= PHASE 3: fixed
    log("PHASE 3: FIXED build")
    head1 = ledger_chain_length()
    r = call("FIXED adminMint(short memo site)", SHEN, "adminMint", '(principal "%s", 1 : nat)' % PS)
    assert r.ok and "Ok" in r.val, r
    set_games(stub, [game_rec(0, P1, 1.0), game_rec(1, P2, 1.0)])
    set_backers(stub, [backer_rec(B1, A1), backer_rec(B2, A2), backer_rec(B3, A3)])
    observer_tick("FIXED observer tick 1 (new gift for P2, new backer B3; P1/B1/B2 have no new event)")
    blocks_post = ledger_blocks(head1)
    id_gift2 = "signup-" + P2
    memo_gift2 = sha256_hex(id_gift2.encode())
    gift_blk = block_by_memo_hex(blocks_post, memo_gift2)
    check("P3-1", "NEW signup gift committed on the first try: 1 mint, 45_000_000_000 units to chip(P2), 32-byte memo == sha256('signup-'+P2)",
          len(gift_blk) == 1 and gift_blk[0]["op"] == "mint" and gift_blk[0]["amount"] == GIFT_NET and gift_blk[0]["memo_len"] == 32
          and gift_blk[0]["to"]["owner"] == SHEN and gift_blk[0]["to"]["subaccount"] == chip_sub(P2).hex(), gift_blk)
    base3, casc3, net3 = mint_units(A3, PP_PER_ICP_BACKER)
    cand_texts = [ftext(A3), repr(A3)]
    bk_blk, id_b3 = [], None
    for t in cand_texts:
        cand = "backer-%s-%s" % (B3, t)
        hit = block_by_memo_hex(blocks_post, sha256_hex(cand.encode()))
        if hit:
            bk_blk, id_b3 = hit, cand
    check("P3-2", "NEW backer mint committed on the first try: 1 mint of the net delta to chip(B3), 32-byte memo == sha256(backer event id)",
          len(bk_blk) == 1 and id_b3 is not None and len(id_b3.encode()) == 79 and bk_blk[0]["op"] == "mint" and bk_blk[0]["amount"] == net3
          and bk_blk[0]["memo_len"] == 32 and bk_blk[0]["to"]["subaccount"] == chip_sub(B3).hex(), (id_b3, bk_blk, net3))
    legs = {"cascade-residual-signup-" + P2: 5_000_000_000,   # 10% of the 500 PP gift
            "cascade-residual-game-1": casc1,
            "cascade-residual-" + id_b3: casc3}
    leg_ok, leg_detail = True, {}
    for text, amt in legs.items():
        hit = block_by_memo_hex(blocks_post, sha256_hex(text.encode()) if len(text.encode()) > 32 else text.encode().hex())
        good = len(hit) == 1 and hit[0]["op"] == "mint" and hit[0]["amount"] == amt and hit[0]["memo_len"] == (32 if len(text.encode()) > 32 else len(text))
        leg_ok &= good
        leg_detail[text[:48]] = {"event_id_bytes": len(text.encode()), "block": hit[0]["block"] if hit else None, "memo_len": hit[0]["memo_len"] if hit else None, "amount": hit[0]["amount"] if hit else None}
    check("P3-3", "cascade legs commit too: cascade-residual-signup-P2 and cascade-residual-backer-... (both > 32 B -> hashed 32 B) and cascade-residual-game-1 (short, raw)",
          leg_ok, leg_detail)
    game1 = block_by_memo_hex(blocks_post, b"game-1".hex())
    check("P3-4", "game mint memo is unchanged raw UTF-8 'game-1'", len(game1) == 1 and game1[0]["memo_text"] == "game-1" and game1[0]["amount"] == net1, game1)
    am_post = [b for b in blocks_post if b["memo_text"] == "admin-mint"]
    am_pre = [b for b in blocks_pre if b["memo_text"] == "admin-mint"]
    check("P3-5", "admin-mint memo bytes identical before and after the fix", len(am_post) == 1 and len(am_pre) == 1 and am_post[0]["memo_hex"] == am_pre[0]["memo_hex"] == b"admin-mint".hex(),
          (am_pre, am_post))
    check("P3-6", "no block committed by the fixed build has a memo above 32 bytes", all((b["memo_len"] or 0) <= 32 for b in blocks_post), [b["memo_len"] for b in blocks_post])
    miss_post = missed_backers()
    check("P3-7", "the earlier failed entries remain recorded exactly as they were (this change does not re-credit them)", miss_post == miss_pre, miss_post)
    bal_post = {"P1_gift_player": chip_balance(P1), "B1": chip_balance(B1), "B2": chip_balance(B2), "P2_gift_player": chip_balance(P2),
                "B3": chip_balance(B3), "PS_short": chip_balance(PS)}
    check("P3-8", "balances: P2 = gift + game net; B3 = backer net; P1/B1/B2 unchanged (still no gift / backer PP)",
          bal_post["P2_gift_player"] == GIFT_NET + net1 and bal_post["B3"] == net3 and bal_post["P1_gift_player"] == bal_pre["P1_gift_player"]
          and bal_post["B1"] == 0 and bal_post["B2"] == 0 and bal_post["PS_short"] == 2 * 100_000_000, bal_post)
    head2 = ledger_chain_length()
    observer_tick("FIXED observer tick 2 (nothing new -> nothing minted)")
    head3 = ledger_chain_length()
    check("P3-9", "a second observer tick adds no ledger block (mintedEventIds / cursors / backerSeen hold)", head2 == head3, (head2, head3))
    logs_post = [l for l in logs_of(SHEN) if P2 in l or B3 in l]
    check("P3-10", "no 'failed' log line for the new gift or the new backer", not [l for l in logs_post if "failed" in l.lower()], logs_post[:3])
    st_post = observer_status()
    observe("OBS-post-status", "observer status after phase 3", st_post)

    # ================================================================= PHASE 4: observation only (NO new code)
    # What the UNCHANGED observer does with the earlier failures once the fix is live. This is the evidence for
    # shenanigans/test/replica/memo/recovery-evidence.md; it adds no recovery path and is not part of the fix.
    log("PHASE 4 (observation only): P1's next game and B1's top-up, after the fix")
    head4 = ledger_chain_length()
    A1_TOP = 5.0
    set_games(stub, [game_rec(0, P1, 1.0), game_rec(1, P2, 1.0), game_rec(2, P1, 1.0)])
    set_backers(stub, [backer_rec(B1, A1_TOP), backer_rec(B2, A2), backer_rec(B3, A3)])
    observer_tick("FIXED observer tick 3 (P1 plays again; B1 tops up 2.345678 -> 5.0)")
    blocks_p4 = ledger_blocks(head4)
    gift_p1 = block_by_memo_hex(blocks_p4, sha256_hex(id_gift1.encode()))
    check("P4-1", "OBSERVED: the unchanged observer commits P1's lost gift at P1's NEXT qualifying game, under the ORIGINAL event id 'signup-'+P1 (32-byte memo)",
          len(gift_p1) == 1 and gift_p1[0]["amount"] == GIFT_NET and gift_p1[0]["to"]["subaccount"] == chip_sub(P1).hex(), gift_p1)
    delta1 = A1_TOP - A1
    _, _, net_d1 = mint_units(delta1, PP_PER_ICP_BACKER)
    id_b1_top, b1_blk = None, []
    for t in (ftext(A1_TOP), repr(A1_TOP), "5"):
        cand = "backer-%s-%s" % (B1, t)
        hit = block_by_memo_hex(blocks_p4, sha256_hex(cand.encode()))
        if hit:
            id_b1_top, b1_blk = cand, hit
    check("P4-2", "OBSERVED: B1's top-up mints ONLY the new delta (5.0 - 2.345678) under a NEW event id; the lost 2.345678 is not re-presented",
          len(b1_blk) == 1 and b1_blk[0]["amount"] == net_d1 and chip_balance(B1) == net_d1, (id_b1_top, b1_blk, net_d1))
    miss_p4 = missed_backers()
    check("P4-3", "OBSERVED: missedBackerMints still lists B1 and B2 unchanged (entries are never cleared or retried by the observer)", miss_p4 == miss_pre, miss_p4)
    bal_p4 = {"P1_gift_player": chip_balance(P1), "B1": chip_balance(B1), "B2": chip_balance(B2)}
    check("P4-4", "OBSERVED: B2 (no top-up) is still at zero; P1 now holds game-0 + gift + game-2", bal_p4["B2"] == 0 and bal_p4["P1_gift_player"] == 2 * net1 + GIFT_NET, bal_p4)
    observe("OBS-phase4", "unchanged-observer behaviour after the fix", {"p1_gift_event_id": id_gift1, "b1_topup_event_id": id_b1_top, "balances": bal_p4})

    # ================================================================= receipt
    receipt = {
        "title": "PP-ledger 32-byte memo fix - local replica rehearsal",
        "generated": now_iso(), "network": "local (127.0.0.1:4943), dfx 0.29.2 pocket-ic", "salt": SALT,
        "canisters": {"pp_ledger": LEDGER, "shenanigans": SHEN, "memo_games_stub": stub, "admin_principal": admin},
        "wasm": {"prefix_sha256": pre_sha, "fixed_sha256": fix_sha},
        "event_ids": {"gift_pre_fail": id_gift1, "backer_pre_fail_79B": id_b1, "backer_pre_fail_80B": id_b2, "gift_post_ok": id_gift2, "backer_post_ok": id_b3, "phase4_p1_gift_commits_under": id_gift1, "phase4_b1_topup": id_b1_top},
        "principals": {"P1_pre_gift_player": P1, "B1_pre_backer_8char": B1, "B2_pre_backer_9char": B2, "P2_post_gift_player": P2, "B3_post_backer": B3, "PS_short": PS},
        "balances_chip_units": {"after_pre_fix": bal_pre, "after_upgrade": bal_mid, "after_fixed": bal_post, "after_phase4_observation": bal_p4},
        "ledger_blocks": {"pre_fix_phase": blocks_pre, "fixed_phase": blocks_post, "phase4_observation": blocks_p4},
        "observations": OBS, "checks": CHECKS, "failed": sorted(set(FAILED)), "steps": STEPS,
        "not_proven": [
            "Mainnet behaviour (no mainnet call was made; the local pp_ledger is the same release but not the mainnet instance).",
            "Short-memo call sites other than admin-mint / game-N / cascade-residual-game-N were not exercised live (chip-deposit, cash-out-N, spell-N, backfire-N, cast-N, karma-*, kickup-A<rank>-chipdep-N, siphon-*): covered by memo-static.test.sh (every site routed through ledgerMemo) and ledger-memo.test.mo (byte-identical for <= 32 B).",
            "An upline-ranked cascade leg (cascade-A<rank>-signup-...) was not exercised: the rehearsal upline is the inactive house, so the leg is cascade-residual-...; same code path (mintWithEffects -> mintInternal -> ledgerMemo).",
            "No recovery of earlier failures is performed or proven (scope: fix only). P1's lost gift and B1/B2's lost backer mints stay recorded as before.",
        ],
        "all_passed": not FAILED,
    }
    with open(RECEIPT, "w") as f:
        json.dump(receipt, f, indent=1, default=str)
    log("receipt: %s  (%d checks, %d failed)" % (RECEIPT, len(CHECKS), len(set(FAILED))))
    return 0 if not FAILED else 1


if __name__ == "__main__":
    try:
        rc = main()
    except AssertionError as e:
        log("ASSERTION: %s" % e)
        rc = 2
    sys.exit(rc)
