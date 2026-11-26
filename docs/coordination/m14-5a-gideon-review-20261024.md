# M14 5a re-pin: Gideon's bounded independent review (2026-10-24)

**Target:** frozen commit `1b9517285e6a34ef6322f4a3e9066916f6ecf94f`.

**Reviewer:** Gideon, the Codex supervisor. Gideon delivered the review to Silas's chat through the supported UI. No suite or build was duplicated.

## Approval scope: RESOLVED

Gideon independently saw Rob's actual message quoting Silas's exact Block A question and answering `yyeess,, aapppprroovveedd bblloocckk aa`. The scope is Block A on devnet only, including 5a. It is resolved and is not to be asked again.

## Source and artifact delta: PASS (at this boundary)

- **Production and source delta.** Only `declare_id!` and the test constant changed, plus the generated authority-table and IDL references and the fixture updates. No economics or authority behaviour changed.
- **ELF.** Independently hashed on disk and in the durable copy: `0c88d4e7a51eb84c10cb1dd683181dc96bca48c64962dfff896f0d852a6311ef`, 1,399,208 bytes. The durable provenance was checked.
- **Pins.** Source set recomputed: `e50e83df7c7385ddd4f81630818b364f7ea755a256ae03f02603acd35b85378d`. The lock is unchanged. Both IDL copies are `d6370a73…`, with an address-only delta.
- **Addresses and fixtures.**
  - All 28 old and new PDA plus bump derivations check out.
  - All 29 manifest flow, discriminator, event, error and version identities match.
  - The old id is absent outside history.

## Docs-only coherence corrections required before final packet acceptance (applied in packet revision 9)

1. The section 2 new-ELF row pointed its durable copy at the OLD `8f97…`; "Which one is deployed" and D5 recommended the OLD `8f97…`. Now the new `0c88d4e7…`.
2. A0 named the OLD ELF, source-set and IDL hashes. Now the new pins.
3. The 3c `initialize` provider tuple named the OLD client `7b1ebe…`. Now `9k91kwTA…`.
4. 3a and 3c carried the old [R6] absence observations onto newly derived addresses. They are now labelled historical, and the new addresses' existence is unobserved until the authorized A1 target checks.
5. Further current operational references were updated the same way: section 1 row A, 3d, RG-2, RG-3, B1's event authority, section 8's `elf_len` and MI-1.
6. **Truthful-evidence fixes:**
   - The canonical source digest covers **25** files, not 19. Silas re-counted with `source-set-digest.sh --list`.
   - The old 32×`0x66` id is **on curve** (`PublicKey.isOnCurve` true; packet [L9]). "No keypair could exist" is unsupported. The corrected wording is "synthetic placeholder, no known private key", and the approved 5a stands.
   - These fixes were applied in the packet, the 5a receipt, `latest-build.json` (helper note and superseded-pin note), the approval record and the kanban.

The historical appendix and receipts are unchanged. No rebuild or test re-run was needed for prose fixes.

**Lookout:** the earlier hold on the setup handoff is resolved. Rob explicitly approved the exact setup-chat handoff, and setup completed the cutover (see the publication log).

## FINAL acceptance (Gideon, 2026-10-24)

Gideon independently reviewed exact `aeab47059477f977294cdcf084998933460a896f`, packet revision 9, sha256 `ae8a6a0e5e5dbbd3545dff85ed7c32c24c34bf67235165d7f8b2048c7816392f`:
- Every requested docs finding is closed.
- The approved Block A quote is byte-identical to revision 7.
- PR #125 checks were read independently: runs 40120791680 and 40120785977, all 4 jobs SUCCESS in each (8/8), no pending statuses. Silas re-read both runs and got the same result.

Combined with the source, artifact, PDA and fixture review above, **Gideon ACCEPTS the 5a pins and packet revision 9 locally. Gates 7 and 8 are fulfilled.** Rob's explicit authority for Block A (devnet only, including 5a) is verified in both chats.

**Silas may proceed to A1 and the bounded Block A**, preserving:
- the genesis and target checks before any key use or signing;
- the exact artifact and the COMMITTED packet blob pin;
- no default wallet;
- public-faucet-only devnet SOL, at most 25 SOL, in a WRITTEN process ledger;
- the per-command ceilings;
- the write-ahead and unknown-outcome holds;
- faithful send, finality and effect receipts.

No further approval question and no duplicate local suites or build. HELD: all ICP B and C rows, real PP minting, unknown retry, mainnet and the PR #125 merge. This acceptance is local; it implies no public deployment proof.

**Packet pin for execution:** `--packet-sha256 ae8a6a0e5e5dbbd3545dff85ed7c32c24c34bf67235165d7f8b2048c7816392f --packet-commit aeab47059477f977294cdcf084998933460a896f`. The packet file stays byte-identical to that blob throughout Block A; the tool refuses otherwise.
