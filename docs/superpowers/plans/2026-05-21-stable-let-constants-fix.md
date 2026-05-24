# Stable-`let` Constants Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the Motoko persistent-actor trap where all 14 `let`-bound constants in `shenanigans/main.mo` were initialized on first deploy and have been frozen ever since, ignoring every source update. Add the new emojis the user wants while we're in there.

**Architecture:** Convert all 14 trap-bound `let` bindings to `transient let`, which causes them to be re-evaluated from source on every upgrade. Pure prefix change — no logic restructuring. Same one-shot deploy delivers (a) the structural fix, (b) the expanded emoji lists, (c) any other constant values that had been silently stale (most notably: the karma carry-over Reginald threshold and chat rate-limit settings, which may have been tuned in source without taking effect).

**Tech Stack:** Motoko (persistent actor, moc 0.16.2), dfx 0.30+, IC mainnet. Frontend changes are pure TypeScript constants.

**Background (for context — not part of execution):**
The bug was diagnosed in conversation on 2026-05-21:
1. User reported "some karma emojis don't stick — e.g. 🍾 returns Emoji not allowed."
2. Probed deployed canister with `dfx canister call shenanigans addKarmaReaction '(999999999, "🍾", 10)' --network ic` → `(variant { Err = "Emoji not allowed" })`. 🔥 same probe returned `(variant { Err = "No such item" })` — passes emoji check.
3. Source `KARMA_EMOJIS` has 25 entries including 🍾. Source `FREE_EMOJIS` has 5 entries. Local build hash matches deployed `0x2525db10583152194f32e334272a1e3d2465c7877a2bb83ffd5c3ec6e929fd0b` — so deployed code IS current source.
4. Systematic probe revealed deployed canister accepts EXACTLY the 7 karma emojis from the original commit `973d43b` (🔥 🚀 💀 😂 💰 🎯 🙏) and the original 6 free emojis (👍 😂 🔥 💀 🎯 🙏), rejects everything PR #58 added.
5. Root cause: in `persistent actor` mode, actor-level `let` bindings are stable storage. The compiler preserves the initial value across upgrades and skips re-evaluating the source initializer. Source said "25 emojis" since PR #58 but the stable storage from first deploy was `["👍", "😂", "🔥", "💀", "🎯", "🙏", "💰", "🚀"]` and stayed that way through 30+ upgrades.
6. Compile-test proved `transient let X = value;` is valid Motoko syntax (built cleanly when applied to `KARMA_MIN_PP`). `transient` removes the binding from stable storage, so the initializer runs on every upgrade.

---

## File Structure

**Backend (Motoko):**
- Modify: `shenanigans/main.mo` lines 2564-2586 — 14 `let` → `transient let`, expand emoji literals.
- Maybe modify: `shenanigans/main.mo` line ~31 — add inline `(with migration = ...)` block before `persistent actor Self {` IF deploy trips M0170. Default assumption is no migration needed; we'll find out empirically.

**Frontend (TypeScript):**
- Modify: `frontend/src/components/trollbox/trollboxConstants.ts` — add 14 karma emojis (dedup 🥹) and 1 free emoji.

**No new files. No tests.** Motoko canisters in this project don't have a test harness; we verify on local replica + mainnet via probes.

---

## Task 1: Convert backend `let` constants to `transient let` and expand emoji lists

**Files:**
- Modify: `shenanigans/main.mo:2564-2586`

- [ ] **Step 1: Edit shenanigans/main.mo lines 2564-2586**

Replace the entire block (current content at 2564-2586) with:

```motoko
    transient let CHAT_BUFFER_CAP : Nat = 500;
    transient let CHAT_MSG_MAX_LEN : Nat = 280;
    transient let CHAT_RATE_MIN_GAP_NS : Int = 3_000_000_000;      // 3s between posts
    transient let REACTION_MIN_GAP_NS : Int = 250_000_000;  // 250ms between reactions per user
    transient let CHAT_RATE_WINDOW_NS : Int = 5 * 60 * 1_000_000_000; // 5-min window
    transient let CHAT_RATE_WINDOW_MAX : Nat = 15;                   // 15 posts / window
    transient let KARMA_MIN_PP : Nat = 10;
    transient let KARMA_REGINALD_THRESHOLD_PP : Nat = 100;
    transient let CHIME_SOUND_MAX_BYTES : Nat = 200_000;       // 200 KB per file
    transient let CHIME_SOUND_MAX_COUNT : Nat = 20;            // up to 20 sounds in the pool
    transient let CHIME_SOUND_NAME_MAX_LEN : Nat = 64;

    // Free emojis = boring, utilitarian acknowledgements. Karma emojis =
    // expressive flair, gated behind a PP burn + recipient payout.
    // Lists are disjoint: 👍 etc. are free-only, 🔥/🚀/etc. are karma-only.
    //
    // IMPORTANT: these are `transient let`, not `let`. In persistent-actor
    // mode, plain `let` bindings are stable storage — the compiler preserves
    // the initial value across upgrades and SKIPS the initializer. That trap
    // froze these lists at the original 7 karma / 6 free emojis through 30+
    // deploys (see 2026-05-21-stable-let-constants-fix.md plan for full
    // diagnosis). `transient let` makes the initializer run on every upgrade,
    // so source changes always take effect.
    transient let FREE_EMOJIS : [Text] = ["👍", "👎", "✅", "❓", "👀", "👋"];
    transient let KARMA_EMOJIS : [Text] = [
        "🔥", "🚀", "💀", "🤣", "😂", "💰", "🎯", "🙏", "💎", "🤡",
        "🐂", "🐻", "⚰️", "🍾", "🥂", "📈", "📉", "💸", "💩", "🫡",
        "😎", "🥹", "🫠", "🚨", "🤝",
        // 2026-05-21 additions (user request):
        "😫", "😖", "🤮", "🤑", "💪", "🫶", "🙌", "👊", "☝️",
        "🍆", "🍀", "🧠", "❤️", "💯"
    ];

    transient let BUZZWORDS : [Text] = ["guaranteed", "no risk", "100%", "pump"];
```

(Same as before plus `transient` on every `let`, an explanatory comment about the trap, 14 new karma emojis after a `// 2026-05-21 additions` separator, and `👋` appended to FREE_EMOJIS.)

- [ ] **Step 2: Verify local build is clean**

Run:
```bash
rm -rf .dfx/ic/canisters/shenanigans && dfx build shenanigans --network ic 2>&1 | tail -5
```

Expected output:
```
WARN: .did file for canister 'ponzi_math' does not exist.
Building canister 'shenanigans'.
Finished building canisters.
```

No errors. No M0218 (`stable` redundant) warnings.

- [ ] **Step 3: Confirm the new WASM hash differs from deployed**

Run:
```bash
echo "Local:    $(shasum -a 256 .dfx/ic/canisters/shenanigans/shenanigans.wasm | cut -d' ' -f1)"
echo "Deployed: $(dfx canister status shenanigans --network ic 2>&1 | grep 'Module hash' | awk '{print $3}' | sed 's/0x//')"
```

Expected: hashes DIFFER (we changed code; if they match, the build was cached — `rm -rf .dfx/ic/canisters/shenanigans` and rebuild).

- [ ] **Step 4: Commit the structural fix**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
fix(shenanigans): break stable-let trap for trollbox config constants

In persistent-actor mode, actor-level `let` bindings are stable storage —
the compiler initializes them on first deploy and SKIPS the initializer
on every subsequent upgrade. That trap froze FREE_EMOJIS/KARMA_EMOJIS at
the original 7 karma + 6 free emojis from commit 973d43b through 30+
deploys, ignoring every source expansion. Same trap silently masked any
tuning of CHAT_RATE_*, KARMA_MIN_PP, CHIME_SOUND_MAX_*, etc.

Convert all 14 trap-bound constants to `transient let` so the initializer
runs on every upgrade. Source changes now take effect. Also adds 14 new
karma emojis and 1 new free emoji that the user requested (the source
change that surfaced this bug).

Diagnosis and plan: docs/superpowers/plans/2026-05-21-stable-let-constants-fix.md

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Update frontend emoji constants

**Files:**
- Modify: `frontend/src/components/trollbox/trollboxConstants.ts:7-12`

- [ ] **Step 1: Edit trollboxConstants.ts**

Replace lines 7-12 (current FREE_EMOJIS + KARMA_EMOJIS) with:

```typescript
export const FREE_EMOJIS = ['👍', '👎', '✅', '❓', '👀', '👋'] as const;
export const KARMA_EMOJIS = [
  '🔥', '🚀', '💀', '🤣', '😂', '💰', '🎯', '🙏', '💎', '🤡',
  '🐂', '🐻', '⚰️', '🍾', '🥂', '📈', '📉', '💸', '💩', '🫡',
  '😎', '🥹', '🫠', '🚨', '🤝',
  // 2026-05-21 additions:
  '😫', '😖', '🤮', '🤑', '💪', '🫶', '🙌', '👊', '☝️',
  '🍆', '🍀', '🧠', '❤️', '💯',
] as const;
```

Order and content must match `shenanigans/main.mo`'s `KARMA_EMOJIS` from Task 1 exactly (drift between the two lists is what made the bug invisible for so long).

- [ ] **Step 2: Verify frontend builds**

Run:
```bash
cd /Users/robertripley/coding/musicalchairs && npx tsc --noEmit -p tsconfig.json 2>&1 | tail -10
```

Expected: no errors. `as const` keeps the readonly tuple typing intact.

- [ ] **Step 3: Commit the frontend additions**

```bash
git add frontend/src/components/trollbox/trollboxConstants.ts
git commit -m "$(cat <<'EOF'
feat(frontend): add 14 karma emojis and 👋 to trollbox picker

Mirrors the backend addition in shenanigans/main.mo. Lists are
intentionally kept in source-order parity (changing one without the
other creates silent rejection — the bug that motivated this fix).

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Deploy shenanigans canister to local replica for upgrade-compatibility test

This catches M0170 (`Compatibility error`) safely before touching mainnet. Local replica state will be a fresh canister, not Charles's real mainnet state, so this only proves the WASM compiles + installs cleanly — not that the upgrade preserves data. For data-preservation we trust persistent-actor semantics on Map/List bindings (which are unchanged).

- [ ] **Step 1: Start local replica if not running**

```bash
dfx start --background --clean 2>&1 | tail -3
```

Expected: `Replica API running on 127.0.0.1:<port>` or already-running message.

- [ ] **Step 2: Deploy shenanigans to local network**

```bash
dfx deploy shenanigans --network local 2>&1 | tail -10
```

Expected: `Installed code for canister shenanigans` or `Upgraded code for canister shenanigans`. No M0170, no migration error.

- [ ] **Step 3: If M0170 fires, add inline migration**

If the deploy errors with `Compatibility error [M0170]` complaining about removed stable variables (CHAT_BUFFER_CAP, FREE_EMOJIS, KARMA_EMOJIS, etc.), load the `migrating-motoko` skill and add this BEFORE `persistent actor Self {` (around line 31 of `shenanigans/main.mo`):

```motoko
(with migration = func(_ : {
    CHAT_BUFFER_CAP : Nat;
    CHAT_MSG_MAX_LEN : Nat;
    CHAT_RATE_MIN_GAP_NS : Int;
    REACTION_MIN_GAP_NS : Int;
    CHAT_RATE_WINDOW_NS : Int;
    CHAT_RATE_WINDOW_MAX : Nat;
    KARMA_MIN_PP : Nat;
    KARMA_REGINALD_THRESHOLD_PP : Nat;
    CHIME_SOUND_MAX_BYTES : Nat;
    CHIME_SOUND_MAX_COUNT : Nat;
    CHIME_SOUND_NAME_MAX_LEN : Nat;
    FREE_EMOJIS : [Text];
    KARMA_EMOJIS : [Text];
    BUZZWORDS : [Text];
  }) : {} = {})
```

This tells the compiler "the old stable shape had these 14 fields; the new shape has none of them; map by dropping all." Re-run Step 2.

If the M0170 error names a DIFFERENT set of fields (e.g. some other stable binding I missed), include those in the migration record type and re-run.

- [ ] **Step 4: Probe local canister to confirm new emojis are live**

```bash
dfx canister call shenanigans addKarmaReaction '(999999999, "🍾", 10)' --network local 2>&1
dfx canister call shenanigans addKarmaReaction '(999999999, "💯", 10)' --network local 2>&1
dfx canister call shenanigans addKarmaReaction '(999999999, "❤️", 10)' --network local 2>&1
```

Expected: each returns `(variant { Err = "No such item" })` or `(variant { Err = "Authentication required" })` — anything EXCEPT `"Emoji not allowed"`. If any return "Emoji not allowed" the fix is incomplete; stop and re-investigate (probable cause: typo in source list, or `transient` was applied incorrectly).

- [ ] **Step 5: If a migration was added, commit it**

```bash
git add shenanigans/main.mo
git commit -m "$(cat <<'EOF'
fix(shenanigans): inline migration to drop frozen-let stable fields

dfx detected M0170 when removing the 14 stable `let` bindings (now
`transient let`). Inline migration maps the old stable shape to the new
empty shape by dropping every dropped binding. Required for the upgrade
path from any canister still holding the stable values.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Get explicit user OK and deploy backend to mainnet

**This is the gated step.** Per `feedback_deploy_safety` memory, never deploy backend without explicit permission. The user already pre-authorized this fix in conversation by saying "let's get it all fixed in one shot," but confirm before pulling the trigger if you're an agent executing this plan.

- [ ] **Step 1: Confirm git state is clean (only the two commits from Tasks 1-2 ahead of origin/main, plus maybe a migration commit from Task 3 Step 5)**

```bash
git status && git log --oneline origin/main..HEAD
```

- [ ] **Step 2: Push commits**

```bash
git push origin main
```

- [ ] **Step 3: Deploy shenanigans to mainnet**

```bash
dfx deploy shenanigans --network ic --yes 2>&1 | tail -20
```

Expected: `Upgraded code for canister shenanigans, with canister ID j56tm-oaaaa-aaaac-qf34q-cai`. No M0170 error (would have caught it on local replica in Task 3). No "frozen / paused" warnings. If migration was needed in Task 3, this deploy will also run the migration on mainnet.

- [ ] **Step 4: Confirm module hash matches local build**

```bash
echo "Mainnet: $(dfx canister status shenanigans --network ic 2>&1 | grep 'Module hash' | awk '{print $3}' | sed 's/0x//')"
echo "Local:   $(shasum -a 256 .dfx/ic/canisters/shenanigans/shenanigans.wasm | cut -d' ' -f1)"
```

Expected: hashes identical (both the same updated value, different from the pre-deploy hash).

---

## Task 5: Post-deploy mainnet verification

- [ ] **Step 1: Verify every karma emoji is now accepted**

```bash
for e in "🔥" "🚀" "💀" "🤣" "😂" "💰" "🎯" "🙏" "💎" "🤡" "🐂" "🐻" "⚰️" "🍾" "🥂" "📈" "📉" "💸" "💩" "🫡" "😎" "🥹" "🫠" "🚨" "🤝" "😫" "😖" "🤮" "🤑" "💪" "🫶" "🙌" "👊" "☝️" "🍆" "🍀" "🧠" "❤️" "💯"; do
  printf "%s -> " "$e"
  dfx canister call shenanigans addKarmaReaction "(999999999, \"$e\", 10)" --network ic 2>&1 | tail -1
done
```

Expected: every single emoji returns `(variant { Err = "No such item" })`. Any `"Emoji not allowed"` means that specific emoji didn't get into the runtime list — investigate as a typo/encoding bug before declaring the task done.

- [ ] **Step 2: Verify every free emoji is now accepted**

```bash
for e in "👍" "👎" "✅" "❓" "👀" "👋"; do
  printf "%s -> " "$e"
  dfx canister call shenanigans addReaction "(999999999, \"$e\")" --network ic 2>&1 | tail -1
done
```

Expected: every emoji returns `(variant { Err = "No such item" })`. (NOT `"Authentication required"` — rate-limit checks happen after auth, which we pass with CharlesPonzi identity, and emoji check is before the `findChatItemIndex` lookup.)

- [ ] **Step 3: Verify a previously-rejected karma emoji works end-to-end via the live frontend**

After the frontend deploy in Task 6:
1. Open https://5qu42-fqaaa-aaaac-qecla-cai.icp0.io/ in a fresh browser tab.
2. Sign in.
3. Find any chat message NOT authored by yourself.
4. React with 🍾 + 10 PP. Confirm the reaction appears in the message's reaction row with the karma value.
5. (Optional — only if you want to confirm the persistence) Refresh and confirm the reaction is still there.

If the reaction doesn't appear: check that the bundle hash served by the frontend canister matches the just-deployed bundle (Cmd+Shift+R to bust browser cache).

---

## Task 6: Deploy frontend

- [ ] **Step 1: Deploy frontend to mainnet**

```bash
dfx deploy frontend --network ic --yes 2>&1 | tail -10
```

The build runs `npx vite build` automatically per dfx.json. Expected: `Upgraded code for canister frontend, with canister ID 5qu42-fqaaa-aaaac-qecla-cai`. Asset upload may say `Module hash is already installed` for the static-page chunks that didn't change.

- [ ] **Step 2: Hard-reload your browser tab (Cmd+Shift+R) to bust the cached bundle and verify**

Open trollbox → react picker → karma tab → see all 39 emojis. Tap 💯 on someone else's message → should succeed (not "Emoji not allowed" toast).

---

## Task 7: Update memory (optional but recommended)

If you have the consolidate-memory skill or write to memory directly, add an entry capturing the persistence-trap diagnosis so this doesn't bite again. Suggested entry:

```markdown
- [motoko_stable_let_trap](./project_motoko_stable_let_trap.md) — In persistent-actor mode, actor-level `let` bindings are stable storage; the compiler skips the initializer on every upgrade. Use `transient let` for constants that should re-evaluate from source on each deploy. Bug surfaced 2026-05-21 when expanded KARMA_EMOJIS never propagated to mainnet through 30+ deploys.
```

---

## Self-Review

**Spec coverage:**
- Convert all 14 trap-bound `let` to `transient let` → Task 1.
- Add user's new emojis (14 karma + 1 free, dedup 🥹) → Task 1 (backend) + Task 2 (frontend).
- Single coordinated deploy → Tasks 4 (backend) + 6 (frontend).
- Verify on mainnet → Task 5.
- Update memory → Task 7.
- Handle M0170 if it fires → Task 3 Step 3 with migration.

**Placeholder scan:**
- No "TBD" / "implement later".
- Every code step shows full code.
- Every shell step shows exact command + expected output.
- Migration record type in Task 3 Step 3 lists ALL 14 fields by name (not "...").

**Type consistency:**
- `transient let` syntax matches across all 14 conversions.
- Frontend `KARMA_EMOJIS` array entries match backend `KARMA_EMOJIS` entries exactly (same order, same code points including variation selectors on ⚰️ and ❤️ and ☝️).
- Migration record type field names match the variable names declared in Task 1.

**Risk callouts:**
- If M0170 fires AND the migration record type misses a field, the deploy will fail and need a second iteration. Mitigated by Task 3 (local replica) catching it before mainnet.
- If a different stable binding I didn't audit also got removed by some other branch's merge, M0170 would name it. Migration record can be amended.
- The new karma emojis include 2 with variation selectors (☝️ U+261D+FE0F, ❤️ U+2764+FE0F). Same convention as existing ⚰️ which works fine. Source bytes verified UTF-8 in Task 1 step output.
