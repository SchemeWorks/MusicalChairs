/**
 * Ledger memo encoding for the PP ledger (pp_ledger, 5xv2o-iiaaa-aaaac-qeclq-cai).
 *
 * The PP ledger enforces `icrc1:max_memo_length = 32` and TRAPS (IC0503: "the
 * memo field size of N bytes is above the allowed limit of 32 bytes") on a
 * longer memo, so a transfer whose memo is built from a long event id such as
 * `signup-<63-char principal>` or `backer-<principal>-<amount>` can never
 * commit. `ledgerMemo` is the ONE function every ledger memo in main.mo goes
 * through:
 *
 *   - UTF-8 encoding <= 32 bytes -> returned unchanged (byte-identical to the
 *     previous `Text.encodeUtf8`, so every existing short memo — `game-N`,
 *     `chip-deposit`, `cash-out-N`, `spell-N`, `cascade-A1-game-N`, ... — and
 *     any ledger-side expectation about them is unchanged);
 *   - longer -> the 32-byte SHA-256 of the UTF-8 text. Deterministic per input
 *     (same event id -> same memo), and distinct event ids give distinct memos
 *     (up to SHA-256 collision resistance).
 *
 * Idempotency keys elsewhere (`mintedEventIds`) stay keyed by the event-id TEXT
 * and are unaffected: only the bytes handed to the ledger change.
 */

import Text "mo:base/Text";
import Sha256 "Sha256";

module {
    /// icrc1:max_memo_length of the PP ledger.
    public let MAX_MEMO_BYTES : Nat = 32;

    public func ledgerMemo(text : Text) : Blob {
        let raw = Text.encodeUtf8(text);
        if (raw.size() <= MAX_MEMO_BYTES) { raw } else { Sha256.sha256(raw) };
    };
};
