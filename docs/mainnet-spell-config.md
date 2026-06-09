# Mainnet Shenanigan Spell Config — Snapshot

Canister: `j56tm-oaaaa-aaaac-qf34q-cai` (shenanigans)
Query: `getShenaniganConfigs : () -> async [ShenaniganConfig]`
Captured: 2026-05-27 via `dfx canister --network ic call`

| id | name              | costSuccess | costFailure | costBackfire | success | failure | backfire | duration (h) | cooldown (h) | castLimit | effectValues          |
| -- | ----------------- | ----------- | ----------- | ------------ | ------- | ------- | -------- | ------------ | ------------ | --------- | --------------------- |
| 0  | MEV Attack        | 10          | 15          | 25           | 57      | 27      | 16       | 0            | 4            | 0         | [7, 19, 250]          |
| 1  | Contagion         | 5           | 15          | 60           | 43      | 39      | 18       | 0            | 6            | 0         | [2, 5, 60]            |
| 2  | Cease & Desist    | 125         | 20          | 50           | 37      | 56      | 7        | 48           | 8            | 0         | [2]                   |
| 3  | Trailing Commission | 15        | 15          | 60           | 43      | 49      | 8        | 168          | 48           | 0         | [5, 1000]             |
| 4  | Crossline Poach   | 150         | 40          | 140          | 15      | 71      | 14       | 0            | 96           | 2         | []                    |
| 5  | Poison Pill       | 80          | 40          | 222          | 64      | 35      | 1        | 0            | 6            | 2         | []                    |
| 6  | Yield Boost       | 10          | 15          | 10           | 70      | 30      | 0        | 0            | 36           | 3         | [5, 15]               |
| 7  | Bridge Exploit    | 30          | 10          | 0            | 23      | 60      | 17       | 0            | 12           | 0         | [25, 50, 1600]        |
| 8  | Wealth Tax        | 50          | 20          | 0            | 37      | 48      | 15       | 0            | 6            | 0         | [20, 900]             |
| 9  | Override Bonus    | 60          | 20          | 100          | 48      | 42      | 10       | 0            | 36           | 2         | [1.3]                 |
| 10 | Whitelisted       | 420         | 42          | 69           | 45      | 45      | 10       | 72           | 72           | 0         | [72, 168]             |

**NOT YET ON MAINNET** (added in code 2026-05-27, will seed on next postupgrade):
- id 11 Tender Offer
- id 12 Stimulus Check
- id 13 Bear Raid
