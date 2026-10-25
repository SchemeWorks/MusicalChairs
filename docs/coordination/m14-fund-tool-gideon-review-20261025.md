# M14-FUND-TOOL: Gideon's bounded review and closeout (2026-10-25)

**Reviewer:** Gideon, the Codex supervisor, delivered through the supported UI. No suite or build was duplicated.

## Bounded review of frozen candidate `d3c8270`

Gideon inspected:
- the full `fund.rs`;
- the exact shared `devnet_ops.rs` delta;
- the 32 test cases and hooks;
- the committed plan, the inventory and the bucket arithmetic;
- Silas's verification summary.

**Findings.** The scope and core guards are coherent:
- plan and inventory blob binding;
- whole-plan cap and balance checks;
- the role-plus-recipient once-only check BEFORE the balance read;
- semantic annotations;
- held and ambiguous sends stop the batch;
- shared-reader compatibility.

Program source, ELF, IDL, lock and packet are unchanged.

**FUND-R1 (medium).** A confirmed transfer whose post-send read-back contradicted the funding effect did not stop later sends.

## Delta-only closeout at `df65edff3d4cec7132324092a5e64cbce273d8e1`: FUND-R1 CLOSED

- **The fix.** Gideon inspected the actual stop path and the two hooked regressions. A confirmed transfer followed by a contradictory or failed read-back now stops later sends immediately. The accurate discrepancy is retained, the original confirmed write-ahead resolution is preserved, and a re-run cannot repay it.
- **No other changes.** Source and shared guards are unchanged beyond the narrow delta.
- **Local results accepted** at the stated proof: fund-wallets 34/34, devnet_ops_local 33/33, runner 14/14, clippy clean.
- **Scope.** The full `d3c8270` review remains valid. No further source changes are needed.

## Pins Gideon independently read

| item | value |
| --- | --- |
| binary | sha256 `d3c6696632d4a6319d99a4d02a441eec2b60854d1bc00c51c4ea18b2cd19cfd5` |
| `fund.rs` | sha256 `ecf2c3a2417da3cfb5aaa41c20f4d5b1d22353ba58971a0567a7de9575305faa` |
| funding plan | sha256 `0ae55574620b599e4dfe6ae4842b7304053978593c08d49a6d3b53b34bd20a33`, byte-equal to the blob at `edd7dd1928b628b483edf0a94b596da30c35e8f0` |
| inventory | sha256 `9c2b75522d6cfea0764f838133d9664a3c060814a1b245c4e7b15532c3d93913`, byte-equal to the blob at the same commit |
| role bindings | all 17 role/pubkey bindings match the inventory; target sum 3,830,000,000 |
| frozen packet | `ae8a6a0e5e5dbbd3545dff85ed7c32c24c34bf67235165d7f8b2048c7816392f` at `aeab470` |

## Authority for the next steps (already-approved Block A; no new human question)

1. `fund-wallets` with that exact plan commit and the existing single write-ahead file. These are internal transfers from D. The process ledger stays at received 15 + reserved 5 = 20 of 25, and the reserve is unchanged.
2. Verify each transfer, its finality and the balances, and append spend and effect receipts. Any contradiction, held record or unknown outcome means STOP, with no clearing, rebuild or unsafe retry.
3. Then the existing guarded `fund-vault` for 0.015 SOL, within the packet's 0.02 bucket.
4. Then A4: prepare and verify BOTH the `initialize` and the ORAO `Register` vault transactions through the real G, and collect 2 approvals each in one sitting. Proposer = fee payer, the sponsor signer is preserved, the stored messages are checked exactly, and the full 172,800 s wait applies, with no execution before each approval-derived deadline and no shortened or warped clock. Record the exact approval times and the earliest execution times in a durable receipt.

All ICP actions, mainnet, PP minting, the PR merge and any unknown-outcome retry remain held.
