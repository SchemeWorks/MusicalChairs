# Demo-first deferred work (native Solana)

A canonical log of work that was deliberately not done so that working demos come first. It is not a second board, and it does not change the accepted Block A scope or its 39-step definition of done. Entries are added only when work is actually skipped, never to collect hypothetical concerns.

## Source

- **Origin.** Gideon's supervisor lane log of 2026-10-29. Silas folded it in here on 2026-10-30.
- **Rob's instruction.** Rob told Cody directly, and Gideon read that human turn: prioritize working demo features, skip nonessential extra investigation and verification, and keep an honest log to revisit before client money. The policy text lives in the delegated-vault repository (`docs/coordination/DEMO_FIRST_DELIVERY_POLICY.md`).
- **Still required.** Exact action holds, authorization, state and secret protections, the clock and maturity gates, once-only and unknown-outcome holds, immutable pins, conservation checks and enforced release checks.
- **Two separate claims.** "Demo-ready" and "client-money-ready" are different claims.

## Entries

| ID | Omitted work | Owner | Why deferred | What the demo does not show | Revisit before client money |
| --- | --- | --- | --- | --- | --- |
| MC-DEMO-001 | The optional guardian pause/unpause rehearsal (card M14-F12-OPT) | Silas; Gideon adjudicates scope | Already optional and deferred on the board. The accepted flow plan has no step for it (`docs/solana/m14-flow-plan.md` §9), and `devnet_ops` has no pause command. Building a command just to run an optional exercise would not deliver the next demo feature. | No live guardian pause/unpause, and no live proof that exits keep working during a pause. Local source and test evidence keep their original limits. | Check whether the actual release definition needs this proof. If it does, or if it is chosen explicitly, give it bounded implementation and review through the normal authority path. It is not a demo gate. Frozen packet F12 and default D11 apply. |
| MC-DEMO-002 | Repeated full green local suites, and repeat independent reviews of unchanged accepted foundations (the 5a re-pin and F3) | Gideon (review); Silas (verification) | The accepted receipts are reused. No source change or concrete new defect invalidates them, and repetition would not deliver a feature. | Reused evidence covers only the source, artifacts and conditions it originally tested. It is not a new end-to-end demo, a new deployed-byte observation, oracle-liveness proof or client-money readiness. | Check whether the relevant inputs, behavior or release criteria changed, and run only the checks that change justifies or a release requirement enforces. |

## Status notes

- **2026-10-30.** The governance waiting-period rehearsal (M14-F11) is complete under Rob's direct approval. Receipts: `docs/solana/evidence/2026-10-30-devnet-governance.json` and `m14-f11-rehearsal-20261030.md`.
  - It was never a deferral. It is the only change since the supervisor log, which recorded it as held.
- **Next demonstrable feature.** The first player withdrawal, the backer's fee share and the first backer claim: flow steps 10–12, cards M14-A7-WD1 and M14-A7-CLAIM1. The window is 2026-11-03T13:11:35Z (inclusive) to 15:37:47Z (exclusive) on the cluster clock.
- **Optional local demo improvement.** DEMO-LOCAL (queued 2026-10-30) makes the existing local click-through start with one command. It is not an acceptance gate.
