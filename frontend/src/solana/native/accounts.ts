// PDA derivations and account readers for the native Musical Chairs program.
//
// Imports @solana/web3.js at module top level (PublicKey.findProgramAddressSync
// needs its ed25519 off-curve check), so this module belongs to the lazily
// loaded native chunk: app code reaches it only through dynamic import() in
// hooks/useNativeSolana.ts, the same way useQueries.ts loads web3.js for
// useSolBalance. The web3-free pieces (codecs, economics, selectors, status,
// types) are what the main bundle may import statically.
//
// Seeds are the frozen byte strings of solana/programs/musical_chairs/src/
// state.rs; accounts.test.ts cross-checks every derivation here against the
// seeds declared in the program IDL.

import { PublicKey } from '@solana/web3.js';
import type { Commitment, Connection } from '@solana/web3.js';
import bs58 from 'bs58';
import {
  BorshWriter,
  PAGE_CAPACITY,
  bytesToHex,
  PAGE_DISCRIMINATORS,
  decodeAccountData,
  decodeCandidatePage,
  decodeParticipantPage,
  decodeRegistryPage,
  decodeSnapshotPage,
  idl,
} from './codecs';
import type {
  Address,
  BackerSeries,
  CandidatePageData,
  NativeAdmission,
  NativeBacker,
  NativeCloseRecord,
  NativeConfigVersion,
  NativeOps,
  NativePool,
  NativePosition,
  NativeTicket,
  NativeUser,
  NativeUserRound,
  PageKindArg,
  ParticipantPageData,
  RegistryPageData,
  SnapshotPageData,
} from './types';

export const PROGRAM_ID = new PublicKey(idl.address);

export type KeyLike = string | PublicKey;
export const toPublicKey = (key: KeyLike): PublicKey => (typeof key === 'string' ? new PublicKey(key) : key);

/** Position account size (state.rs `Position::SPACE`), for `getProgramAccounts` dataSize filters. */
export const POSITION_SPACE = 159;

const text = new TextEncoder();
const SEED = {
  pool: text.encode('pool'),
  admission: text.encode('admission'),
  gameVault: text.encode('game-vault'),
  ops: text.encode('ops'),
  opsCash: text.encode('ops-cash'),
  ticket: text.encode('ticket'),
  user: text.encode('user'),
  userRound: text.encode('user-round'),
  position: text.encode('position'),
  backer: text.encode('backer'),
  registry: text.encode('registry'),
  snapshot: text.encode('snapshot'),
  rngAuthority: text.encode('rng-authority'),
  config: text.encode('config'),
  close: text.encode('close'),
  participants: text.encode('participants'),
  candidates: text.encode('candidates'),
  eventAuthority: text.encode('__event_authority'),
  oraoRequest: text.encode('OraoVrfCbRequest'),
} as const;

const u64 = (value: bigint | number): Uint8Array => new BorshWriter().u64(value).toBytes();
const u32 = (value: bigint | number): Uint8Array => new BorshWriter().u32(value).toBytes();

const derived = new Map<string, PublicKey>();
function derive(seeds: Uint8Array[], programId: PublicKey = PROGRAM_ID): PublicKey {
  const cacheKey = `${programId.toBase58()}|${seeds.map((s) => bytesToHex(s)).join('/')}`;
  let found = derived.get(cacheKey);
  if (!found) {
    [found] = PublicKey.findProgramAddressSync(seeds, programId);
    derived.set(cacheKey, found);
  }
  return found;
}

export const poolAddress = () => derive([SEED.pool]);
export const admissionAddress = () => derive([SEED.admission]);
export const gameVaultAddress = () => derive([SEED.gameVault]);
export const opsAddress = () => derive([SEED.ops]);
export const opsCashAddress = () => derive([SEED.opsCash]);
export const ticketAddress = () => derive([SEED.ticket]);
export const rngAuthorityAddress = () => derive([SEED.rngAuthority]);
export const eventAuthorityAddress = () => derive([SEED.eventAuthority]);
export const configAddress = (version: number | bigint) => derive([SEED.config, u32(version)]);
export const userAddress = (owner: KeyLike) => derive([SEED.user, toPublicKey(owner).toBytes()]);
export const userRoundAddress = (round: bigint | number, owner: KeyLike) =>
  derive([SEED.userRound, u64(round), toPublicKey(owner).toBytes()]);
export const positionAddress = (owner: KeyLike, nonce: bigint | number) =>
  derive([SEED.position, toPublicKey(owner).toBytes(), u64(nonce)]);
export const backerAddress = (series: BackerSeries | number, owner: KeyLike) =>
  derive([SEED.backer, Uint8Array.of(series), toPublicKey(owner).toBytes()]);
export const registryAddress = (index: bigint | number) => derive([SEED.registry, u64(index)]);
export const snapshotAddress = (index: bigint | number) => derive([SEED.snapshot, u64(index)]);
export const participantAddress = (round: bigint | number, index: bigint | number) =>
  derive([SEED.participants, u64(round), u64(index)]);
export const candidateAddress = (closeId: bigint | number, setTag: number, index: bigint | number) =>
  derive([SEED.candidates, u64(closeId), Uint8Array.of(setTag), u64(index)]);
export const closeRecordAddress = (closeId: bigint | number) => derive([SEED.close, u64(closeId)]);

/** The page account a `PageKind` + index names (provision_page / close_shared_account). */
export function pageAddress(page: PageKindArg, index: bigint | number): PublicKey {
  switch (page.kind) {
    case 'Registry':
      return registryAddress(index);
    case 'Snapshot':
      return snapshotAddress(index);
    case 'Participant':
      return participantAddress(page.round, index);
    case 'Candidate':
      return candidateAddress(page.closeId, page.setTag, index);
    default:
      throw new Error('Unknown page kind');
  }
}

/** ORAO VRF request PDA for (client, protocol seed) under the provider program. */
export function oraoRequestAddress(client: KeyLike, seed: Uint8Array, providerProgram: KeyLike): PublicKey {
  if (seed.length !== 32) throw new Error('protocol seed must be 32 bytes');
  return derive([SEED.oraoRequest, toPublicKey(client).toBytes(), seed], toPublicKey(providerProgram));
}

/** Index of the page holding `slot` (8 rows per page). */
export const pageOf = (slot: bigint | number): bigint => BigInt(slot) / BigInt(PAGE_CAPACITY);

// ---------------------------------------------------------------------------
// Readers
// ---------------------------------------------------------------------------

/** The slice of web3.js `Connection` the readers use. */
export type ReaderConnection = Pick<Connection, 'getAccountInfo' | 'getMultipleAccountsInfo' | 'getProgramAccounts'>;

export interface NativeReader {
  readPool(): Promise<NativePool | null>;
  readAdmission(): Promise<NativeAdmission | null>;
  readOps(): Promise<NativeOps | null>;
  readConfig(version: number): Promise<NativeConfigVersion | null>;
  readPosition(owner: KeyLike, nonce: bigint): Promise<NativePosition | null>;
  /** Every Position the owner holds (getProgramAccounts with discriminator + owner filters). */
  listPositions(owner: KeyLike): Promise<NativePosition[]>;
  readBacker(owner: KeyLike, series: BackerSeries): Promise<NativeBacker | null>;
  readBackerAt(address: KeyLike): Promise<NativeBacker | null>;
  readUser(owner: KeyLike): Promise<NativeUser | null>;
  readUserRound(round: bigint, owner: KeyLike): Promise<NativeUserRound | null>;
  readTicket(): Promise<NativeTicket | null>;
  readCloseRecord(closeId: bigint): Promise<NativeCloseRecord | null>;
  readRegistryPages(indices: bigint[]): Promise<Map<bigint, RegistryPageData>>;
  readSnapshotPages(indices: bigint[]): Promise<Map<bigint, SnapshotPageData>>;
  readParticipantPages(round: bigint, indices: bigint[]): Promise<Map<bigint, ParticipantPageData>>;
  readCandidatePage(closeId: bigint, setTag: number, index: bigint): Promise<CandidatePageData | null>;
  /** True when the program owns a non-empty account at `address` (a provisioned page / record). */
  isProvisioned(address: KeyLike): Promise<boolean>;
}

export function createReaders(connection: ReaderConnection, commitment: Commitment = 'confirmed'): NativeReader {
  const decodeInfo = <T>(typeName: string, info: { data: Uint8Array; owner: PublicKey } | null): T | null => {
    // A system-owned account sitting at a PDA (someone transferred lamports to it) is "not initialized".
    if (!info || info.data.length === 0 || !info.owner.equals(PROGRAM_ID)) return null;
    return decodeAccountData<T>(typeName, info.data);
  };

  const readOne = async <T>(typeName: string, address: PublicKey): Promise<T | null> =>
    decodeInfo<T>(typeName, await connection.getAccountInfo(address, commitment));

  const readMany = async <T>(addresses: PublicKey[], decode: (data: Uint8Array) => T): Promise<Array<T | null>> => {
    if (addresses.length === 0) return [];
    const infos = await connection.getMultipleAccountsInfo(addresses, commitment);
    return infos.map((info) => {
      if (!info || info.data.length === 0 || !info.owner.equals(PROGRAM_ID)) return null;
      return decode(info.data);
    });
  };

  const readPages = async <T>(
    indices: bigint[],
    addressOf: (index: bigint) => PublicKey,
    decode: (data: Uint8Array) => T,
  ): Promise<Map<bigint, T>> => {
    const unique = [...new Set(indices)];
    const pages = await readMany(unique.map(addressOf), decode);
    const out = new Map<bigint, T>();
    unique.forEach((index, i) => {
      const page = pages[i];
      if (page) out.set(index, page);
    });
    return out;
  };

  return {
    readPool: () => readOne<NativePool>('Pool', poolAddress()),
    readAdmission: () => readOne<NativeAdmission>('Admission', admissionAddress()),
    readOps: () => readOne<NativeOps>('Ops', opsAddress()),
    readConfig: (version) => readOne<NativeConfigVersion>('ConfigVersion', configAddress(version)),
    readPosition: (owner, nonce) => readOne<NativePosition>('Position', positionAddress(owner, nonce)),
    async listPositions(owner) {
      const accounts = await connection.getProgramAccounts(PROGRAM_ID, {
        commitment,
        filters: [
          { dataSize: POSITION_SPACE },
          { memcmp: { offset: 0, bytes: bs58.encode(idlDiscriminator('Position')) } },
          { memcmp: { offset: 9, bytes: toPublicKey(owner).toBase58() } }, // 8 discriminator + 1 bump
        ],
      });
      return accounts
        .map(({ account }) => decodeInfo<NativePosition>('Position', account))
        .filter((p): p is NativePosition => p !== null)
        .sort((a, b) => (a.nonce < b.nonce ? -1 : a.nonce > b.nonce ? 1 : 0));
    },
    readBacker: (owner, series) => readOne<NativeBacker>('Backer', backerAddress(series, owner)),
    readBackerAt: (address) => readOne<NativeBacker>('Backer', toPublicKey(address)),
    readUser: (owner) => readOne<NativeUser>('User', userAddress(owner)),
    readUserRound: (round, owner) => readOne<NativeUserRound>('UserRound', userRoundAddress(round, owner)),
    readTicket: () => readOne<NativeTicket>('Ticket', ticketAddress()),
    readCloseRecord: (closeId) => readOne<NativeCloseRecord>('CloseRecord', closeRecordAddress(closeId)),
    readRegistryPages: (indices) => readPages(indices, registryAddress, decodeRegistryPage),
    readSnapshotPages: (indices) => readPages(indices, snapshotAddress, decodeSnapshotPage),
    readParticipantPages: (round, indices) => readPages(indices, (i) => participantAddress(round, i), decodeParticipantPage),
    async readCandidatePage(closeId, setTag, index) {
      const info = await connection.getAccountInfo(candidateAddress(closeId, setTag, index), commitment);
      if (!info || info.data.length === 0 || !info.owner.equals(PROGRAM_ID)) return null;
      return decodeCandidatePage(info.data);
    },
    async isProvisioned(address) {
      const info = await connection.getAccountInfo(toPublicKey(address), commitment);
      return !!info && info.data.length > 0 && info.owner.equals(PROGRAM_ID);
    },
  };
}

function idlDiscriminator(account: string): Uint8Array {
  const found = idl.accounts.find((a) => a.name === account);
  const disc = found ? Uint8Array.from(found.discriminator) : PAGE_DISCRIMINATORS[account];
  if (!disc) throw new Error(`no discriminator for ${account}`);
  return disc;
}

/** Registry/snapshot/participant/candidate row address helpers used by the keeper builders. */
export const addressOf = (key: KeyLike): Address => toPublicKey(key).toBase58();
