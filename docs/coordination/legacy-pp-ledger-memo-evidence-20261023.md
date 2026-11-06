# Legacy Shenanigans PP-ledger memo limit: read-only evidence (2026-10-23)

This is a separate legacy card (LEG-1 for the local fix, LEG-2 for any recovery decision). It is not a native M14 gate. The native PP path builds its own 32-byte hashed memos (`shenanigans/NativeRewards.mo`, `sha256("musicalchairs/native-reward-leg/v1" ‖ key ‖ be32 index)`), so it does not depend on this fix.

## Authorization and ownership

- **Who decided.** On 2026-10-23 Rob was offered a choice that included fixing this bug. He replied: "I would prefer you decide this sort of thing and leave me out of it. Just use your best judgment, please." The coordinator selected a **local** source/test fix. The newer Codex direction the same day narrowed it explicitly: no compensation, no ledger writes, no mainnet repair or upgrade, and recovery as a separate evidence-backed decision.
- **LEG-1 owner.** The Claude coordinator, with one Sonnet worker. Branch `fix/pp-ledger-memo-limit` (off `origin/main` f96b33f), worktree `/Users/robertripley/.codex/worktrees/pp-memo-hotfix`. Scope is the `ledgerMemo` helper, SHA-256, tests and a local-replica rehearsal.
- **LEG-2 owner.** Unassigned. It needs a separate recovery decision based on the evidence below.

## Read-only queries run by the coordinator (exact relevant output)

All were anonymous queries unless marked otherwise. No update calls were made and nothing was written.

1. **Mainnet PP ledger memo limit.** Command: `dfx canister call --network ic --identity anonymous --query 5xv2o-iiaaa-aaaac-qeclq-cai icrc1_metadata`. Output: `"icrc1:max_memo_length"; variant { Nat = 32 : nat }`. The local ledger built from `dfx.json`'s init arg reports the same value.
2. **Live missed backer mints.** Command: `dfx canister call --network ic --identity anonymous --query j56tm-oaaaa-aaaac-qf34q-cai getMissedBackerMints`. Output:
   ```
   principal "gcbfr-3yu36-ks7mt-grhik-mk2ff-3wx55-jffxr-julan-rakf4-5icoa-xqe";
   "ppLedger call failed: IC0503: Error from Canister 5xv2o-iiaaa-aaaac-qeclq-cai: Canister called `ic0.trap` with message: 'the memo field size of 80 bytes is above the allowed limit of 32 bytes'. …"
   principal "srnah-j667a-dkpvi-idh6x-gdop4-ohmb6-5umna-5nvzk-kxu4y-7dj4j-4ae";
   "ppLedger call failed: IC0503: … 'the memo field size of 79 bytes is above the allowed limit of 32 bytes'. …"
   ```
3. **Live missed game mints.** Same command with `getMissedGameMints`. Output: `(vec {})`.
4. **Live mint config.** Same command with `getMintConfig`. The output includes `signupGiftPp = 500 : nat`, `cascadeInitialBps = 1_000`, `cascadePassthroughBps = 5_000` and `observerIntervalSeconds = 21_600`.
5. **Live module and controllers.** Command: `dfx canister info --network ic j56tm-oaaaa-aaaac-qf34q-cai`. Output: module hash `0x7fa413608bdb4d1412ff21e16c92421b6949c0bf3861738919e8a649874067bc`; controllers `6pwpo-d5iaw-mfjrn-owfb3-v4oz6-72woh-pc5t2-cwn73-zrzeq-4bjeh-tqe` (local dfx identity `CharlesPonzi`) and `cpbhu-5iaaa-aaaad-aalta-cai` (CycleOps).
6. **Live Candid interface.** Command: `dfx canister metadata --network ic j56tm-… candid:service`.
7. **Live stable signature.** Command: `dfx canister metadata --network ic --identity CharlesPonzi j56tm-… motoko:stable-types`. This is a controller-only read but still read-only. It was saved for the compatibility checks, and `motoko:compiler` reports `0.16.2`.

## Deployment lineage vs served bytes

- **Lineage is inferred, not proven.** The live Candid interface and that of `main@ee9a402` ("Add Cycle Manager observatory support (#122)"; Shenanigans is unchanged on `main` since) are mutually subtypes under `didc check` in both directions. They differ only in doc comments. The live stable signature is structurally identical to a local build of `main@ee9a402` once compiler-generated type-name suffixes are removed (0 diff lines). `moc --stable-compatible live → main build` passes. Both use the same compiler, 0.16.2.
- **Served bytes are not verified.** A local `dfx build --network ic shenanigans` of `main@f96b33f` (whose `shenanigans/` equals ee9a402) hashes `13bddcc40030fee7…`. That differs from the live `7fa41360…`. So no commit is proven byte-equal to the served module. The earlier coordinator note that mainnet ran the old `deploy/winnings-burn-exitliq` lineage (7fc9475) is superseded: that lineage lacks the Cycle Manager methods the live interface exposes.
- **Source fact used for the diagnosis.** In `main`, `mintInternal` passes the event-id text unchanged as the memo (`memo = ?Text.encodeUtf8(memoText)`). Event ids: game deposits `game-<id>` (short); signup gifts `signup-<principal>` or `signup-sol-<principal>` (about 70–74 bytes); backer mints `backer-<principal>-<float>` (79–80 bytes, matching the live trap text); cascade kickups `kickup-A<rank>-<eventId>` and `kickup-residual-<eventId>` (they inherit the event id's length).

## What is proven, inferred and unproven

- **Proven live (read-only).** Two backer mints failed with the memo-length trap and are recorded in `missedBackerMints`. No game mint is recorded as failed. The ledger's limit is 32 bytes.
- **Observed locally.** In the IMPL-49h replica run, a 70-byte signup-gift memo was rejected by the real local ledger (IC0503).
- **Inferred from source, not shown live.** Signup gifts and cascade kickups with long event ids fail on the live ledger. In `main` the signup claim is written only after `#Ok`, so a failed gift is retried on the player's next qualifying game. Two missed backer mints alone do NOT prove that signup gifts were lost for every user. Claimant coverage is unknown: which principals qualified, which gifts if any ever landed, whether the gift amount or ledger limit changed over time, and the retry history. All of this needs evidence before any recovery.
- **Not done.** No compensation, no ledger write, no mainnet repair or upgrade, no re-credit method. The local fix (LEG-1) only stops new failures once it is separately approved and deployed. Recovery (LEG-2) is a separate decision covering claimant coverage, original ledger effects (a trapped mint has none), retry semantics and duplicate prevention per event id.
