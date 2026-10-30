# Solana skills assessment

Date: 2026-10-12. Initial assessment was advisory; Rob subsequently authorized the Foundation refresh. The verified refresh receipt below supersedes the local-version observations. No product dependencies or MCP configuration changed.

## Solana Foundation: refresh the existing skill

The local `/Users/robertripley/.codex/skills/solana-dev` resolves to `/Users/robertripley/.agents/skills/solana-dev`; its entrypoint still describes January 2026 framework-kit guidance. Local entrypoint sha256: `8075a16ed4dbd2a99e9d7c6a253b5bfb65c105e87c9f75b2857b4019a519a074`.

Current [Foundation repository](https://github.com/solana-foundation/solana-dev-skill) was inspected at `491a19a4f69e1e80cdf0f8fa476066d726894138`, skill version 2.6.0. Its [entrypoint](https://github.com/solana-foundation/solana-dev-skill/blob/491a19a4f69e1e80cdf0f8fa476066d726894138/skills/solana-dev/SKILL.md) and references cover modern client/transaction tooling, program design, security, fuzzing and integration testing, and differ materially from the local defaults. Recommendation: **update this existing skill rather than install another copy**, after an explicit update request.

Use its program/testing/security guidance as a reviewed reference now. Preserve Musical Chairs' accepted GUI scope; upstream SDK defaults do not authorize a frontend overhaul or automatic dependency migration. Pin compatible program/client/provider versions through actual compatibility checks, particularly where provider examples target older Anchor APIs. Skill maintenance is useful groundwork, not a gate blocking the requested design work.

Any later refresh should preserve the symlink relationship and intentional local additions, review changes first, pin provenance and use the normal installer/update workflow. Do not automatically install upstream-suggested MCP servers or run repository installer/package scripts as a side effect of assessment.

## Mindrally: already represented locally; no additional install

[Mindrally source](https://github.com/Mindrally/skills/tree/main/solana) was read at `7682ca77710e0971eab4e0ae5dddfa281aea0ba5`, file `solana/SKILL.md`. It is a short generic Rust/Anchor/web3.js guide without Foundation's detailed reference tree. Its [pinned entrypoint](https://github.com/Mindrally/skills/blob/7682ca77710e0971eab4e0ae5dddfa281aea0ba5/solana/SKILL.md) adds little for this migration.

Coordinator verified the existing `/Users/robertripley/.codex/skills/solana/SKILL.md` resolves into the locally managed `omgskills/repos/Mindrally--skills/solana` directory. Its guidance body matches the reviewed Mindrally material, with local frontmatter differences. Local entrypoint sha256: `3b2da800bb87bfa3ae47f645309426e5bb6cedfbb162595745cf41a0224fcfa2`. It is already available; no duplicate or wholesale second Solana instruction set is recommended.

Review provenance: native Luna public-source comparison plus coordinator reads of both local entrypoints and upstream files. No private repository content was sent to another provider. The skill question was “should we add,” so installation/replacement was not treated as authorized.

## Authorized Foundation refresh receipt

Rob then explicitly requested the refresh. Coordinator read the installer and staged the skill with its `install-skill-from-github.py` helper using the git method and exact public commit `491a19a4f69e1e80cdf0f8fa476066d726894138`; `git ls-remote` confirmed that commit was still upstream main. The downloaded guidance was inspected before replacement; no upstream scripts were executed.

- Installed version: **2.6.0** at existing shared target `/Users/robertripley/.agents/skills/solana-dev`.
- Existing `/Users/robertripley/.codex/skills/solana-dev` symlink preserved.
- Full old tree preserved and checksum-verified at `/Users/robertripley/.agents/skill-backups/solana-dev-20261012-c9da2fa`.
- All **33 upstream files** checksum-matched staged source after installation; entrypoint relative references checked.
- New `SKILL.md` sha256: `c14678610ab408c6ca09dba6c5e4b1e8d516ee8ab4f48361f6ad47d578b2ad16`.
- No Mindrally duplicate, toolchain/dependency install, MCP configuration change, GUI change or product implementation. The new skill is available through normal discovery next turn and was read directly for this design pass.

Upstream defaults are references, not proof of Musical Chairs compatibility or authorization for unrelated installation. Existing GUI/wallet boundaries remain the user requirement. Toolchain/provider and transaction-format choices are reviewed separately in the defaults pass.
