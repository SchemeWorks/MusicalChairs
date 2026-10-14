// Shared-page prerequisites of the player actions (IMPL-46c, browser finding F1).
//
// Three actions read a shared page that nobody may have provisioned yet, and the pre-sign
// simulation then refuses them ("A required account is not provisioned yet", nothing signed):
//
//   open_position      the round's ParticipantPage at index `participant_count / 8`, read on the
//                      owner's FIRST position of the round. A new round starts with
//                      participant_pages = 0 (finalize_round resets it), so the first open of
//                      every round needs page 0 first. Participant pages are keyed by round, so
//                      round R+1's page cannot exist before finalize: provision_page requires
//                      `round == pool.round_id`.
//   fund_series_a      the RegistryPage at index `registry_len / 8`, read when the funding appends
//                      a new open Backer row (the Backer is not active yet).
//   payout with a toll the SnapshotPage(s) covering the registry: `snapshot_pages * 8 >= registry_len`
//                      (snapshots_cover), or the payout that starts the allocation run fails.
//
// `provision_page` is permissionless and takes the NEXT sequential index (the pool's page
// counters), funded here by the caller (`Funding::Caller`: the wallet fronts the account rent; the
// registry/snapshot pages are permanent, so that rent is a one-time deposit).
//
// Solana vs ICP: on ICP a canister allocates its own storage on demand. A Solana program cannot
// create an account the transaction did not name and fund, so the client must see that the account
// exists, and create it in an earlier transaction when it does not. This is the same pattern as
// the CloseRecord provisioning in panelsClosing.runPayout.
//
// Web3-free: type-only imports of the client; the readers are injected.

import type { NativeClient } from './client';
import { PAGE_CAPACITY } from './codecs';
import { waitUntilLanded } from './panelsClosing';
import type { NativeBacker, NativePool, NativeTxResult, NativeUserRound, PageKindArg, StatusUpdate } from './types';

const CAPACITY = BigInt(PAGE_CAPACITY);

export interface PagePrereq {
  kind: 'participant' | 'registry' | 'snapshot';
  /** The sequential index provision_page must be called with. */
  index: bigint;
  page: PageKindArg;
  /** Sentence fragment for status copy: "the round's participant page". */
  label: string;
}

/** A preparatory transaction that was sent, with what it prepared. */
export interface PrepTx {
  tx: NativeTxResult;
  label: string;
}

// ---------------------------------------------------------------------------
// What is missing, from the pool's own page counters
// ---------------------------------------------------------------------------

function sequence(have: bigint, needInclusive: bigint, make: (index: bigint) => PagePrereq): PagePrereq[] {
  const out: PagePrereq[] = [];
  for (let index = have; index <= needInclusive; index += 1n) out.push(make(index));
  return out;
}

/** Participant pages the NEXT participant of the round needs (index participant_count / 8, sequential from participant_pages). */
export function participantPrereqs(pool: Pick<NativePool, 'roundId' | 'participantCount' | 'participantPages'>): PagePrereq[] {
  return sequence(pool.participantPages, pool.participantCount / CAPACITY, (index) => ({
    kind: 'participant',
    index,
    page: { kind: 'Participant', round: pool.roundId },
    label: "this round's participant page",
  }));
}

/** Registry pages a new open Backer row needs (index registry_len / 8, sequential from registry_pages). */
export function registryPrereqs(pool: Pick<NativePool, 'registryLen' | 'registryPages'>): PagePrereq[] {
  return sequence(pool.registryPages, pool.registryLen / CAPACITY, (index) => ({
    kind: 'registry',
    index,
    page: { kind: 'Registry' },
    label: 'the Backer registry page',
  }));
}

/** Snapshot pages covering the registry: snapshot_pages x 8 >= registry_len. */
export function snapshotPrereqs(pool: Pick<NativePool, 'registryLen' | 'snapshotPages'>): PagePrereq[] {
  if (pool.registryLen === 0n) return [];
  const needed = (pool.registryLen + CAPACITY - 1n) / CAPACITY;
  return sequence(pool.snapshotPages, needed - 1n, (index) => ({
    kind: 'snapshot',
    index,
    page: { kind: 'Snapshot' },
    label: 'the allocation snapshot page',
  }));
}

/** The slice of the verified readers these checks use. */
export interface PrereqReaders {
  readPool(): Promise<NativePool | null>;
  readUserRound(round: bigint, owner: string): Promise<Pick<NativeUserRound, 'deposits'> | null>;
  readBacker(owner: string, series: 0 | 1): Promise<Pick<NativeBacker, 'active'> | null>;
  readParticipantPages(round: bigint, indices: bigint[]): Promise<Map<bigint, unknown>>;
  readRegistryPages(indices: bigint[]): Promise<Map<bigint, unknown>>;
  readSnapshotPages(indices: bigint[]): Promise<Map<bigint, unknown>>;
}

/** open_position reads the participant page only on the owner's first position of the round. */
export async function openPrereqs(readers: Pick<PrereqReaders, 'readUserRound'>, pool: NativePool, owner: string): Promise<PagePrereq[]> {
  const userRound = await readers.readUserRound(pool.roundId, owner);
  if (userRound && userRound.deposits > 0n) return [];
  return participantPrereqs(pool);
}

/** fund_series_a appends a registry row only for a Backer that is not active yet. */
export async function fundSeriesAPrereqs(readers: Pick<PrereqReaders, 'readBacker'>, pool: NativePool, owner: string): Promise<PagePrereq[]> {
  const backer = await readers.readBacker(owner, 0);
  if (backer?.active) return [];
  return registryPrereqs(pool);
}

/** A payout that charges a toll starts an allocation run, which needs the snapshot pages. Conservative: any payout. */
export function payoutPrereqs(pool: NativePool): PagePrereq[] {
  return snapshotPrereqs(pool);
}

async function isVisible(readers: PrereqReaders, prereq: PagePrereq): Promise<boolean> {
  switch (prereq.kind) {
    case 'participant': {
      const round = prereq.page.kind === 'Participant' ? prereq.page.round : 0n;
      return (await readers.readParticipantPages(round, [prereq.index])).has(prereq.index);
    }
    case 'registry':
      return (await readers.readRegistryPages([prereq.index])).has(prereq.index);
    default:
      return (await readers.readSnapshotPages([prereq.index])).has(prereq.index);
  }
}

// ---------------------------------------------------------------------------
// Running them
// ---------------------------------------------------------------------------

export type PrepStage =
  | { kind: 'preparing'; label: string; step: number; of: number }
  | { kind: 'sending' };

export const describePrepStage = (stage: PrepStage, action: string): string =>
  stage.kind === 'preparing'
    ? `Step ${stage.step} of ${stage.of}: preparing ${stage.label} (a one-time account deposit paid by your wallet)…`
    : `Sending ${action}…`;

export interface PrepOutcome {
  prep: PrepTx[];
  /** A preparatory transaction that has not landed (or whose page is not visible yet): the action was NOT sent. */
  blocked: PrepTx | null;
}

export interface PrepOptions {
  onStage?: (stage: PrepStage) => void;
  wait?: (tx: NativeTxResult) => Promise<StatusUpdate>;
  sleep?: (ms: number) => Promise<void>;
  /** Steps counted in "Step i of N" beyond the preparatory ones (the action itself). */
  totalSteps?: number;
}

const LANDED = new Set(['confirmed', 'finalized']);

/**
 * Provision each missing page in order, waiting for it to land and be visible before the next
 * step. Stops (blocked) when one has not landed so the caller shows its signature instead of
 * sending an action the program would refuse.
 */
export async function ensurePrereqs(
  client: Pick<NativeClient, 'provisionPage'>,
  readers: PrereqReaders,
  prereqs: PagePrereq[],
  opts: PrepOptions = {},
): Promise<PrepOutcome> {
  const wait = opts.wait ?? waitUntilLanded;
  const sleep = opts.sleep ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)));
  const prep: PrepTx[] = [];
  const of = prereqs.length + (opts.totalSteps ?? 1);
  let step = 0;
  for (const prereq of prereqs) {
    if (await isVisible(readers, prereq)) continue; // already there (another wallet got there first)
    step += 1;
    opts.onStage?.({ kind: 'preparing', label: prereq.label, step, of });
    const tx = await client.provisionPage({ page: prereq.page, index: prereq.index, funding: 'Caller' });
    const sent = { tx, label: `Preparing ${prereq.label}` };
    prep.push(sent);
    const landed = await wait(tx);
    if (landed.status === 'failed' || landed.status === 'expired') {
      throw new Error(`Preparing ${prereq.label} did not land. Nothing else was sent; try again.`);
    }
    if (!LANDED.has(landed.status)) return { prep, blocked: sent };
    // The action reads the page at `confirmed`; give the node a moment to show it.
    let visible = false;
    for (let attempt = 0; attempt < 6 && !visible; attempt += 1) {
      visible = await isVisible(readers, prereq);
      if (!visible) await sleep(500);
    }
    if (!visible) return { prep, blocked: sent };
  }
  return { prep, blocked: null };
}

export interface PreparedOutcome<T> {
  prep: PrepTx[];
  /** The action's outcome; null when a preparatory step has not landed (the action was not sent). */
  result: T | null;
}

/** Prepare, then send. `prereqsOf` runs against FRESH state (the pool as read now, not as last rendered). */
export async function runPrepared<T>(
  client: Pick<NativeClient, 'provisionPage'>,
  readers: PrereqReaders,
  prereqsOf: (pool: NativePool) => Promise<PagePrereq[]> | PagePrereq[],
  send: () => Promise<T>,
  opts: PrepOptions = {},
): Promise<PreparedOutcome<T>> {
  const pool = await readers.readPool();
  const prereqs = pool ? await prereqsOf(pool) : [];
  const { prep, blocked } = await ensurePrereqs(client, readers, prereqs, opts);
  if (blocked) return { prep, result: null };
  opts.onStage?.({ kind: 'sending' });
  return { prep, result: await send() };
}
