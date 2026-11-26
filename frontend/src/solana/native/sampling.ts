// Byte-exact ports of the program's off-chain-checkable hashing rules
// (solana/programs/musical_chairs/src/sampling.rs; vectors in
// solana/fixtures/sampling.json and seed-vectors.json):
//   * the protocol seed bound into the ORAO request,
//   * the ordinal mapping of a 64-byte provider output to one candidate,
//   * the chained candidate digest.
// A keeper needs the first (to name the request account for request_round_draw)
// and the second (to find the winner's accounts for apply_promotion).
//
// Web3-free. SHA-256 comes from WebCrypto (`crypto.subtle`), which every
// browser provides in a secure context (the app is served over https).

import { BorshWriter, concatBytes, pubkeyToBytes, type PubkeyLike } from './codecs';

const text = new TextEncoder();
export const INDEX_DOMAIN = text.encode('MC/EEC/INDEX/v1');
export const SEED_DOMAIN = text.encode('SchemeWorks/MC/EEC/v1');
export const CANDIDATE_DOMAIN = text.encode('MC/EEC/CANDIDATES/v1');
/** Counters tried before a draw is declared entropy-exhausted. */
export const MAX_COUNTERS = 64;

export async function sha256(...parts: Uint8Array[]): Promise<Uint8Array> {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) throw new Error('WebCrypto (crypto.subtle) is unavailable; a secure context is required to hash.');
  return new Uint8Array(await subtle.digest('SHA-256', concatBytes(...parts)));
}

const u64le = (value: bigint | number): Uint8Array => new BorshWriter().u64(value).toBytes();
const u128le = (value: bigint | number): Uint8Array => new BorshWriter().u128(value).toBytes();

export interface ProtocolSeedInput {
  /** Pool.clusterDomain (32 bytes). */
  clusterDomain: Uint8Array;
  programId: PubkeyLike;
  roundId: bigint;
  closeId: bigint;
  /** Selected set tag: 0 preferred, 1 fallback. */
  setTag: number;
  selectedCount: bigint;
  /** The sealed digest of the selected set (32 bytes). */
  sealedDigest: Uint8Array;
}

/** SHA256(seed_domain || cluster_domain || program_id || round_le || close_le || set_tag || count_le || digest). */
export function protocolSeed(input: ProtocolSeedInput): Promise<Uint8Array> {
  if (input.clusterDomain.length !== 32) throw new Error('clusterDomain must be 32 bytes');
  if (input.sealedDigest.length !== 32) throw new Error('sealedDigest must be 32 bytes');
  return sha256(
    SEED_DOMAIN,
    input.clusterDomain,
    pubkeyToBytes(input.programId),
    u64le(input.roundId),
    u64le(input.closeId),
    Uint8Array.of(input.setTag),
    u64le(input.selectedCount),
    input.sealedDigest,
  );
}

/**
 * Map a 64-byte provider output to `{ ordinal, counter }` by rejection
 * sampling (no modulo bias): word_c = u64_le(SHA256(index_domain || output ||
 * round_le || c_le)[0..8]); the first word below floor(2^64 / N) * N is
 * accepted and reduced mod N. null when N == 0 or all 64 counters reject.
 */
export async function mapOutput(
  output: Uint8Array,
  roundId: bigint,
  candidateCount: bigint,
): Promise<{ ordinal: bigint; counter: number } | null> {
  if (output.length !== 64) throw new Error('provider output must be 64 bytes');
  if (candidateCount === 0n) return null;
  const limit = ((1n << 64n) / candidateCount) * candidateCount;
  for (let counter = 0; counter < MAX_COUNTERS; counter += 1) {
    const digest = await sha256(INDEX_DOMAIN, output, u64le(roundId), u64le(counter));
    let word = 0n;
    for (let i = 7; i >= 0; i -= 1) word = (word << 8n) | BigInt(digest[i]);
    if (word < limit) return { ordinal: word % candidateCount, counter };
  }
  return null;
}

/** Digest of an empty candidate set: SHA256(candidates_domain || set_tag). */
export const candidateDigestStart = (setTag: number): Promise<Uint8Array> =>
  sha256(CANDIDATE_DOMAIN, Uint8Array.of(setTag));

/** Append one candidate: SHA256(previous || owner || loss_le(16)). */
export const candidateDigestNext = (previous: Uint8Array, owner: PubkeyLike, loss: bigint): Promise<Uint8Array> =>
  sha256(previous, pubkeyToBytes(owner), u128le(loss));
