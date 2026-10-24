# M14 Block A: Rob's approval record (2026-10-24)

> **Status: PENDING SCOPE CLARIFICATION. This is not yet adjudicated Block A authority.**
>
> Gideon, the Codex supervisor, independently read Rob's message "approved". The message immediately before it was the coordinator's reply finishing the Lookout presentation-metadata task. That reply ended "Block A and every Solana, ICP, retry, mainnet and merge action remain held for your separate yes", but it did not itself ask the Block A question. The copy-ready "Approve Block A on Solana devnet only?" question was in an EARLIER reply, the F3 close-out. So reading "approved" as a Block A yes is the coordinator's inference.
>
> Gideon has asked Rob one narrow clarification. Until Rob confirms:
> - no public transaction is sent, faucet requests included;
> - silence is not approval;
> - only the local 5a re-pin, build and tests continue (the packet's 5a re-pinning and local re-review need no further approval);
> - the fresh program-id key, generated locally, stays unused. If Block A is not confirmed it is simply never used.
>
> No accepted implementation or F3 work reopens.

## The approval

- **Rob's message in the coordinator chat (verbatim):** "approved"
- **Context (corrected):** the message immediately before "approved" was the coordinator's Lookout presentation-metadata reply, ending "Block A and every Solana, ICP, retry, mainnet and merge action remain held for your separate yes." The copy-ready question "Approve Block A on Solana devnet only?" (packet section 0, revision 7) was asked in the earlier F3 close-out reply.
- **Approved text:** `docs/coordination/native-solana-m14-approval-packet-20261023.md`, committed at `db401a6`, sha256 `63e7187d9c99130d36a2c70361dd227b7489e9cede4f8b5376dbe957bbdff1a4`. It was unchanged at `45825c5`, the head when the approval arrived.

Once Rob's clarification confirms the scope, this chat message is the authority. The packet identity pin that `devnet_ops` requires is only an artifact-identity check, not approval.

## Scope, as the coordinator interprets it (Rob may correct any line)

**Block A only, on Solana devnet only.** It covers exactly the packet's five items:

1. **Keys.** Fresh devnet test keys, generated and held by the coordinator for this test only. Never `~/.config/solana/id.json`, never a mainnet key.
2. **Funds.** Public-faucet devnet SOL only, at most 25 SOL in total. This is a process budget the coordinator keeps in a written ledger of every faucet request and spend. The tool enforces only its per-command ceilings: deploy at most 15 SOL, `fund-vault` total at most 0.05 SOL.
3. **Program id.** Rob did not say that anyone holds the key for `7tj9…`, so MI-1 resolves to the packet's stated default, the section 5a fresh program-id envelope. Per 5a's recommendation it is ONE fresh id for devnet and the later mainnet deploy, so the bytes proved on devnet are the bytes M15 would deploy. The fresh id is re-pinned and given the normal local re-review, including Codex's acceptance of the new pins, BEFORE any public transaction. Airdrops count as public transactions.
4. **Setup.** Deploy with `devnet_ops`, move the upgrade authority to the G vault, create the real Squads multisigs G (2-of-3, 172,800 s) and P (2-of-3, 0 s), and run `initialize` and ORAO `register` through G.
5. **Flow.** The committed Seq-B plan, about 41.463 days, which also settles MI-2. It includes the deployed-byte proof and the conservation reconciles.

**Not covered:**
- MI-5, another faucet or a funded devnet account. If the public faucet cannot supply a bucket, Block A stops and the coordinator asks.
- **Held, each needing its own approval:**
  - every ICP deployment or upgrade (B1–B4);
  - activation (C1, C2) and real PP minting (C3);
  - any retry of a held-unknown transaction (it needs an exact signature, effect and retry approval plus a review);
  - anything on mainnet;
  - merging PR #125.

## Program-id key (5a)

- Generated 2026-10-24 by the coordinator with the pinned `solana-keygen` 3.1.0 (Agave 3.1.0-beta.0).
- Public key (the new program id): **`7aSvuR19bh9WBHoPC5ceLNvP6txxFkowTJ2FSi9fnf3o`**.
- **Custody (packet D1):**
  - a 0700 directory outside every git worktree, with the file at mode 0600;
  - never committed, printed or pasted; only the public key appears anywhere.
- **Why this key matters until mainnet.** The key's only power is to sign the FIRST deploy to that address on a cluster. The program's upgrade authority then moves to the G vault. Because 5a uses the same id for the later mainnet deploy, the key must stay private until M15's first mainnet deploy. A leak before then would let someone occupy the address on mainnet and force a new id. It cannot move anyone's funds. The coordinator recommends that Rob keep an offline backup copy.

## Gates before the first public transaction (5a "normal local re-review")

1. Restore the pinned toolchain: platform-tools v1.48 (archive sha256 `799e4cfc…16d2dd`) and cargo-build-sbf 3.1.0-beta.0.
2. Change `declare_id!` and the test constants. Rebuild with `build-sbf.sh` at the same absolute worktree path, with the provenance guard.
3. Recompute the source set. Regenerate and check the IDL in both copies. Regenerate the keeper authority table (`--check`).
4. Re-derive every PDA (packet 3a) and the ORAO Client. Regenerate the seed vectors, sampling, instruction encodings, the RPC fixtures from a local-cluster run, and the bridge, keeper and frontend test fixtures. Leave the historical M0–M13 receipts unedited, and record old → new in a new receipt.
5. Run the LiteSVM suites on the new ELF; the `moc` suites for the keeper and the bridge; frontend `tsc` and `vitest`; and the `devnet_ops` and flow-runner suites. CI must be green at the new head.
6. Make a durable copy of the new ELF with `SHA256SUMS` and `PROVENANCE.txt`, and update `latest-build.json` at promotion.
7. Codex accepts the new pins in a receipt.
8. Revise the packet with the new pins. Its scope does not change; only the 5a re-pins are added. `devnet_ops` then binds to that revision.

Only after gate 8 does A1 begin: genesis check, keys, faucet.

Nothing has been executed on any public cluster.
