// Instruction builders: account lists/flags from the IDL, derived addresses,
// and the remaining-account layouts of the keeper steps (reference: the LiteSVM
// harness in solana/tests/financial-runtime).
import { describe, it, expect } from 'vitest';
import { Keypair, PublicKey, SystemProgram, ComputeBudgetProgram, Transaction, type TransactionInstruction } from '@solana/web3.js';
import {
  PROGRAM_ID,
  admissionAddress,
  backerAddress,
  candidateAddress,
  closeRecordAddress,
  configAddress,
  eventAuthorityAddress,
  gameVaultAddress,
  opsAddress,
  opsCashAddress,
  oraoRequestAddress,
  participantAddress,
  poolAddress,
  positionAddress,
  registryAddress,
  rngAuthorityAddress,
  snapshotAddress,
  ticketAddress,
  userAddress,
  userRoundAddress,
  type KeyLike,
} from './accounts';
import { createNativeClient, NativeStateError } from './client';
import { describeTransactionError } from './status';
import { bytesToHex, encodeInstructionData, getInstruction, hexToBytes, instructionDiscriminator, idl } from './codecs';
import { ixFixture, argsFromSample, baseName, buildAccount, fakeConnection, fakeWallet, readSamplingFixture, type FakeAccount } from './testFixtures';
import {
  BACKER_REMAINING_CUTOFF,
  NONE_PLACEHOLDER,
  PHASE_INSTRUCTION,
  assembleTransaction,
  buildIx,
  claimBackerIx,
  closeSimplePositionIx,
  computeUnitLimitFor,
  fundSeriesAIx,
  inlineAllocationAccounts,
  openPositionIx,
  payoutIx,
  planPrune,
  provisionPageIx,
} from './transactions';
import type { NativePool, NativeTicket, SnapshotRow } from './types';

const pk = (n: number) => new PublicKey(new Uint8Array(32).fill(n));
const addr = (n: number) => pk(n).toBase58();
const OWNER = pk(1);

/** IDL account name -> key, for readable assertions. */
function byName(name: string, ix: TransactionInstruction): Record<string, { pubkey: PublicKey; isSigner: boolean; isWritable: boolean }> {
  const def = getInstruction(name);
  return Object.fromEntries(def.accounts.map((a, i) => [a.name, ix.keys[i]]));
}

const SINGLETON_DERIVATIONS: Record<string, () => PublicKey> = {
  pool: poolAddress,
  admission: admissionAddress,
  game_vault: gameVaultAddress,
  ops: opsAddress,
  ops_cash: opsCashAddress,
  ticket: ticketAddress,
  rng_authority: rngAuthorityAddress,
  event_authority: eventAuthorityAddress,
  program: () => PROGRAM_ID,
};

const poolOf = (over: Partial<NativePool> = {}): NativePool =>
  ({ roundId: 4n, activeConfigVersion: 2, participantCount: 11n, registryLen: 9n, nextCloseId: 6n, ...over }) as NativePool;

describe('generic IDL builder (all 36 instructions)', () => {
  it('builds every fixture entry with the fixture bytes and the IDL account list, order and flags', () => {
    const covered = new Set<string>();
    for (const entry of ixFixture.instructions) {
      const name = baseName(entry.name);
      const def = getInstruction(name);
      const provided: Record<string, PublicKey> = {};
      for (const ref of def.accounts) {
        if (!ref.address && !SINGLETON_DERIVATIONS[ref.name]) provided[ref.name] = Keypair.generate().publicKey;
      }
      const ix = buildIx(name, { args: argsFromSample(name, entry.sample), accounts: provided });
      expect(bytesToHex(ix.data), entry.name).toBe(entry.data_hex);
      expect(ix.programId.equals(PROGRAM_ID)).toBe(true);
      expect(ix.keys, `${entry.name} account count`).toHaveLength(def.accounts.length);
      def.accounts.forEach((ref, i) => {
        const key = ix.keys[i];
        expect(key.isSigner, `${entry.name}.${ref.name} signer`).toBe(!!ref.signer);
        expect(key.isWritable, `${entry.name}.${ref.name} writable`).toBe(!!ref.writable);
        if (ref.address) expect(key.pubkey.toBase58()).toBe(ref.address);
        else if (SINGLETON_DERIVATIONS[ref.name]) expect(key.pubkey.equals(SINGLETON_DERIVATIONS[ref.name]())).toBe(true);
        else expect(key.pubkey.equals(provided[ref.name])).toBe(true);
      });
      covered.add(name);
    }
    expect(covered.size).toBe(36);
  });

  it('guards: unknown account names, missing required accounts and null for a required account', () => {
    expect(() => buildIx('fund_series_a', { args: { gross: 1n, expectedRoundId: 1n }, accounts: { ownr: pk(1) } })).toThrow(/unknown account "ownr"/);
    expect(() => buildIx('fund_series_a', { args: { gross: 1n, expectedRoundId: 1n }, accounts: {} })).toThrow(/missing account "owner"/);
    expect(() => buildIx('close_simple_position', { accounts: { owner: null } })).toThrow(/required/);
    // The fixed system program cannot be redirected.
    expect(() => buildIx('fund_operations', { args: { amount: 1n }, accounts: { sender: pk(1), system_program: pk(9) } })).toThrow(/fixed/);
  });

  it('an omitted or null optional account is Anchor’s None: the program id, read-only', () => {
    const ix = buildIx('commit_ticket', { args: { expectedTicketId: 1n }, accounts: { close_record: null } });
    const accounts = byName('commit_ticket', ix);
    expect(accounts.close_record.pubkey.equals(PROGRAM_ID)).toBe(true);
    expect(accounts.close_record.isWritable).toBe(false);
    expect(accounts.close_record.isSigner).toBe(false);
    expect(byName('commit_ticket', buildIx('commit_ticket', { args: { expectedTicketId: 1n } })).close_record.pubkey.equals(PROGRAM_ID)).toBe(true);
    expect(NONE_PLACEHOLDER().pubkey.equals(PROGRAM_ID)).toBe(true);
  });
});

describe('player instructions', () => {
  it('open_position names every derived account of the quoted round and config', () => {
    const pool = poolOf();
    const ix = openPositionIx({ owner: OWNER, pool, plan: 'Compound15', gross: 10_000_000n, nonce: 77n });
    const a = byName('open_position', ix);
    expect(a.owner.pubkey.equals(OWNER)).toBe(true);
    expect(a.owner.isSigner && a.owner.isWritable).toBe(true);
    expect(a.user.pubkey.equals(userAddress(OWNER))).toBe(true);
    expect(a.user_round.pubkey.equals(userRoundAddress(4n, OWNER))).toBe(true);
    expect(a.position.pubkey.equals(positionAddress(OWNER, 77n))).toBe(true);
    expect(a.config.pubkey.equals(configAddress(2))).toBe(true);
    // participant_count 11 -> the next participant lands on page 1 (8 per page)
    expect(a.participant_page.pubkey.equals(participantAddress(4n, 1n))).toBe(true);
    expect(a.system_program.pubkey.equals(SystemProgram.programId)).toBe(true);
    expect(bytesToHex(ix.data)).toBe(
      bytesToHex(encodeInstructionData('open_position', { plan: 'Compound15', gross: 10_000_000n, clientNonce: 77n, expectedRoundId: 4n, expectedConfig: 2 })),
    );
  });

  it('fund_series_a appends at the registry tail page', () => {
    const a = byName('fund_series_a', fundSeriesAIx({ owner: OWNER, pool: poolOf({ registryLen: 16n }), gross: 50_000_000n }));
    expect(a.backer.pubkey.equals(backerAddress(0, OWNER))).toBe(true);
    expect(a.registry.pubkey.equals(registryAddress(2n))).toBe(true); // 16 / 8
    expect(a.config.pubkey.equals(configAddress(2))).toBe(true);
  });

  const payoutBase = {
    owner: OWNER,
    pool: poolOf(),
    nonce: 5n,
    positionConfigVersion: 1,
    destination: pk(9),
    minimumReceive: 96_800_000n,
    maxWorkContribution: 0n,
  };

  it('withdraw_simple / settle_compounding use the position’s pinned config and the optional close record', () => {
    for (const kind of ['withdraw_simple', 'settle_compounding'] as const) {
      const without = payoutIx({ ...payoutBase, kind, closeRecord: null });
      const a = byName(kind, without);
      expect(a.position.pubkey.equals(positionAddress(OWNER, 5n))).toBe(true);
      expect(a.config.pubkey.equals(configAddress(1))).toBe(true); // the position's, not the pool's active (2)
      expect(a.user_round.pubkey.equals(userRoundAddress(4n, OWNER))).toBe(true);
      expect(a.destination.pubkey.equals(pk(9))).toBe(true);
      expect(a.close_record.pubkey.equals(PROGRAM_ID)).toBe(true);
      expect(a.ticket.isWritable).toBe(true);
      expect(bytesToHex(without.data)).toBe(bytesToHex(encodeInstructionData(kind, { minimumReceive: 96_800_000n, maxWorkContribution: 0n })));

      const withClose = payoutIx({ ...payoutBase, kind, closeRecord: closeRecordAddress(6n) });
      expect(byName(kind, withClose).close_record.pubkey.equals(closeRecordAddress(6n))).toBe(true);
    }
  });

  it('inline payout appends [Backer slot 0.., RegistryPage 0, SnapshotPage 0], all writable, only for 1..8 Backers', () => {
    const keys = Array.from({ length: 8 }, (_, i) => addr(40 + i));
    expect(inlineAllocationAccounts(0n, [])).toBeNull();
    expect(inlineAllocationAccounts(9n, keys)).toBeNull();
    const three = inlineAllocationAccounts(3n, keys)!;
    expect(three).toHaveLength(5);
    expect(three.slice(0, 3).map((m) => m.pubkey.toBase58())).toEqual(keys.slice(0, 3));
    expect(three[3].pubkey.equals(registryAddress(0))).toBe(true);
    expect(three[4].pubkey.equals(snapshotAddress(0))).toBe(true);
    expect(three.every((m) => m.isWritable && !m.isSigner)).toBe(true);
    expect(inlineAllocationAccounts(8n, keys)).toHaveLength(10);
    expect(() => inlineAllocationAccounts(3n, keys.slice(0, 2))).toThrow(/every registry key/);

    const ix = payoutIx({ ...payoutBase, kind: 'withdraw_simple', closeRecord: null, inlineAccounts: three });
    const idlCount = getInstruction('withdraw_simple').accounts.length;
    expect(ix.keys).toHaveLength(idlCount + 5);
    expect(ix.keys.slice(idlCount).map((k) => k.pubkey.toBase58())).toEqual(three.map((m) => m.pubkey.toBase58()));
  });

  it('close_simple_position and claim_backer (combined claim, missing series is None)', () => {
    const c = byName('close_simple_position', closeSimplePositionIx({ owner: OWNER, nonce: 5n, configVersion: 1 }));
    expect(c.position.pubkey.equals(positionAddress(OWNER, 5n))).toBe(true);
    expect(c.config.pubkey.equals(configAddress(1))).toBe(true);

    const both = byName('claim_backer', claimBackerIx({ owner: OWNER, destination: pk(9), hasSeriesA: true, hasSeriesB: true }));
    expect(both.backer_a.pubkey.equals(backerAddress(0, OWNER))).toBe(true);
    expect(both.backer_b.pubkey.equals(backerAddress(1, OWNER))).toBe(true);
    expect(both.backer_a.isWritable && both.backer_b.isWritable).toBe(true);
    expect(both.owner.isSigner).toBe(true);
    const onlyB = byName('claim_backer', claimBackerIx({ owner: OWNER, destination: pk(9), hasSeriesA: false, hasSeriesB: true }));
    expect(onlyB.backer_a.pubkey.equals(PROGRAM_ID)).toBe(true);
    expect(onlyB.backer_b.pubkey.equals(backerAddress(1, OWNER))).toBe(true);
  });

  it('provision_page: candidate pages carry their close record, shared pages do not', () => {
    const reg = byName('provision_page', provisionPageIx({ payer: OWNER, page: { kind: 'Registry' }, index: 3n, funding: 'Caller' }));
    expect(reg.page.pubkey.equals(registryAddress(3n))).toBe(true);
    expect(reg.close_record.pubkey.equals(PROGRAM_ID)).toBe(true);
    expect(reg.payer.isSigner).toBe(true);
    const cand = byName('provision_page', provisionPageIx({ payer: OWNER, page: { kind: 'Candidate', closeId: 7n, setTag: 1 }, index: 2n, funding: 'LockedClose' }));
    expect(cand.page.pubkey.equals(candidateAddress(7n, 1, 2n))).toBe(true);
    expect(cand.close_record.pubkey.equals(closeRecordAddress(7n))).toBe(true);
  });
});

describe('transaction assembly', () => {
  it('puts the compute-unit limit first and sizes it per instruction', () => {
    const feePayer = pk(1);
    const ix = closeSimplePositionIx({ owner: feePayer, nonce: 1n, configVersion: 1 });
    const tx = assembleTransaction({ feePayer, blockhash: addr(3), lastValidBlockHeight: 10, instructions: [ix], computeUnits: computeUnitLimitFor(['close_simple_position']) });
    expect(tx).toBeInstanceOf(Transaction);
    expect(tx.instructions[0].programId.equals(ComputeBudgetProgram.programId)).toBe(true);
    expect(tx.instructions[1]).toBe(ix);
    expect(tx.lastValidBlockHeight).toBe(10);
    expect(computeUnitLimitFor(['withdraw_simple'])).toBe(400_000);
    expect(computeUnitLimitFor(['withdraw_simple'], true)).toBeGreaterThan(computeUnitLimitFor(['withdraw_simple']));
    expect(computeUnitLimitFor(['expire_promotion', 'finalize_round', 'open_position'])).toBe(600_000);
    expect(computeUnitLimitFor(Array(10).fill('prune_rows'))).toBe(1_400_000); // clamped to the transaction maximum
  });
});

// ---------------------------------------------------------------------------
// Prune planning (port of the harness prune_plan with chain cutting)
// ---------------------------------------------------------------------------

describe('planPrune', () => {
  const registry = [addr(10), addr(11), addr(12), addr(13)]; // slots 0..3
  const ticket = { id: 1n, cursor: 4n } as NativeTicket;
  const row = (slot: number, remaining = 1_000_000n): SnapshotRow => ({ key: registry[slot], series: 0, remaining, slot: BigInt(slot) });
  const base = {
    ticket,
    registryLen: 4n,
    maxRows: 8,
    registryKeyAt: (slot: bigint) => registry[Number(slot)],
    snapshotRowAt: (slot: bigint) => row(Number(slot)),
    liveRemaining: () => 1_000_000n,
  };
  const placeholders = (meta: { pubkey: PublicKey }[]) => meta.every((m) => m.pubkey.equals(PROGRAM_ID));

  it('keeps every row when nothing is capped: one snapshot page + four program-id placeholders per row', () => {
    const plan = planPrune(base);
    expect(plan.rows).toBe(4);
    expect(plan.pruned).toBe(0);
    expect(plan.remaining).toHaveLength(4 * 5);
    for (let r = 0; r < 4; r += 1) {
      const group = plan.remaining.slice(r * 5, r * 5 + 5);
      expect(group[0].pubkey.equals(snapshotAddress(0))).toBe(true);
      expect(placeholders(group.slice(1))).toBe(true);
      expect(group.every((m) => !m.isWritable)).toBe(true);
    }
  });

  it('swap-deletes a middle row with the last row (moved Backer) and stops sharing nothing', () => {
    // slot 2 (addr 12) is newly capped by this ticket's credits; the registry's last row (addr 13) moves into slot 2.
    const plan = planPrune({ ...base, liveRemaining: (k) => (k === addr(12) ? 0n : 1_000_000n) });
    expect(plan.rows).toBe(4);
    expect(plan.pruned).toBe(1);
    const slot2 = plan.remaining.slice(5, 10); // rows run 3, 2, 1, 0
    expect(placeholders(plan.remaining.slice(1, 5))).toBe(true); // slot 3 kept
    expect(slot2.map((m) => m.pubkey.toBase58())).toEqual([snapshotAddress(0), addr(12), registryAddress(0), registryAddress(0), addr(13)].map((x) => (typeof x === 'string' ? x : x.toBase58())));
    expect(slot2.map((m) => m.isWritable)).toEqual([false, true, true, true, true]);
  });

  it('prunes a captured-closed row even when skipNewlyCapped is set; skipNewlyCapped defers live-capped rows', () => {
    const closedAtCapture = planPrune({ ...base, skipNewlyCapped: true, snapshotRowAt: (s) => row(Number(s), s === 1n ? BACKER_REMAINING_CUTOFF : 1_000_000n) });
    expect(closedAtCapture.pruned).toBe(1);
    const deferred = planPrune({ ...base, skipNewlyCapped: true, liveRemaining: () => 0n });
    expect(deferred.pruned).toBe(0);
  });

  it('prunes the last row as an alias (row Backer == moved Backer)', () => {
    const plan = planPrune({ ...base, ticket: { id: 1n, cursor: 4n } as NativeTicket, liveRemaining: (k) => (k === addr(13) ? 0n : 1_000_000n), maxRows: 1 });
    expect(plan.rows).toBe(1);
    expect(plan.pruned).toBe(1);
    expect(plan.remaining.map((m) => m.pubkey.toBase58())).toEqual([snapshotAddress(0).toBase58(), addr(13), registryAddress(0).toBase58(), registryAddress(0).toBase58(), addr(13)]);
  });

  it('cuts the batch BEFORE a row whose moved Backer was already written earlier in the batch', () => {
    // slot 3 kept; slot 2 pruned (D moves into slot 2, written); slot 1 pruned would move D again -> duplicate writable -> cut.
    const plan = planPrune({ ...base, liveRemaining: (k) => (k === addr(12) || k === addr(11) ? 0n : 1_000_000n) });
    expect(plan.rows).toBe(2);
    expect(plan.pruned).toBe(1);
  });

  it('respects maxRows and never walks below slot 0', () => {
    expect(planPrune({ ...base, maxRows: 2 }).rows).toBe(2);
    expect(planPrune({ ...base, ticket: { id: 1n, cursor: 1n } as NativeTicket, maxRows: 8 }).rows).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// Client keeper flows against a fake chain
// ---------------------------------------------------------------------------

function chain(entries: Array<[PublicKey, Uint8Array]>) {
  const accounts = new Map<string, FakeAccount>();
  for (const [key, data] of entries) accounts.set(key.toBase58(), { owner: PROGRAM_ID, data });
  return accounts;
}

function clientFor(accounts: Map<string, FakeAccount>) {
  const wallet = fakeWallet();
  const connection = fakeConnection(accounts);
  const client = createNativeClient({ connection, wallet, sessionPubkey: wallet.publicKey!, sleep: async () => {} });
  return { client, wallet, connection };
}

const page8 = (start: number) => Array.from({ length: 8 }, (_, i) => addr(start + i));

describe('NativeClient allocation keeper steps', () => {
  it('discover: three accounts per slot [registry page ro, snapshot page w, backer ro], clamped to the captured length', async () => {
    const accounts = chain([
      [ticketAddress(), buildAccount('Ticket', { id: 1n, phase: 0, capturedLen: 10n, cursor: 8n })],
      [registryAddress(1n), buildAccount('RegistryPage', { index: 1n, keys: page8(100) })],
    ]);
    const { client } = clientFor(accounts);
    const plan = await client.buildAdvancePhase({});
    expect(plan.phase).toBe('Discover');
    expect(plan.instructionName).toBe('discover_rows');
    expect(plan.rows).toBe(2);
    const ix = plan.instruction;
    expect(bytesToHex(ix.data)).toBe(bytesToHex(encodeInstructionData('discover_rows', { expectedTicketId: 1n, expectedCursor: 8n, rows: 2 })));
    const rest = ix.keys.slice(getInstruction('discover_rows').accounts.length);
    expect(rest.map((k) => [k.pubkey.toBase58(), k.isWritable])).toEqual([
      [registryAddress(1n).toBase58(), false],
      [snapshotAddress(1n).toBase58(), true],
      [addr(100), false],
      [registryAddress(1n).toBase58(), false],
      [snapshotAddress(1n).toBase58(), true],
      [addr(101), false],
    ]);
  });

  it('senior names the oldest Series A, or None when the ticket captured no A', async () => {
    const withA = clientFor(chain([[ticketAddress(), buildAccount('Ticket', { id: 2n, phase: 1, aCount: 3n, oldest: addr(55) })]]));
    const a = byName('apply_senior', (await withA.client.buildAdvancePhase({})).instruction);
    expect(a.backer.pubkey.toBase58()).toBe(addr(55));
    expect(a.backer.isWritable).toBe(true);
    const noA = clientFor(chain([[ticketAddress(), buildAccount('Ticket', { id: 2n, phase: 1, aCount: 0n, oldest: addr(55) })]]));
    expect(byName('apply_senior', (await noA.client.buildAdvancePhase({})).instruction).backer.pubkey.equals(PROGRAM_ID)).toBe(true);
  });

  it('allocate: [snapshot page ro, the Backer the snapshot row names w] per slot', async () => {
    const rows = Array.from({ length: 8 }, (_, i) => ({ key: addr(70 + i), series: i % 2, remaining: 5_000n, slot: BigInt(i) }));
    const accounts = chain([
      [ticketAddress(), buildAccount('Ticket', { id: 3n, phase: 3, capturedLen: 3n, cursor: 1n })],
      [snapshotAddress(0n), buildAccount('SnapshotPage', { index: 0n, ticketId: 3n, rows })],
    ]);
    const plan = await clientFor(accounts).client.buildAdvancePhase({ rows: 8 });
    expect(plan.phase).toBe('Allocate');
    expect(plan.rows).toBe(2);
    const rest = plan.instruction.keys.slice(getInstruction('allocate_rows').accounts.length);
    expect(rest.map((k) => [k.pubkey.toBase58(), k.isWritable])).toEqual([
      [snapshotAddress(0n).toBase58(), false],
      [addr(71), true],
      [snapshotAddress(0n).toBase58(), false],
      [addr(72), true],
    ]);
    expect(bytesToHex(plan.instruction.data)).toBe(bytesToHex(encodeInstructionData('allocate_rows', { expectedTicketId: 3n, expectedCursor: 1n, rows: 2 })));
  });

  it('prune reads the live Backers and plans the swap-delete; skipNewlyCapped defers it', async () => {
    const keys = [addr(10), addr(11), addr(12), addr(13), ...Array(4).fill(addr(0))];
    const rows = [0, 1, 2, 3, 4, 5, 6, 7].map((i) => ({ key: keys[i], series: 0, remaining: 1_000_000n, slot: BigInt(i) }));
    const accounts = chain([
      [poolAddress(), buildAccount('Pool', { registryLen: 4n })],
      [ticketAddress(), buildAccount('Ticket', { id: 4n, phase: 4, capturedLen: 4n, cursor: 4n })],
      [registryAddress(0n), buildAccount('RegistryPage', { index: 0n, keys })],
      [snapshotAddress(0n), buildAccount('SnapshotPage', { index: 0n, ticketId: 4n, rows })],
      ...[10, 11, 12, 13].map((n): [PublicKey, Uint8Array] => [
        new PublicKey(addr(n)),
        buildAccount('Backer', { entitlement: 1_000_000n, lifetime: n === 12 ? 1_000_000n : 10n, series: 0 }),
      ]),
    ]);
    const { client } = clientFor(accounts);
    const plan = await client.buildAdvancePhase({});
    expect(plan.phase).toBe('Prune');
    expect(plan.rows).toBe(4);
    const rest = plan.instruction.keys.slice(getInstruction('prune_rows').accounts.length);
    // rows walk 3, 2, 1, 0: slot 2 is capped by this ticket (lifetime == entitlement) and swaps with slot 3's Backer
    expect(rest.slice(5, 10).map((k) => k.pubkey.toBase58())).toEqual([snapshotAddress(0n).toBase58(), addr(12), registryAddress(0n).toBase58(), registryAddress(0n).toBase58(), addr(13)]);

    const deferred = await client.buildAdvancePhase({ skipNewlyCapped: true });
    const restDeferred = deferred.instruction.keys.slice(getInstruction('prune_rows').accounts.length);
    expect(restDeferred.filter((k) => k.pubkey.equals(PROGRAM_ID))).toHaveLength(16);
  });

  it('commit carries the CloseRecord only for a closing ticket', async () => {
    const closing = clientFor(chain([[ticketAddress(), buildAccount('Ticket', { id: 5n, phase: 5, closeId: 6n })]]));
    expect(byName('commit_ticket', (await closing.client.buildAdvancePhase({})).instruction).close_record.pubkey.equals(closeRecordAddress(6n))).toBe(true);
    const plain = clientFor(chain([[ticketAddress(), buildAccount('Ticket', { id: 5n, phase: 5 })]]));
    expect(byName('commit_ticket', (await plain.client.buildAdvancePhase({})).instruction).close_record.pubkey.equals(PROGRAM_ID)).toBe(true);
  });

  it('advancePhase emits exactly one of the six typed instructions for the ticket phase, and refuses Idle', async () => {
    const names = ['discover_rows', 'apply_senior', 'seal_weights', 'allocate_rows', 'prune_rows', 'commit_ticket'];
    expect(Object.values(PHASE_INSTRUCTION)).toEqual(names);
    for (let phase = 0; phase <= 5; phase += 1) {
      const { client } = clientFor(chain([
        [poolAddress(), buildAccount('Pool')],
        [ticketAddress(), buildAccount('Ticket', { id: 9n, phase, capturedLen: 0n, cursor: 0n })],
      ]));
      const plan = await client.buildAdvancePhase({});
      expect(plan.instructionName).toBe(names[phase]);
      expect(bytesToHex(plan.instruction.data.subarray(0, 8))).toBe(bytesToHex(instructionDiscriminator(names[phase])));
    }
    const idle = clientFor(chain([[ticketAddress(), buildAccount('Ticket', { id: 9n, phase: 6 })]]));
    await expect(idle.client.buildAdvancePhase({})).rejects.toBeInstanceOf(NativeStateError);
    await expect(clientFor(new Map()).client.buildAdvancePhase({})).rejects.toBeInstanceOf(NativeStateError);
  });

  it('advancePhase sends the one instruction and reports the phase', async () => {
    const { client, wallet } = clientFor(chain([[ticketAddress(), buildAccount('Ticket', { id: 2n, phase: 2 })]]));
    const result = await client.advancePhase({});
    expect(result.phase).toBe('Seal');
    const tx = wallet.signTransaction.mock.calls[0][0] as Transaction;
    expect(tx.instructions).toHaveLength(2); // compute budget + seal_weights
    expect(bytesToHex(tx.instructions[1].data)).toBe(bytesToHex(encodeInstructionData('seal_weights', { expectedTicketId: 2n })));
  });
});

// ---------------------------------------------------------------------------
// Closing round
// ---------------------------------------------------------------------------

describe('NativeClient closing-round steps', () => {
  const provider = { program: addr(81), network: addr(82), treasury: addr(83), client: addr(84) };
  const configAccount = (version: number) =>
    buildAccount('ConfigVersion', {
      version,
      providerProgram: provider.program,
      providerNetworkState: provider.network,
      providerTreasury: provider.treasury,
      providerClient: provider.client,
    });

  it('build_candidates: [participant page, UserRound, User] per slot and candidate pages only when provisioned', async () => {
    const owners = page8(120);
    const accounts = chain([
      [closeRecordAddress(3n), buildAccount('CloseRecord', {
        closeId: 3n, roundId: 2n, participantCount: 10n, candidateCursor: 8n,
        preferredCount: 0n, provisionedPreferredPages: 1n, fallbackCount: 0n, provisionedFallbackPages: 0n,
        ownerRefundDestination: addr(90),
      })],
      [participantAddress(2n, 1n), buildAccount('ParticipantPage', { roundId: 2n, page: 1n, len: 2, rows: owners })],
    ]);
    const { client, wallet } = clientFor(accounts);
    await client.buildCandidates({ closeId: 3n });
    const tx = wallet.signTransaction.mock.calls[0][0] as Transaction;
    const ix = tx.instructions[1];
    const a = byName('build_candidates', ix);
    expect(a.preferred_page.pubkey.equals(candidateAddress(3n, 0, 0n))).toBe(true);
    expect(a.fallback_page.pubkey.equals(PROGRAM_ID)).toBe(true);
    expect(a.owner_refund.pubkey.toBase58()).toBe(addr(90));
    expect(a.ops.pubkey.equals(opsAddress())).toBe(true);
    const rest = ix.keys.slice(getInstruction('build_candidates').accounts.length);
    expect(rest.map((k) => k.pubkey.toBase58())).toEqual([
      participantAddress(2n, 1n).toBase58(), userRoundAddress(2n, owners[0]).toBase58(), userAddress(owners[0]).toBase58(),
      participantAddress(2n, 1n).toBase58(), userRoundAddress(2n, owners[1]).toBase58(), userAddress(owners[1]).toBase58(),
    ]);
    expect(rest.every((k) => !k.isWritable)).toBe(true);
    expect(bytesToHex(ix.data)).toBe(bytesToHex(encodeInstructionData('build_candidates', { rows: 2 })));
  });

  it('request_round_draw: names the ORAO request PDA of the protocol seed (vector: seed-vectors.json, litesvm domain)', async () => {
    const vector = readSamplingFixture();
    const seedVector = vector.protocol_seed.vectors.find((v) => v.cluster === 'litesvm-fixture')!;
    const accounts = chain([
      [poolAddress(), buildAccount('Pool', { clusterDomain: new TextEncoder().encode(seedVector.cluster_domain) })],
      [closeRecordAddress(1n), buildAccount('CloseRecord', {
        closeId: 1n, roundId: vector.protocol_seed.round_id, configVersion: 1, sealed: true, selectedSetTag: vector.protocol_seed.set_tag,
        preferredCount: BigInt(vector.protocol_seed.selected_count), preferredDigest: hexToBytes(vector.protocol_seed.sealed_digest_hex),
      })],
      [configAddress(1), configAccount(1)],
    ]);
    const { client, wallet } = clientFor(accounts);
    await client.requestDraw({ closeId: 1n });
    const ix = (wallet.signTransaction.mock.calls[0][0] as Transaction).instructions[1];
    const a = byName('request_round_draw', ix);
    const expectedRequest = oraoRequestAddress(provider.client, hexToBytes(seedVector.seed_hex), provider.program);
    expect(a.request.pubkey.toBase58()).toBe(expectedRequest.toBase58());
    expect(a.provider_program.pubkey.toBase58()).toBe(provider.program);
    expect(a.network_state.pubkey.toBase58()).toBe(provider.network);
    expect(a.treasury.pubkey.toBase58()).toBe(provider.treasury);
    expect(a.client.pubkey.toBase58()).toBe(provider.client);
    expect(a.rng_authority.pubkey.equals(rngAuthorityAddress())).toBe(true);
    expect(a.config.pubkey.equals(configAddress(1))).toBe(true);
    expect(a.payer.isSigner).toBe(true);
  });

  it('request_round_draw refuses an unsealed close', async () => {
    const accounts = chain([
      [poolAddress(), buildAccount('Pool')],
      [closeRecordAddress(1n), buildAccount('CloseRecord', { closeId: 1n, sealed: false })],
      [configAddress(0), configAccount(0)],
    ]);
    await expect(clientFor(accounts).client.requestDraw({ closeId: 1n })).rejects.toBeInstanceOf(NativeStateError);
  });

  it('apply_promotion maps the captured output to the winner’s accounts (vector: sampling.json ordinal 18 of 20)', async () => {
    const vector = readSamplingFixture().output_vectors.find((v) => v.label === 'ramp output, twenty candidates')!;
    expect([vector.candidate_count, vector.ordinal].map(Number)).toEqual([20, 18]);
    const winner = addr(130);
    const rows = page8(200);
    rows[2] = winner; // ordinal 18 = page 2, row 2
    const accounts = chain([
      [poolAddress(), buildAccount('Pool', { registryLen: 5n })],
      [closeRecordAddress(1n), buildAccount('CloseRecord', {
        closeId: 1n, roundId: 1n, selectedSetTag: 0, preferredCount: 20n, drawState: 2, randomness: hexToBytes(vector.output_hex), ownerRefundDestination: addr(90),
      })],
      [candidateAddress(1n, 0, 2n), buildAccount('CandidatePage', { closeId: 1n, setTag: 0, page: 2n, len: 4, rows })],
    ]);
    const { client, wallet } = clientFor(accounts);
    await client.applyPromotion({ closeId: 1n });
    const a = byName('apply_promotion', (wallet.signTransaction.mock.calls[0][0] as Transaction).instructions[1]);
    expect(a.candidate_page.pubkey.equals(candidateAddress(1n, 0, 2n))).toBe(true);
    expect(a.winner.pubkey.toBase58()).toBe(winner);
    expect(a.winner_user.pubkey.equals(userAddress(winner))).toBe(true);
    expect(a.winner_user_round.pubkey.equals(userRoundAddress(1n, winner))).toBe(true);
    expect(a.winner_backer_b.pubkey.equals(backerAddress(1, winner))).toBe(true);
    expect(a.registry_tail.pubkey.equals(registryAddress(0n))).toBe(true); // 5 / 8
    expect(a.owner_refund.pubkey.toBase58()).toBe(addr(90));
  });

  it('apply_promotion passes only None for the winner side when nothing maps (no candidates)', async () => {
    const accounts = chain([
      [poolAddress(), buildAccount('Pool')],
      [closeRecordAddress(1n), buildAccount('CloseRecord', { closeId: 1n, roundId: 1n, selectedSetTag: 1, fallbackCount: 0n, drawState: 2, ownerRefundDestination: addr(90) })],
    ]);
    const { client, wallet } = clientFor(accounts);
    await client.applyPromotion({ closeId: 1n });
    const a = byName('apply_promotion', (wallet.signTransaction.mock.calls[0][0] as Transaction).instructions[1]);
    for (const name of ['candidate_page', 'winner', 'winner_user', 'winner_user_round', 'winner_backer_b', 'registry_tail']) {
      expect(a[name].pubkey.equals(PROGRAM_ID), name).toBe(true);
    }
  });

  it('capture, expire and finalize name the close record and the provider client', async () => {
    const accounts = chain([
      [closeRecordAddress(4n), buildAccount('CloseRecord', { closeId: 4n, configVersion: 1, requestAccount: addr(150), ownerRefundDestination: addr(90) })],
      [configAddress(1), configAccount(1)],
    ]);
    const { client, wallet } = clientFor(accounts);
    await client.captureDraw({ closeId: 4n });
    await client.expirePromotion({ closeId: 4n });
    await client.finalizeRound({ closeId: 4n });
    const sent = wallet.signTransaction.mock.calls.map((c: unknown[]) => (c[0] as Transaction).instructions[1]);
    const capture = byName('capture_round_draw', sent[0]);
    expect(capture.request.pubkey.toBase58()).toBe(addr(150));
    expect(capture.client.pubkey.toBase58()).toBe(provider.client);
    expect(byName('expire_promotion', sent[1]).owner_refund.pubkey.toBase58()).toBe(addr(90));
    expect(byName('finalize_round', sent[2]).close_record.pubkey.equals(closeRecordAddress(4n))).toBe(true);
  });

  it('composeExpireFinalizeOpen: expire (if Pending) + finalize + open into round + 1, in one transaction', async () => {
    const closeData = (promotion: unknown) => buildAccount('CloseRecord', { closeId: 3n, roundId: 5n, promotion, ownerRefundDestination: addr(90) });
    const build = (promotion: unknown) =>
      chain([
        [poolAddress(), buildAccount('Pool', { roundId: 5n, currentCloseId: 3n, activeConfigVersion: 2, participantCount: 40n })],
        [closeRecordAddress(3n), closeData(promotion)],
      ]);

    const pending = clientFor(build('Pending'));
    const result = await pending.client.composeExpireFinalizeOpen({ closeId: 3n, plan: 'Simple', gross: 10_000_000n, nonce: 9n });
    expect(result.nonce).toBe(9n);
    const tx = pending.wallet.signTransaction.mock.calls[0][0] as Transaction;
    expect(tx.instructions).toHaveLength(4);
    expect(tx.instructions.slice(1).map((i) => bytesToHex(i.data.subarray(0, 8)))).toEqual(
      ['expire_promotion', 'finalize_round', 'open_position'].map((n) => bytesToHex(instructionDiscriminator(n))),
    );
    const open = tx.instructions[3];
    const o = byName('open_position', open);
    expect(o.user_round.pubkey.equals(userRoundAddress(6n, OWNER_OF(pending.wallet)))).toBe(true);
    expect(o.participant_page.pubkey.equals(participantAddress(6n, 0n))).toBe(true); // empty list in the new round
    expect(o.config.pubkey.equals(configAddress(2))).toBe(true);
    expect(bytesToHex(open.data)).toBe(
      bytesToHex(encodeInstructionData('open_position', { plan: 'Simple', gross: 10_000_000n, clientNonce: 9n, expectedRoundId: 6n, expectedConfig: 2 })),
    );

    // A promotion that is already decided needs no expiry.
    const applied = clientFor(build({ kind: 'Applied', winner: addr(5), entitlement: 1n }));
    await applied.client.composeExpireFinalizeOpen({ closeId: 3n, plan: 'Compound30', gross: 10_000_000n });
    expect((applied.wallet.signTransaction.mock.calls[0][0] as Transaction).instructions).toHaveLength(3);
  });

  it('composeExpireFinalizeOpen refuses a close that is not the one in progress or is already finalized', async () => {
    const other = chain([
      [poolAddress(), buildAccount('Pool', { roundId: 5n, currentCloseId: 7n })],
      [closeRecordAddress(3n), buildAccount('CloseRecord', { closeId: 3n, roundId: 5n })],
    ]);
    await expect(clientFor(other).client.composeExpireFinalizeOpen({ closeId: 3n, plan: 'Simple', gross: 1n })).rejects.toBeInstanceOf(NativeStateError);
    const done = chain([
      [poolAddress(), buildAccount('Pool', { roundId: 6n, currentCloseId: null })],
      [closeRecordAddress(3n), buildAccount('CloseRecord', { closeId: 3n, roundId: 5n, finalized: true })],
    ]);
    await expect(clientFor(done).client.composeExpireFinalizeOpen({ closeId: 3n, plan: 'Simple', gross: 1n })).rejects.toBeInstanceOf(NativeStateError);
  });
});

const OWNER_OF = (wallet: { publicKey: string | null }): PublicKey => new PublicKey(wallet.publicKey!);

describe('NativeClient player flows', () => {
  it('withdrawSimple reads the position, binds a provisioned next CloseRecord and offers inline allocation on request', async () => {
    const wallet = fakeWallet();
    const owner = wallet.keypair.publicKey;
    const keys = page8(60);
    const accounts = chain([
      [poolAddress(), buildAccount('Pool', { roundId: 3n, nextCloseId: 8n, registryLen: 3n })],
      [positionAddress(owner, 5n), buildAccount('Position', { owner: owner.toBase58(), nonce: 5n, configVersion: 1 })],
      [closeRecordAddress(8n), buildAccount('CloseRecord', { closeId: 8n })],
      [registryAddress(0n), buildAccount('RegistryPage', { index: 0n, keys })],
    ]);
    const connection = fakeConnection(accounts);
    const client = createNativeClient({ connection, wallet, sessionPubkey: wallet.publicKey!, sleep: async () => {} });

    await client.withdrawSimple({ nonce: 5n, minimumReceive: 1_000n });
    const plain = (wallet.signTransaction.mock.calls[0][0] as Transaction).instructions;
    expect(plain[0].programId.equals(ComputeBudgetProgram.programId)).toBe(true);
    const a = byName('withdraw_simple', plain[1]);
    expect(a.close_record.pubkey.equals(closeRecordAddress(8n))).toBe(true);
    expect(a.destination.pubkey.equals(owner)).toBe(true);
    expect(plain[1].keys).toHaveLength(getInstruction('withdraw_simple').accounts.length);

    await client.withdrawSimple({ nonce: 5n, minimumReceive: 1_000n, inline: true });
    const inline = (wallet.signTransaction.mock.calls[1][0] as Transaction).instructions[1];
    const idlCount = getInstruction('withdraw_simple').accounts.length;
    expect(inline.keys.slice(idlCount).map((k) => k.pubkey.toBase58())).toEqual([
      keys[0], keys[1], keys[2], registryAddress(0n).toBase58(), snapshotAddress(0n).toBase58(),
    ]);
  });

  it('withdrawSimple passes None for a CloseRecord that is not provisioned yet, and settleCompounding mirrors it', async () => {
    const wallet = fakeWallet();
    const owner = wallet.keypair.publicKey;
    const accounts = chain([
      [poolAddress(), buildAccount('Pool', { roundId: 3n, nextCloseId: 8n })],
      [positionAddress(owner, 5n), buildAccount('Position', { owner: owner.toBase58(), nonce: 5n, configVersion: 1, kind: 1 })],
    ]);
    const client = createNativeClient({ connection: fakeConnection(accounts), wallet, sessionPubkey: wallet.publicKey!, sleep: async () => {} });
    await client.settleCompounding({ nonce: 5n, minimumReceive: 0n, maxWorkContribution: 5_000_000n });
    const ix = (wallet.signTransaction.mock.calls[0][0] as Transaction).instructions[1];
    expect(byName('settle_compounding', ix).close_record.pubkey.equals(PROGRAM_ID)).toBe(true);
    expect(bytesToHex(ix.data)).toBe(bytesToHex(encodeInstructionData('settle_compounding', { minimumReceive: 0n, maxWorkContribution: 5_000_000n })));
    await expect(client.withdrawSimple({ nonce: 999n, minimumReceive: 0n })).rejects.toBeInstanceOf(NativeStateError);
  });

  it('claimBacker claims whichever series exist in one instruction', async () => {
    const wallet = fakeWallet();
    const owner = wallet.keypair.publicKey;
    const accounts = chain([[backerAddress(1, owner), buildAccount('Backer', { owner: owner.toBase58(), series: 1 })]]);
    const client = createNativeClient({ connection: fakeConnection(accounts), wallet, sessionPubkey: wallet.publicKey!, sleep: async () => {} });
    await client.claimBacker({});
    const a = byName('claim_backer', (wallet.signTransaction.mock.calls[0][0] as Transaction).instructions[1]);
    expect(a.backer_a.pubkey.equals(PROGRAM_ID)).toBe(true);
    expect(a.backer_b.pubkey.equals(backerAddress(1, owner))).toBe(true);
    await expect(createNativeClient({ connection: fakeConnection(new Map()), wallet, sessionPubkey: wallet.publicKey!, sleep: async () => {} }).claimBacker({})).rejects.toBeInstanceOf(NativeStateError);
  });

  it('fundSeriesA and closeSimplePosition read what they need', async () => {
    const wallet = fakeWallet();
    const owner = wallet.keypair.publicKey;
    const accounts = chain([
      [poolAddress(), buildAccount('Pool', { roundId: 2n, activeConfigVersion: 1, registryLen: 9n })],
      [positionAddress(owner, 5n), buildAccount('Position', { owner: owner.toBase58(), nonce: 5n, configVersion: 3 })],
    ]);
    const client = createNativeClient({ connection: fakeConnection(accounts), wallet, sessionPubkey: wallet.publicKey!, sleep: async () => {} });
    await client.fundSeriesA({ gross: 50_000_000n });
    await client.closeSimplePosition({ nonce: 5n });
    const [fund, close] = wallet.signTransaction.mock.calls.map((c: unknown[]) => (c[0] as Transaction).instructions[1]);
    expect(byName('fund_series_a', fund).registry.pubkey.equals(registryAddress(1n))).toBe(true);
    expect(byName('close_simple_position', close).config.pubkey.equals(configAddress(3))).toBe(true);
    await expect(client.fundSeriesA({ gross: 0n })).rejects.toThrow(/positive/);
  });

  it('every IDL error code resolves to its name when a rejection is explained', () => {
    expect(idl.errors).toHaveLength(52);
    for (const error of idl.errors) {
      expect(describeTransactionError({ InstructionError: [1, { Custom: error.code }] })).toEqual({ code: error.code, name: error.name });
      expect(describeTransactionError(new Error(`custom program error: 0x${error.code.toString(16)}`))).toEqual({ code: error.code, name: error.name });
    }
    expect(describeTransactionError({ InstructionError: [0, 'InvalidAccountData'] })).toBeNull();
  });
});
