# Native SOL economics model

This document describes the local Rust reference model in
`solana/crates/economics`. It is an integer model and local test oracle; it is
not a deployed Solana program or evidence of a live transaction.

Gross deposits are split with a 4% Front-End Load. The remaining 96% is the
pot credit. Simple positions accrue `floor(P * 11 * elapsed / 8,640,000)`
lamports over 21 days on the gross principal. Accrual stops at expiry and
retains the numerator remainder between claims, so repeated claims produce the
same floor as one claim at maturity. A backwards clock observation is rejected
without changing state. Compound positions settle only at maturity with the
floor Q18 factors pinned in the Rust module. Principal is not returned by the
earnings payout model.

Because the host harness exposes state for serialization-oriented tests,
accrual validates `expires_at >= last_accrued_at` and
`remainder < 8,640,000` before doing arithmetic. Compound `Plan` values also
recompute the expected maturity from the pinned plan kind and opening time.
Malformed public state returns a dedicated error and leaves the value intact.

Carried Interest for Simple positions is 12% before day 7, 7.5% from day 7
through the second before day 14, and 3% from day 14 onward. Jackpot Fees are
9% for the 15-day compound plan and 13% for the 30-day plan. In an insolvent
settlement, `gross_owed > pot` is the strict insolvency test, and the fee is
charged on the actual gross paid from the pot. Equality is solvent. A positive
claim against an empty pot is representable with `minimum_receive = 0` and
settles to zero; a positive minimum fails atomically.

Cash and transfers use checked `u64`; cumulative owed and entitlement values
use checked `u128`; U256 intermediates protect multiplication and division.
Series A entitlement is `floor(gross * 124 / 100)`. Before accepting a game
deposit, prospective Series B entitlement is checked as
`existing_B + floor(max(deposits - payouts, 0) * 116 / 100)` in U256 and must
fit `u128`.

Admission enforces 10,000,000 lamports for game positions and 50,000,000 for
Series A. An owner gets three successful game/Series A calls in a rolling
3,600-second window. A timestamp exactly one hour old expires. Failed calls,
claims, and round progress do not consume quota. This host model carries no
dynamic 20%-of-pot or 5-SOL admission cap.

The independent fixture at `solana/fixtures/economics.json` holds the accepted
lamport vectors. Focused tests parse those constants and exercise tier
boundaries, maturity, remainder carry, strict insolvency, width limits,
minimum receive behavior, quota expiry, and failed-call atomicity.
The host helpers preserve the `remaining > 10` Backer-open cutoff while any
positive funded lamport remains claimable as dust.

The 30-day compound row uses exact integer conservation: 12,267,678,469 less
13% (1,594,798,200) is 10,672,880,269 lamports. The recommendation table's
10,672,980,269 net value is an arithmetic typo and is not used as an oracle.
