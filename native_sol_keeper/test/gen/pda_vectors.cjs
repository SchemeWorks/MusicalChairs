// Reference PDA / on-curve vectors from @solana/web3.js (the independent oracle).
// Offline: pure local computation, no RPC. Run from the repo root:
//   node native_sol_keeper/test/gen/pda_vectors.cjs
// Prints JSON consumed by fixtures.py. Seeds are hex strings.
const crypto = require('crypto');
const { PublicKey } = require('@solana/web3.js');

const PROGRAM = '7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7';
const hex = (b) => Buffer.from(b).toString('hex');
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
const text = (s) => Buffer.from(s, 'utf8');

const vectors = [];
function add(label, program, seeds) {
  const [addr, bump] = PublicKey.findProgramAddressSync(seeds, new PublicKey(program));
  vectors.push({ label, program, seeds: seeds.map(hex), address: addr.toBase58(), bump });
}

const owner1 = Buffer.from(Array.from({ length: 32 }, (_, i) => (i * 7 + 3) % 256));
const owner2 = Buffer.from(Array.from({ length: 32 }, (_, i) => (255 - i * 5) % 256));

add('pool', PROGRAM, [text('pool')]);
add('admission', PROGRAM, [text('admission')]);
add('game-vault', PROGRAM, [text('game-vault')]);
add('ops', PROGRAM, [text('ops')]);
add('ops-cash', PROGRAM, [text('ops-cash')]);
add('ticket', PROGRAM, [text('ticket')]);
add('rng-authority', PROGRAM, [text('rng-authority')]);
add('event-authority', PROGRAM, [text('__event_authority')]);
for (const i of [0, 1, 2, 7, 8, 40, 255, 256]) add('registry-' + i, PROGRAM, [text('registry'), u64(i)]);
for (const i of [0, 1, 5, 9]) add('snapshot-' + i, PROGRAM, [text('snapshot'), u64(i)]);
for (const i of [0, 1, 7, 300, 70000]) add('close-' + i, PROGRAM, [text('close'), u64(i)]);
add('config-3', PROGRAM, [text('config'), u32(3)]);
add('participants-2-1', PROGRAM, [text('participants'), u64(2), u64(1)]);
add('backer-A-owner1', PROGRAM, [text('backer'), Buffer.from([0]), owner1]);
add('backer-B-owner2', PROGRAM, [text('backer'), Buffer.from([1]), owner2]);
// Classic external vector: the associated token account of (owner, token program, mint).
const ATA = 'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';
const TOKEN = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA').toBuffer();
const USDC = new PublicKey('EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v').toBuffer();
add('ata-usdc-owner1', ATA, [owner1, TOKEN, USDC]);
// Seeds whose first candidate (bump 255) lands on the curve: the loop must keep going.
let found = 0;
for (let i = 0; i < 400 && found < 4; i++) {
  const [, bump] = PublicKey.findProgramAddressSync([text('probe'), u64(i)], new PublicKey(PROGRAM));
  if (bump < 253) { add('low-bump-' + i, PROGRAM, [text('probe'), u64(i)]); found++; }
}

// On-curve oracle over deterministic pseudo-random 32-byte strings plus structured edge cases.
const onCurve = [];
function oc(label, bytes) { onCurve.push({ label, hex: hex(bytes), onCurve: PublicKey.isOnCurve(bytes) }); }
oc('zero', Buffer.alloc(32));
oc('one (identity y=1)', Buffer.from([1, ...new Array(31).fill(0)]));
oc('two', Buffer.from([2, ...new Array(31).fill(0)]));
oc('system program', new PublicKey('11111111111111111111111111111111').toBuffer());
oc('pool pda', new PublicKey(vectors[0].address).toBuffer());
for (let i = 0; i < 48; i++) {
  oc('sha256(' + i + ')', crypto.createHash('sha256').update('keeper-oncurve-' + i).digest());
}
const yes = onCurve.filter((x) => x.onCurve).length;
console.log(JSON.stringify({ program: PROGRAM, vectors, onCurve, onCurveYes: yes, onCurveNo: onCurve.length - yes }));
