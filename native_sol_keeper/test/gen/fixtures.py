#!/usr/bin/env python3
"""Generate native_sol_keeper/test/fixtures.mo (golden data for the keeper tests).

    python3 native_sol_keeper/test/gen/fixtures.py            # rewrite fixtures.mo
    python3 native_sol_keeper/test/gen/fixtures.py --check    # exit 1 if stale

Offline and deterministic. Sources, all independent of the Motoko code under test:
  * program account layouts: serialized here with a Borsh serializer driven by the
    program IDL (frontend/src/solana/native/idl/musical_chairs.json) for Pool,
    Ticket, CloseRecord and Backer; RegistryPage / SnapshotPage (not in the IDL)
    from solana/programs/musical_chairs/src/state.rs. Every struct field gets a
    distinct value wider than the next-smaller integer so a width or order mistake
    in the decoder shows up as a wrong value. Asserted here: CloseRecord max size
    = 657 - 8 bytes (docs/solana/measurements-2026-10.md), CandidatePage-style
    page sizes.
  * captured localnet transactions: solana/fixtures/rpc/{flows-011..016,closing-010}.json
    (real discover_rows / apply_senior / seal_weights / allocate_rows / prune_rows /
    commit_ticket / finalize_round transactions) - message bytes, account order and
    instruction data are the goldens for Tx and Planner.
  * PDA / on-curve vectors: @solana/web3.js via pda_vectors.cjs.
  * SHA-256 / base58 / base64: Python hashlib / base64.
"""
import base64
import hashlib
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
IDL = os.path.join(ROOT, "frontend", "src", "solana", "native", "idl", "musical_chairs.json")
RPC_DIR = os.path.join(ROOT, "solana", "fixtures", "rpc")
OUT = os.path.join(ROOT, "native_sol_keeper", "test", "fixtures.mo")

A58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def b58dec(s):
    n = 0
    for c in s:
        n = n * 58 + A58.index(c)
    b = n.to_bytes((n.bit_length() + 7) // 8, "big") if n else b""
    return b"\x00" * (len(s) - len(s.lstrip("1"))) + b


def b58enc(b):
    n = int.from_bytes(b, "big")
    out = ""
    while n:
        n, r = divmod(n, 58)
        out = A58[r] + out
    return "1" * (len(b) - len(b.lstrip(b"\x00"))) + out


# ---------------------------------------------------------------------------
# Borsh driven by IDL type descriptors
# ---------------------------------------------------------------------------
IDL_JSON = json.load(open(IDL))
TYPES = {t["name"]: t["type"] for t in IDL_JSON["types"]}
# state.rs types the IDL omits (no instruction names them).
TYPES["SnapshotRow"] = {"kind": "struct", "fields": [
    {"name": "key", "type": "pubkey"}, {"name": "series", "type": "u8"},
    {"name": "remaining", "type": "u128"}, {"name": "slot", "type": "u64"}]}
TYPES["RegistryPage"] = {"kind": "struct", "fields": [
    {"name": "bump", "type": "u8"}, {"name": "index", "type": "u64"},
    {"name": "keys", "type": {"array": ["pubkey", 8]}},
    {"name": "rent", "type": {"defined": {"name": "RentComponents"}}}]}
TYPES["SnapshotPage"] = {"kind": "struct", "fields": [
    {"name": "bump", "type": "u8"}, {"name": "index", "type": "u64"}, {"name": "ticket_id", "type": "u64"},
    {"name": "rows", "type": {"array": [{"defined": {"name": "SnapshotRow"}}, 8]}},
    {"name": "rent", "type": {"defined": {"name": "RentComponents"}}}]}
PRIM = {"u8": 1, "u16": 2, "u32": 4, "u64": 8, "u128": 16, "bool": 1, "pubkey": 32}


def ser(t, v):
    if isinstance(t, str):
        if t == "bool":
            return bytes([1 if v else 0])
        if t == "pubkey":
            assert len(v) == 32
            return bytes(v)
        return int(v).to_bytes(PRIM[t], "little")
    if "array" in t:
        elem, n = t["array"]
        assert len(v) == n, (len(v), n)
        return b"".join(ser(elem, x) for x in v)
    if "option" in t:
        return b"\x00" if v is None else b"\x01" + ser(t["option"], v)
    if "defined" in t:
        d = TYPES[t["defined"]["name"]]
        if d["kind"] == "struct":
            return b"".join(ser(f["type"], v[f["name"]]) for f in d["fields"])
        name, fields = v
        for idx, var in enumerate(d["variants"]):
            if var["name"] == name:
                out = bytes([idx])
                for f in var.get("fields", []):
                    out += ser(f["type"], fields[f["name"]])
                return out
        raise KeyError(name)
    raise ValueError(t)


def maxsize(t):
    if isinstance(t, str):
        return PRIM[t]
    if "array" in t:
        return maxsize(t["array"][0]) * t["array"][1]
    if "option" in t:
        return 1 + maxsize(t["option"])
    d = TYPES[t["defined"]["name"]]
    if d["kind"] == "struct":
        return sum(maxsize(f["type"]) for f in d["fields"])
    return 1 + max(sum(maxsize(f["type"]) for f in var.get("fields", [])) for var in d["variants"])


def disc(name):
    return hashlib.sha256(("account:" + name).encode()).digest()[:8]


def fill(tname, k0, over=None):
    """Distinct, wide values for every field of struct `tname`."""
    over = over or {}
    out = {}
    for i, f in enumerate(TYPES[tname]["fields"]):
        k = k0 + i
        t = f["type"]
        if f["name"] in over:
            out[f["name"]] = over[f["name"]]
        elif t == "u8":
            out[f["name"]] = (k * 5) % 200 + 1
        elif t == "u16":
            out[f["name"]] = 10_000 + k
        elif t == "u32":
            out[f["name"]] = 5_000_000_000 % (2**32) + k
        elif t == "u64":
            out[f["name"]] = (1 << 40) + k * 7919
        elif t == "u128":
            out[f["name"]] = (1 << 80) + k * 104729
        elif t == "bool":
            out[f["name"]] = k % 2 == 0
        elif t == "pubkey":
            out[f["name"]] = bytes((k * 3 + j) % 256 for j in range(32))
        elif isinstance(t, dict) and "array" in t:
            elem, n = t["array"]
            if elem == "u8":
                out[f["name"]] = bytes((k + j) % 256 for j in range(n))
            elif elem == "pubkey":
                out[f["name"]] = [bytes((k * 11 + e * 5 + j) % 256 for j in range(32)) for e in range(n)]
            else:
                out[f["name"]] = [fill(elem["defined"]["name"], k * 13 + e) for e in range(n)]
        elif isinstance(t, dict) and "option" in t:
            out[f["name"]] = (1 << 33) + k
        elif isinstance(t, dict) and "defined" in t:
            sub = t["defined"]["name"]
            if TYPES[sub]["kind"] == "struct":
                out[f["name"]] = fill(sub, k * 17)
            else:
                raise ValueError("enum field needs an override: " + f["name"])
        else:
            raise ValueError(t)
    return out


def account_bytes(tname, value):
    return disc(tname) + ser({"defined": {"name": tname}}, value)


# ---------------------------------------------------------------------------
# Motoko emit helpers
# ---------------------------------------------------------------------------
def mo_blob(b):
    return '"' + "".join("\\%02X" % x for x in b) + '"'


def mo_bool(b):
    return "true" if b else "false"


def mo_opt(v):
    return "null" if v is None else "?" + str(v)


def camel(s):
    parts = s.split("_")
    return parts[0] + "".join(p.capitalize() for p in parts[1:])


def mo_promotion(p):
    if p[0] == "Pending":
        return "#Pending"
    if p[0] == "Applied":
        return "#Applied({ winner = %s; entitlement = %d })" % (mo_blob(p[1]["winner"]), p[1]["entitlement"])
    return "#Skipped({ reason = %d; stage = %d })" % (p[1]["reason"], p[1]["stage"])


EXPOSED = {
    "Pool": ["cluster_domain", "round_id", "active_config_version", "pot", "seed", "management", "pending_toll",
             "registry_len", "registry_version", "next_sequence", "next_ticket", "active_ticket", "finalized_ticket",
             "snapshot_pages", "registry_pages", "participant_count", "participant_pages", "next_close_id",
             "current_close_id"],
    "Ticket": ["id", "phase", "close_id", "captured_len", "expected_version", "cursor", "a_count", "b_count",
               "oldest", "toll"],
    "CloseRecord": ["close_id", "round_id", "close_started_at", "promotion_deadline", "closing_ticket_id",
                    "cash_committed", "participant_count", "candidate_cursor", "preferred_count", "fallback_count",
                    "selected_set_tag", "sealed", "draw_state", "request_account", "captured_at", "promotion",
                    "finalized", "required_pages_per_set", "provisioned_preferred_pages",
                    "provisioned_fallback_pages", "owner_refund_destination", "reserve_settled"],
    "Backer": ["owner", "series", "entitlement", "lifetime", "active", "slot"],
}


def mo_value(tname, field, v):
    t = next(f["type"] for f in TYPES[tname]["fields"] if f["name"] == field)
    if field == "promotion":
        return mo_promotion(v)
    if t == "bool":
        return mo_bool(v)
    if t == "pubkey" or (isinstance(t, dict) and "array" in t):
        return mo_blob(v)
    if isinstance(t, dict) and "option" in t:
        return mo_opt(v)
    return str(v)


def mo_record(tname, value):
    parts = []
    for f in EXPOSED[tname]:
        parts.append("%s = %s" % (camel(f), mo_value(tname, f, value[f])))
    return "{ " + "; ".join(parts) + " }"


# ---------------------------------------------------------------------------
# Account fixtures
# ---------------------------------------------------------------------------
def account_fixtures():
    out = []
    # Pool: current_close_id Some / None (Borsh Option shifts nothing before it, the fields after it are not read).
    pool1 = fill("Pool", 1, {"current_close_id": 5})
    pool2 = fill("Pool", 40, {"current_close_id": None, "active_ticket": 0})
    out.append(("pool1", "Pool", pool1))
    out.append(("pool2", "Pool", pool2))
    t1 = fill("Ticket", 100, {"close_id": 9, "phase": 3})
    t2 = fill("Ticket", 140, {"close_id": None, "phase": 4})
    out.append(("ticket1", "Ticket", t1))
    out.append(("ticket2", "Ticket", t2))
    c1 = fill("CloseRecord", 200, {"closing_ticket_id": 12, "promotion": ("Pending", {}), "cash_committed": True,
                                   "finalized": False, "reserve_settled": False, "sealed": True, "draw_state": 1,
                                   "selected_set_tag": 1})
    c2 = fill("CloseRecord", 260, {"closing_ticket_id": None,
                                   "promotion": ("Applied", {"winner": bytes(range(32)), "entitlement": (1 << 90) + 3}),
                                   "cash_committed": True, "finalized": True, "reserve_settled": True,
                                   "sealed": False, "draw_state": 2, "selected_set_tag": 0})
    c3 = fill("CloseRecord", 320, {"closing_ticket_id": 4, "promotion": ("Skipped", {"reason": 2, "stage": 1}),
                                   "cash_committed": False, "finalized": False, "reserve_settled": True,
                                   "sealed": True, "draw_state": 3, "selected_set_tag": 1})
    out.append(("close1", "CloseRecord", c1))
    out.append(("close2", "CloseRecord", c2))
    out.append(("close3", "CloseRecord", c3))
    b1 = fill("Backer", 400, {"series": 0, "active": True})
    out.append(("backer1", "Backer", b1))
    return out


def registry_fixture():
    keys = [bytes((0xA0 + e * 3 + j) % 256 for j in range(32)) for e in range(8)]
    v = {"bump": 253, "index": 11, "keys": keys,
         "rent": {"ops": 1 << 33, "owner": 5, "owner_refund": bytes(range(32, 64))}}
    return keys, v


def snapshot_fixture():
    rows = []
    for e in range(8):
        rows.append({"key": bytes((0x40 + e * 9 + j) % 256 for j in range(32)), "series": e % 2,
                     "remaining": (1 << 70) + e * 31 if e != 3 else 10, "slot": 88 + e})
    v = {"bump": 252, "index": 11, "ticket_id": (1 << 36) + 5, "rows": rows,
         "rent": {"ops": 1, "owner": 2, "owner_refund": bytes(range(64, 96))}}
    return rows, v


# ---------------------------------------------------------------------------
# Captured transactions
# ---------------------------------------------------------------------------
def shortvec(n):
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            break
    return bytes(out)


def legacy_message(m):
    h = m["header"]
    out = bytes([h["numRequiredSignatures"], h["numReadonlySignedAccounts"], h["numReadonlyUnsignedAccounts"]])
    out += shortvec(len(m["accountKeys"])) + b"".join(b58dec(k) for k in m["accountKeys"])
    out += b58dec(m["recentBlockhash"])
    out += shortvec(len(m["instructions"]))
    for ix in m["instructions"]:
        d = b58dec(ix["data"])
        out += bytes([ix["programIdIndex"]]) + shortvec(len(ix["accounts"])) + bytes(ix["accounts"]) + shortvec(len(d)) + d
    return out


CAPTURES = [
    ("flows-011.json", "discover_rows"), ("flows-012.json", "apply_senior"), ("flows-013.json", "seal_weights"),
    ("flows-014.json", "allocate_rows"), ("flows-015.json", "prune_rows"), ("flows-016.json", "commit_ticket"),
    ("closing-010.json", "finalize_round"),
]
COMPUTE_BUDGET = "ComputeBudget111111111111111111111111111111"


def capture(fname, step):
    d = json.load(open(os.path.join(RPC_DIR, fname)))["result"]["transaction"]
    m = d["message"]
    keys = m["accountKeys"]
    h = m["header"]
    n = len(keys)
    signer = lambda i: i < h["numRequiredSignatures"]
    writable = lambda i: (i < h["numRequiredSignatures"] - h["numReadonlySignedAccounts"]) if signer(i) else (
        i < n - h["numReadonlyUnsignedAccounts"])
    ixs = []
    keeper = None
    for idx, ix in enumerate(m["instructions"]):
        prog = keys[ix["programIdIndex"]]
        if prog != COMPUTE_BUDGET:
            assert keeper is None, "one keeper instruction per capture"
            keeper = idx
        ixs.append({"program": prog, "data": b58dec(ix["data"]),
                    "accounts": [(keys[a], signer(a), writable(a)) for a in ix["accounts"]]})
    assert keeper is not None
    assert ixs[keeper]["program"] == IDL_JSON["address"]
    return {"file": fname, "step": step, "feePayer": keys[0], "blockhash": m["recentBlockhash"],
            "signature": d["signatures"][0], "message": legacy_message(m), "ixs": ixs, "keeper": keeper}


# ---------------------------------------------------------------------------
# Emit
# ---------------------------------------------------------------------------
def build():
    o = []
    w = o.append
    w("/// GENERATED by native_sol_keeper/test/gen/fixtures.py - do not edit.")
    w("/// Golden data for the keeper tests (see the generator's docstring for the sources).")
    w("import Accounts \"../Accounts\";")
    w("")
    w("module {")
    w("    public let PROGRAM : Text = %s;" % json.dumps(IDL_JSON["address"]))
    w("")
    # --- account fixtures
    w("    public type PoolFx = { bytes : Blob; expect : Accounts.Pool };")
    w("    public type TicketFx = { bytes : Blob; expect : Accounts.Ticket };")
    w("    public type CloseFx = { bytes : Blob; expect : Accounts.CloseRecord };")
    w("    public type BackerFx = { bytes : Blob; expect : Accounts.Backer };")
    w("")
    sizes = {}
    for name, tname, value in account_fixtures():
        raw = account_bytes(tname, value)
        sizes.setdefault(tname, []).append(len(raw))
        mty = {"Pool": "PoolFx", "Ticket": "TicketFx", "CloseRecord": "CloseFx", "Backer": "BackerFx"}[tname]
        w("    public let %s : %s = {" % (name, mty))
        w("        bytes = %s;" % mo_blob(raw))
        w("        expect = %s;" % mo_record(tname, value))
        w("    };")
    assert maxsize({"defined": {"name": "CloseRecord"}}) + 8 == 657, maxsize({"defined": {"name": "CloseRecord"}})
    w("")
    keys, rv = registry_fixture()
    raw = disc("RegistryPage") + ser({"defined": {"name": "RegistryPage"}}, rv)
    assert len(raw) == 8 + maxsize({"defined": {"name": "RegistryPage"}}) == 321
    w("    public let registryPage : { bytes : Blob; index : Nat64; keys : [Blob] } = {")
    w("        bytes = %s;" % mo_blob(raw))
    w("        index = %d;" % rv["index"])
    w("        keys = [%s];" % ", ".join(mo_blob(k) for k in keys))
    w("    };")
    rows, sv = snapshot_fixture()
    raw = disc("SnapshotPage") + ser({"defined": {"name": "SnapshotPage"}}, sv)
    assert len(raw) == 8 + maxsize({"defined": {"name": "SnapshotPage"}}) == 529
    w("    public let snapshotPage : { bytes : Blob; index : Nat64; ticketId : Nat64; rows : [Accounts.SnapshotRow] } = {")
    w("        bytes = %s;" % mo_blob(raw))
    w("        index = %d;" % sv["index"])
    w("        ticketId = %d;" % sv["ticket_id"])
    w("        rows = [")
    for r in rows:
        w("            { key = %s; series = %d; remaining = %d; slot = %d }," % (mo_blob(r["key"]), r["series"], r["remaining"], r["slot"]))
    w("        ];")
    w("    };")
    w("")
    # --- captured transactions
    w("    /// (base58 key, isSigner, isWritable) - flags merged over the whole message, as the header encodes them.")
    w("    public type Meta = (Text, Bool, Bool);")
    w("    public type CapturedIx = { program : Text; data : Blob; accounts : [Meta] };")
    w("    public type Captured = {")
    w("        file : Text;")
    w("        step : Text;")
    w("        feePayer : Text;")
    w("        blockhash : Text;")
    w("        signature : Text;")
    w("        message : Blob;")
    w("        ixs : [CapturedIx];")
    w("        /// Index of the program instruction inside `ixs` (the others are ComputeBudget).")
    w("        keeper : Nat;")
    w("    };")
    w("    public let captured : [Captured] = [")
    for fname, step in CAPTURES:
        c = capture(fname, step)
        w("        {")
        w("            file = %s; step = %s; feePayer = %s; blockhash = %s; signature = %s;" % tuple(json.dumps(c[k]) for k in ("file", "step", "feePayer", "blockhash", "signature")))
        w("            message = %s;" % mo_blob(c["message"]))
        w("            keeper = %d;" % c["keeper"])
        w("            ixs = [")
        for ix in c["ixs"]:
            metas = ", ".join("(%s, %s, %s)" % (json.dumps(k), mo_bool(s), mo_bool(wr)) for k, s, wr in ix["accounts"])
            w("                { program = %s; data = %s; accounts = [%s] }," % (json.dumps(ix["program"]), mo_blob(ix["data"]), metas))
        w("            ];")
        w("        },")
    w("    ];")
    w("")
    # --- PDA vectors
    node = subprocess.run(["node", os.path.join(HERE, "pda_vectors.cjs")], cwd=ROOT, check=True,
                          capture_output=True, text=True).stdout
    pv = json.loads(node)
    w("    public type PdaVector = { name : Text; seeds : [Blob]; program : Text; address : Text; bump : Nat8 };")
    w("    public let pdaVectors : [PdaVector] = [")
    for v in pv["vectors"]:
        seeds = ", ".join(mo_blob(bytes.fromhex(s)) for s in v["seeds"])
        w("        { name = %s; seeds = [%s]; program = %s; address = %s; bump = %d }," % (json.dumps(v["label"]), seeds, json.dumps(v["program"]), json.dumps(v["address"]), v["bump"]))
    w("    ];")
    w("    public type CurveVector = { name : Text; point : Blob; onCurve : Bool };")
    w("    public let curveVectors : [CurveVector] = [")
    for v in pv["onCurve"]:
        w("        { name = %s; point = %s; onCurve = %s }," % (json.dumps(v["label"]), mo_blob(bytes.fromhex(v["hex"])), mo_bool(v["onCurve"])))
    w("    ];")
    w("")
    # --- SHA-256 / base58 / base64
    w("    public type HashVector = { message : Blob; digest : Blob };")
    w("    public let sha256Vectors : [HashVector] = [")
    msgs = [b"", b"abc", b"a" * 55, b"a" * 56, b"a" * 63, b"a" * 64, b"a" * 65, b"a" * 119, b"a" * 120, b"a" * 128,
            bytes(i % 256 for i in range(1000)), b"ProgramDerivedAddress"]
    for m in msgs:
        w("        { message = %s; digest = %s }," % (mo_blob(m), mo_blob(hashlib.sha256(m).digest())))
    w("    ];")
    w("    public type TextVector = { bytes : Blob; text : Text };")
    w("    public let base58Vectors : [TextVector] = [")
    for m in [b"", b"\x00", b"\x00\x00\x01", b"\x01", b"\xff" * 4, b58dec(IDL_JSON["address"]), bytes(64),
              bytes(range(64)), b"\x00" * 31 + b"\x01", bytes(range(32, 0, -1))]:
        w("        { bytes = %s; text = %s }," % (mo_blob(m), json.dumps(b58enc(m))))
    w("    ];")
    w("    public let base64Vectors : [TextVector] = [")
    for m in [b"", b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar", bytes(range(256)), bytes(range(200, 0, -1))]:
        w("        { bytes = %s; text = %s }," % (mo_blob(m), json.dumps(base64.b64encode(m).decode())))
    w("    ];")
    w("};")
    return "\n".join(o) + "\n"


def main():
    text = build()
    if "--check" in sys.argv:
        cur = open(OUT).read() if os.path.exists(OUT) else ""
        if cur != text:
            print("fixtures.mo is stale: rerun fixtures.py", file=sys.stderr)
            sys.exit(1)
        print("fixtures.mo is current")
        return
    with open(OUT, "w") as f:
        f.write(text)
    print("wrote", OUT, "(%d bytes)" % len(text))


if __name__ == "__main__":
    main()
