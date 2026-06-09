# ICP ↔ Solana Integration Playbook (Motoko)

> **This guide now lives in a skill** — single source of truth. A skill must be
> self-contained to be packaged and shared, so that copy is canonical; keeping a
> full second copy here would only drift out of sync.

The complete, current field guide — the ICP ↔ Solana integration **seam** for
**Motoko** canisters (it does **not** cover Rust) — is here:

- **`docs/skills/icp-solana-motoko/SKILL.md`** — the lean orientation layer: the
  four-actor model, the at-a-glance trap index (25 rows), the build order, and the
  pre-flight checklist.
- **`docs/skills/icp-solana-motoko/references/playbook.md`** — the full detail, one
  section per trap, with Motoko + TypeScript code.

## Using it elsewhere

- **In this repo / for an agent:** the skill auto-loads from
  `docs/skills/icp-solana-motoko/` when a task touches Solana-on-ICP.
- **In another project:** install the packaged bundle `icp-solana-motoko.skill`
  (drop it into that project's skills directory), or copy the
  `docs/skills/icp-solana-motoko/` folder.
