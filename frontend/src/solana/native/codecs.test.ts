// IMPL-46b step 1: the hand-written Borsh codecs must reproduce the byte
// vectors the Rust side committed in solana/fixtures/ix-encodings.json (39
// samples covering all 36 instructions) and decode the program's accounts.
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { REPO_ROOT, argsFromSample, baseName, ixFixture as fixture, typeDef } from './testFixtures';
import {
  idl,
  bytesToHex,
  instructionDiscriminator,
  encodeInstructionData,
  encodeAccountData,
  decodeAccountData,
  decodeRegistryPage,
  decodeSnapshotPage,
  decodeParticipantPage,
  decodeCandidatePage,
  PAGE_DISCRIMINATORS,
  BorshWriter,
  BorshReader,
  type IdlType,
} from './codecs';

describe('instruction encodings (solana/fixtures/ix-encodings.json)', () => {
  it('encodesOpenPositionArgsExactlyAsIdl', () => {
    const samples = fixture.instructions.filter((e) => e.name.startsWith('open_position'));
    expect(samples.map((e) => e.name)).toEqual(['open_position', 'open_position_compound15', 'open_position_compound30']);
    for (const entry of samples) {
      const bytes = encodeInstructionData('open_position', entry.sample);
      expect(bytesToHex(bytes), entry.name).toBe(entry.data_hex);
      expect(bytes.length, `${entry.name} length`).toBe(entry.data_len);
      expect(bytesToHex(bytes.subarray(0, 8)), `${entry.name} discriminator`).toBe(entry.discriminator_hex);
    }
    // The same bytes through camelCase keys, the way the adapter's builders call it.
    const camel = encodeInstructionData('open_position', {
      plan: 'Simple',
      gross: 1_000_000_000n,
      clientNonce: 1n,
      expectedRoundId: 1n,
      expectedConfig: 1,
    });
    expect(bytesToHex(camel)).toBe(samples[0].data_hex);
  });

  it('encodes every one of the 39 fixture entries byte for byte (all 36 instructions)', () => {
    expect(fixture.program).toBe(idl.address);
    expect(fixture.instructions).toHaveLength(39);
    const covered = new Set<string>();
    for (const entry of fixture.instructions) {
      const name = baseName(entry.name);
      const bytes = encodeInstructionData(name, argsFromSample(name, entry.sample));
      expect(bytesToHex(bytes), `${entry.name} data`).toBe(entry.data_hex);
      expect(bytes.length, `${entry.name} length`).toBe(entry.data_len);
      covered.add(name);
    }
    expect(covered.size).toBe(36);
    expect([...covered].sort()).toEqual(idl.instructions.map((i) => i.name).sort());
  });

  it('IDL discriminators equal sha256("global:<name>")[..8] and the adapter returns them', () => {
    for (const ix of idl.instructions) {
      const expected = createHash('sha256').update(`global:${ix.name}`).digest().subarray(0, 8);
      expect(Buffer.from(ix.discriminator).equals(expected), ix.name).toBe(true);
      expect(Buffer.from(instructionDiscriminator(ix.name)).equals(expected), ix.name).toBe(true);
    }
  });

  it('account discriminators (IDL accounts and the four page accounts) equal sha256("account:<Name>")[..8]', () => {
    for (const account of idl.accounts) {
      const expected = createHash('sha256').update(`account:${account.name}`).digest().subarray(0, 8);
      expect(Buffer.from(account.discriminator).equals(expected), account.name).toBe(true);
    }
    for (const [name, disc] of Object.entries(PAGE_DISCRIMINATORS)) {
      const expected = createHash('sha256').update(`account:${name}`).digest().subarray(0, 8);
      expect(Buffer.from(disc).equals(expected), name).toBe(true);
    }
  });

  it('rejects unknown instructions, missing args, out-of-range and fractional integers', () => {
    expect(() => encodeInstructionData('not_an_instruction', {})).toThrow(/unknown instruction/i);
    expect(() => encodeInstructionData('fund_operations', {})).toThrow(/amount/);
    expect(() => encodeInstructionData('fund_operations', { amount: -1n })).toThrow(/range/i);
    expect(() => encodeInstructionData('fund_operations', { amount: 1n << 64n })).toThrow(/range/i);
    expect(() => encodeInstructionData('fund_operations', { amount: 1.5 })).toThrow(/integer/i);
    expect(() => encodeInstructionData('discover_rows', { expectedTicketId: 1, expectedCursor: 0, rows: 256 })).toThrow(/range/i);
    expect(() => encodeInstructionData('open_position', { plan: 'Triple', gross: 1, clientNonce: 1, expectedRoundId: 1, expectedConfig: 1 })).toThrow(/variant/i);
  });
});

describe('IDL copy', () => {
  it('frontend/src/solana/native/idl/musical_chairs.json is byte-identical to the program IDL', () => {
    const mine = readFileSync(new URL('frontend/src/solana/native/idl/musical_chairs.json', REPO_ROOT));
    const theirs = readFileSync(new URL('solana/programs/musical_chairs/idl/musical_chairs.json', REPO_ROOT));
    expect(mine.equals(theirs)).toBe(true);
  });
});

describe('Borsh primitives', () => {
  it('writes little-endian fixed ints, u128 and bool, and reads them back', () => {
    const w = new BorshWriter();
    w.u8(0xab).u16(0x0102).u32(0x01020304).u64(0x0102030405060708n).u128((1n << 100n) + 5n).bool(true);
    const bytes = w.toBytes();
    expect(bytesToHex(bytes)).toBe(
      'ab' + '0201' + '04030201' + '0807060504030201' + '05000000000000000000000010000000' + '01',
    );
    const r = new BorshReader(bytes);
    expect([r.u8(), r.u16(), r.u32(), r.u64(), r.u128(), r.bool()]).toEqual([0xab, 0x0102, 0x01020304, 0x0102030405060708n, (1n << 100n) + 5n, true]);
    expect(r.remaining).toBe(0);
  });

  it('refuses to read past the end of the data', () => {
    expect(() => new BorshReader(new Uint8Array(3)).u64()).toThrow(/end of data|truncated/i);
  });
});

describe('account decoding', () => {
  // state.rs asserts these SPACE values; with every Option set to Some the
  // Borsh body of the type fills it exactly (8 discriminator + INIT_SPACE).
  const SPACE: Record<string, number> = {
    Pool: 374,
    Admission: 50,
    GameVault: 9,
    Ops: 193,
    RngAuthority: 41,
    ConfigVersion: 277,
    User: 84,
    UserRound: 89,
    Position: 159,
    Backer: 147,
    Ticket: 241,
    CloseRecord: 657,
  };

  it('encodes each IDL account to its Rust SPACE and decodes it back (discriminator checked)', () => {
    for (const [name, space] of Object.entries(SPACE)) {
      const maxValue = maxValueOf(name);
      const bytes = encodeAccountData(name, maxValue);
      expect(bytes.length, `${name} encoded length`).toBe(space);
      const decoded = decodeAccountData<Record<string, unknown>>(name, bytes);
      expect(encodeAccountData(name, decoded), `${name} round trip`).toEqual(bytes);
    }
  });

  it('rejects a wrong discriminator and a truncated body', () => {
    const pool = encodeAccountData('Pool', maxValueOf('Pool'));
    const wrong = Uint8Array.from(pool);
    wrong[0] ^= 0xff;
    expect(() => decodeAccountData('Pool', wrong)).toThrow(/discriminator/i);
    expect(() => decodeAccountData('Pool', pool.subarray(0, 60))).toThrow(/truncated|end of data/i);
    expect(() => decodeAccountData('Position', pool)).toThrow(/discriminator/i);
  });

  it('tolerates trailing zero padding (an Option::None leaves its worst-case bytes zeroed)', () => {
    const value = { ...maxValueOf('Position') };
    const padded = new Uint8Array(encodeAccountData('Position', value).length + 20);
    padded.set(encodeAccountData('Position', value));
    const decoded = decodeAccountData<{ nonce: bigint; claimed: boolean }>('Position', padded);
    expect(decoded.nonce).toBe(value.nonce);
  });

  it('decodes camelCase typed fields: Pool.currentCloseId is Option, CloseRecord.promotion is an enum', () => {
    const pool = decodeAccountData<{ currentCloseId: bigint | null; roundId: bigint }>(
      'Pool',
      encodeAccountData('Pool', { ...maxValueOf('Pool'), current_close_id: null, round_id: 7n }),
    );
    expect(pool.currentCloseId).toBeNull();
    expect(pool.roundId).toBe(7n);
    const applied = decodeAccountData<{ promotion: { kind: string; winner?: string; entitlement?: bigint } }>(
      'CloseRecord',
      encodeAccountData('CloseRecord', {
        ...maxValueOf('CloseRecord'),
        promotion: { kind: 'Applied', winner: '11111111111111111111111111111111', entitlement: 99n },
      }),
    );
    expect(applied.promotion).toEqual({ kind: 'Applied', winner: '11111111111111111111111111111111', entitlement: 99n });
    const pending = decodeAccountData<{ promotion: { kind: string } }>(
      'CloseRecord',
      encodeAccountData('CloseRecord', { ...maxValueOf('CloseRecord'), promotion: 'Pending' }),
    );
    expect(pending.promotion).toEqual({ kind: 'Pending' });
  });

  it('decodes the four shared pages (RegistryPage 321, SnapshotPage 529, ParticipantPage 330, CandidatePage 331 bytes)', () => {
    const key = (n: number) => bytesToBase58(new Uint8Array(32).fill(n));
    const rent = { ops: 1n, owner: 2n, ownerRefund: key(9) };
    const registry = buildPage('RegistryPage', { bump: 255, index: 3n, keys: Array.from({ length: 8 }, (_, i) => key(i + 1)), rent });
    expect(registry.length).toBe(321);
    expect(decodeRegistryPage(registry).keys[7]).toBe(key(8));
    expect(decodeRegistryPage(registry).index).toBe(3n);

    const rows = Array.from({ length: 8 }, (_, i) => ({ key: key(i + 1), series: i % 2, remaining: BigInt(i * 1000), slot: BigInt(i) }));
    const snapshot = buildPage('SnapshotPage', { bump: 1, index: 0n, ticketId: 4n, rows, rent });
    expect(snapshot.length).toBe(529);
    const snap = decodeSnapshotPage(snapshot);
    expect(snap.rows[3]).toEqual({ key: key(4), series: 1, remaining: 3000n, slot: 3n });
    expect(snap.ticketId).toBe(4n);

    const participants = buildPage('ParticipantPage', { bump: 2, roundId: 5n, page: 1n, len: 8, rows: Array.from({ length: 8 }, (_, i) => key(i + 20)), rent });
    expect(participants.length).toBe(330);
    expect(decodeParticipantPage(participants).rows[0]).toBe(key(20));

    const candidates = buildPage('CandidatePage', { bump: 2, closeId: 7n, setTag: 1, page: 2n, len: 3, rows: Array.from({ length: 8 }, (_, i) => key(i + 40)), rent });
    expect(candidates.length).toBe(331);
    const cand = decodeCandidatePage(candidates);
    expect([cand.closeId, cand.setTag, cand.page, cand.len]).toEqual([7n, 1, 2n, 3]);
  });
});

// ---------------------------------------------------------------------------
// helpers: build maximal account values straight from the IDL (every Option Some)
// ---------------------------------------------------------------------------

import bs58 from 'bs58';
const bytesToBase58 = (bytes: Uint8Array) => bs58.encode(bytes);

function maxValueOf(name: string): Record<string, unknown> {
  const def = typeDef(name);
  const out: Record<string, unknown> = {};
  for (const field of def.type.fields) out[field.name] = sampleValue(field.type as IdlType);
  return out;
}

function sampleValue(type: IdlType): unknown {
  if (type === 'bool') return true;
  if (type === 'pubkey') return bytesToBase58(new Uint8Array(32).fill(7));
  if (type === 'u8' || type === 'u16' || type === 'u32') return 3;
  if (type === 'u64' || type === 'u128') return 123456789n;
  if (typeof type === 'object' && type !== null) {
    if ('option' in type) return sampleValue((type as { option: IdlType }).option);
    if ('array' in type) {
      const [element, length] = (type as { array: [IdlType, number] }).array;
      return Array.from({ length }, () => sampleValue(element));
    }
    if ('defined' in type) {
      const defName = (type as { defined: { name: string } }).defined.name;
      const def = (idl.types as Array<{ name: string; type: { kind: string; fields?: Array<{ name: string; type: IdlType }>; variants?: Array<{ name: string; fields?: Array<{ name: string; type: IdlType }> }> } }>).find((t) => t.name === defName);
      if (!def) throw new Error(`no type ${defName}`);
      if (def.type.kind === 'struct') {
        return Object.fromEntries(def.type.fields!.map((f) => [f.name, sampleValue(f.type)]));
      }
      // enum: first variant with the largest payload is what SPACE accounts for; Promotion::Skipped / Applied
      const largest = [...def.type.variants!].sort((a, b) => (b.fields?.length ?? 0) - (a.fields?.length ?? 0))[0];
      return { kind: largest.name, ...Object.fromEntries((largest.fields ?? []).map((f) => [f.name, sampleValue(f.type)])) };
    }
  }
  throw new Error(`no sample for ${JSON.stringify(type)}`);
}

/** Page accounts are not in the IDL; build them with the adapter's own page schemas. */
function buildPage(name: string, value: Record<string, unknown>): Uint8Array {
  return encodeAccountData(name, value);
}

