# M14 Block A: wallet-funding envelope and proposed guarded funding card (2026-10-25)

For Gideon's (Codex supervisor) adjudication against the frozen packet (revision 9, `ae8a6a0e…`). Block A is already approved. No new approval from Rob is needed or requested.

## Why this is needed now

A3 is complete: the program is deployed and the G vault holds the upgrade authority (`docs/solana/evidence/2026-10-25-devnet-deploy.json`). A4 needs the G voters to pay proposal rent and fees, and the sponsor to co-sign and fund `initialize`. The flow plan later needs the keeper and the eight test wallets funded too.

`devnet_ops` has only one transfer, `fund-vault`. It pays the G vault, with a total cap of 0.05 SOL. There is no guarded transfer to the voters, sponsor, keeper, management recipient or test wallets. The coordinator will not send unguarded CLI transfers.

## The packet's envelope (sections 4b and 4c), corrected per Gideon (2026-10-25)

- **Per transfer.** One transfer between devnet keys is at most **2 SOL** (4c). Any action that would exceed a bucket cap stops; no top-up without a new approval.
- **Source.** The deployer's post-deploy remainder: **7,879,683,960 lamports** (finalized). These are internal transfers, NOT new faucet intake, so the process ledger stays at **20.0 of 25 SOL committed** (15.0 received + 5.0 reserved).
- **Bucket accounting.** Each bucket counts the history already spent against it plus the targets plus a 5,000-lamport fee per transfer, charged to the recipient's bucket.
- **Pinned plan.** The recipients are pinned to the committed D1/D3 inventory (`docs/solana/evidence/2026-10-24-devnet-a1.json`) in `docs/solana/devnet-funding-plan.json`:

| bucket (cap) | already spent | targets | fees | total |
| --- | --- | --- | --- | --- |
| Squads (0.5) | A2 creation 3,667,440 (rent 2 x 1,823,720 + fees 20,000) | G voter 1 (proposer and fee payer) 0.19; G voters 2 and 3 0.05 each; P voter 1 0.10; P voters 2 and 3 0.05 each = **0.49** | 30,000 | 493,697,440 |
| Sponsor (1.0) | 0 | sponsor S **0.98** (initialize accounts about 0.019, first pages about 0.008, operations funding 0.5, ORAO client funding 0.05, extra pages, fees); management recipient **0.01** (exists rent-exempt before any withdrawal) | 10,000 | 990,010,000 |
| Keeper (0.1) | 0 | keeper K **0.05** (the accepted flow table's exact funding) | 5,000 | 50,005,000 |
| Test wallets (7.0) | 0 | A1 1.10, A2 0.06, P1 0.21, P2 0.11, P3 0.03, P4 0.08, P5 0.08, W 0.63 = **2.30** (the accepted flow table `docs/solana/m14-flow-plan.md` section 7, exactly) | 40,000 | 2,300,040,000 |
| **plan total** | | **3.83** | 85,000 | |

- **Not in this plan:** the G vault gets 15,000,000 lamports through the existing guarded `fund-vault` (ORAO Client rent 1,275,080; with its fee it stays within the G vault bucket of 0.02). B2 watchdog and any ICP funding stay held.
- **What remains.** About 4.03 SOL stays unused on the deployer and is not spent without a reason recorded in the ledger. Every target is at most 2 SOL; the largest is A1 at 1.10.
- **Order:** the A4 participants (voters, sponsor, management recipient, G vault) come first. The keeper and test wallets come before flow step 1.

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
