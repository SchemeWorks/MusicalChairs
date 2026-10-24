// Standalone test for Sha256 + LedgerMemo (the PP-ledger 32-byte memo fix).
//
// Run (dfx 0.29.2 bundled compiler, moc 0.16.2, same base package dfx uses):
//   M=~/.cache/dfinity/versions/0.29.2
//   $M/moc -r --package base $M/base shenanigans/test/ledger-memo.test.mo
//
// Every `check` traps on failure, so a clean run that prints the final
// "ALL ... PASSED" line is a pass. Reference digests were produced with
// python3 hashlib.sha256 (independent of this implementation); the first
// four are the FIPS 180-4 / NIST CAVP example messages.

import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Debug "mo:base/Debug";
import Iter "mo:base/Iter";
import Nat "mo:base/Nat";
import Nat8 "mo:base/Nat8";
import Principal "mo:base/Principal";
import Text "mo:base/Text";
import LedgerMemo "../LedgerMemo";
import Sha256 "../Sha256";

var checks : Nat = 0;

func check(name : Text, cond : Bool) {
    checks += 1;
    if (not cond) { Debug.trap("FAIL: " # name) };
    Debug.print("  ok  " # name);
};

let hexDigits : [Char] = Iter.toArray("0123456789abcdef".chars());

func hex(b : Blob) : Text {
    var out = "";
    for (byte in b.vals()) {
        let n = Nat8.toNat(byte);
        out #= Text.fromChar(hexDigits[n / 16]) # Text.fromChar(hexDigits[n % 16]);
    };
    out;
};

func repeat(s : Text, n : Nat) : Text {
    var out = "";
    var i = 0;
    while (i < n) { out #= s; i += 1 };
    out;
};

func utf8(t : Text) : Blob { Text.encodeUtf8(t) };

// The PP ledger rejects (traps, IC0503) any memo above this many bytes.
let LEDGER_LIMIT : Nat = 32;
func ledgerWouldReject(memo : Blob) : Bool { memo.size() > LEDGER_LIMIT };

// ---------------------------------------------------------------- SHA-256
Debug.print("SHA-256 vectors");
check("sha256 constant matches LedgerMemo.MAX_MEMO_BYTES", LedgerMemo.MAX_MEMO_BYTES == LEDGER_LIMIT);
check("sha256 empty string (NIST)", hex(Sha256.sha256(utf8(""))) == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
check("sha256 \"abc\" (NIST, 24 bits)", hex(Sha256.sha256(utf8("abc"))) == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
check("sha256 448-bit message (NIST; padding spills into a 2nd block)",
    hex(Sha256.sha256(utf8("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"))) == "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
check("sha256 896-bit message (NIST; 112 bytes -> 2 blocks)",
    hex(Sha256.sha256(utf8("abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"))) == "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1");
check("sha256 55 bytes (exactly 1 block after padding)", hex(Sha256.sha256(utf8(repeat("a", 55)))) == "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318");
check("sha256 56 bytes (length field forces a 2nd block)", hex(Sha256.sha256(utf8(repeat("a", 56)))) == "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a");
check("sha256 63 bytes", hex(Sha256.sha256(utf8(repeat("a", 63)))) == "7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34");
check("sha256 64 bytes (full block + padding block)", hex(Sha256.sha256(utf8(repeat("a", 64)))) == "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb");
check("sha256 119 bytes (2 blocks, 8 spare)", hex(Sha256.sha256(utf8(repeat("a", 119)))) == "31eba51c313a5c08226adf18d4a359cfdfd8d2e816b13f4af952f7ea6584dcfb");
check("sha256 200 bytes (4 blocks)", hex(Sha256.sha256(utf8(repeat("a", 200)))) == "c2a908d98f5df987ade41b5fce213067efbcc21ef2240212a41e54b5e7c28ae5");
check("sha256 output is always 32 bytes", Sha256.sha256(utf8("x")).size() == 32);

// -------------------------------------------------- real-shaped event ids
// gcbfr-... is a real mainnet principal (63 chars). Amount texts of 8 and 9
// chars reproduce the 79 / 80 byte backer ids seen in the live
// getMissedBackerMints entries.
let P : Text = "gcbfr-3yu36-ks7mt-grhik-mk2ff-3wx55-jffxr-julan-rakf4-5icoa-xqe";
check("principal text is 63 chars", P.size() == 63);

let signupId = "signup-" # P;
let signupSolId = "signup-sol-" # P;
let backer79 = "backer-" # P # "-2.345678";
let backer80 = "backer-" # P # "-12.345678";
let backerSolId = "backer-sol-" # P # "-12.345678";
let cascadeA1Signup = "cascade-A1-signup-" # P;
let cascadeResSignup = "cascade-residual-signup-" # P;
let kickupASignup = "kickup-A1-signup-" # P;
let kickupResSignup = "kickup-residual-signup-" # P;
let siphonBacker = "siphon-backer-" # P # "-12.345678";

let longIds : [(Text, Text, Text)] = [
    ("signup", signupId, "aa055716db6fedfc8d2480b29629c5670fd673adcd19f9f4c393ebd53988e9b0"),
    ("signup-sol", signupSolId, "34ca5ddd371bd05d3511e8208edb30fce685b6551ed500f9a17e464f09d57d48"),
    ("backer (79 B, 8-char amount)", backer79, "573cfb35cb94d138866d5ee2a6989a01ec7d1f559ec9e23bb4dfbbdd0ad2b2b9"),
    ("backer (80 B, 9-char amount)", backer80, "4967845b3fde588a50d90d5352884151bb2b592e459ac213fb589fef67f27a85"),
    ("backer-sol", backerSolId, "94cd9b3485b0ccafd5cd842b310c7b8ce5c614daac69b4636941d63fd4585853"),
    ("cascade-A1-signup", cascadeA1Signup, "19572632de5d571930afbf45a42a56fcc60f6064371442d2b396bd0872ab2f8b"),
    ("cascade-residual-signup", cascadeResSignup, "229bd234cadeb809fad550b44478bdc49ab4dc11dd7b93cee67cf918bfa7b7c3"),
    ("kickup-A1-signup", kickupASignup, "595c678848b89e8e43046b0c913d4c52407795d14cc40ffe87da0e8a8cdf4624"),
    ("kickup-residual-signup", kickupResSignup, "232e051b240a1600b073c79b404a62e9e7e03e5295f13712bbe7216b39327972"),
    ("siphon-backer", siphonBacker, "5a68f2bef20d8b41ed24449b4e9baaa110d0313a8ee47825b9ea41c63f69925f"),
];

// ------------------------------------------------------ NEGATIVE CONTROL
// NEGATIVE CONTROL (documents the unfixed behaviour; NOT a test of the fix):
// the raw UTF-8 of each real long event id is above the ledger's 32-byte
// limit, i.e. what the pre-fix `Text.encodeUtf8(<id>)` would have sent and the
// ledger would have trapped on. If any of these ever fits, the control (and
// the premise of the fix) is void.
Debug.print("NEGATIVE CONTROL: raw UTF-8 of real long ids exceeds the ledger limit");
for ((name_, id, _) in longIds.vals()) {
    check("negative control: raw utf8 of " # name_ # " (" # Nat.toText(utf8(id).size()) # " B) is > 32 -> ledger would reject",
        ledgerWouldReject(utf8(id)));
};
check("negative control: signup id is 70 B", utf8(signupId).size() == 70);
check("negative control: backer ids are 79 / 80 B", utf8(backer79).size() == 79 and utf8(backer80).size() == 80);

// ------------------------------------------------------------ the fix
Debug.print("ledgerMemo: long ids -> exactly 32 bytes, equal to SHA-256(utf8)");
for ((name_, id, ref) in longIds.vals()) {
    let m = LedgerMemo.ledgerMemo(id);
    check("ledgerMemo(" # name_ # ") is exactly 32 bytes", m.size() == 32);
    check("ledgerMemo(" # name_ # ") is accepted by the ledger limit", not ledgerWouldReject(m));
    check("ledgerMemo(" # name_ # ") == independent sha256(utf8(id)) reference", hex(m) == ref);
    check("ledgerMemo(" # name_ # ") is deterministic", Blob.equal(m, LedgerMemo.ledgerMemo(id)));
};

Debug.print("ledgerMemo: distinct ids -> distinct memos");
let hashedMemos = Array.map<(Text, Text, Text), Blob>(longIds, func(t) = LedgerMemo.ledgerMemo(t.1));
var i = 0;
while (i < hashedMemos.size()) {
    var j = i + 1;
    while (j < hashedMemos.size()) {
        check("distinct memo: " # longIds[i].0 # " vs " # longIds[j].0, not Blob.equal(hashedMemos[i], hashedMemos[j]));
        j += 1;
    };
    i += 1;
};
// Two different principals under the same prefix, built from real 29-byte principals.
func principalFromSeed(seed : Nat8) : Text {
    Principal.toText(Principal.fromBlob(Blob.fromArray(Array.tabulate<Nat8>(29, func(k) = seed +% Nat8.fromNat(k)))));
};
let pa = principalFromSeed(1);
let pb = principalFromSeed(2);
check("synthetic principals are 63 chars", pa.size() == 63 and pb.size() == 63);
check("distinct principals -> distinct signup memos",
    not Blob.equal(LedgerMemo.ledgerMemo("signup-" # pa), LedgerMemo.ledgerMemo("signup-" # pb)));
check("same principal, ICP vs SOL gift ids -> distinct memos",
    not Blob.equal(LedgerMemo.ledgerMemo("signup-" # pa), LedgerMemo.ledgerMemo("signup-sol-" # pa)));
check("backer ids differing only in the amount digit -> distinct memos",
    not Blob.equal(LedgerMemo.ledgerMemo("backer-" # pa # "-2.345678"), LedgerMemo.ledgerMemo("backer-" # pa # "-2.345679")));

Debug.print("ledgerMemo: <= 32 bytes -> unchanged, byte-for-byte");
let shortIds : [Text] = [
    "chip-deposit", "cash-out-0", "cash-out-123456", "admin-mint", "rename-custom",
    "game-0", "game-123456789", "game-sol-0", "game-sol-123456",
    "spell-1", "spell-9999999", "backfire-1", "backfire-9999999", "cast-42",
    "karma-burn-7", "karma-pay-7", "karma-mgmt-7",
    "cascade-A1-game-123", "cascade-A3-game-sol-12345", "cascade-residual-game-sol-123456",
    "kickup-A1-chipdep-77", "kickup-residual-chipdep-77", "siphon-game-123", "siphon-game-sol-123",
    "", "x",
];
for (id in shortIds.vals()) {
    check("short memo unchanged: \"" # id # "\" (" # Nat.toText(utf8(id).size()) # " B)",
        utf8(id).size() <= 32 and Blob.equal(LedgerMemo.ledgerMemo(id), utf8(id)));
};

Debug.print("ledgerMemo: boundary at 32 / 33 bytes and multi-byte text");
let b32 = repeat("x", 32);
let b33 = repeat("x", 33);
check("exactly 32 bytes is returned unchanged", Blob.equal(LedgerMemo.ledgerMemo(b32), utf8(b32)) and LedgerMemo.ledgerMemo(b32).size() == 32);
check("33 bytes is hashed (differs from the raw bytes)", not Blob.equal(LedgerMemo.ledgerMemo(b33), utf8(b33)));
check("33 bytes -> 32-byte digest == sha256 reference",
    LedgerMemo.ledgerMemo(b33).size() == 32 and hex(LedgerMemo.ledgerMemo(b33)) == "11ba55a3a7c1ee0f8eb8867dc40a62c67240eb4a5ea125ee5c383fe996b57cd6");
// The limit is in BYTES, not characters: 16 x U+00E9 = 32 bytes (16 chars), 17 = 34 bytes (17 chars).
let e16 = repeat("é", 16);
let e17 = repeat("é", 17);
check("16 x e-acute is 32 bytes and unchanged", utf8(e16).size() == 32 and Blob.equal(LedgerMemo.ledgerMemo(e16), utf8(e16)));
check("17 x e-acute is 34 bytes (17 chars) and is hashed", utf8(e17).size() == 34 and e17.size() == 17 and LedgerMemo.ledgerMemo(e17).size() == 32
    and hex(LedgerMemo.ledgerMemo(e17)) == "b9d7a28694bca22000fc3d6eaca87f311a5968993b343a1dd4adcdc027d66d0c");

Debug.print("ALL " # Nat.toText(checks) # " CHECKS PASSED");
