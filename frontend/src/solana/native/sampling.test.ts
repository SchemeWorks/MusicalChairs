// The draw's hashing rules must reproduce the Rust vectors byte for byte
// (solana/fixtures/sampling.json, solana/fixtures/seed-vectors.json).
import { describe, it, expect } from 'vitest';
import { bytesToHex, hexToBytes, idl } from './codecs';
import { readFixtureJson, readSamplingFixture } from './testFixtures';
import { candidateDigestNext, candidateDigestStart, mapOutput, protocolSeed } from './sampling';

const fixture = readSamplingFixture();

describe('sampling.rs ports', () => {
  it('maps every provider-output vector to the same ordinal and counter', async () => {
    expect(fixture.output_vectors.length).toBeGreaterThanOrEqual(10);
    for (const v of fixture.output_vectors) {
      const got = await mapOutput(hexToBytes(v.output_hex), BigInt(v.round_id), BigInt(v.candidate_count));
      if (v.ordinal === null) {
        expect(got, v.label).toBeNull();
      } else {
        expect(got, v.label).toEqual({ ordinal: BigInt(v.ordinal), counter: Number(v.counter) });
      }
    }
  });

  it('returns null for zero candidates', async () => {
    expect(await mapOutput(new Uint8Array(64), 1n, 0n)).toBeNull();
  });

  it('chains the candidate digest exactly as the program does', async () => {
    const { set_tag, rows, empty_digest_hex, digest_hex } = fixture.candidates;
    let digest = await candidateDigestStart(set_tag);
    expect(bytesToHex(digest)).toBe(empty_digest_hex);
    for (const row of rows) digest = await candidateDigestNext(digest, hexToBytes(row.owner_hex), BigInt(row.loss));
    expect(bytesToHex(digest)).toBe(digest_hex);
  });

  it('computes the protocol seed for both cluster domains (litesvm fixture + devnet)', async () => {
    const p = fixture.protocol_seed;
    expect(p.program_id).toBe(idl.address);
    expect(p.vectors.length).toBeGreaterThanOrEqual(2);
    for (const vector of p.vectors) {
      const seed = await protocolSeed({
        clusterDomain: new TextEncoder().encode(vector.cluster_domain),
        programId: p.program_id,
        roundId: BigInt(p.round_id),
        closeId: BigInt(p.close_id),
        setTag: p.set_tag,
        selectedCount: BigInt(p.selected_count),
        sealedDigest: hexToBytes(p.sealed_digest_hex),
      });
      expect(bytesToHex(seed), vector.cluster).toBe(vector.seed_hex);
    }
  });

  it('seed-vectors.json agrees (the file the LiteSVM close suite checks against the compiled program)', async () => {
    const sv = readFixtureJson<{ program_id: string; round_id: number; close_id: number; set_tag: number; selected_count: number; sealed_digest_hex: string; vectors: Array<{ cluster_domain: string; seed_hex: string }> }>('seed-vectors.json');
    for (const vector of sv.vectors) {
      const seed = await protocolSeed({
        clusterDomain: new TextEncoder().encode(vector.cluster_domain),
        programId: sv.program_id,
        roundId: BigInt(sv.round_id),
        closeId: BigInt(sv.close_id),
        setTag: sv.set_tag,
        selectedCount: BigInt(sv.selected_count),
        sealedDigest: hexToBytes(sv.sealed_digest_hex),
      });
      expect(bytesToHex(seed)).toBe(vector.seed_hex);
    }
  });
});
