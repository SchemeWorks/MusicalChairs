# `ponzi_math` → Solana Port — Design Spec

**Date:** 2026-05-23
**Status:** Draft — thought experiment, not yet greenlit for implementation.

## Goal

Make the Musical Chairs game playable on Solana with native SOL, keeping the
existing React frontend. Users who connect Phantom play a Solana-native
flavor of the game (deposit SOL, earn SOL, withdraw SOL). Users who connect
Internet Identity continue playing the existing IC flavor unchanged.

The two flavors are **independent games** — separate pools, separate users,
separate jackpots, separate leaderboards. There is no bridge, no shared
liquidity, no cross-chain accounting.

## Non-goals

- **Bridging pools or unifying the two games.** They run side by side, fully
  independent.
- **Shenanigans / Ponzi Points on the Solana side.** PP is II-bound. Phantom
  users get the financial game only; no spells, no chat, no PP, no NFTs.
- **Deriving an Internet Identity principal from a Phantom signature.** Not
  worth the complexity. If a user wants social features, they sign in with II.
- **Stablecoin denomination.** Native SOL only. Per user direction:
  centralized fiat-backed assets are off the table for this game.
- **State migration from the IC version to the Solana version.** Solana starts
  from zero. The IC `ponzi_math` canister keeps running its own game.
- **Blackholing the Solana program in the near term.** Same upgrade-controller
  story we already have on IC — admin can patch bugs until the program is
  stable enough to renounce.

## Architecture summary

```
┌──────────────────────────────────────────────────────────────────────────┐
│  USER                                                                    │
└──────────┬───────────────────────────────────────┬───────────────────────┘
           │                                       │
           │ chooses wallet at connect time        │
           ↓                                       ↓
   ┌───────────────┐                       ┌───────────────┐
   │ Internet      │                       │ Phantom       │
   │ Identity      │                       │ (or any       │
   │               │                       │ wallet-       │
   │               │                       │ standard      │
   │               │                       │ Solana wallet)│
   └──────┬────────┘                       └───────┬───────┘
          │                                        │
          │ IC flavor (unchanged)                  │ Solana flavor (new)
          ↓                                        ↓
   ┌───────────────┐                       ┌────────────────────────────┐
   │ ponzi_math    │                       │ ponzi_math_solana          │
   │  (Motoko, IC) │                       │  (Anchor program, Solana)  │
   │               │                       │                            │
   │ existing      │                       │ - Pool PDA (holds SOL)     │
   │ canister,     │                       │ - Game PDAs                │
   │ no changes    │                       │ - Backer PDAs              │
   │               │                       │ - Config PDA               │
   │               │                       │ - Emits Anchor events      │
   └──────▲────────┘                       └────────┬───────────────────┘
          │ live state                              │ live state
          │ + history                               │ (via RPC)
          │                                         ↓
          │                                ┌────────────────────────────┐
          │                                │ ponzi_math_solana_indexer  │
          │                                │  (Motoko canister, IC)     │
          │                                │                            │
          │                                │ - timer polls Solana RPC   │
          │                                │ - HTTPS outcalls           │
          │                                │ - parses Anchor events     │
          │                                │ - stable memory storage    │
          │                                │ - serves history queries   │
          │                                │   (mirrors ponzi_math API) │
          │                                └────────────┬───────────────┘
          │                                             │
          │              history queries                │
          │                                             │
   ┌──────┴─────────────────────────────────────────────┴────────────┐
   │                    Frontend (React, unchanged components)        │
   │                                                                  │
   │              service interface (chain-aware adapter)             │
   │                                                                  │
   │   IcPonziService          │          SolanaPonziService          │
   │   (existing)              │          (new)                       │
   └──────────────────────────────────────────────────────────────────┘
```

Three pieces to build, plus an adapter refactor on the existing frontend.

## What ports vs what doesn't

| ponzi_math feature                            | Solana port?  | Notes                                                |
|-----------------------------------------------|---------------|------------------------------------------------------|
| 3 plan types (simple21Day, comp15Day, comp30) | Yes           | Same rates, same maturities                          |
| Carried interest tiers (12/7.5/3)             | Yes           | Same percentages, computed via integer math          |
| Compounding ROI (1.12^15, 1.09^30)            | Yes           | Hardcoded as u128 fixed-point constants              |
| Front-end load (4% cover charge)              | Yes           | Destination = dev treasury (not PP)                  |
| Insolvency partial-payout scaling             | Yes           | Same logic, integer math                             |
| Round-based seed reserve carry                | Yes           | 50% of toll → next round                             |
| Backer system (Series A self-funded)          | Yes           | Same 24% bonus entitlement                           |
| Series B promotion at round reset             | Yes           | RNG via slot hash sysvar (not raw_rand)              |
| Exit toll distribution (50% to seed reserve)  | Yes           | Pull-model index for backer share; see Component A   |
| Oldest Series A 35% bonus                      | **V2**        | Dropped for V1; see V2 work in Component A           |
| Per-user rate limit (3 deposits/hour)         | Yes           | Per-user RateLimit PDA                               |
| General ledger (all events on-chain)          | **No**        | Becomes Anchor events + IC indexer mirror            |
| Admin god-view queries                        | **Moved**     | Served by IC indexer canister                        |
| Concurrency locks (caller, global)            | Dropped       | Solana runtime serializes by account                 |
| Rollback bookkeeping in update funcs          | Dropped       | Atomic instructions, no manual rollback              |
| ICRC-2 approve → transfer_from                | Dropped       | User signs the transfer instruction directly         |
| ICRC-1 transfer (canister push)               | Replaced      | Direct lamport debit from PDA (program owns it)      |
| Shenanigans/PP cover-charge sweep             | **Dropped**   | No PP on Solana; cover charge → dev treasury         |
| Test admin escape hatches                     | Yes           | Same set, gated to TEST_ADMIN pubkey in Config       |
| ICRC-21 / ICRC-28 / ICRC-10                   | Dropped       | Phantom uses its own approval UX (Solana standard)   |

## Component A: `ponzi_math_solana` (Anchor program)

**Language:** Rust, Anchor framework.
**Why Anchor over Pinocchio:** faster iteration, IDL generation for client
codegen, mature event emission via `emit!()`. The CU savings from Pinocchio
aren't worth the velocity loss for V1.

### Account model (PDA layout)

```rust
// seeds = ["config"] — singleton
#[account]
pub struct Config {
    pub admin: Pubkey,                   // can update knobs, pause, etc.
    pub test_admin: Pubkey,              // test/admin escape hatches
    pub sweeper: Pubkey,                 // can sweep cover-charge vault
    pub treasury: Pubkey,                // where cover charge goes
    pub cover_charge_bps: u16,           // 400 = 4%
    pub min_deposit_lamports: u64,       // 0.1 SOL equivalent floor
    pub paused: bool,
    pub current_round_id: u32,
    pub next_game_id: u64,               // monotonic across rounds
    pub bump: u8,
}

// seeds = ["pool"] — singleton; PDA lamport balance = the pot itself
#[account]
pub struct Pool {
    pub total_principal_lamports: u128,
    pub active_games: u32,
    pub seed_reserve_lamports: u128,     // accumulates 50% of tolls
    pub round_total_deposits_lamports: u128,    // resets on round reset
    pub round_total_withdrawals_lamports: u128, // resets on round reset
    pub round_start_unix_ts: i64,
    pub round_id: u32,                   // matches Config.current_round_id
    // Pull-model toll accumulator (see Toll Distribution below):
    pub backer_reward_index_q64: u128,   // monotonically increasing index
    pub backer_total_weight: u128,       // sum of all backer weights
    pub bump: u8,
}

// seeds = ["game", game_id_le_bytes]
#[account]
pub struct Game {
    pub id: u64,
    pub player: Pubkey,
    pub plan: Plan,                      // enum
    pub amount_lamports: u64,
    pub start_unix_ts: i64,
    pub last_update_unix_ts: i64,
    pub accumulated_earnings_lamports: u64,
    pub total_withdrawn_lamports: u64,
    pub is_active: bool,
    pub round_id: u32,                   // which round this game belongs to
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq)]
pub enum Plan {
    Simple21,
    Compounding15,
    Compounding30,
}

// seeds = ["backer", owner_pubkey, backer_type_byte]
// up to 2 per user (Series A and/or Series B)
#[account]
pub struct Backer {
    pub owner: Pubkey,
    pub backer_type: BackerType,
    pub amount_lamports: u64,
    pub entitlement_lamports: u64,
    pub start_unix_ts: i64,
    pub first_deposit_unix_ts: i64,
    pub is_active: bool,
    pub weight: u128,                    // = entitlement, for the pull-model
    pub reward_index_snapshot_q64: u128, // value of Pool.backer_reward_index_q64
                                         // at last claim/credit
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq)]
pub enum BackerType { SeriesA, SeriesB }

// seeds = ["ratelimit", user_pubkey]
#[account]
pub struct RateLimit {
    pub user: Pubkey,
    pub deposits: [i64; 3],              // ring buffer of last 3 deposit ts
    pub cursor: u8,
    pub bump: u8,
}

// seeds = ["cover_vault"] — singleton; PDA lamport balance = cover-charge accrual
#[account]
pub struct CoverVault {
    pub total_swept_lamports: u128,       // stats only
    pub bump: u8,
}
```

### Instruction set

| Instruction                       | Signer        | What it does                                    |
|-----------------------------------|---------------|-------------------------------------------------|
| `initialize_config(args)`         | one-shot      | Init Config, Pool, CoverVault PDAs              |
| `create_game(plan, amount)`       | user          | system_program transfer SOL → Pool; skim 4% → CoverVault; init Game PDA; emit `Deposit` |
| `add_backer_money(amount)`        | user          | system_program transfer → Pool; init/update Backer PDA; emit `BackerDeposit` |
| `withdraw_earnings(game_id)`      | user (Game.owner) | Simple plan only. Compute earnings + tier toll. Scale for insolvency. Debit Pool. Distribute toll. Transfer net to user. Emit `Withdrawal`. |
| `settle_compounding_game(game_id)`| user (Game.owner) | Compounding plan only. Require matured. Use precomputed ROI const. Rest mirrors withdraw. |
| `claim_repayment(backer_type)`    | user (Backer.owner) | Computes accrued via reward-index delta; pays out; updates snapshot |
| `trigger_reset(promotee_optional)`| anyone        | Verifies pot ≤ 0; closes round; carries seed_reserve → pool; optionally applies Series B promotion |
| `sweep_cover_charge()`            | Config.sweeper | Transfers CoverVault lamports → Config.treasury |
| `update_config(args)`             | Config.admin  | Update knobs                                    |
| `pause()` / `unpause()`           | Config.admin  | Toggle Config.paused                            |
| `admin_force_reset(reason)`       | Config.test_admin | Stuck-state recovery                        |
| `admin_create_backdated_game(...)`| Config.test_admin | Testing matured-payout flows                |

All instructions check `!Config.paused` first (except `unpause` and admin
emergency routes).

### Fixed-point math approach

Replace every `Float` in `ponzi_math/main.mo` with integer math:

**Simple plan earnings:**

```rust
// SAFETY: u128 needed because amount_lamports * 11 * elapsed_secs can overflow u64
// for whales × long durations. Final result fits in u64.
let earnings_lamports: u64 = (
    game.amount_lamports as u128
    * 11
    * elapsed_secs as u128
    / (100 * 86_400)
) as u64;
```

**Compounding plan earnings (at maturity):** Hardcode the multiplier as Q64
fixed-point. `1.12^15 - 1 = 4.47355...` → store as
`44735503_xxxxxxxxxxxx_Q64`. Same for `1.09^30 - 1 = 12.26786...`.

```rust
// Precomputed at compile time, e.g.:
const ROI_COMP15_Q64: u128 = 0x47988B0C... ; // 4.4735 in Q64
const ROI_COMP30_Q64: u128 = 0xC44B5C... ;   // 12.2679 in Q64

let earnings_lamports: u64 = (
    (game.amount_lamports as u128 * ROI_COMP15_Q64) >> 64
) as u64;
```

**Tier-based exit toll (simple plan):** integer percentage of earnings.

```rust
let toll_bps = if elapsed_days < 7  { 1200 }       // 12.00%
               else if elapsed_days < 14 { 750 }   // 7.50%
               else { 300 };                       // 3.00%
let toll_lamports = (earnings_lamports as u128 * toll_bps / 10_000) as u64;
```

**Snapshot views** (the `adminGetActivePlansSnapshot` analog showing in-flight
earnings before maturity): NOT computed on-chain. Frontend / indexer
computes these client-side with full float precision. Read-only, no money at
stake.

### Series B random selection (no `raw_rand` equivalent)

Solana has no native VRF. Approach: **slot hash sysvar** with off-chain
candidate computation.

1. **Off-chain crank** (admin script or any user) computes the candidate set
   by scanning Game PDAs (using `getProgramAccounts` with memcmp filter on
   `is_active`) and finding underwater players who don't already have a
   Backer position.

2. **Crank submits `trigger_reset`** with one of:
   - `promotee = Some(chosen_pubkey)` plus the candidate count and a recent
     slot hash they used for the random pick.
   - `promotee = None` if nobody is underwater.

3. **Program validates:**
   - Pool actually empty (`pool.lamports() <= rent_exemption_min`).
   - If `promotee` is `Some`:
     - That player's Game PDA exists and is underwater.
     - The submitted slot hash is in the recent SlotHashes sysvar.
     - The chosen index = `hash(slot_hash || promotee_pubkey) % candidate_count`
       — or accept any valid candidate index if we don't require commit-reveal.

This is **biased toward whoever cranks the reset** — they pick which slot
hash and could iterate to find a self-favorable one. Mitigation: the prize is
small (Series B grant on their own loss = recovering 116% over the next
round if pot survives), so the incentive to game it is low. Acceptable for
V1.

Upgrade path: swap in Switchboard VRF later if the bias becomes a problem.

### Toll distribution — pull model, not push

The Motoko version iterates **all** backers on every withdrawal to credit
each one. Solana can't iterate; the caller would have to pass every Backer
PDA. Bad.

Instead, use the **MasterChef pattern** (standard in DeFi pools):

- Pool maintains `backer_reward_index_q64` (monotonically increasing).
- When a withdrawal happens and tolls are distributed:
  - 50% of toll → `pool.seed_reserve_lamports` (unchanged).
  - 50% of toll → distributed across backers by *increasing the index*:
    `pool.backer_reward_index_q64 += (toll_half * Q64) / pool.backer_total_weight`
- Each Backer PDA stores `reward_index_snapshot_q64` from when it was last
  credited.
- On `claim_repayment`:
  - Compute pending = `(pool.index - backer.snapshot) * backer.weight / Q64`
  - Pay pending lamports to user.
  - Update `backer.snapshot = pool.index`.

This replaces the 35/25/40 split with a single weight-based distribution.

**V1 decision: drop the oldest-Series-A 35% bonus.** All backers receive a
proportional share of the 50%-of-toll backer half, weighted by entitlement.
This is a deliberate simplification, not an "impossibility" — see V2 work
below for the real solution.

### V2 work — preserving the oldest-Series-A 35% bonus

The Motoko rule (oldest currently-unrepaid Series A backer gets 35% of every
toll's backer-portion) is solvable on Solana, but the implementation requires
infrastructure we're not building for V1. Three viable approaches when we
revisit:

1. **Dual-index with off-chain pointer.** Maintain two reward indexes on the
   Pool PDA:
   - `general_reward_index_q64` — accumulates 65% of the backer portion
     (25% + 40%, distributed proportionally across all backers)
   - `oldest_reward_index_q64` — accumulates 35%, claimable only by whoever
     is currently flagged as `pool.oldest_backer: Pubkey`

   When the current oldest is fully repaid (entitlement exhausted), anyone
   can call `update_oldest(new_oldest_candidate: Pubkey)`. The program
   can't iterate to verify "this is truly the oldest" — it trusts the caller.
   Anyone can challenge with a *strictly older* candidate, and the program
   verifies the timestamp comparison between two accounts that ARE passed in.
   An off-chain crank watches the system and submits updates; mild trust
   assumption (someone is watching), bounded downside (challenges are
   permissionless).

2. **Bounded backer count with Address Lookup Tables.** Cap active backers
   at ~30 and have every withdrawal pass all of them in via an ALT (Solana
   feature that compactly references up to 256 accounts). Program iterates
   the passed-in set and applies the exact Motoko 35/25/40 split. Tradeoff:
   hard cap on backer count, every withdrawal's gas grows with N.

3. **Switchboard / Pyth oracle for the "oldest" pointer.** An off-chain
   oracle publishes "current oldest" on-chain with verifiable signatures.
   Solves the trust assumption but adds a dependency and ongoing cost.

Recommendation when revisiting: approach 1 (dual-index + off-chain pointer)
— preserves the bonus, adds about a day of program work plus a small crank
script, trust assumption is mild.

### Events (Anchor `emit!`)

```rust
#[event]
pub struct Deposit {
    pub game_id: u64,
    pub player: Pubkey,
    pub plan: u8,
    pub amount_lamports: u64,
    pub cover_charge_lamports: u64,
    pub net_to_pool_lamports: u64,
    pub round_id: u32,
    pub ts: i64,
}

#[event]
pub struct Withdrawal { /* mirrors Motoko GeneralLedgerEvent.#withdrawal */ }

#[event]
pub struct Settlement { /* mirrors #settlement */ }

#[event]
pub struct TollDistribution {
    pub toll_lamports: u64,
    pub to_seed_reserve_lamports: u64,
    pub to_backers_lamports: u64,
    pub new_reward_index_q64: u128,
    pub round_id: u32,
    pub ts: i64,
}

#[event]
pub struct BackerDeposit { /* mirrors #backerDeposit */ }
#[event]
pub struct BackerRepaymentClaim { /* mirrors #backerRepaymentClaim */ }
#[event]
pub struct CoverChargeAccrued { /* mirrors */ }
#[event]
pub struct CoverChargeSwept { /* mirrors */ }
#[event]
pub struct GameReset { /* mirrors */ }
#[event]
pub struct SeriesBPromotion { /* mirrors */ }
```

Every event includes `round_id` and `ts` so the indexer can index by both.

### Security checklist (Anchor)

- [ ] Every instruction checks `Signer` correctly on user actions
- [ ] PDA derivation includes bumps stored in account; validate via
      `seeds = [...], bump = account.bump`
- [ ] No arbitrary CPI — system_program transfers only
- [ ] No floats in financial paths (lint via clippy)
- [ ] All u128 multiplications checked for overflow
- [ ] `is_active` guards on Game / Backer to prevent double-withdraw
- [ ] Slot hash verification on Series B promotion (no fake hashes)
- [ ] `Config.paused` check on every user-facing instruction
- [ ] Rent-exemption preserved on all PDAs after every operation

Run through `solana-vulnerability-scanner` skill before mainnet.

## Component B: `ponzi_math_solana_indexer` (Motoko canister on IC)

**Language:** Motoko, `persistent actor`, mo:core 2.0+.
**Why Motoko over Rust:** matches existing IC codebase, simpler upgrade
story, HTTPS outcalls well-supported via `mo:ic/Call`.

### State layout

```motoko
persistent actor PonziMathSolanaIndexer {

  // -------- bookmark for polling --------
  var lastProcessedSignature : ?Text = null;
  var lastProcessedSlot : Nat64 = 0;

  // -------- the mirrored event log (same shape as ponzi_math.generalLedger) --------
  type IndexedEvent = {
    id : Nat;
    sig : Text;                  // Solana tx signature
    slot : Nat64;
    timestamp : Int;              // unix ns, from event payload
    roundId : Nat;
    event : EventPayload;         // variant mirroring ponzi_math events
  };

  let events = Map.empty<Nat, IndexedEvent>();
  var nextEventId : Nat = 0;

  // -------- secondary indexes (mirror ponzi_math query patterns) --------
  let eventIdsBySig = Map.empty<Text, Nat>();           // dedup
  let eventIdsByRound = Map.empty<Nat, List.List<Nat>>();
  let eventIdsByPlayer = Map.empty<Text, List.List<Nat>>(); // base58 pubkey
  let eventIdsByGameId = Map.empty<Nat, List.List<Nat>>();

  // -------- latest snapshots for fast frontend reads --------
  var poolSnapshot : ?PoolSnapshot = null;
  var currentRoundId : Nat = 1;
  let gamesById = Map.empty<Nat, GameSnapshot>();   // active+closed
  let backersByOwnerType = Map.empty<Text, BackerSnapshot>(); // key = "pubkey:A" or "pubkey:B"

  // -------- config (set at deploy) --------
  let SOLANA_RPC_URL : Text = "https://mainnet.helius-rpc.com/?api-key=...";
  let PROGRAM_PUBKEY : Text = "<ponzi_math_solana program ID>";
  let POOL_PDA : Text = "<derived pool PDA>";
  var pollIntervalSeconds : Nat = 60;

  // -------- transient (re-registered on upgrade) --------
  transient var pollTimerId : ?Timer.TimerId = null;
}
```

### Polling loop

```motoko
// Fires every pollIntervalSeconds via Timer.recurringTimer<system>
public shared func poll() : async () {
  // 1. getSignaturesForAddress(PROGRAM_PUBKEY, { until: lastProcessedSignature, limit: 1000 })
  let sigs = await fetchNewSignatures();

  // 2. For each new sig (oldest first), getTransaction(sig)
  for (sig in sigs.vals()) {
    let tx = await fetchTransaction(sig);

    // 3. Parse logMessages for Anchor event payloads
    let parsedEvents = parseAnchorEvents(tx.meta.logMessages);

    // 4. Store each event in stable memory (dedup via eventIdsBySig)
    for (ev in parsedEvents.vals()) {
      indexEvent(sig, tx.slot, ev);
    };

    // 5. Apply event to snapshot state (gamesById, backersByOwnerType, etc.)
    for (ev in parsedEvents.vals()) {
      applyToSnapshot(ev);
    };

    lastProcessedSignature := ?sig;
    lastProcessedSlot := tx.slot;
  };

  // 6. Refresh pool snapshot via getAccountInfo(POOL_PDA)
  poolSnapshot := ?await fetchPoolAccount();
};
```

### HTTPS outcall details

- **Method:** POST to Helius RPC endpoint, `Content-Type: application/json`,
  JSON-RPC body
- **Transform function:** strip all response headers (cloudflare-ray, dates,
  etc.). Body is deterministic for finalized slots — same query, same answer
  across replicas
- **`max_response_bytes`:**
  - `getSignaturesForAddress`: 50_000 (1000 sigs ≈ 200 bytes each)
  - `getTransaction`: 100_000 (most txs with logs < 50KB; budget 2× for safety)
  - `getAccountInfo`: 5_000 (Pool PDA is small)
- **Idempotency:** Solana signatures are unique; dedup by sig before insert

### Catch-up logic (after downtime)

If `lastProcessedSignature` is older than 1000 transactions ago,
`getSignaturesForAddress` only returns the most recent 1000. Need
pagination:

```motoko
func catchUp(bookmark : Text) : async [Text] {
  var allNew : List.List<Text> = List.empty();
  var before : ?Text = null;
  loop {
    let batch = await fetchSignatures(before, ?bookmark, 1000);
    if (batch.size() == 0) break loop;
    allNew := List.concat(List.fromArray(batch), allNew);
    before := ?batch[batch.size() - 1];
    if (batch.size() < 1000) break loop;
  };
  // batch returns newest-first; need to process oldest-first to advance bookmark monotonically
  List.toArray(List.reverse(allNew));
};
```

**Bound:** if catch-up requires more than ~50 batches, we're in degraded
state — page through asynchronously and serve partial state in the meantime.

### Query API (mirrors `ponzi_math` Candid)

The indexer's Candid surface deliberately matches `ponzi_math/main.mo` so the
frontend service adapter can swap implementations transparently. Differences
forced by chain semantics:

- **Pubkey type:** `Text` (base58) instead of `Principal`. Caller-identity
  queries (`getUserGames()` with implicit caller) become
  `getUserGames(player : Text)` since the IC indexer doesn't know who's
  signing the Solana transactions.
- **Lamports instead of Float ICP:** all amounts are `Nat` (lamports). Front
  end formats for display.

```motoko
// Live state
public query func getPlatformStats() : async PlatformStats;
public query func getPoolBalance() : async Nat;  // lamports
public query func getGameById(gameId : Nat) : async ?GameSnapshot;
public query func getUserGames(player : Text) : async [GameSnapshot];
public query func getActiveGameCount() : async Nat;
public query func getBackerPositions() : async [BackerSnapshot];
public query func getBackerPosition(owner : Text, btype : Text) : async ?BackerSnapshot;

// History
public query func getGeneralLedger() : async [IndexedEvent];
public query func getGeneralLedgerPage(offset : Nat, limit : Nat)
  : async { entries : [IndexedEvent]; total : Nat };
public query func getEventsByRound(roundId : Nat) : async [IndexedEvent];
public query func getEventsForGame(gameId : Nat) : async [IndexedEvent];
public query func getEventsForPlayer(player : Text) : async [IndexedEvent];

// Round summaries
public query func getCurrentRoundId() : async Nat;
public query func getRoundSummaries() : async [RoundSummary];

// Admin god-view (gated by an admin allowlist set at deploy)
public query ({ caller }) func adminGetActivePlansSnapshot()
  : async [ActivePlanSnapshot];

// Indexer-specific health
public query func getIndexerHealth() : async {
  lastProcessedSignature : ?Text;
  lastProcessedSlot : Nat64;
  secondsSinceLastPoll : Nat;
  eventCount : Nat;
};
```

### Cycle economics

Costs on a 13-node subnet:

| Cost item                          | Per call (cycles)  | Notes                              |
|------------------------------------|--------------------|------------------------------------|
| Base outcall                       | 49_140_000         | Fixed                              |
| Per request byte                   | 5_200              | URL + headers + body               |
| Per max_response_byte              | 10_400             | Charged against the cap, not actual |

For our workload (poll every 60s):

- `getSignaturesForAddress` once per poll: ~110M cycles
- ~3-10 `getTransaction` per poll (during active play): ~570M cycles each
- `getAccountInfo` for Pool once per poll: ~60M cycles

**Idle estimate:** 1440 polls/day × ~200M cycles = ~290B cycles/day ≈ **$0.40/day** ($150/yr)
**Active estimate:** 1440 polls/day × ~3B cycles = ~4.3T cycles/day ≈ **$5/day** ($1.8K/yr)

Plus the Helius/Triton subscription (~$50-200/mo). Mitigations:

- Increase poll interval to 5min when no events in last hour (adaptive)
- Use batched JSON-RPC if supported by provider (single outcall, multiple tx
  fetches inside)
- Reduce `max_response_bytes` for `getTransaction` once we measure real
  worst-case sizes

### Operational

- **Cycle monitoring:** add CycleOps controller (`cpbhu-5iaaa-aaaad-aalta-cai`
  from the reference memory) so we get alerts before freezing
- **RPC provider:** Helius default (best IC outcall compatibility, batched
  JSON-RPC support). Triton or QuickNode as backups. Store endpoint in mutable
  config so we can swap without redeploying parser
- **Upgrade safety:** indexer state is in stable memory (persistent actor),
  re-register timer in init/post-upgrade hook
- **Event format versioning:** include a `schema_version: u8` field in every
  Anchor event; indexer parser handles both current and previous version
  during schema migrations

## Component C: Frontend adapter layer

### Service interface

```typescript
interface PonziService {
  // Live state
  getPoolStats(): Promise<PoolStats>;
  getUserGames(): Promise<Game[]>;
  getGameById(gameId: bigint): Promise<Game | null>;
  getActiveGameCount(): Promise<bigint>;
  getBackerPositions(): Promise<BackerPosition[]>;

  // History
  getGeneralLedgerPage(offset: bigint, limit: bigint): Promise<LedgerPage>;
  getEventsByRound(roundId: bigint): Promise<LedgerEvent[]>;
  getCurrentRoundId(): Promise<bigint>;

  // Writes
  createGame(plan: Plan, amount: bigint, isCompounding: boolean): Promise<{ ok: bigint } | { err: string }>;
  withdrawEarnings(gameId: bigint): Promise<{ ok: bigint } | { err: string }>;
  settleCompoundingGame(gameId: bigint): Promise<{ ok: bigint } | { err: string }>;
  addBackerMoney(amount: bigint): Promise<{ ok: bigint } | { err: string }>;
  claimBackerRepayment(): Promise<{ ok: bigint } | { err: string }>;

  // Admin (no-op or throws on Solana side when caller isn't admin)
  adminGetActivePlansSnapshot(): Promise<ActivePlanSnapshot[]>;
  adminGetEventsByRound(roundId: bigint): Promise<LedgerEvent[]>;

  // Chain identity (so UI can show "playing on IC" vs "playing on Solana")
  readonly chain: 'ic' | 'solana';
  readonly userId: string;  // Principal text or base58 pubkey
}
```

### Implementations

**`IcPonziService`** — wraps existing `ponzi_math` actor calls. No changes
to the actor itself; just wrap it behind the interface. Trivial refactor.

**`SolanaPonziService`** — composes two underlying clients:

```typescript
class SolanaPonziService implements PonziService {
  readonly chain = 'solana' as const;

  constructor(
    private programClient: PonziMathSolanaClient,     // @solana/kit + Codama-generated
    private indexerActor: SolanaIndexerActor,         // @icp-sdk/core actor
    private walletSigner: SolanaSigner,               // Phantom via wallet-standard
  ) {}

  get userId() { return this.walletSigner.publicKey.toBase58(); }

  // ---- live state → direct Solana RPC (fresh) ----
  async getPoolStats() { return this.programClient.fetchPool(); }
  async getUserGames() { return this.programClient.fetchGamesForPlayer(this.walletSigner.publicKey); }
  async getGameById(id) { return this.programClient.fetchGame(id); }

  // ---- history → IC indexer (lagged but rich) ----
  async getGeneralLedgerPage(offset, limit) { return this.indexerActor.getGeneralLedgerPage(offset, limit); }
  async getEventsByRound(rid) { return this.indexerActor.getEventsByRound(rid); }

  // ---- writes → Solana program, user signs in Phantom ----
  async createGame(plan, amount, _isCompounding) {
    // _isCompounding ignored; plan encodes it
    return this.programClient.createGame({ plan, amount }, this.walletSigner);
  }
  // ... etc
}
```

### Wallet router

At app init / wallet-connect time:

```typescript
function makePonziService(connection: WalletConnection): PonziService {
  switch (connection.kind) {
    case 'internet-identity':
      return new IcPonziService(connection.icAgent, PONZI_MATH_CANISTER_ID);
    case 'phantom':
    case 'solflare':
    case 'backpack':
      return new SolanaPonziService(
        new PonziMathSolanaClient(SOLANA_RPC_URL, PROGRAM_ID),
        new SolanaIndexerActor(connection.icAgent, INDEXER_CANISTER_ID),
        connection.signer,
      );
  }
}
```

The `SolanaPonziService` still needs an IC agent for the indexer canister.
**It does NOT need II login** — the indexer queries are anonymous read-only.

### Component refactor scope

Most components shouldn't change. They consume `usePonziService()` which
returns the active implementation. A few specific changes:

- **Wallet connect modal:** add Phantom option alongside II
- **Header chip (Charles):** show pubkey-as-short-base58 for Solana users
  (`A1b2..3C4d`) instead of Principal
- **Amount formatting:** unify into a helper that takes lamports OR e8s with
  a chain-aware decimals constant
- **Shenanigans tabs / PP UI:** hidden when `service.chain === 'solana'`
- **"Play on the other chain" CTA:** new component encouraging cross-promo

## Cost model

| Bucket                                | Setup    | Ongoing            |
|---------------------------------------|----------|--------------------|
| Solana program audit                  | $10-30K  | -                  |
| Solana program deploy (rent + cycles) | ~$50     | -                  |
| Helius (or equiv) RPC                 | -        | $50-200/mo         |
| Indexer canister cycles               | -        | $150-2K/yr (load-dep)  |
| Cover-charge treasury setup           | -        | manual sweep gas   |
| Frontend dev (adapter + wallet mgmt)  | 1-2 wks  | minor              |

Hidden cost: **two codebases now move in lockstep** for any economic change.
Bugfixes / parameter tweaks / new plans must be implemented twice. Plan for
a feature-freeze window after launch to let both stabilize.

## Phasing

### Phase 0: Decide to do this

Confirmed it's worth the operational burden. Defer if the IC version still
has growth runway untapped.

### Phase 1: Solana program standalone (4-6 wks)

- Anchor scaffold + PDA structs + Config init
- `create_game`, `withdraw_earnings`, `settle_compounding_game` (simple path)
- Fixed-point math + LiteSVM unit tests against IC version's expected outputs
- Cover charge accrual + sweep
- Insolvency partial-payout scaling
- `trigger_reset` (without Series B for now)
- Anchor `emit!` for every event
- Surfpool integration tests against a forked mainnet state
- **Deliverable:** program deployed to devnet, deposit→earn→withdraw works
  with real SOL

### Phase 2: Backer system + Series B (2 wks)

- `add_backer_money`, `claim_repayment` with pull-model toll distribution
- Series B promotion logic + slot-hash random selection
- Off-chain crank script for the underwater-player scan
- Round-reset flow with Series B grant
- **Deliverable:** full game economy working on devnet

### Phase 3: IC indexer (1-2 wks)

- Motoko persistent actor scaffold
- HTTPS outcall + transform function + Helius setup
- Anchor event parser (base64 decode of `Program data:` lines)
- Stable memory layout + secondary indexes
- Mirror query API
- Catch-up logic + health endpoint
- CycleOps monitoring
- **Deliverable:** indexer canister deployed to IC mainnet, mirroring devnet
  program events

### Phase 4: Frontend adapter (1-2 wks)

- Extract `PonziService` interface from existing IC-coupled code
- Implement `SolanaPonziService` against devnet program + IC indexer
- Wallet connect modal: Phantom integration via wallet-standard
- Component refactors (Charles chip, amount formatting, shenanigans-hide)
- **Deliverable:** local frontend can play on devnet with Phantom

### Phase 5: Audit + mainnet (3-4 wks)

- Solana audit (use `solana-vulnerability-scanner` skill in-house first, then
  external auditor for $10-30K)
- Bug bash on devnet with internal users
- Deploy program to Solana mainnet
- Indexer cuts over from devnet to mainnet endpoint
- **Deliverable:** live with real money

**Total: 11-16 weeks** for a senior Solana dev + your existing IC capacity.

## Open questions

1. **Cover charge destination.** Plan says "dev treasury." Specifically
   which pubkey? Multisig? Same admin as IC version's `payManagement` target?
2. **Decimals.** SOL has 9 decimals, ICP has 8. The frontend currently
   handles 8 decimals via `formatICP`. New helper or fork?
3. **Minimum deposit.** ICP version is 0.1 ICP. SOL equivalent —
   0.1 SOL? Or pegged to USD? (Probably pin to lamports number for
   determinism: 100_000_000 lamports = 0.1 SOL.)
4. **Round-reset trigger.** On IC anyone interacting can trigger it (it
   happens inside `withdrawEarnings` when pot empties). On Solana, the
   slot-hash + off-chain candidate set means we probably want a separate
   `trigger_reset` instruction. **Who pays the gas for it?** Whoever cranks.
   Could incentivize with a small bounty paid from seed reserve.
5. **RPC provider lock-in.** Helius pricing tier vs Triton vs running our
   own. Probably Helius for V1.
6. **Indexer upgrade story.** When the program's event schema changes, we
   need to coordinate upgrades. Define the version-bump protocol now.
7. **Decimal rounding policy.** The IC version uses `roundToEightDecimals`
   at several points. On Solana with integer lamports, we never lose
   precision, but the outputs will be slightly different from the IC
   version's float-rounded values. Test parity carefully or accept the
   divergence.

## Deferred to V2

These are intentional V1 simplifications we want to revisit before
positioning the Solana flavor as feature-complete with the IC one:

- **Oldest Series A 35% bonus.** See Component A → V2 work for the
  implementation plan (dual-index with off-chain pointer). The mechanism
  exists, we're just not building it for V1.
- **Switchboard VRF for Series B selection.** V1 uses slot-hash RNG, which is
  cheap but gameable by whoever cranks the reset. Swap to Switchboard when
  the bias becomes a real problem (instruction-level surgery, no other
  changes needed).
- **Adaptive polling cadence on the indexer.** V1 polls every 60s flat;
  V2 should ramp down to every 5min during idle hours to cut cycle costs.

## Risks

- **Math parity drift.** Float → fixed-point translation can introduce tiny
  rounding differences. If a savvy user notices, they'll claim the Solana
  version is "cheating." Mitigation: publish a parity test suite that runs
  the same inputs through both implementations and diffs.
- **Indexer becomes a single point of failure.** Frontend depends on it for
  history. If indexer is down/lagging, users see "no events" tab. Mitigation:
  health endpoint surfaced in UI ("Indexer lag: 3 minutes"); fallback to
  direct RPC for recent transactions.
- **Cycle starvation in attack scenarios.** Adversary spams the program with
  tiny txs to inflate indexer outcall costs. Mitigation: rate-limit at the
  Solana program level (min deposit), and add a cycles-protection threshold
  on the indexer (stop polling if cycles below 1T).
- **Solana program bug requires admin intervention before blackholing.**
  Same risk we have on IC. Mitigation: same admin escape hatches, same
  caution before renouncing upgrade authority.
- **RPC provider rate-limit changes.** Helius / Triton can change pricing or
  rate caps. Mitigation: keep parser provider-agnostic; store endpoint in
  mutable indexer config.
- **Slot-hash RNG bias gets exploited.** Series B picker biased toward
  whoever cranks reset. Mitigation: switch to Switchboard VRF if it becomes
  a real problem (instruction-level swap, not architectural).
- **Cross-chain UX confusion.** Users connect Phantom, see "pool: 50 SOL,"
  switch to II, see "pool: 200 ICP," wonder why the pool changed. Mitigation:
  prominent UI labeling of which chain is active; "this is a separate game"
  copy.

## Out of scope (explicit)

- Bridging or unifying the two games' liquidity
- Ponzi Points, shenanigans, NFT referral cascade on Solana
- Multi-token support (any non-SOL token)
- Cross-chain leaderboards
- Migrating existing IC users to Solana
- Closing the IC version
- Mobile-app Phantom integration (web only for V1)

## Appendix A: relevant skills

When implementing, load these:

- **`solana-dev`** — overall Solana playbook, framework-kit, Anchor patterns
- **`solana-vulnerability-scanner`** — pre-audit pass on the Anchor program
- **`https-outcalls`** — IC indexer canister design
- **`stable-memory`** — indexer state persistence
- **`motoko`** — indexer canister language pitfalls
- **`cycles-management`** — indexer ops monitoring
- **`rust-best-practices`** — Anchor program code quality
- **`canister-security`** — IC-side hardening of the indexer

## Appendix B: glossary

For future-you or any contractor coming in cold. None of these are
Musical-Chairs-specific; they're standard Solana / DeFi terminology that
shows up throughout this spec.

- **Account (Solana)** — a chunk of on-chain storage, identified by an
  address. Every piece of state lives in an account. The program itself is
  one account (the executable); each user's data is another (a PDA).

- **PDA (Program Derived Address)** — an account whose address was generated
  by hashing a program ID + arbitrary "seeds" instead of from a keypair. No
  private key exists for it. Only the program that derived it can authorize
  changes to it. This is how programs "own" state.

- **Seeds** — the arbitrary bytes mixed with the program ID to derive a PDA.
  Naming pattern: `seeds = ["pool"]` (singleton), `seeds = ["position",
  user_pubkey]` (one per user), etc.

- **Lamport** — the smallest unit of SOL. 1 SOL = 1,000,000,000 lamports
  (10⁹). Analogous to e8s for ICP (but 9 decimals instead of 8).

- **Rent / rent-exemption** — every Solana account must hold a minimum
  lamport balance proportional to its byte size, or it gets garbage-collected
  by the runtime. "Rent-exempt" = holds enough to never be collected.
  ~0.002 SOL for a small account. Refunded when the account is closed.

- **CPI (Cross-Program Invocation)** — one Solana program calling another
  in the same transaction. The standard way to interact with the System
  program (SOL transfers), the SPL Token program (token transfers), etc.

- **System program** — the built-in Solana program that handles SOL transfers
  and account creation. You CPI into it for `system_program::transfer`.

- **SPL Token program** — the standard Solana Program Library program that
  manages fungible tokens (USDC, etc.). Irrelevant to us since we're using
  native SOL, not SPL tokens.

- **Anchor** — the standard Rust framework for writing Solana programs.
  Provides macros (`#[program]`, `#[account]`, `#[derive(Accounts)]`),
  automatic deserialization, IDL generation. The fast-iteration choice.

- **Pinocchio** — alternative low-level Solana framework. Zero dependencies,
  better compute-unit efficiency, more manual. The performance choice. We're
  not using it (Anchor is sufficient for V1).

- **IDL (Interface Definition Language)** — JSON file describing a program's
  instructions and account types. Anchor generates it from the Rust source.
  Frontends/clients consume it for type-safe interaction.

- **Compute Units (CU)** — Solana's gas equivalent. Each instruction has a
  CU budget; if your code exceeds it, the transaction fails. Floating-point
  math, loops, and CPIs eat CUs.

- **Sysvar** — special read-only accounts the Solana runtime maintains.
  Examples: `Clock` (current timestamp), `SlotHashes` (last 512 slot hashes),
  `Rent` (current rent rates).

- **Slot** — Solana's time unit. ~400ms. Each slot has a block produced in
  it with a hash (the "slot hash") that's effectively unpredictable in
  advance to anyone who isn't a validator.

- **Crank / cranking** — DeFi slang for "calling a function that needs to be
  called periodically but isn't user-initiated." Example: on IC, the round
  reset happens automatically inside `withdrawEarnings` when the pot empties.
  On Solana, it's a separate `trigger_reset` instruction someone has to call
  — that act of calling it is "cranking the reset."

- **Push model** (rewards distribution) — when an event happens that owes
  many parties money, the program immediately credits each of them in the
  same transaction. Requires iterating all recipients. Doesn't work well on
  Solana past a small N.

- **Pull model** (rewards distribution) — when an event happens, the program
  just records a single number capturing what's owed. Each recipient pulls
  their share themselves later via a claim transaction. No iteration needed
  at distribution time.

- **Reward index pattern** (a.k.a. MasterChef pattern) — the standard pull-
  model implementation. Maintain one global "cumulative rewards per unit of
  stake, ever" counter. Each recipient stores a snapshot of that counter at
  their last claim. Pending = `(current_counter - snapshot) × weight`.
  Invented in SushiSwap's MasterChef contract; copied by basically every
  yield-farming protocol since.

- **Q64 fixed-point** — integer representation of a fractional number with
  64 implied decimal places (binary). The number `1.0` is stored as `2^64`.
  Used so we can do fractional math (like "0.0125 lamports per unit weight")
  with integer ops only — Solana programs avoid floats.

- **RNG / VRF** — Random Number Generator / Verifiable Random Function. A
  VRF is an RNG whose output is unpredictable in advance AND cryptographically
  provable to have come from the genuine random process. Solana has no native
  RNG; common stand-ins are slot hashes (cheap, gameable) and Switchboard or
  Pyth VRF (paid, secure).

- **Switchboard** — Solana oracle network that, among other things, provides
  VRF as a service. ~$0.10 per random number request.

- **RPC (Remote Procedure Call), RPC provider** — Solana doesn't have a
  built-in way to query state from off-chain code. You hit an RPC endpoint
  (a server running a Solana node + JSON-RPC API) to read accounts, send
  transactions, fetch transaction history. Helius, Triton, QuickNode are the
  major commercial providers; `api.mainnet-beta.solana.com` is the rate-
  limited public one.

- **`getProgramAccounts`** — an RPC method that returns ALL accounts owned by
  a given program, with optional filters (e.g. "where byte 8-40 equals this
  pubkey"). The standard way for a frontend or off-chain tool to "iterate
  all accounts of type X" — slow (~1s) but works without an indexer.

- **`getSignaturesForAddress`** — an RPC method that returns recent
  transaction signatures touching a given account. Used by our indexer to
  paginate new transactions to process.

- **`emit!()`** — Anchor macro that logs a structured event to the
  transaction's logs. Indexers read these logs to reconstruct what happened.
  The Solana analog of ponzi_math's `recordLedger` calls.

- **Indexer** — off-chain service that ingests on-chain events (program logs,
  account changes) and stores them in a database for fast querying. Standard
  Solana pattern because the chain itself only retains ~2 days of
  transaction history. Our indexer is an IC canister doing HTTPS outcalls,
  rather than the usual Postgres + cron setup.

- **MEV (Maximal Extractable Value)** — value that block producers can
  extract by choosing which transactions to include, in what order. The
  general class of attack that our slot-hash RNG is vulnerable to (in
  miniature). Not Solana-specific; bigger problem on Ethereum.

- **Wallet Standard** — the cross-wallet interop standard on Solana. Lets
  any wallet (Phantom, Solflare, Backpack, etc.) be detected and connected
  via the same API surface. Our frontend uses this rather than wiring each
  wallet individually.

- **Address Lookup Table (ALT)** — Solana feature that lets a transaction
  reference up to 256 accounts compactly via a lookup table account. Used
  to work around the ~64-account transaction limit. Relevant to the V2
  oldest-bonus implementation (if we go the bounded-backer route).
