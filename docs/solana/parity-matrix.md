# Native SOL economic parity matrix

Accepted 2026-10-12; source preservation reference `f96b33ff95020458be754bfa0d7471b72a7403af`. Source is not deployed-state proof. This pass produces host reference models and local probes only.

| Feature | First-release contract | Classification |
| --- | --- | --- |
| Three plans and earnings | Gross principal; Simple 11% daily capped21d; Compound12%15d/9%30d maturity earnings; no principal refund | Preserve rates/durations/base |
| Front-End Load | floor(gross*400/10000) management; remainder pot | Preserve |
| Carried Interest / Jackpot Fee | 12% before7d,7.5% before14d,3% thereafter; compound9%/13% | Preserve rates/boundaries |
| Backer toll | Half seed; Backer half35%oldest A,45%post-senior A headroom,20%initial B membership; caps/residue/missing tier folding | Preserve source order and game |
| Integer precision | 1-lamport floors, carried Simple remainder, floor Q18 compound factors | Explicit accepted difference from legacy Float/10-lamport truncation |
| Insolvency | owed>pot strictly; cap actual gross to pot, calculate toll on actual gross; equality stays active; owner-authorized eligible zero-pot close | Preserve trigger; explicit integer partition choice |
| Network fees | Owner GUI default, separately paid; third-party fee sponsorship adds no authority | Explicit removal of fixed5,000-lamport payout deduction/silent tiny transfer success |
| Backer history | Same-owner A/B aggregation, original seniority on reopen, lifetime credits/caps survive claims, permanent existence history | Preserve rules; native sequence replaces subsecond timestamp |
| Public Series A | Enabled at fresh launch | Accepted difference from legacy default-off flag |
| Minimum/quota | Game0.01SOL,A0.05SOL; 3 successful game/A calls rolling hour, exacthour expiry | Preserve minima; explicit atomic quota tightening |
| Round promotion | Preferred losers never A/B, else all losers; all current-round positions contribute P/L includingclosed; B loss*1.16 entitlement/no cash | Preserve |
| Randomness outage | One bound draw; capture<=close+600s, expire>; skip promotion if untimely, protect cash, seedcarryonce | Accepted availability/trust refinement; no wall-clock completion guarantee |
| Processing | Public inline or bounded progress; narrower admission barrier; no exclusive keeper identity | Accepted native availability difference; exact batches remain unmeasured |
| Operating costs | Segregated sponsor storage/provider/B reserve, zero rewards; explicit bounded caller contribution defaultzero; no game-pool subsidy | Accepted infrastructure difference |
| Unsolicited SOL/recovery | Unallocated excess, no arbitrary production credit/refund/sweep/winner/reset | Explicit native restriction; legacy manual recovery remains separate |
| PP/social/desk | ICP services preserved; authenticated native event rewards and durable per-leg outbox; separate legacy desk | Separate financial/social rail, disclosed observation trust |
| GUI | Existing screens/look/feel and wallet stack; adapter changes later, accurate gross projection/source split | Preserve presentation; fix source/display drift later |

Economic vectors and independent models are implemented under `solana/crates/economics`; provider/transaction serialization probes are isolated under `solana/probes/provider-resources`. Preservation of a rule in this matrix is not proof it has passed all host/runtime tests.
