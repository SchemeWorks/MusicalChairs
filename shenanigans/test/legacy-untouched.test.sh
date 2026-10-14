#!/bin/bash
# legacyMintWithEffectsUntouchedByNativePath (static test; plan M12 Shenanigans list).
#
# moc -r cannot read files, so the "the native path never reaches the legacy
# mint code, and the legacy code is byte-for-byte unchanged" property is checked
# statically:
#   1. NativeRewards.mo (comments stripped) never mentions the legacy mint helpers.
#   2. The NATIVE SOLANA REWARDS block in main.mo (comments stripped) never mentions them.
#   3. The legacy functions' source text is pinned by SHA-256 (captured from the
#      pre-M12 shenanigans/main.mo); any edit to them fails this test.
#   4. Optional: pass a pre-M12 copy of main.mo as $1 to assert the diff is purely additive.
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

hits = FORBIDDEN.findall(strip_comments(mod))
check(not hits, 'NativeRewards.mo references no legacy mint helper (found: %s)' % sorted(set(hits)))

m = re.search(r'NATIVE SOLANA REWARDS \(plan M12 / spec section 9\)\s+-- BEGIN(.*?)NATIVE SOLANA REWARDS \(plan M12 / spec section 9\)\s+-- END', main, flags=re.S)
check(m is not None, 'main.mo has the delimited NATIVE SOLANA REWARDS block')
if m:
    region = strip_comments(m.group(1))
    hits = FORBIDDEN.findall(region)
    check(not hits, 'the native glue block references no legacy mint helper (found: %s)' % sorted(set(hits)))
    check('ppLedger.icrc1_transfer' in region, 'the native glue block performs its own icrc1_transfer')
    check(region.count('ppLedger.icrc1_transfer') == 1, 'exactly one icrc1_transfer call site on the native path')

lines = main.split('\n')
for name, want in PINNED.items():
    start = next((i for i, l in enumerate(lines) if re.match(r'^    func ' + name + r'\(', l)), None)
    if start is None:
        check(False, 'legacy func %s present' % name)
        continue
    end = next(j for j in range(start, len(lines)) if lines[j] == '    };')
    got = hashlib.sha256('\n'.join(lines[start:end + 1]).encode()).hexdigest()
    check(got == want, 'legacy func %s is byte-identical to the pre-M12 source' % name)

if baseline:
    r = subprocess.run(['diff', baseline, 'main.mo'], capture_output=True, text=True)
    removed = [l for l in r.stdout.split('\n') if l.startswith('<')]
    check(not removed, 'main.mo diff vs pre-M12 baseline has no removed/changed lines (%d)' % len(removed))

print('ALL PASS' if ok else 'TEST FAILURES')
sys.exit(0 if ok else 1)
PY
