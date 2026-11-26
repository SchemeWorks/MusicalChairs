#!/usr/bin/env python3
"""Verify saved public loader linkage and local build receipts; no network."""
import base64
import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parent.parent
sha = lambda data: hashlib.sha256(data).hexdigest()
meta = json.loads((root / 'fixtures/orao-provenance.json').read_text())
pin = meta['program_binary']
p = json.loads((root / pin['program_account_snapshot']).read_text())['result']
d = json.loads((root / pin['programdata_snapshot']).read_text())['result']
assert p['context']['slot'] == pin['program_account_context_slot']
assert d['context']['slot'] == pin['programdata_context_slot']
pa, da = p['value'], d['value']
assert pa['owner'] == da['owner'] == pin['program_owner']
assert pa['executable'] and not da['executable']
pb, db = [base64.b64decode(v['data'][0], validate=True) for v in (pa, da)]
assert len(pb) == 36 and int.from_bytes(pb[:4], 'little') == 2
assert len(db) == pin['programdata_space'] and int.from_bytes(db[:4], 'little') == 3
alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
n = 0
for c in pin['programdata_address']:
    n = n * 58 + alphabet.index(c)
assert pb[4:36] == n.to_bytes(32, 'big')
assert int.from_bytes(db[4:12], 'little') == pin['deployment_slot']
assert db[12] == 1
elf = (root / pin['elf_path']).read_bytes()
assert db[45:] == elf and elf[:4] == b'\x7fELF'
assert sha(elf) == pin['elf_sha256']
assert sha(db) == meta['provenance']['programdata_account_bin_sha256']
build = json.loads((root / 'fixtures/build-provenance.json').read_text())
assert sha((root / build['source']['path']).read_bytes()) == build['source']['sha256']
assert sha((root / build['lockfile']['path']).read_bytes()) == build['lockfile']['sha256']
for dep in build['dependency_sources']:
    assert sha((root / 'program/src' / dep['path']).read_bytes()) == dep['sha256']
assert sha((root / build['artifact']['path']).read_bytes()) == build['artifact']['sha256']
print('PASS: public loader linkage, captured ORAO ELF and local source/lock/SBF hashes')
