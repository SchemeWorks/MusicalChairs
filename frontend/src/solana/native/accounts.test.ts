// PDA derivations are cross-checked two ways: against the seeds the program IDL
// declares (evaluated here by a tiny seed interpreter), and against golden
// addresses computed independently (Python sha256 + ed25519 off-curve check,
// not web3.js) for the accounts whose seeds the IDL does not list.
import { describe, it, expect, vi } from 'vitest';
import { Keypair, PublicKey, SystemProgram } from '@solana/web3.js';
import bs58 from 'bs58';
import {
  PROGRAM_ID,
  POSITION_SPACE,
  admissionAddress,
  backerAddress,
  candidateAddress,
  closeRecordAddress,
  configAddress,
  createReaders,
  eventAuthorityAddress,
  gameVaultAddress,
  opsAddress,
  opsCashAddress,
  oraoRequestAddress,
  pageAddress,
  participantAddress,
  poolAddress,
  positionAddress,
  registryAddress,
  rngAuthorityAddress,
  snapshotAddress,
  ticketAddress,
  userAddress,
  userRoundAddress,
} from './accounts';
import { BorshWriter, idl } from './codecs';
import { buildAccount, fakeConnection, type FakeAccount } from './testFixtures';

const OWNER = new PublicKey(new Uint8Array(32).fill(1)); // 4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi
const CLIENT = new PublicKey(new Uint8Array(32).fill(2));
const ORAO = new PublicKey('VRFCBePmGTpZ234BhbzNNzmyg39Rgdd6VgdfhHwKypU');
const u64 = (n: number | bigint) => new BorshWriter().u64(n).toBytes();
const u32 = (n: number | bigint) => new BorshWriter().u32(n).toBytes();

describe('PDA derivations', () => {
  it('program id is the IDL address', () => {
    expect(PROGRAM_ID.toBase58()).toBe('7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7');
  });

  it('matches independently computed golden addresses (seeds from state.rs)', () => {
    expect(poolAddress().toBase58()).toBe('AYEAZnLGHzjV95vMtRu4StknBBRtmZUGLwK297Na1jqK');
    expect(opsCashAddress().toBase58()).toBe('4FSt2vmzZmniwMmT9LWBU9UvttLLkE1DPXNPsfKadqEc');
    expect(userAddress(OWNER).toBase58()).toBe('Akahwp2ViGWwao37gpAwYpRiFsPt9YvyyGFunaeSJQ3X');
    expect(userRoundAddress(3n, OWNER).toBase58()).toBe('GxAY5z97rFmejHYcGcRuBiCwvEcXTfqfZb8tWnnr7Fi6');
    expect(positionAddress(OWNER, 77n).toBase58()).toBe('6Ni7hMFyJ1ShELbA3YiK1wVdtv6fQ642A25amA7tf7pF');
    expect(backerAddress(0, OWNER).toBase58()).toBe('HYmJaT5ayFoifUu42HgyWw8P2GcwdqRTVRBghK1eH4EF');
    expect(backerAddress(1, OWNER).toBase58()).toBe('RYMPPTKyDjudALBJPEjSdE9nvwFsuzEXq6yGGkE13NW');
    expect(registryAddress(3n).toBase58()).toBe('HNSq22Mj9LsGRbjah1V5CNJv751rTrq7XZUJK11JRCj4');
    expect(snapshotAddress(3n).toBase58()).toBe('EN8j5vZ8RsCXmuHwAPX94B1iqb1XsELbdT3pNMxrhW5w');
    expect(participantAddress(5n, 2n).toBase58()).toBe('C5Xg8ivGZ4RzWmd4tsNsTujJRd7EJ1HuHZRjnjnoCvqS');
    expect(candidateAddress(7n, 1, 2n).toBase58()).toBe('BZMhyStcWi2GsdK1nPrvRfdp6hDSrvGZqBFM4oHKV7oH');
    expect(closeRecordAddress(9n).toBase58()).toBe('ARdMbsJgn9tMuHVJA1vrVdYSW4fkGDntiR6YsCzhgdsg');
    expect(configAddress(2).toBase58()).toBe('4qNFvZ1pQ218nhpx6EthL4bZDTFvKTnx4nHH87R6yWza');
    expect(oraoRequestAddress(CLIENT, Uint8Array.from({ length: 32 }, (_, i) => i), ORAO).toBase58()).toBe(
      'CaukssTx6BYTy3RbiX4e9iUfoZSfjcXm2iwyJSFwMB1w',
    );
  });

  it('pageAddress maps every PageKind to its seed family', () => {
    expect(pageAddress({ kind: 'Registry' }, 3n).equals(registryAddress(3n))).toBe(true);
    expect(pageAddress({ kind: 'Snapshot' }, 3n).equals(snapshotAddress(3n))).toBe(true);
    expect(pageAddress({ kind: 'Participant', round: 5n }, 2n).equals(participantAddress(5n, 2n))).toBe(true);
    expect(pageAddress({ kind: 'Candidate', closeId: 7n, setTag: 1 }, 2n).equals(candidateAddress(7n, 1, 2n))).toBe(true);
  });

  it('agrees with every PDA seed list in the IDL (evaluated by a seed interpreter)', () => {
    const ctx = { owner: OWNER, round: 4n, nonce: 99n, configVersion: 3, closeId: 8n };
    const resolvePath = (path: string): Uint8Array => {
      switch (path) {
        case 'owner':
          return ctx.owner.toBytes();
        case 'pool.round_id':
          return u64(ctx.round);
        case 'version':
        case 'pool.active_config_version':
        case 'position.config_version':
        case 'close_record.config_version':
          return u32(ctx.configVersion);
        case 'position.nonce':
        case 'client_nonce':
          return u64(ctx.nonce);
        case 'close_record.close_id':
          return u64(ctx.closeId);
        default:
          throw new Error(`unhandled IDL seed path "${path}" - extend this interpreter and the derivation`);
      }
    };
    const mine: Record<string, (ixName: string) => PublicKey> = {
      pool: () => poolAddress(),
      admission: () => admissionAddress(),
      game_vault: () => gameVaultAddress(),
      ops: () => opsAddress(),
      ticket: () => ticketAddress(),
      rng_authority: () => rngAuthorityAddress(),
      event_authority: () => eventAuthorityAddress(),
      user: () => userAddress(ctx.owner),
      user_round: () => userRoundAddress(ctx.round, ctx.owner),
      position: () => positionAddress(ctx.owner, ctx.nonce),
      // initialize derives the genesis ConfigVersion #1 from a constant seed; every other use pins a context version.
      config: (ixName) => configAddress(ixName === 'initialize' ? 1 : ctx.configVersion),
      close_record: () => closeRecordAddress(ctx.closeId),
    };
    let checked = 0;
    for (const ix of idl.instructions) {
      for (const account of ix.accounts) {
        if (!account.pda) continue;
        const seeds = account.pda.seeds.map((seed) => {
          if (seed.kind === 'const') return Uint8Array.from(seed.value as number[]);
          return resolvePath(String(seed.path));
        });
        const expected = PublicKey.findProgramAddressSync(seeds, PROGRAM_ID)[0];
        const derive = mine[account.name];
        expect(derive, `${ix.name}.${account.name} has a derivation`).toBeDefined();
        expect(derive(ix.name).toBase58(), `${ix.name}.${account.name}`).toBe(expected.toBase58());
        checked += 1;
      }
    }
    expect(checked).toBeGreaterThan(60);
  });
});

describe('readers', () => {
  const owner = Keypair.generate().publicKey;
  const accountsWith = (entries: Array<[PublicKey, FakeAccount]>) => new Map(entries.map(([k, v]) => [k.toBase58(), v]));

  it('decodes program-owned accounts and returns null for missing or foreign-owned ones', async () => {
    const accounts = accountsWith([
      [poolAddress(), { owner: PROGRAM_ID, data: buildAccount('Pool', { roundId: 9n, registryLen: 3n }) }],
      // lamports sent to a PDA by someone else: system-owned, empty. Not an initialized account.
      [opsAddress(), { owner: SystemProgram.programId, data: new Uint8Array(0) }],
      // an account at the Ticket PDA that some OTHER program owns must never be decoded
      [ticketAddress(), { owner: SystemProgram.programId, data: buildAccount('Ticket', { id: 5n }) }],
    ]);
    const readers = createReaders(fakeConnection(accounts));
    const pool = await readers.readPool();
    expect(pool?.roundId).toBe(9n);
    expect(pool?.registryLen).toBe(3n);
    expect(await readers.readOps()).toBeNull();
    expect(await readers.readTicket()).toBeNull();
    expect(await readers.readCloseRecord(1n)).toBeNull();
  });

  it('reads positions, backers and tickets by their derived addresses', async () => {
    const accounts = accountsWith([
      [positionAddress(owner, 5n), { owner: PROGRAM_ID, data: buildAccount('Position', { owner: owner.toBase58(), nonce: 5n, principal: 1_000n, kind: 2 }) }],
      [backerAddress(1, owner), { owner: PROGRAM_ID, data: buildAccount('Backer', { owner: owner.toBase58(), series: 1, entitlement: 500n, lifetime: 20n }) }],
      [ticketAddress(), { owner: PROGRAM_ID, data: buildAccount('Ticket', { id: 4n, phase: 3, closeId: 12n }) }],
    ]);
    const readers = createReaders(fakeConnection(accounts));
    const position = await readers.readPosition(owner, 5n);
    expect([position?.nonce, position?.principal, position?.kind]).toEqual([5n, 1_000n, 2]);
    expect(await readers.readPosition(owner, 6n)).toBeNull();
    const b = await readers.readBacker(owner, 1);
    expect([b?.series, b?.entitlement, b?.lifetime]).toEqual([1, 500n, 20n]);
    expect(await readers.readBacker(owner, 0)).toBeNull();
    const ticket = await readers.readTicket();
    expect([ticket?.id, ticket?.phase, ticket?.closeId]).toEqual([4n, 3, 12n]);
  });

  it('lists an owner’s positions through getProgramAccounts with discriminator, size and owner filters', async () => {
    const mk = (nonce: bigint, who: PublicKey) => ({
      pubkey: positionAddress(who, nonce),
      account: { data: Buffer.from(buildAccount('Position', { owner: who.toBase58(), nonce })), owner: PROGRAM_ID, lamports: 1, executable: false, rentEpoch: 0 },
    });
    const getProgramAccounts = async (_program: PublicKey, config: { filters: Array<Record<string, any>> }) => {
      // The fake node honours the memcmp owner filter so a wrong offset would surface here.
      const ownerFilter = config.filters.find((f) => f.memcmp && f.memcmp.offset === 9)!.memcmp.bytes as string;
      return [mk(9n, owner), mk(2n, owner), mk(1n, OWNER)].filter(({ account }) => bs58.encode(account.data.subarray(9, 41)) === ownerFilter);
    };
    const connection = fakeConnection(new Map(), { getProgramAccounts: vi.fn(getProgramAccounts) });
    const readers = createReaders(connection);
    const positions = await readers.listPositions(owner);
    expect(positions.map((p) => p.nonce)).toEqual([2n, 9n]);
    const config = connection.getProgramAccounts.mock.calls[0][1] as { filters: Array<Record<string, any>> };
    expect(config.filters.some((f) => f.dataSize === POSITION_SPACE)).toBe(true);
    expect(config.filters.some((f) => f.memcmp?.offset === 0)).toBe(true);
  });

  it('POSITION_SPACE is the size of a Position account (state.rs asserts 159; Position has no Option fields)', () => {
    expect(POSITION_SPACE).toBe(159);
    expect(buildAccount('Position').length).toBe(POSITION_SPACE);
  });

  it('reads several registry pages in one multi-account call and skips unprovisioned ones', async () => {
    const accounts = accountsWith([
      [registryAddress(0n), { owner: PROGRAM_ID, data: buildAccount('RegistryPage', { index: 0n }) }],
      [registryAddress(2n), { owner: PROGRAM_ID, data: buildAccount('RegistryPage', { index: 2n }) }],
    ]);
    const connection = fakeConnection(accounts);
    const pages = await createReaders(connection).readRegistryPages([0n, 1n, 2n, 0n]);
    expect([...pages.keys()].sort()).toEqual([0n, 2n]);
    expect(connection.getMultipleAccountsInfo).toHaveBeenCalledTimes(1);
    expect((connection.getMultipleAccountsInfo.mock.calls[0][0] as PublicKey[]).length).toBe(3); // deduplicated
  });
});
