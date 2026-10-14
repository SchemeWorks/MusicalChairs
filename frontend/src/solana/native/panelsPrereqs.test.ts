// IMPL-46c browser finding F1: shared pages a player action reads must exist before the action is
// built. The need is derived from the pool's own page counters (provision_page takes the NEXT
// sequential index), and each preparatory transaction must land and be visible before the next step.
import { describe, expect, it, vi } from 'vitest';
import {
  describePrepStage,
  ensurePrereqs,
  fundSeriesAPrereqs,
  openPrereqs,
  participantPrereqs,
  payoutPrereqs,
  registryPrereqs,
  runPrepared,
  snapshotPrereqs,
  type PrereqReaders,
} from './panelsPrereqs';
import { PAGE_CAPACITY } from './codecs';
import { testPool, OWNER } from './panelsTestFixtures';
import type { NativeTxResult, StatusUpdate } from './types';

const tx = (signature: string): NativeTxResult =>
  ({ signature, nonce: null, status: 'submitted', status$: (async function* () {})(), retry: async () => ({ signature, status: 'confirmed' }), safeToRebuild: false }) as unknown as NativeTxResult;
const instant = async () => undefined;
const landed = async (t: NativeTxResult): Promise<StatusUpdate> => ({ signature: t.signature, status: 'confirmed' });

describe('what is missing, from the pool counters', () => {
  it('page capacity is 8 rows', () => {
    expect(PAGE_CAPACITY).toBe(8);
  });

  it('a fresh round (after finalize_round) needs participant page 0 of THAT round', () => {
    const pool = testPool({ roundId: 2n, participantCount: 0n, participantPages: 0n });
    expect(participantPrereqs(pool)).toEqual([
      { kind: 'participant', index: 0n, page: { kind: 'Participant', round: 2n }, label: "this round's participant page" },
    ]);
  });

  it('is satisfied while the next participant still lands on a provisioned page, and grows with the participant count', () => {
    expect(participantPrereqs(testPool({ roundId: 1n, participantCount: 3n, participantPages: 1n }))).toEqual([]);
    expect(participantPrereqs(testPool({ roundId: 1n, participantCount: 7n, participantPages: 1n }))).toEqual([]);
    // the 9th participant lands on index 1
    const full = participantPrereqs(testPool({ roundId: 1n, participantCount: 8n, participantPages: 1n }));
    expect(full.map((p) => p.index)).toEqual([1n]);
  });

  it('provisions sequentially from the counter when more than one page is missing', () => {
    const pool = testPool({ roundId: 1n, participantCount: 16n, participantPages: 1n });
    expect(participantPrereqs(pool).map((p) => p.index)).toEqual([1n, 2n]);
  });

  it('registry growth: a new open Backer row at registry_len / 8', () => {
    expect(registryPrereqs(testPool({ registryLen: 3n, registryPages: 1n }))).toEqual([]);
    expect(registryPrereqs(testPool({ registryLen: 8n, registryPages: 1n })).map((p) => [p.kind, p.index, p.page.kind])).toEqual([['registry', 1n, 'Registry']]);
  });

  it('snapshot pages cover the registry: snapshot_pages x 8 >= registry_len', () => {
    expect(snapshotPrereqs(testPool({ registryLen: 0n, snapshotPages: 0n }))).toEqual([]);
    expect(snapshotPrereqs(testPool({ registryLen: 8n, snapshotPages: 1n }))).toEqual([]);
    expect(snapshotPrereqs(testPool({ registryLen: 9n, snapshotPages: 1n })).map((p) => p.index)).toEqual([1n]);
    expect(snapshotPrereqs(testPool({ registryLen: 17n, snapshotPages: 1n })).map((p) => p.index)).toEqual([1n, 2n]);
    expect(payoutPrereqs(testPool({ registryLen: 9n, snapshotPages: 1n }))).toHaveLength(1);
  });
});

function fakeReaders(opts: { userRoundDeposits?: bigint | null; backerActive?: boolean | null; visible?: () => boolean } = {}): PrereqReaders {
  const visible = opts.visible ?? (() => true);
  const pages = async (indices: bigint[]) => new Map(indices.filter(() => visible()).map((i) => [i, {}]));
  return {
    readPool: vi.fn(async () => testPool({ roundId: 2n, participantCount: 0n, participantPages: 0n })),
    readUserRound: vi.fn(async () => (opts.userRoundDeposits === null || opts.userRoundDeposits === undefined ? null : { deposits: opts.userRoundDeposits })),
    readBacker: vi.fn(async () => (opts.backerActive === null || opts.backerActive === undefined ? null : { active: opts.backerActive })),
    readParticipantPages: vi.fn(async (_round: bigint, indices: bigint[]) => pages(indices)),
    readRegistryPages: vi.fn(async (indices: bigint[]) => pages(indices)),
    readSnapshotPages: vi.fn(async (indices: bigint[]) => pages(indices)),
  };
}

describe('which actions need which page', () => {
  it('open_position needs the participant page only on the first position of the round', async () => {
    const pool = testPool({ roundId: 2n, participantCount: 0n, participantPages: 0n });
    expect(await openPrereqs(fakeReaders({ userRoundDeposits: null }), pool, OWNER)).toHaveLength(1);
    expect(await openPrereqs(fakeReaders({ userRoundDeposits: 0n }), pool, OWNER)).toHaveLength(1);
    expect(await openPrereqs(fakeReaders({ userRoundDeposits: 5n }), pool, OWNER)).toEqual([]);
  });

  it('fund_series_a needs the registry page only when it appends a new open Backer row', async () => {
    const pool = testPool({ registryLen: 8n, registryPages: 1n });
    expect(await fundSeriesAPrereqs(fakeReaders({ backerActive: null }), pool, OWNER)).toHaveLength(1);
    expect(await fundSeriesAPrereqs(fakeReaders({ backerActive: false }), pool, OWNER)).toHaveLength(1);
    expect(await fundSeriesAPrereqs(fakeReaders({ backerActive: true }), pool, OWNER)).toEqual([]);
  });
});

describe('ensurePrereqs and runPrepared', () => {
  const prereq = participantPrereqs(testPool({ roundId: 2n, participantCount: 0n, participantPages: 0n }));

  it('provisions with Caller funding, waits for it to land and be visible, THEN sends the action', async () => {
    const order: string[] = [];
    let provisioned = false;
    const readers = fakeReaders({ visible: () => provisioned });
    const client = {
      provisionPage: vi.fn(async () => {
        order.push('provision');
        provisioned = true;
        return tx('page');
      }),
    };
    const stages: string[] = [];
    const out = await runPrepared(client, readers, () => prereq, async () => {
      order.push('open');
      return tx('open');
    }, { wait: landed, sleep: instant, onStage: (s) => stages.push(describePrepStage(s, 'your deposit')) });
    expect(client.provisionPage).toHaveBeenCalledWith({ page: { kind: 'Participant', round: 2n }, index: 0n, funding: 'Caller' });
    expect(order).toEqual(['provision', 'open']);
    expect(out.prep.map((p) => p.tx.signature)).toEqual(['page']);
    expect(out.result?.signature).toBe('open');
    expect(stages[0]).toMatch(/Step 1 of 2: preparing this round's participant page/);
    expect(stages[1]).toBe('Sending your deposit…');
  });

  it('sends nothing but the action when the page is already there (another wallet provisioned it)', async () => {
    const client = { provisionPage: vi.fn(async () => tx('page')) };
    const out = await runPrepared(client, fakeReaders(), () => prereq, async () => tx('open'), { wait: landed, sleep: instant });
    expect(client.provisionPage).not.toHaveBeenCalled();
    expect(out.prep).toEqual([]);
    expect(out.result?.signature).toBe('open');
  });

  it('does not send the action when the preparation fails or expires (nothing else was sent)', async () => {
    const client = { provisionPage: vi.fn(async () => tx('page')) };
    const send = vi.fn(async () => tx('open'));
    const readers = fakeReaders({ visible: () => false });
    await expect(runPrepared(client, readers, () => prereq, send, { wait: async () => ({ signature: 'page', status: 'failed' }), sleep: instant })).rejects.toThrow(/did not land/);
    await expect(runPrepared(client, readers, () => prereq, send, { wait: async () => ({ signature: 'page', status: 'expired' }), sleep: instant })).rejects.toThrow(/Nothing else was sent/);
    expect(send).not.toHaveBeenCalled();
  });

  it('returns the preparation with no action when it has not confirmed (unknown) so the UI shows its signature', async () => {
    const client = { provisionPage: vi.fn(async () => tx('page')) };
    const send = vi.fn(async () => tx('open'));
    const out = await runPrepared(client, fakeReaders({ visible: () => false }), () => prereq, send, {
      wait: async () => ({ signature: 'page', status: 'unknown' }),
      sleep: instant,
    });
    expect(out.result).toBeNull();
    expect(out.prep.map((p) => p.tx.signature)).toEqual(['page']);
    expect(send).not.toHaveBeenCalled();
  });

  it('holds the action back while the page is not yet visible after it landed', async () => {
    const client = { provisionPage: vi.fn(async () => tx('page')) };
    const send = vi.fn(async () => tx('open'));
    const out = await runPrepared(client, fakeReaders({ visible: () => false }), () => prereq, send, { wait: landed, sleep: instant });
    expect(out.result).toBeNull();
    expect(send).not.toHaveBeenCalled();
  });

  it('provisions several missing pages one at a time, in index order, with "Step i of N"', async () => {
    const pool = testPool({ roundId: 1n, participantCount: 16n, participantPages: 1n });
    const calls: bigint[] = [];
    const seen = new Set<bigint>();
    const readers = fakeReaders();
    readers.readParticipantPages = vi.fn(async (_r: bigint, indices: bigint[]) => new Map(indices.filter((i) => seen.has(i)).map((i) => [i, {}])));
    const client = {
      provisionPage: vi.fn(async (p: { index: bigint }) => {
        calls.push(p.index);
        seen.add(p.index);
        return tx(`page${p.index}`);
      }),
    };
    const stages: string[] = [];
    const out = await ensurePrereqs(client, readers, participantPrereqs(pool), { wait: landed, sleep: instant, onStage: (s) => stages.push(describePrepStage(s, 'x')) });
    expect(calls).toEqual([1n, 2n]);
    expect(out.blocked).toBeNull();
    expect(stages[0]).toMatch(/Step 1 of 3/);
    expect(stages[1]).toMatch(/Step 2 of 3/);
  });

  it('with no pool read (not initialized) there is nothing to prepare and the action decides', async () => {
    const readers = fakeReaders();
    readers.readPool = vi.fn(async () => null);
    const client = { provisionPage: vi.fn() };
    const out = await runPrepared(client, readers, () => prereq, async () => tx('open'), { wait: landed, sleep: instant });
    expect(client.provisionPage).not.toHaveBeenCalled();
    expect(out.result?.signature).toBe('open');
  });
});
