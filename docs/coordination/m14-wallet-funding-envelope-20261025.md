# M14 Block A: wallet-funding envelope and proposed guarded funding card (2026-10-25)

For Gideon's (Codex supervisor) adjudication against the frozen packet (revision 9, `ae8a6a0e…`). Block A is already approved. No new approval from Rob is needed or requested.

## Why this is needed now

A3 is complete: the program is deployed and the G vault holds the upgrade authority (`docs/solana/evidence/2026-10-25-devnet-deploy.json`). A4 needs the G voters to pay proposal rent and fees, and the sponsor to co-sign and fund `initialize`. The flow plan later needs the keeper and the eight test wallets funded too.

`devnet_ops` has only one transfer, `fund-vault`. It pays the G vault, with a total cap of 0.05 SOL. There is no guarded transfer to the voters, sponsor, keeper, management recipient or test wallets. The coordinator will not send unguarded CLI transfers.

## The packet's envelope (sections 4b and 4c)

- **Per transfer.** One transfer between devnet keys is at most **2 SOL** (4c). Any action that would exceed a bucket cap stops; no top-up without a new approval.
- **Source.** The deployer's post-deploy remainder: **7,879,683,960 lamports** (finalized). This is verified faucet intake, so the process ledger stays at **20.0 of 25 SOL committed** (15.0 received + 5.0 reserved). No new faucet intake is needed.
- **Recipients and amounts.** Each is within its 4b bucket, and the test-wallet amounts are the flow plan's funding column:

| bucket (cap) | recipient | amount (SOL) |
| --- | --- | --- |
| Squads (0.5) | G voter 1 (proposer and fee payer for `vault_transaction_create` and proposals) | 0.20 |
| | G voters 2 and 3 | 0.05 each |
| | P voter 1 (proposer for the optional P pause rehearsal, D11) | 0.10 |
| | P voters 2 and 3 | 0.05 each |
| Sponsor (1.0) | sponsor S (initialize accounts about 0.019, first pages about 0.008, operations funding 0.5, ORAO client funding 0.05, fees) | 0.99 |
| | management recipient (so the account exists rent-exempt before any withdrawal reaches it) | 0.01 |
| Keeper (0.1) | keeper fee payer K | 0.10 |
| G vault (0.02) | via the existing guarded `fund-vault` (ORAO Client rent 1,275,080) | 0.02 |
| Test wallets (7.0) | A1 1.10, A2 0.06, P1 0.21, P2 0.11, P3 0.03, P4 0.08, P5 0.08, W 0.63 | 2.30 |
| **total** | | **3.92** |

This leaves about 3.96 SOL unused on the deployer, which is not spent without a reason recorded in the ledger. Every amount is at most 2 SOL. The largest is A1 at 1.10.

Order: the A4 participants (voters, sponsor, management recipient, G vault) come first. The keeper and test wallets come before flow step 1, which is due once `initialize` executes after the 48 h lock.

## Proposed card M14-FUND-TOOL (bounded; source change to the reviewed tool)

Add `devnet_ops fund-wallets --payer <keypair> --targets <file>`. The targets file lists `role pubkey lamports`, one per line, and is a public-key-only file covered by the existing file guards.

Rules:
- **Top-up semantics.** For each target, send `target - current_balance` only when that is above 0, reading at the seeded `minContextSlot`. Under fresh reads this is idempotent by on-chain state.
- **Per-recipient step id** `A1.fund.<role>`. The existing `refuse_repeat_of_confirmed` once-only guard applies, so a confirmed record for that role refuses a second send whatever a lagging node reports. An open or held record blocks, as now.
- **Ceilings refused, never clamped.** On devnet: a single transfer above 2 SOL is refused; each bucket total is checked against 4b (Squads 0.5, Sponsor 1.0, Keeper 0.1, test wallets 7.0); and the command total is checked against the sum of those buckets. Unknown roles are refused.
- **Reuse.** `Ctx::run_step` throughout: size check, simulate, write-ahead before send, history-searching unknown-outcome handling (held, never cleared by the tool), the genesis, packet-pin and host gates, and no default wallet.
- **Tests (LiteSVM and Hooked):**
  - cap refusals;
  - the top-up is a no-op when already funded;
  - once-only refusal when a confirmed record exists and a lagging node reports unfunded;
  - an ambiguous send ends held with zero additional sends;
  - unknown-role refusal.

  The existing `devnet_ops_local` and `m14_flow_tool` suites stay green.
- **Review.** A bounded independent review (Gideon) of the delta before any devnet use. No change to accepted program bytes, pins or the packet.

Alternatives considered:
- Rob funding about 17 addresses at the web faucet: impractical, and the faucet bars agents.
- Unguarded `solana transfer`: rejected, because it has no write-ahead and no unknown-outcome handling.
