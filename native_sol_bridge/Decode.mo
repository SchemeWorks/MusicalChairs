import Array "mo:base/Array";
import Blob "mo:base/Blob";
import Buffer "mo:base/Buffer";
import Char "mo:base/Char";
import Int "mo:base/Int";
import Iter "mo:base/Iter";
import Nat "mo:base/Nat";
import Nat8 "mo:base/Nat8";
import Nat32 "mo:base/Nat32";
import Nat64 "mo:base/Nat64";
import Result "mo:base/Result";
import Text "mo:base/Text";

import Events "Events";
import Rpc "Rpc";

/// Decode — turns a `getTransaction` observation into a normalized `Tx`, and
/// event-CPI instruction data into typed `Events.Event`s.
///
/// What the SOL RPC canister really returns: its `EncodedTransaction` is only
/// `binary(text, base58|base64)` or `legacyBinary(text)` — a serialized wire
/// transaction, NOT the JSON `message.accountKeys` form. The signed message
/// (signature count, header, account keys, instructions, v0 lookup tables) is
/// therefore parsed here from raw bytes; `meta` is typed and supplies the
/// inner instructions, the loaded (lookup-table) addresses and the status.
/// `fromJson` accepts the same facts in the JSON-shaped form of the checked-in
/// fixtures (`solana/fixtures/rpc/*.json`); the tests assert both paths yield
/// the identical `Tx` for every fixture.
///
/// Inner-instruction data is kept as the original base58 text and decoded
/// only for the single instruction a descriptor points at, so a transaction
/// with large unrelated CPI payloads costs nothing extra.
module {
    // ==================================================================
    // Base58 — PINNED copy of ponzi_math_sol/Base58.mo (Solana/Bitcoin
    // alphabet). Do not diverge; behaviour is covered by decode.test.mo.
    // ==================================================================

    private let ALPHABET : [Char] = [
        '1', '2', '3', '4', '5', '6', '7', '8', '9',
        'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'L', 'M', 'N', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
    ];

    // Reverse lookup by ASCII code point, -1 = not in alphabet.
    private let DECODE_TABLE : [Int] = [
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, -1, -1, -1, -1, -1, -1,
        -1, 9, 10, 11, 12, 13, 14, 15, 16, -1, 17, 18, 19, 20, 21, -1,
        22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, -1, -1, -1, -1, -1,
        -1, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, -1, 44, 45, 46,
        47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, -1, -1, -1, -1, -1,
    ];

    public func base58Encode(bytes : Blob) : Text {
        let arr = Blob.toArray(bytes);
        if (arr.size() == 0) { return "" };
        var leadingZeros : Nat = 0;
        var idx : Nat = 0;
        while (idx < arr.size() and arr[idx] == (0 : Nat8)) {
            leadingZeros += 1;
            idx += 1;
        };
        var num : Nat = 0;
        for (b in arr.vals()) { num := num * 256 + Nat8.toNat(b) };
        var reverseChars = "";
        while (num > 0) {
            let rem = num % 58;
            num := num / 58;
            reverseChars := Text.fromChar(ALPHABET[rem]) # reverseChars;
        };
        var prefix = "";
        var i : Nat = 0;
        while (i < leadingZeros) { prefix := prefix # "1"; i += 1 };
        prefix # reverseChars;
    };

    /// `null` on an invalid character.
    public func base58Decode(s : Text) : ?Blob {
        if (s.size() == 0) { return ?Blob.fromArray([]) };
        var leadingOnes : Nat = 0;
        let chars = Iter.toArray(Text.toIter(s));
        var i : Nat = 0;
        while (i < chars.size() and chars[i] == '1') { leadingOnes += 1; i += 1 };
        var num : Nat = 0;
        while (i < chars.size()) {
            let cp = Nat32.toNat(Char.toNat32(chars[i]));
            if (cp >= 128) { return null };
            let v = DECODE_TABLE[cp];
            if (v < 0) { return null };
            num := num * 58 + Int.abs(v);
            i += 1;
        };
        var buf = num;
        var byteList : [Nat8] = [];
        if (buf > 0) {
            // base58 -> base256 never needs more bytes than input characters.
            let tmp = Array.init<Nat8>(chars.size() + 1, 0);
            var bi : Nat = 0;
            while (buf > 0) {
                tmp[bi] := Nat8.fromNat(buf % 256);
                buf := buf / 256;
                bi += 1;
            };
            let final = Array.init<Nat8>(bi, 0);
            var k : Nat = 0;
            while (k < bi) {
                final[k] := tmp[bi - 1 - k];
                k += 1;
            };
            byteList := Array.freeze(final);
        };
        let out = Array.init<Nat8>(leadingOnes + byteList.size(), 0);
        var oi : Nat = leadingOnes;
        for (b in byteList.vals()) { out[oi] := b; oi += 1 };
        ?Blob.fromArray(Array.freeze(out));
    };

    /// A base58 string that decodes to exactly `n` bytes.
    public func isBase58Of(s : Text, n : Nat) : Bool {
        switch (base58Decode(s)) {
            case (null) { false };
            case (?b) { b.size() == n };
        };
    };

    // ==================================================================
    // Base64 (standard alphabet, '=' padding optional)
    // ==================================================================

    private func b64val(c : Char) : ?Nat {
        let cp = Nat32.toNat(Char.toNat32(c));
        if (cp >= 65 and cp <= 90) { ?(cp - 65) } // A-Z
        else if (cp >= 97 and cp <= 122) { ?(cp - 97 + 26) } // a-z
        else if (cp >= 48 and cp <= 57) { ?(cp - 48 + 52) } // 0-9
        else if (cp == 43) { ?62 } // +
        else if (cp == 47) { ?63 } // /
        else { null };
    };

    public func base64Decode(s : Text) : ?[Nat8] {
        let out = Buffer.Buffer<Nat8>(s.size() * 3 / 4 + 3);
        var acc : Nat = 0;
        var bits : Nat = 0;
        var padding = false;
        for (c in s.chars()) {
            if (c == '=') { padding := true } else {
                if (padding) { return null }; // data after padding
                switch (b64val(c)) {
                    case (null) { return null };
                    case (?v) {
                        acc := acc * 64 + v;
                        bits += 6;
                        if (bits >= 8) {
                            let shift = Int.abs(bits - 8);
                            let p = Nat.pow(2, shift);
                            out.add(Nat8.fromNat((acc / p) % 256));
                            acc := acc % p;
                            bits := shift;
                        };
                    };
                };
            };
        };
        ?Buffer.toArray(out);
    };

    // ==================================================================
    // Byte reader (little-endian, Borsh / wire primitives)
    // ==================================================================

    /// Sticky-failure reader: any over-read sets `bad` and yields zeros, so a
    /// parse is valid iff `not bad` and every byte was consumed.
    public class Reader(data : [Nat8]) {
        var pos : Nat = 0;
        public var bad : Bool = false;

        public func remaining() : Nat { Int.abs(data.size() - pos) };

        public func u8() : Nat8 {
            if (pos < data.size()) {
                let b = data[pos];
                pos += 1;
                b;
            } else { bad := true; 0 };
        };

        public func bytes(n : Nat) : [Nat8] {
            if (pos + n <= data.size()) {
                let s = Array.subArray<Nat8>(data, pos, n);
                pos += n;
                s;
            } else { bad := true; [] };
        };

        public func skip(n : Nat) {
            if (pos + n <= data.size()) { pos += n } else { bad := true };
        };

        private func uN(n : Nat) : Nat {
            var acc : Nat = 0;
            var mul : Nat = 1;
            var i : Nat = 0;
            while (i < n) {
                acc += Nat8.toNat(u8()) * mul;
                mul *= 256;
                i += 1;
            };
            acc;
        };

        public func u32() : Nat32 { Nat32.fromNat(uN(4)) };
        public func u64() : Nat64 { Nat64.fromNat(uN(8)) };
        public func u128() : Nat { uN(16) };

        public func bool() : Bool {
            let b = u8();
            if (b > 1) { bad := true };
            b == 1;
        };

        public func pubkey() : Text {
            let b = bytes(32);
            if (bad) { "" } else { base58Encode(Blob.fromArray(b)) };
        };

        public func optU64() : ?Nat64 {
            let tag = u8();
            if (tag == 0) { null } else if (tag == 1) { ?u64() } else {
                bad := true;
                null;
            };
        };

        /// Solana compact-u16 ("shortvec").
        public func shortvec() : Nat {
            var value : Nat = 0;
            var mul : Nat = 1;
            var i : Nat = 0;
            label l loop {
                let b = u8();
                if (bad) { break l };
                value += Nat8.toNat(b & 0x7f) * mul;
                if ((b & 0x80) == 0) { break l };
                mul *= 128;
                i += 1;
                if (i >= 3) { bad := true; break l };
            };
            if (value > 65535) { bad := true };
            value;
        };
    };

    // ==================================================================
    // Normalized transaction
    // ==================================================================

    public type TxVersion = { #legacy; #v0 };

    /// Highest `maxSupportedTransactionVersion` this decoder handles
    /// (legacy + v0). Sent on every `getTransaction`; a transaction above it
    /// is a GAP, never skipped (spec section 9).
    public let MAX_SUPPORTED_TX_VERSION : Nat8 = 0;

    /// Outer instruction with raw data.
    public type Ix = { programIdIndex : Nat8; accounts : [Nat8]; data : Blob };

    /// Inner (CPI) instruction; `data` stays base58 text until needed.
    public type InnerIx = {
        programIdIndex : Nat8;
        accounts : [Nat8];
        data : Text;
        stackHeight : ?Nat;
    };

    public type InnerGroup = { index : Nat; instructions : [InnerIx] };

    /// Everything provenance depends on; two providers "agree" iff their
    /// `Tx` values are equal. `accountKeys` is static keys ++ loaded
    /// writable ++ loaded readonly (the order instruction indices use).
    /// Signers are `accountKeys[0 .. numRequiredSignatures)`; the standard
    /// RPC form carries no per-account signer flags.
    public type Tx = {
        slot : Nat64;
        version : TxVersion;
        succeeded : Bool;
        signature : Text;
        numRequiredSignatures : Nat;
        accountKeys : [Text];
        instructions : [Ix];
        inner : [InnerGroup];
    };

    public type DecodeFailure = {
        #UnsupportedVersion : ?Nat8;
        #MissingMeta;
        #MissingInnerInstructions;
        #Malformed : Text;
    };

    public func failureText(f : DecodeFailure) : Text {
        switch (f) {
            case (#UnsupportedVersion(?v)) { "unsupported transaction version " # Nat8.toText(v) };
            case (#UnsupportedVersion(null)) { "unsupported transaction version" };
            case (#MissingMeta) { "transaction meta missing" };
            case (#MissingInnerInstructions) { "inner instructions not recorded" };
            case (#Malformed(m)) { "malformed transaction: " # m };
        };
    };

    // ------------------------------------------------------------------
    // Wire parse (legacy and v0 messages)
    // ------------------------------------------------------------------

    type Wire = {
        version : TxVersion;
        firstSignature : Text;
        numRequiredSignatures : Nat;
        staticKeys : [Text];
        instructions : [Ix];
        lookupWritable : Nat;
        lookupReadonly : Nat;
    };

    func parseWire(bytes : [Nat8]) : Result.Result<Wire, DecodeFailure> {
        let r = Reader(bytes);
        let sigCount = r.shortvec();
        if (r.bad or sigCount == 0 or sigCount > 127) {
            return #err(#Malformed("signature count"));
        };
        var firstSignature = "";
        var s = 0;
        while (s < sigCount) {
            let sig = r.bytes(64);
            if (s == 0 and not r.bad) { firstSignature := base58Encode(Blob.fromArray(sig)) };
            s += 1;
        };
        let prefix = r.u8();
        var version : TxVersion = #legacy;
        var numReq : Nat8 = prefix;
        if ((prefix & 0x80) != 0) {
            let v = prefix & 0x7f;
            if (v != 0) { return #err(#UnsupportedVersion(?v)) };
            version := #v0;
            numReq := r.u8();
        };
        ignore r.u8(); // numReadonlySignedAccounts
        ignore r.u8(); // numReadonlyUnsignedAccounts
        let keyCount = r.shortvec();
        if (r.bad or keyCount > 256) { return #err(#Malformed("account key count")) };
        let keys = Buffer.Buffer<Text>(keyCount);
        var k = 0;
        while (k < keyCount and not r.bad) {
            keys.add(base58Encode(Blob.fromArray(r.bytes(32))));
            k += 1;
        };
        r.skip(32); // recent blockhash
        let ixCount = r.shortvec();
        if (r.bad or ixCount > 1232) { return #err(#Malformed("instruction count")) };
        let ixs = Buffer.Buffer<Ix>(ixCount);
        var i = 0;
        while (i < ixCount and not r.bad) {
            let pid = r.u8();
            let accCount = r.shortvec();
            let accounts = r.bytes(accCount);
            let dataLen = r.shortvec();
            let data = r.bytes(dataLen);
            ixs.add({ programIdIndex = pid; accounts; data = Blob.fromArray(data) });
            i += 1;
        };
        var lw = 0;
        var lr = 0;
        if (version == #v0) {
            let lutCount = r.shortvec();
            if (r.bad or lutCount > 256) { return #err(#Malformed("lookup table count")) };
            var t = 0;
            while (t < lutCount and not r.bad) {
                r.skip(32);
                let w = r.shortvec();
                r.skip(w);
                let ro = r.shortvec();
                r.skip(ro);
                lw += w;
                lr += ro;
                t += 1;
            };
        };
        if (r.bad) { return #err(#Malformed("truncated transaction")) };
        if (r.remaining() != 0) { return #err(#Malformed("trailing bytes")) };
        if (sigCount != Nat8.toNat(numReq)) {
            return #err(#Malformed("signature count != numRequiredSignatures"));
        };
        #ok({
            version;
            firstSignature;
            numRequiredSignatures = Nat8.toNat(numReq);
            staticKeys = Buffer.toArray(keys);
            instructions = Buffer.toArray(ixs);
            lookupWritable = lw;
            lookupReadonly = lr;
        });
    };

    // ------------------------------------------------------------------
    // From the typed SOL RPC canister result
    // ------------------------------------------------------------------

    func mergeKeys(staticKeys : [Text], la : Rpc.LoadedAddresses) : [Text] {
        Array.flatten<Text>([staticKeys, la.writable, la.readonly]);
    };

    public func fromRpc(r : Rpc.EncodedConfirmedTransactionWithStatusMeta) : Result.Result<Tx, DecodeFailure> {
        let responseVersion : ?TxVersion = switch (r.transaction.version) {
            case (null) { null };
            case (?#legacy) { ?#legacy };
            case (?#number(0)) { ?#v0 };
            case (?#number(n)) { return #err(#UnsupportedVersion(?n)) };
        };
        let meta = switch (r.transaction.meta) {
            case (null) { return #err(#MissingMeta) };
            case (?m) { m };
        };
        let innerRaw = switch (meta.innerInstructions) {
            case (null) { return #err(#MissingInnerInstructions) };
            case (?x) { x };
        };
        let bytesOpt : ?[Nat8] = switch (r.transaction.transaction) {
            case (#binary((s, #base64))) { base64Decode(s) };
            case (#binary((s, #base58))) { Option_map(base58Decode(s)) };
            case (#legacyBinary(s)) { Option_map(base58Decode(s)) };
        };
        let bytes = switch (bytesOpt) {
            case (null) { return #err(#Malformed("transaction encoding")) };
            case (?b) { b };
        };
        let wire = switch (parseWire(bytes)) {
            case (#err(e)) { return #err(e) };
            case (#ok(w)) { w };
        };
        switch (responseVersion) {
            case (?v) { if (v != wire.version) { return #err(#Malformed("version mismatch")) } };
            case (null) {};
        };
        let loaded : Rpc.LoadedAddresses = switch (meta.loadedAddresses) {
            case (null) { { writable = []; readonly = [] } };
            case (?l) { l };
        };
        if (loaded.writable.size() != wire.lookupWritable or loaded.readonly.size() != wire.lookupReadonly) {
            return #err(#Malformed("loadedAddresses do not match the message's lookup tables"));
        };
        let keys = mergeKeys(wire.staticKeys, loaded);
        if (keys.size() > 256) { return #err(#Malformed("too many account keys")) };
        let inner = Array.map<Rpc.InnerInstructions, InnerGroup>(
            innerRaw,
            func(g) {
                {
                    index = Nat8.toNat(g.index);
                    instructions = Array.map<Rpc.Instruction, InnerIx>(
                        g.instructions,
                        func(i) {
                            switch (i) {
                                case (#compiled(c)) {
                                    {
                                        programIdIndex = c.programIdIndex;
                                        accounts = Blob.toArray(c.accounts);
                                        data = c.data;
                                        stackHeight = switch (c.stackHeight) {
                                            case (null) { null };
                                            case (?h) { ?Nat32.toNat(h) };
                                        };
                                    };
                                };
                            };
                        },
                    );
                };
            },
        );
        #ok({
            slot = r.slot;
            version = wire.version;
            succeeded = switch (meta.status) { case (#Ok) { true }; case (#Err(_)) { false } };
            signature = wire.firstSignature;
            numRequiredSignatures = wire.numRequiredSignatures;
            accountKeys = keys;
            instructions = wire.instructions;
            inner;
        });
    };

    private func Option_map(b : ?Blob) : ?[Nat8] {
        switch (b) {
            case (null) { null };
            case (?x) { ?Blob.toArray(x) };
        };
    };

    // ------------------------------------------------------------------
    // From the JSON-shaped form (checked-in fixtures; getTransaction with
    // encoding json, maxSupportedTransactionVersion 0)
    // ------------------------------------------------------------------

    public type JsonIx = {
        programIdIndex : Nat8;
        accounts : [Nat8];
        data : Text;
        stackHeight : ?Nat32;
    };

    public type JsonTx = {
        slot : Nat64;
        version : ?{ #legacy; #number : Nat8 };
        /// `meta.err != null`
        failed : Bool;
        signatures : [Text];
        numRequiredSignatures : Nat8;
        accountKeys : [Text];
        loadedAddresses : ?Rpc.LoadedAddresses;
        instructions : [JsonIx];
        innerInstructions : ?[{ index : Nat8; instructions : [JsonIx] }];
    };

    public func fromJson(j : JsonTx) : Result.Result<Tx, DecodeFailure> {
        let version : TxVersion = switch (j.version) {
            case (null) { #legacy };
            case (?#legacy) { #legacy };
            case (?#number(0)) { #v0 };
            case (?#number(n)) { return #err(#UnsupportedVersion(?n)) };
        };
        let innerRaw = switch (j.innerInstructions) {
            case (null) { return #err(#MissingInnerInstructions) };
            case (?x) { x };
        };
        if (j.signatures.size() == 0) { return #err(#Malformed("no signatures")) };
        let loaded : Rpc.LoadedAddresses = switch (j.loadedAddresses) {
            case (null) { { writable = []; readonly = [] } };
            case (?l) { l };
        };
        var bad = false;
        let outer = Array.map<JsonIx, Ix>(
            j.instructions,
            func(i) {
                switch (base58Decode(i.data)) {
                    case (null) { bad := true; { programIdIndex = i.programIdIndex; accounts = i.accounts; data = Blob.fromArray([]) } };
                    case (?d) { { programIdIndex = i.programIdIndex; accounts = i.accounts; data = d } };
                };
            },
        );
        if (bad) { return #err(#Malformed("instruction data is not base58")) };
        let inner = Array.map<{ index : Nat8; instructions : [JsonIx] }, InnerGroup>(
            innerRaw,
            func(g) {
                {
                    index = Nat8.toNat(g.index);
                    instructions = Array.map<JsonIx, InnerIx>(
                        g.instructions,
                        func(i) {
                            {
                                programIdIndex = i.programIdIndex;
                                accounts = i.accounts;
                                data = i.data;
                                stackHeight = switch (i.stackHeight) {
                                    case (null) { null };
                                    case (?h) { ?Nat32.toNat(h) };
                                };
                            };
                        },
                    );
                };
            },
        );
        #ok({
            slot = j.slot;
            version;
            succeeded = not j.failed;
            signature = j.signatures[0];
            numRequiredSignatures = Nat8.toNat(j.numRequiredSignatures);
            accountKeys = mergeKeys(j.accountKeys, loaded);
            instructions = outer;
            inner;
        });
    };

    // ------------------------------------------------------------------
    // One provider's reply, classified
    // ------------------------------------------------------------------

    public type ProviderObservation = {
        /// A decoded transaction at the commitment that was REQUESTED (the
        /// reply itself carries no commitment field).
        #Observed : { commitment : Rpc.CommitmentLevel; tx : Tx };
        #NotFound;
        #Error : Text;
        #Inconsistent;
        #Undecodable : DecodeFailure;
    };

    public func classify(commitment : Rpc.CommitmentLevel, res : Rpc.MultiGetTransactionResult) : ProviderObservation {
        switch (res) {
            case (#Inconsistent(_)) { #Inconsistent };
            case (#Consistent(#Err(#JsonRpcError(e)))) {
                if (e.code == Rpc.JSON_RPC_UNSUPPORTED_TX_VERSION) {
                    #Undecodable(#UnsupportedVersion(null));
                } else { #Error(Rpc.errorText(#JsonRpcError(e))) };
            };
            case (#Consistent(#Err(e))) { #Error(Rpc.errorText(e)) };
            case (#Consistent(#Ok(null))) { #NotFound };
            case (#Consistent(#Ok(?t))) {
                switch (fromRpc(t)) {
                    case (#ok(tx)) { #Observed({ commitment; tx }) };
                    case (#err(f)) { #Undecodable(f) };
                };
            };
        };
    };

    // ==================================================================
    // Event CPI data and Borsh bodies
    // ==================================================================

    public type EventCpi = { discriminator : Blob; body : Blob };

    /// Splits `TAG ++ discriminator ++ body`; `null` when the data is shorter
    /// than 16 bytes or does not start with the event-CPI tag.
    public func parseEventCpiData(data : Blob) : ?EventCpi {
        let a = Blob.toArray(data);
        if (a.size() < 16) { return null };
        var i = 0;
        while (i < 8) {
            if (a[i] != Events.EVENT_IX_TAG[i]) { return null };
            i += 1;
        };
        ?{
            discriminator = Blob.fromArray(Array.subArray<Nat8>(a, 8, 8));
            body = Blob.fromArray(Array.subArray<Nat8>(a, 16, a.size() - 16));
        };
    };

    /// First body byte (the event's `schema_version`).
    public func peekSchemaVersion(c : EventCpi) : ?Nat8 {
        let b = Blob.toArray(c.body);
        if (b.size() == 0) { null } else { ?b[0] };
    };

    /// The only Borsh layout this decoder understands.
    public let SUPPORTED_EVENT_SCHEMA : Nat8 = 1;

    public type EventDecodeError = {
        #UnknownDiscriminator : Text;
        #UnsupportedSchema : Nat8;
        #Malformed : Text;
    };

    public func decodeEvent(c : EventCpi) : Result.Result<Events.Event, EventDecodeError> {
        let discHex = Events.hex(c.discriminator);
        let name = switch (Events.nameOfDiscriminator(discHex)) {
            case (null) { return #err(#UnknownDiscriminator(discHex)) };
            case (?n) { n };
        };
        let body = Blob.toArray(c.body);
        if (body.size() == 0) { return #err(#Malformed("empty event body")) };
        let sv = body[0];
        if (sv != SUPPORTED_EVENT_SCHEMA) { return #err(#UnsupportedSchema(sv)) };
        let r = Reader(body);
        ignore r.u8(); // schema_version
        let ev : Events.Event = switch (name) {
            case ("PositionOpened") {
                #PositionOpened({
                    schema_version = sv;
                    owner = r.pubkey();
                    nonce = r.u64();
                    round_id = r.u64();
                    close_id = r.optU64();
                    plan = r.u8();
                    gross = r.u64();
                    load = r.u64();
                    config_version = r.u32();
                });
            };
            case ("SeriesAFunded") {
                #SeriesAFunded({
                    schema_version = sv;
                    owner = r.pubkey();
                    round_id = r.u64();
                    close_id = r.optU64();
                    gross = r.u64();
                    entitlement_after = r.u128();
                });
            };
            case ("PayoutPaid") {
                #PayoutPaid({
                    schema_version = sv;
                    owner = r.pubkey();
                    nonce = r.u64();
                    round_id = r.u64();
                    gross_paid = r.u64();
                    toll = r.u64();
                    net = r.u64();
                    destination = r.pubkey();
                    closing = r.bool();
                    close_id = r.optU64();
                    ticket_id = r.u64();
                });
            };
            case ("BackerClaimed") {
                #BackerClaimed({
                    schema_version = sv;
                    owner = r.pubkey();
                    round_id = r.u64();
                    amount = r.u64();
                    a_part = r.u64();
                    b_part = r.u64();
                });
            };
            case ("PromotionApplied") {
                #PromotionApplied({
                    schema_version = sv;
                    round_id = r.u64();
                    close_id = r.optU64();
                    winner = r.pubkey();
                    loss = r.u64();
                    entitlement = r.u128();
                });
            };
            case ("RoundFinalized") {
                #RoundFinalized({
                    schema_version = sv;
                    close_id = r.u64();
                    new_round = r.u64();
                    seed_carried = r.u64();
                });
            };
            case (_) { #Other({ name; schema_version = sv; body = c.body }) };
        };
        switch (ev) {
            case (#Other(_)) { #ok(ev) };
            case (_) {
                if (r.bad) { #err(#Malformed(name # ": truncated or invalid body")) } else if (r.remaining() != 0) {
                    #err(#Malformed(name # ": trailing bytes"));
                } else { #ok(ev) };
            };
        };
    };

    // ------------------------------------------------------------------
    // Diagnostic scan (tests / tooling): every inner instruction that is a
    // well-formed event CPI of the given program, wherever it sits. The
    // verifier does NOT use this; it checks the single instruction the
    // descriptor names.
    // ------------------------------------------------------------------

    public type FoundEvent = {
        instruction_index : Nat;
        inner_instruction_index : Nat;
        event : Events.Event;
    };

    public func scanEvents(tx : Tx, programId : Text, eventAuthority : Text) : [FoundEvent] {
        let out = Buffer.Buffer<FoundEvent>(4);
        for (g in tx.inner.vals()) {
            var j = 0;
            for (ix in g.instructions.vals()) {
                let pidIdx = Nat8.toNat(ix.programIdIndex);
                if (ix.stackHeight == ?2 and pidIdx < tx.accountKeys.size() and tx.accountKeys[pidIdx] == programId and ix.accounts.size() > 0) {
                    let a0 = Nat8.toNat(ix.accounts[0]);
                    if (a0 < tx.accountKeys.size() and tx.accountKeys[a0] == eventAuthority) {
                        switch (base58Decode(ix.data)) {
                            case (null) {};
                            case (?data) {
                                switch (parseEventCpiData(data)) {
                                    case (null) {};
                                    case (?c) {
                                        switch (decodeEvent(c)) {
                                            case (#ok(e)) {
                                                out.add({ instruction_index = g.index; inner_instruction_index = j; event = e });
                                            };
                                            case (#err(_)) {};
                                        };
                                    };
                                };
                            };
                        };
                    };
                };
                j += 1;
            };
        };
        Buffer.toArray(out);
    };
};
