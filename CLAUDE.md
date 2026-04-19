# Musical Chairs — repo notes for Claude

## Naming: "Exit Toll" → "Carried Interest"

The user-facing term for the withdrawal fee on Simple positions is **Carried Interest** (a deliberately VC-sleazy euphemism). The compounding-plan version is still called the **Jackpot Fee**.

Internal code identifiers still use the old name `exitToll`:

- Backend: `calculateExitToll` ([backend/main.mo](backend/main.mo))
- Frontend constants: `EXIT_TOLL_EARLY`, `EXIT_TOLL_MID`, `EXIT_TOLL_LATE` ([frontend/src/lib/gameConstants.ts](frontend/src/lib/gameConstants.ts))
- Frontend helpers: `calculateExitTollFee`, `getExitTollInfo`

These were left unrenamed deliberately to avoid backend churn for a pure naming change. **Do not rename these identifiers** without explicit instruction. Treat them as the internal name for the same concept the UI calls "Carried Interest".

When writing new user-facing strings, use **Carried Interest**. When writing new code that touches these identifiers, you can either keep using `exitToll`/`EXIT_TOLL_*` or introduce parallel `carriedInterest` names — coordinate with the user before doing the latter.

## Naming: "Cover Charge" → "Front-End Load"

The user-facing term for the entry fee skimmed off every deposit is **Front-End Load** (mutual-fund jargon — on-brand with the VC/MLM voice). This is the third name for this concept:

1. **Entry Skim** (original)
2. **Cover Charge** (renamed once)
3. **Front-End Load** (current)

Internal code identifiers still use the second name `coverCharge` / `COVER_CHARGE_RATE`:

- Backend: `coverCharge`, `payManagement` and related ([backend/main.mo](backend/main.mo))
- Frontend: `COVER_CHARGE_RATE` ([frontend/src/lib/gameConstants.ts](frontend/src/lib/gameConstants.ts)), `coverChargeData`/`coverChargeLoading` (admin wallet UI), and various comments

These were left unrenamed deliberately to avoid backend churn for a pure naming change. **Do not rename these identifiers** without explicit instruction. Treat them as the internal name for the same concept the UI calls "Front-End Load".

When writing new user-facing strings, use **Front-End Load**.
