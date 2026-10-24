# Recovery evidence note: PP-ledger 32-byte memo defect (read-only)

Status: evidence only. This change set contains the memo fix (`ledgerMemo`, `Sha256.mo`, tests, rehearsal) and
**no** re-credit, backfill, compensation or retry method or state. Nothing here pays anyone. Every statement
about the canister's code is read from `shenanigans/main.mo` at this branch (the observer and mint code are
unchanged by the fix apart from the memo bytes); statements marked **[rehearsal]** were reproduced on the local
replica with the pre-fix wasm (`receipt-2026-10-23.json`, check ids in brackets); statements marked **[live]**
come from the coordinator's mainnet observation in the brief and are not re-verified here.

## 1. What the canister records, per failure class

| Failure class | Recorded in state? | Exactly what |
|---|---|---|
| Backer delta mint (ICP) fails 10 ticks in a row | Yes, one map | `missedBackerMints : principal -> Text` (stable). Key = backer principal; value = the **last** error text only (`ppLedger call failed: IC0503: ... memo field size of 79\|80 bytes is above the allowed limit of 32 bytes ...`). No amount, no event id, no timestamp, no attempt count, no denomination field. A later failed episode for the same owner overwrites the earlier text (`principalMap.put`). Exposed by `getMissedBackerMints()`; cleared only by the admin call `clearMissedBackerMint(owner)`. [rehearsal P1-3] |
| Backer delta mint (SOL) fails 10 ticks | Yes, separate map | `missedSolBackerMints` has the same shape but **no public getter**; only its size is visible as the `timer:observer_missed_sol_backer_mints:cycles` gauge in `cycle_manager_metrics()`. |
| Backer give-up side effect | Yes, and it destroys information | At give-up the observer sets `backerSeen[owner] := backer.amount` (cumulative amount in the failed event id) so the delta is never re-presented. The previous `backerSeen` value (the baseline) is overwritten. `backerSeen` values have no getter (only `backerSeenCount`). |
| Signup gift mint fails | **Only `Debug.print`** | `Signup-gift mint failed for signup-<principal>: <err>` (canister log, bounded retention). No map, no counter, no retry limit. The gift is unclaimed because `signupGiftClaimed[player]` is written only after `#Ok`. `signupAnnouncedSet[player]` (set before the gift attempt) is the only state trace that the player was observed, and it has no getter. Visible indirectly: the `#signup` chat item (`getRecentChatItems`, 500-item bounded buffer) and the absence of a gift mint on the ledger. [rehearsal P1-2] |
| Game mint fails 10 ticks | Yes | `missedGameMints : nat -> Text` (game id -> error). **[live]** it is empty: game event ids (`game-N`, `game-sol-N`) are short. |
| Cascade leg / residual / siphon / kickup mint fails | **No** | `distributeDeductiveCascade` swallows a failed leg (`case (#Err(_))`, nothing recorded, `remaining` is left to flow to the residual); the residual mint is `let _ = await ...` (result discarded); `distributeCascadeFromChips` swallows with `catch (_)`. Only a failed *siphon* mint prints (`siphon mint failed for siphon-<id>`). |
| `mintedEventIds` on a failed mint | Not written | `mintWithEffects` writes `mintedEventIds[eventId]` only in the `#Ok` arm, so a failed event id stays mintable. [rehearsal P4-1 shows the original gift id committing later] |
| Other side effects of a failed mint | None | `mintInternal` updates `knownPpHolders` and `ppMintedPerPlayerPerRound` only on `#Ok`; `lastQualifyingDeposit` is set only after a successful mint. |

### Cascade kickups for long event ids (class c)

Not recorded anywhere, and, more importantly, **never attempted**. Every cascade call sits inside the `#Ok` arm
of the primary mint (`distributeDeductiveCascade(...)` after `mintWithEffects(...)` in `processNewGames` for both
the gift and the game, and in `processBackerDeltas`). The long-id primaries (`signup-*`, `backer-*`) never
returned `#Ok`, so their `cascade-A<rank>-<id>` and `cascade-residual-<id>` legs were never started. Nothing was
minted for the gift or backer **including the 10 % cascade share** (gift at the default `signupGiftPp = 500`: 450 PP
net to the player and 50 PP to the cascade; the whole 500 PP is unminted). The chips-deposit kickups
(`kickup-A<rank>-chipdep-<idx>`, `kickup-residual-chipdep-<idx>`) use the short ledger index and stay under 32
bytes until `idx` has 8+ digits. The only other long-id derivatives are the `cascade-*` and `siphon-*` memos of
gift/backer events, which exist only after a primary success. The reconstruction the brief warns against is not
needed and is not attempted.

## 2. Original ledger effects

**None** for a trapped mint. The ledger traps (`ic0.trap`, reported to the caller as IC0503) before it creates a
block, so no block, no balance change and no supply change exist for the failed gift/backer/cascade ids.
[rehearsal P1-4, P1-5: balances unchanged, no ledger block with a `signup-`/`backer-` memo, none above 32 B.]
Shenanigans caught the reject (`try/catch` in `mintInternal`), returned `#Err`, and the observer carried on. The
same-tick short-memo mints did commit (`game-N`, `cascade-residual-game-N`; [rehearsal P1-6]).

Event-id shapes (ICP; SOL adds `sol-`): gift `signup-<63 chars>` = 70 B (`signup-sol-` = 74 B); backer
`backer-<63 chars>-<amount>` where `<amount>` is `Float.toText` of the cumulative amount, rendered by this wasm
with **fixed 6 decimals** (observed: `5.0` -> `5.000000`), so 8 chars for 1-9 ICP and 9 chars for 10-99 ICP,
i.e. the live 79 / 80 B errors. The memo size in a `missedBackerMints` text therefore reveals only the digit
count of the integer part of the amount, not the amount.

## 3. Does the observer re-present the same game/backer later, and with which id?

* **Game**: no. `gameIdCursor` advances after the game's own (short-memo) mint, independent of the gift outcome.
* **Signup gift**: the gift is attempted again, with the **same event id**, at the player's **next qualifying game
  record** the observer processes (`signup-<P>` for an ICP game, `signup-sol-<P>` for a SOL game; the claim is
  per principal, ICP games process first within a tick). It is attempted on every such game until it commits.
  With the memo fix that retry commits, so players who play again self-heal with no new code.
  [rehearsal P4-1: P1's lost gift committed on P1's next game under `signup-<P1>`, memo = sha256 of it.] A player who
  never plays again is never retried.
* **Backer**: no. After give-up `backerSeen == backer.amount`, so only a **top-up** produces a new delta, with a
  **new** event id (`backer-<P>-<new cumulative amount>`) and **only** `new - amountAtGiveUp`; the lost portion is
  never re-presented. [rehearsal P4-2] The `missedBackerMints` entry is not cleared or consulted by the observer
  [rehearsal P4-3]; B2-style owners with no top-up stay at zero [P4-4].
* After the fix, the earlier recorded failures are untouched by the upgrade [rehearsal P2-2, P3-7].

## 4. What a correct recovery would have to prove (not done here)

1. **Claimant coverage.**
   * Gift: every principal with a game record on ponzi_math (ICP) / ponzi_math_sol (SOL) since `bootstrapped`,
     excluding grandfathered pre-cutover players (`seedMigrationV2` wrote their `signupGiftClaimed`; they never
     got a gift by design) and excluding players the observer already healed or will heal on their next game.
     `signupGiftClaimed` and `signupAnnouncedSet` have no getters, so claimed-vs-unclaimed must be proven from
     the ledger (no 45_000_000_000-unit gift mint into `chip(player)`; pre-fix none can exist) plus the games list.
   * Backer: the lost amount per owner is `amountAtLastGiveUp - baselineBeforeFirstFailure`, summed over every
     failed episode, while the map holds one error per owner. The baseline is unrecorded: with a 32-byte limit no
     `backer-*` mint can ever have committed, so a non-zero baseline can only come from `primeObserverCursors`
     (which writes `backerSeen` without minting). It must be established independently (priming date vs the
     backer's `startTime`/`firstDepositDate`, ledger history, logs) before any amount is derived.
   * SOL backers: `missedSolBackerMints` cannot be listed (no getter, count only).
2. **No duplicate mint per event id.** `mintedEventIds` is the only dedup: ledger-level dedup never fires because
   every call stamps `created_at_time = now`. It is written after the ledger commit and read before the await, so
   two in-flight mints of one id (an admin action racing an observer tick) can both pass the check. The only lock
   is the transient `observerRunning` flag. There is also the commit-then-trap window the code comment notes.
3. **Retry semantics.** Which event id and memo the recovery mint uses. Using the original id through
   `mintWithEffects` keeps the observer's later natural gift retry a no-op (dedup returns `#Ok(0)`, then the claim
   is written and the cascade, whose leg ids dedup individually, is a no-op), but a recovery that mints under a
   different id double-pays any player who then plays again (the observer would still find the gift unclaimed).
4. **Amounts and context at attempt time.** The observer uses the *current* `mintConfig` (rates, `signupGiftPp`,
   cascade bps) and the *current* referral chain and `isActive` snapshot; a late recovery will differ from what
   the on-time mint would have paid. A failed backer was also never marked active (`lastQualifyingDeposit` is set
   only after a successful mint), so recovery changes cascade routing for that backer's downline.
5. **Active mint-siphon.** In `mintWithEffects` the `mintSiphons.siphonedSoFar` cap is advanced *before* the
   primary mint and the siphon mint is attempted *even when the primary returned `#Err`*. A victim with an active
   siphon during the failed attempts may have had cap consumed (up to 10 times per backer) with no PP delivered
   (the long `siphon-<id>` memo also trapped). Pre-existing; recovery would need to show it is absent or account
   for it.
6. **Leaderboard bucket.** `ppMintedPerPlayerPerRound` uses `cachedCurrentRoundId` at mint time, so a late mint
   lands in the then-current round.

## 5. Read-only queries that could supply the evidence on the live canister

Shenanigans `j56tm-oaaaa-aaaac-qf34q-cai`:
* `getMissedBackerMints()` (owners + last error), `getMissedGameMints()`, `getObserverStatus()` (cursors,
  `backerSeenCount`, `missedBackerMintsCount`, `intervalSeconds`), `isBootstrapped()`, `getMintConfig()`
  (`signupGiftPp`, rates, cascade bps, activity flags), `cycle_manager_metrics()` (gauges: minted-event-id count,
  missed ICP/SOL backer counts), `getKnownPpHolders()` (principals with at least one successful mint),
  `getRecentChatItems(500)` (`#signup` and `#roundResult` items: who was observed, which games minted),
  `getReferrer` / `getReferralStats(user)` (chain; `recentSignups.joinedAt`).
* Canister logs (read-only, controllers): the only record of failed gift event ids
  (`Signup-gift mint failed for ...`), per-attempt backer event ids, and `Giving up on backer mint for <owner> at
  amount <A> after 10 attempts`. Subject to log-buffer retention; per project notes the observer poll was slowed
  to 6 h on 2026-09-04 (not re-verified here), so entries would be sparse.

PP ledger `5xv2o-iiaaa-aaaac-qeclq-cai`:
* `icrc1_balance_of({owner = shenanigans; subaccount = chip(player)})`, and `get_blocks` / `icrc3_get_blocks` over
  the whole chain to prove (a) no mint with a `signup-`/`backer-` memo exists, (b) the exact mints each claimant
  did receive (game nets, cascade shares), (c) the absence of a gift-sized mint into `chip(player)`.
  `chip(p)` = the principal's raw bytes right-padded with zeros to 32 bytes.

ponzi_math `guy42-yqaaa-aaaaj-qr5pq-cai` / ponzi_math_sol `spc6q-xyaaa-aaaac-qg2ma-cai`:
* `getAllGames()` (claimant universe: player, id, amount, plan, startTime) and `getBackerPositions()` (owner,
  current amount, startTime, firstDepositDate).

Not obtainable read-only: `signupGiftClaimed`, `signupAnnouncedSet`, `backerSeen` values, `mintedEventIds`
contents, `lastQualifyingDeposit`, `mintSiphons`, `missedSolBackerMints` entries.
