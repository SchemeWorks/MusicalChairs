# Spell Tuning — Three-Way Diff: spec.md vs code defaults vs mainnet

**Captured**: 2026-05-27
**Canister**: `j56tm-oaaaa-aaaac-qf34q-cai` (shenanigans, mainnet)
**Spec source**: `spec.md` lines 198–221, 814–825
**Code-default source**: `shenanigans/main.mo` line 1469 (`defaultConfigs`) and line 1032 (`newConfigs` postupgrade)

> **Cost-model caveat.** spec.md quotes a single PP **cost** per spell (the old upfront-burn model). The code and mainnet have **three tiered costs** — `costSuccess` / `costFailure` / `costBackfire` — paid based on outcome. So spec's "120 PP" for MEV Attack is not directly comparable to "10/15/25"; treat the spec column as a historical single number and the others as the live tiered model.

## Headline divergences

1. **Spec is stale on count.** Spec asserts "exactly 11 shenanigans"; code has **14** (Tender Offer, Stimulus Check, Bear Raid were added 2026-05-27).
2. **Spec is stale on cost model.** Spec uses a single upfront cost; live system uses three-tier costs since (at least) the current code defaults.
3. **Mainnet has been retuned across the board** — only one spell (Override Bonus's effectValue `1.3x`) matches code defaults outright. Every other spell has at least one field where mainnet differs from both spec and code defaults.
4. **Mainnet is missing spells 11–13.** The postupgrade hook will seed them on next deploy; spec.md doesn't mention them at all.

---

## Per-spell diff

Legend per row:
- ✱ = mainnet diverges from BOTH spec and code default (authoritative new value)
- △ = mainnet matches code default (spec is the only one wrong)
- — = spec didn't specify this field

### id 0 — MEV Attack

| field        | spec.md            | code default    | mainnet         | status |
|--------------|--------------------|-----------------|-----------------|--------|
| cost         | 120 PP (single)    | 10/10/10        | 10/15/25        | ✱      |
| odds (S/F/B) | 60/25/15           | 60/25/15        | 57/27/16        | ✱      |
| cooldown     | —                  | 2 h             | 4 h             | ✱      |
| effectValues | 2–8% max 250       | [2, 8, 250]     | [7, 19, 250]    | ✱      |
| castLimit    | —                  | 0               | 0               | match  |

### id 1 — Contagion

| field        | spec.md            | code default    | mainnet         | status |
|--------------|--------------------|-----------------|-----------------|--------|
| cost         | 600 PP             | 20/20/20        | 5/15/60         | ✱      |
| odds         | 40/40/20           | 40/40/20        | 43/39/18        | ✱      |
| cooldown     | —                  | 12 h            | 6 h             | ✱      |
| castLimit    | 1/round            | 1               | **0** (no cap!) | ✱      |
| effectValues | 1–3% max 60        | [1, 3, 60]      | [2, 5, 60]      | ✱      |

### id 2 — Cease & Desist

| field        | spec.md            | code default    | mainnet         | status |
|--------------|--------------------|-----------------|-----------------|--------|
| cost         | 200 PP             | 10/10/10        | 125/20/50       | ✱      |
| odds         | 90/5/5             | 90/5/5          | 37/56/7         | ✱      |
| cooldown     | —                  | 24 h            | 8 h             | ✱      |
| duration     | 7 days (168 h)     | 168 h           | **48 h**        | ✱      |
| effectValues | [7] days           | [7]             | **[2]** days    | ✱      |

### id 3 — Trailing Commission

| field        | spec.md            | code default    | mainnet         | status |
|--------------|--------------------|-----------------|-----------------|--------|
| cost         | 1200 PP            | 15/15/15        | 15/15/60        | ✱      |
| odds         | 70/20/10           | 70/20/10        | 43/49/8         | ✱      |
| cooldown     | —                  | 24 h            | 48 h            | ✱      |
| duration     | 168 h              | 168 h           | 168 h           | match  |
| effectValues | 5%, max 1000       | [5, 1000]       | [5, 1000]       | match  |

### id 4 — Crossline Poach

| field        | spec.md                  | code default    | mainnet         | status |
|--------------|--------------------------|-----------------|-----------------|--------|
| cost         | 500 PP                   | 15/15/15        | 150/40/140      | ✱      |
| odds         | variable L3:30/L2:20/L1:10/F:30/BF:10 | 30/60/10 | 15/71/14    | ✱      |
| cooldown     | —                        | 8 h             | 96 h            | ✱      |
| castLimit    | 1 success/round          | 1               | 2               | ✱      |

### id 5 — Poison Pill

| field        | spec.md            | code default    | mainnet         | status |
|--------------|--------------------|-----------------|-----------------|--------|
| cost         | 200 PP             | 5/5/5           | 80/40/222       | ✱      |
| odds         | 100/0/0            | 100/0/0         | 64/35/1         | ✱      |
| cooldown     | —                  | 6 h             | 6 h             | match  |
| castLimit    | "stack up to 2"    | 2               | 2               | match  |

### id 6 — Yield Boost

| field        | spec.md            | code default    | mainnet         | status |
|--------------|--------------------|-----------------|-----------------|--------|
| cost         | 300 PP             | 10/10/10        | 10/15/10        | ✱      |
| odds         | 100/0/0            | 100/0/0         | 70/30/0         | ✱      |
| cooldown     | —                  | 24 h            | 36 h            | ✱      |
| castLimit    | 1/player           | 1               | **3**           | ✱      |
| effectValues | 5–15%              | [5, 15]         | [5, 15]         | match  |

### id 7 — Bridge Exploit

| field        | spec.md            | code default    | mainnet         | status |
|--------------|--------------------|-----------------|-----------------|--------|
| cost         | 900 PP             | 15/15/15        | 30/10/0         | ✱      |
| odds         | 20/50/30           | 20/50/30        | 23/60/17        | ✱      |
| cooldown     | —                  | 8 h             | 12 h            | ✱      |
| effectValues | 25–50%, max **800**| [25, 50, 800]   | [25, 50, **1600**] | ✱   |

### id 8 — Wealth Tax

| field        | spec.md                | code default    | mainnet         | status |
|--------------|------------------------|-----------------|-----------------|--------|
| cost         | 800 PP                 | 20/20/20        | 50/20/0         | ✱      |
| odds         | 50/30/20               | 50/30/20        | 37/48/15        | ✱      |
| cooldown     | —                      | 12 h            | 6 h             | ✱      |
| effectValues | 20%, max **300**/whale | [20, 300]       | [20, **900**]   | ✱      |

### id 9 — Override Bonus

| field        | spec.md            | code default    | mainnet         | status |
|--------------|--------------------|-----------------|-----------------|--------|
| cost         | 400 PP             | 10/10/10        | 60/20/100       | ✱      |
| odds         | 100/0/0            | 100/0/0         | 48/42/10        | ✱      |
| cooldown     | —                  | 24 h            | 36 h            | ✱      |
| castLimit    | 1/player           | 1               | 2               | ✱      |
| effectValues | 1.3x               | [1.3]           | [1.3]           | match  |

### id 10 — Whitelisted

| field        | spec.md                       | code default    | mainnet         | status |
|--------------|-------------------------------|-----------------|-----------------|--------|
| cost         | 100 (24 h) / 400 (7 d)        | 5/5/5           | 420/42/69       | ✱      |
| odds         | 100/0/0                       | 100/0/0         | 45/45/10        | ✱      |
| cooldown     | —                             | 24 h            | 72 h            | ✱      |
| duration     | 24 h or 168 h (two SKUs)      | 24 h            | **72 h**        | ✱      |
| castLimit    | 1/player                      | 1               | **0** (no cap)  | ✱      |
| effectValues | [24, 168]                     | [24, 168]       | [72, 168]       | ✱      |

### ids 11–13 — Tender Offer, Stimulus Check, Bear Raid

Not in spec.md; not yet on mainnet (added in code 2026-05-27). Will seed on next postupgrade with the code-default values.

---

## Recommended spec.md edits (for user review)

> **Source-of-truth principle:** The user tuned mainnet directly. Treat mainnet as authoritative and update spec.md to match. Code defaults are only relevant for fresh-canister bootstraps and don't need to change unless we want fresh deploys to behave like mainnet.

### Structural edits

1. **Line 199**: Change `"includes exactly 11 shenanigans"` → `"includes 14 shenanigans"` (or 11 active + 3 pending depending on whether spec should describe code or live state).
2. **Cost model**: Decide whether spec.md should adopt the three-tier (success/failure/backfire) cost language. If yes, every spell entry below needs to switch from "costs N PP" to "costs S/F/B".
3. **Add entries** for Tender Offer, Stimulus Check, Bear Raid (currently undocumented).
4. **Lines 814–825** (Shenanigans Shop card descriptions): same single-cost language; needs the same rewrite.

### Per-spell numeric updates (lines 201–221)

| line | spell              | spec says today                        | mainnet says now                                |
|------|--------------------|----------------------------------------|-------------------------------------------------|
| 201  | MEV Attack         | 120 PP, 60/25/15, 2–8% max 250         | S/F/B 10/15/25, **57/27/16**, **7–19%** max 250, cooldown 4 h |
| 203  | Contagion          | 600 PP, 40/40/20, 1–3% max 60, 1/round | S/F/B 5/15/60, **43/39/18**, **2–5%** max 60, cooldown 6 h, **no per-round cap** |
| 205  | Cease & Desist     | 200 PP, 90/5/5, 7 days                 | S/F/B 125/20/50, **37/56/7**, **2 days**, cooldown 8 h |
| 207  | Trailing Commission| 1200 PP, 70/20/10                      | S/F/B 15/15/60, **43/49/8**, cooldown 48 h     |
| 209  | Crossline Poach    | 500 PP, variable L3:30/L2:20/L1:10/F:30/BF:10, 1/round | S/F/B 150/40/140, fixed **15/71/14**, cooldown 96 h, cast limit **2** |
| 211  | Poison Pill        | 200 PP, 100/0/0, stack 2               | S/F/B 80/40/222, **64/35/1**, stack 2          |
| 213  | Yield Boost        | 300 PP, 100/0/0, 1/player              | S/F/B 10/15/10, **70/30/0**, cooldown 36 h, cast limit **3** |
| 215  | Bridge Exploit     | 900 PP, 20/50/30, 25–50% max **800**   | S/F/B 30/10/0, **23/60/17**, max **1600**, cooldown 12 h |
| 217  | Wealth Tax         | 800 PP, 50/30/20, 20% max **300**/whale| S/F/B 50/20/0, **37/48/15**, max **900**/whale, cooldown 6 h |
| 219  | Override Bonus     | 400 PP, 100/0/0, 1/player              | S/F/B 60/20/100, **48/42/10**, cooldown 36 h, cast limit **2** |
| 221  | Whitelisted        | 100 PP (24h) / 400 PP (7d), 100/0/0    | S/F/B 420/42/69, **45/45/10**, single duration **72 h**, no cast limit |

### Code-default updates (optional)

Code defaults in `shenanigans/main.mo:1469` only matter for fresh-canister bootstraps (i.e. a new dev deploy or a future canister-from-scratch). Two options:

- **Leave them** — descriptive only; mainnet is admin-tuned anyway. Current behaviour.
- **Re-seed to mainnet values** — devs spinning up a local canister get the live tuning. Requires touching all 11 entries in `defaultConfigs`.

User to decide; spec.md edits should land regardless.

---

## Notes & gotchas

- The `description` strings in mainnet still bake the **old** numeric ranges into prose (e.g. MEV Attack mainnet description: `"Sandwich-attacks the target for 7–19% of their Ponzi Points (max 250 PP)."`). These were updated when the user admin-tuned. So the live UI numbers in card text DO reflect mainnet, but the seed-default descriptions in code (`main.mo:1470`) still say `"2–8%"`. Not a mainnet bug, just code drift.
- `getShenaniganConfigs` is a public query — no auth needed, no risk of mutating.
- Mainnet snapshot saved to `docs/mainnet-spell-config.md` for reference.
