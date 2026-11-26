#!/bin/bash
# legacyMintWithEffectsUntouchedByNativePath (static test; plan M12 Shenanigans list)
# + the IMPL-49a / Codex R1 shape assertions for the shared signup-gift reservation.
#
# moc -r cannot read files, so the structural properties are checked statically:
#   1. NativeRewards.mo (comments stripped) never mentions the legacy mint helpers.
#   2. The NATIVE SOLANA REWARDS block in main.mo (comments stripped) never mentions them.
#   3. The legacy mint functions are pinned by SHA-256 (captured from the pre-M12 main.mo)
#      and are byte-identical: mintInternal, mintWithEffects, distributeDeductiveCascade,
#      describeTransferErr.
#   4. IMPL-49a: the ONE legacy function that changed is processNewGames, and it changed on
#      EXACTLY THREE lines, all tagged `IMPL-49a`:
#         (a) the gift-block check
#               - switch (principalMap.get(signupGiftClaimed, game.player)) {
#               + switch (signupGiftLegacyClaim(game.player)) { // IMPL-49a ...
#             (the shared reservation is claimed here, synchronously, BEFORE the mint await;
#              `null` = granted, any non-null = skip; the `case (?_) {}` / `case (null) {` arms
#              below it are untouched)
#         (b) + signupGiftLegacyFinalize(game.player, true);   after the `#Ok` marker write
#         (c) + signupGiftLegacyFinalize(game.player, false);  first line of the `#Err` arm
#      Removing the tagged lines and restoring line (a) must reproduce the pre-IMPL-49a function
#      byte for byte (pinned SHA), and the post-change function is pinned too. The order claim
#      -> mint await -> finalize is asserted textually. Economics (amounts, cascade, events,
#      retries, cursors) are unchanged.
#   5. The SIWS adjudication: bindAndReserve has exactly ONE await (the SIWS lookup), applyBind
#      comes after it, nothing is awaited after applyBind, and the shared reservation is the only
#      gift state it writes (never signupGiftClaimed). nativeSubmitLeg commits the SubmittedUnknown
#      state before its single icrc1_transfer await.
#   6. Optional: pass a pre-M12 copy of main.mo as $1 to assert that the ONLY line removed from
#      the legacy file is line (a).
#
# Run from anywhere:  bash shenanigans/test/legacy-untouched.test.sh [pre-M12-main.mo]
set -euo pipefail
cd "$(dirname "$0")/.."
BASELINE="${1:-}"
exec python3 -I - "$BASELINE" <<'PY'
import hashlib, re, subprocess, sys

baseline = sys.argv[1]
FORBIDDEN = re.compile(r'\b(mintWithEffects|mintInternal|distributeDeductiveCascade|distributeCascadeFromChips|chipTransfer|burnFrom|adminMint)\b')
PINNED = {
    'mintInternal': 'ae8de66cf971a6eb8ae5ca539593aa3c93eff02934823994c632d2de79eb3eae',
    'mintWithEffects': '42c2cb88f5009cd2c935c52d5fede4509e63f5849e30f7aed410df93638bb025',
    'distributeDeductiveCascade': '228eccbc758e55f0e4dc2f42c96b51c5cb8935990e19d8ee6f3d03d55a2c7546',
    'describeTransferErr': 'cb177404bdf261b9a98b46a9965f2a4fbee294832caf96c2a7f3f232d5c214d4',
}
# processNewGames: the only legacy function IMPL-49a touches.
PROCESS_NEW_GAMES_PRE_49A = 'f3a49b85fdb27894561c41ec6d64cd0d042a730c59684c0d7da7104e55fedfec'
PROCESS_NEW_GAMES_POST_49A = '89be4aef174bfafc46287736b75255d34a02fe2b6247bd24d2668574bf0f22b8'
ORIGINAL_CHECK_LINE = '                    switch (principalMap.get(signupGiftClaimed, game.player)) {'

ok = True
def check(cond, msg):
    global ok
    print(('PASS: ' if cond else 'FAIL: ') + msg)
    if not cond:
        ok = False

def strip_comments(t):
    t = re.sub(r'/\*.*?\*/', '', t, flags=re.S)
    return re.sub(r'//[^\n]*', '', t)

main = open('main.mo').read()
mod = open('NativeRewards.mo').read()
lines = main.split('\n')

def func_text(src_lines, header_re):
    start = next((i for i, l in enumerate(src_lines) if re.match(header_re, l)), None)
    if start is None:
        return None
    end = next(j for j in range(start, len(src_lines)) if src_lines[j] == '    };')
    return '\n'.join(src_lines[start:end + 1])

# 1-2. native path never reaches the legacy mint helpers
hits = FORBIDDEN.findall(strip_comments(mod))
check(not hits, 'NativeRewards.mo references no legacy mint helper (found: %s)' % sorted(set(hits)))

m = re.search(r'NATIVE SOLANA REWARDS \(plan M12 / spec section 9\)\s+-- BEGIN(.*?)NATIVE SOLANA REWARDS \(plan M12 / spec section 9\)\s+-- END', main, flags=re.S)
check(m is not None, 'main.mo has the delimited NATIVE SOLANA REWARDS block')
region = ''
if m:
    region = strip_comments(m.group(1))
    hits = FORBIDDEN.findall(region)
    check(not hits, 'the native glue block references no legacy mint helper (found: %s)' % sorted(set(hits)))
    check('ppLedger.icrc1_transfer' in region, 'the native glue block performs its own icrc1_transfer')
    check(region.count('ppLedger.icrc1_transfer') == 1, 'exactly one icrc1_transfer call site on the native path')

# 3. legacy mint helpers byte-identical
for name, want in PINNED.items():
    t = func_text(lines, r'^    func ' + name + r'\(')
    if t is None:
        check(False, 'legacy func %s present' % name)
        continue
    check(hashlib.sha256(t.encode()).hexdigest() == want, 'legacy func %s is byte-identical to the pre-M12 source' % name)

# 4. IMPL-49a: processNewGames changed on exactly three tagged lines
png = func_text(lines, r'^    func processNewGames\(')
check(png is not None, 'legacy func processNewGames present')
if png is not None:
    check(hashlib.sha256(png.encode()).hexdigest() == PROCESS_NEW_GAMES_POST_49A, 'processNewGames matches its pinned post-IMPL-49a SHA-256')
    png_lines = png.split('\n')
    tagged = [l for l in png_lines if 'IMPL-49a' in l]
    check(len(tagged) == 3, 'processNewGames carries exactly 3 IMPL-49a lines (found %d)' % len(tagged))
    restored = []
    for l in png_lines:
        if re.match(r'^\s*signupGiftLegacyFinalize\(game\.player, (true|false)\); // IMPL-49a', l):
            continue  # added lines (b), (c)
        if re.match(r'^\s*switch \(signupGiftLegacyClaim\(game\.player\)\) \{ // IMPL-49a', l):
            restored.append(ORIGINAL_CHECK_LINE)  # modified line (a)
            continue
        restored.append(l)
    check(hashlib.sha256('\n'.join(restored).encode()).hexdigest() == PROCESS_NEW_GAMES_PRE_49A,
          'removing the 2 added lines and restoring the 1 modified line reproduces the pre-IMPL-49a processNewGames byte for byte')
    # order: claim -> mint await -> finalize(ok) -> cascade ; finalize(err) after the await
    def idx(sub):
        return png.find(sub)
    i_claim = idx('signupGiftLegacyClaim(game.player)')
    i_await = idx('await mintWithEffects(game.player, giftNet')
    i_ok = idx('signupGiftLegacyFinalize(game.player, true)')
    i_err = idx('signupGiftLegacyFinalize(game.player, false)')
    i_casc = idx('await distributeDeductiveCascade(game.player, giftCascade')
    check(0 <= i_claim < i_await, 'the gift reservation is claimed BEFORE the gift mint await')
    check(i_await < i_ok < i_casc, 'the Committed finalize follows the mint await and precedes the gift cascade')
    check(i_await < i_err, 'the Failed finalize follows the mint await')
    check(png.count('await mintWithEffects(game.player, giftNet') == 1 and png.count('signupGiftLegacyClaim(') == 1, 'one gift mint site, one reservation claim')
    between = strip_comments(png[i_claim:i_await]) if 0 <= i_claim < i_await else ''
    check('await' not in between, 'no await between the reservation claim and the gift mint await')

# 5. SIWS / write-ahead structure of the native glue
def body(region_text, header):
    start = region_text.find(header)
    if start < 0:
        return None
    end = region_text.find('\n    };', start)
    return region_text[start:end + 7]

if region:
    bind = body(region, 'func bindAndReserve(')
    check(bind is not None, 'bindAndReserve present in the native block')
    if bind is not None:
        i_siws = bind.find('await siws.get_principal')
        i_apply = bind.find('NativeRewards.applyBind(')
        check(len(re.findall(r'\bawait\b', bind)) == 1, 'bindAndReserve has exactly ONE await (the SIWS lookup)')
        check(0 <= i_siws < i_apply, 'applyBind (re-read + atomic reservation) runs AFTER the SIWS await')
        check('await' not in bind[i_apply:], 'nothing is awaited after applyBind: commits happen in the same atomic block')
        check('signupGiftClaimed' not in bind.replace('signupMarkers = signupGiftClaimed', ''), 'bindAndReserve never writes signupGiftClaimed (only the shared reservation book)')
        check('signupGiftReservations := out.giftBook' in bind, 'bindAndReserve commits the shared reservation book')
    sub = body(region, 'func nativeSubmitLeg(')
    check(sub is not None, 'nativeSubmitLeg present in the native block')
    if sub is not None:
        check(len(re.findall(r'\bawait\b', sub)) == 1, 'nativeSubmitLeg has exactly ONE await (the icrc1_transfer)')
        check(sub.find('nativeCommit(s1)') >= 0 and sub.find('nativeCommit(s1)') < sub.find('await ppLedger.icrc1_transfer'), 'SubmittedUnknown is committed BEFORE the icrc1_transfer await')

# 6. optional: diff against a pre-M12 copy
if baseline:
    r = subprocess.run(['diff', baseline, 'main.mo'], capture_output=True, text=True)
    removed = [l[2:] for l in r.stdout.split('\n') if l.startswith('<')]
    check(removed == [ORIGINAL_CHECK_LINE.lstrip()] or removed == [ORIGINAL_CHECK_LINE], 'the ONLY line removed/changed vs the pre-M12 main.mo is the original gift-check line (%d removed)' % len(removed))

print('ALL PASS' if ok else 'TEST FAILURES')
sys.exit(0 if ok else 1)
PY
