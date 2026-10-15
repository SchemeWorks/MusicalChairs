// IMPL-49c: the verified native activity feed beside the legacy ledger. Static markup (no jsdom): the two feeds
// are separate and labeled, and intake rows (pending / unverified / invalid) can never render.
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { Entry, Status } from '../declarations/native_sol_bridge/native_sol_bridge.did';

const h = vi.hoisted(() => ({
  walletType: 'siws' as string,
  history: { enabled: false, reason: 'not-configured' } as unknown,
  availability: { state: 'native', pool: null, reason: null, error: null } as Record<string, unknown>,
  calls: 0,
}));

vi.mock('../hooks/useWallet', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../hooks/useWallet')>()),
  useWallet: () => ({ walletType: h.walletType, solanaPubkey: 'Bswb3UyeD1pUTaGiE6WvqwFpJZsQSEY1xhJePCDTHdvp', principal: 'aaaaa-aa', identity: null, isConnected: true }),
}));
vi.mock('../hooks/useNativeHistory', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../hooks/useNativeHistory')>()),
  useNativeHistory: () => {
    h.calls += 1;
    return h.history;
  },
}));
vi.mock('../solana/native/panelsHooks', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../solana/native/panelsHooks')>()),
  useNativeAvailability: () => h.availability,
}));
vi.mock('../hooks/useQueries', async (importOriginal) => {
  const real = await importOriginal<typeof import('../hooks/useQueries')>();
  const query = (data: unknown) => () => ({ data, isLoading: false, isFetching: false, error: null, refetch: async () => undefined });
  return {
    ...real,
    useGetGeneralLedger: query([]),
    useGetGeneralLedgerStats: query(undefined),
    useGetSolGeneralLedger: query([
      { id: 1n, timestamp: 1_700_000_000_000_000_000n, event: { deposit: { plan: { simple21Day: null } } } },
    ]),
    useGetSolGeneralLedgerStats: query({ totalInflows: 1, totalOutflows: 0, netFlow: 1, entryCount: 1n }),
  };
});

import NativeHistoryFeed, { NativeHistoryList } from './NativeHistoryFeed';
import { HouseLedgerTab } from './HouseDashboard';
import { toNativeHistoryItems, type NativeHistoryResult } from '../hooks/useNativeHistory';

const SIG = (n: number) => `${n}`.padStart(88, 'S');

function entry(seq: number, status: Status, kind: 'PositionOpened' | 'SeriesAFunded' | 'PromotionApplied' = 'PositionOpened', basis = 1_500_000_000n): Entry {
  return {
    seq: BigInt(seq),
    event_key: `key-${seq}`,
    status,
    verified: [],
    next_attempt_at: 0n,
    binding: { Unbound: null },
    submitted_at: 1n,
    submitted_by: { toText: () => 'aaaaa-aa' } as unknown as Entry['submitted_by'],
    descriptor: {
      beneficiary_pubkey: 'Bswb3UyeD1pUTaGiE6WvqwFpJZsQSEY1xhJePCDTHdvp',
      basis_lamports: basis,
      kind: { [kind]: null } as Entry['descriptor']['kind'],
      plan: kind === 'PositionOpened' ? [{ Simple: null }] : [],
      source_round_id: 4n,
      source_close_id: [],
      event_id: {
        signature: SIG(seq),
        cluster_domain: new Uint8Array(32),
        inner_instruction_index: 0n,
        event_schema_version: 1n,
        program_id: '7tj9biW3KRJ7EEWmVUGigHiouCTXhV2dzcyvwma7Cyu7',
        instruction_index: 1n,
      },
    },
  };
}

const forwarded: Status = { Forwarded: { at: 1_700_000_000_000_000_000n, already_observed: false, obligation_id: 'o' } };
const quarantined: Status = { Quarantined: { at: 1_700_000_001_000_000_000n, reason: 'permanent rejection' } };
const intake: Status[] = [
  { Pending: null },
  { Unverified: { attempts: 1n, last_at: 1n, reason: { NotFound: null } } },
  { Verified: { retries: 0n, last_reason: [], last_at: 1n } },
  { Invalid: { at: 1n, reason: { BasisMismatch: null } } },
];

/** A bridge page that still carries intake rows (an older bridge): the mapper must keep them out. */
const bridgePage = [
  entry(20, intake[0]),
  entry(19, intake[1]),
  entry(18, intake[2]),
  entry(17, intake[3]),
  entry(16, quarantined, 'PromotionApplied', 300_000_000n),
  entry(15, forwarded, 'SeriesAFunded', 500_000_000n),
  entry(14, forwarded, 'PositionOpened', 1_500_000_000n),
];

function enabled(over: Partial<Extract<NativeHistoryResult, { enabled: true }>> = {}): NativeHistoryResult {
  return {
    enabled: true,
    items: toNativeHistoryItems(bridgePage),
    isLoading: false,
    error: null,
    hasMore: false,
    isLoadingMore: false,
    loadMore: () => undefined,
    refetch: () => undefined,
    ...over,
  };
}

const render = (node: React.ReactElement) => renderToStaticMarkup(<QueryClientProvider client={new QueryClient()}>{node}</QueryClientProvider>);
const before = (html: string, a: string, b: string) => html.indexOf(a) !== -1 && html.indexOf(b) !== -1 && html.indexOf(a) < html.indexOf(b);

beforeEach(() => {
  h.walletType = 'siws';
  h.history = enabled();
  h.availability = { state: 'native', pool: null, reason: null, error: null };
  h.calls = 0;
});

describe('NativeHistoryList rows', () => {
  it('shows kind, basis (SOL), round, truncated signature, outcome and time for verified records only', () => {
    const html = render(<NativeHistoryList history={enabled()} />);
    expect(html).toContain('Native program activity · verified by the ICP bridge');
    expect(html.match(/data-testid="native-history-row"/g)).toHaveLength(3);
    // delivered position
    expect(html).toContain('Position opened');
    expect(html).toContain('21-Day Simple');
    expect(html).toContain('Deposit 1.5000 SOL');
    expect(html).toContain('Round 4');
    expect(html).toContain(`${SIG(14).slice(0, 8)}…${SIG(14).slice(-8)}`);
    expect(html).toContain('Delivered');
    // Series A and the rejected conversion
    expect(html).toContain('Series A funded');
    expect(html).toContain('Funded 0.5000 SOL');
    expect(html).toContain('Emergency Equity Conversion');
    expect(html).toContain('Loss basis 0.3000 SOL');
    expect(html).toContain('Rejected: permanent rejection');
    expect(html).toContain('data-outcome="rejected"');
    expect(html).toContain('Informational. Balances, positions and payouts are read from Solana');
  });

  it('pending, unverified, verified and invalid fixtures NEVER render, not even their signatures', () => {
    const html = render(<NativeHistoryList history={enabled()} />);
    for (const seq of [20, 19, 18, 17]) expect(html, `seq ${seq}`).not.toContain(SIG(seq).slice(-8));
    expect(html).not.toMatch(/Pending|Unverified|Invalid|BasisMismatch|NotFound/);
    // every row is a delivery or a permanent rejection
    expect(html.match(/data-outcome="(delivered|rejected)"/g)).toHaveLength(3);
  });

  it('copy rules: no casino words', () => {
    const html = render(<NativeHistoryList history={enabled()} />);
    expect(html).not.toMatch(/\b(winner|winning|lottery|casino|ticket|draw)\b/i);
  });

  it('states: disabled, loading, error (with retry), empty, and Load more', () => {
    expect(render(<NativeHistoryList history={{ enabled: false, reason: 'not-configured' }} />)).toContain('data-testid="native-history-disabled"');
    expect(render(<NativeHistoryList history={enabled({ items: [], isLoading: true })} />)).toContain('data-testid="native-history-loading"');
    const failed = render(<NativeHistoryList history={enabled({ items: [], error: new Error('boom') })} />);
    expect(failed).toContain('data-testid="native-history-error"');
    expect(failed).toContain('Retry');
    expect(render(<NativeHistoryList history={enabled({ items: [] })} />)).toContain('No verified native activity yet.');
    const more = render(<NativeHistoryList history={enabled({ hasMore: true })} />);
    expect(more).toContain('Load more');
    expect(render(<NativeHistoryList history={enabled({ hasMore: false })} />)).not.toContain('Load more');
  });

  it('an all-intake page renders as empty, not as rows', () => {
    const items = toNativeHistoryItems([entry(3, intake[0]), entry(2, intake[1]), entry(1, intake[3])]);
    const html = render(<NativeHistoryList history={enabled({ items })} />);
    expect(html).toContain('No verified native activity yet.');
    expect(html).not.toContain('data-testid="native-history-row"');
  });
});

describe('Solana sessions only', () => {
  it('a Solana session renders the feed and queries the bridge', () => {
    const html = render(<NativeHistoryFeed />);
    expect(html).toContain('data-testid="native-history-feed"');
    expect(h.calls).toBeGreaterThan(0);
  });

  it('any other wallet renders nothing and never touches the bridge hook', () => {
    for (const walletType of ['internet-identity', 'plug', 'oisy', 'none']) {
      h.walletType = walletType;
      h.calls = 0;
      expect(render(<NativeHistoryFeed />), walletType).toBe('');
      expect(h.calls, walletType).toBe(0);
    }
  });
});

describe('the ledger tab keeps the native and legacy feeds separate and labeled', () => {
  it('Solana: the native feed first, then the legacy ledger under "Legacy (Chain Fusion) activity"', () => {
    const html = render(<HouseLedgerTab isSol />);
    expect(before(html, 'data-testid="native-history-feed"', 'data-testid="legacy-activity"')).toBe(true);
    expect(html).toContain('Native program activity · verified by the ICP bridge');
    expect(html).toContain('Legacy (Chain Fusion) activity');
    expect(html).toContain('This ledger lists legacy (Chain Fusion) activity only. Verified native program activity is in its own feed above.');
    // the legacy ledger row (a SOL deposit) is inside the legacy section, the native rows inside the native one
    const nativeSection = html.slice(html.indexOf('data-testid="native-history-feed"'), html.indexOf('data-testid="legacy-activity"'));
    const legacySection = html.slice(html.indexOf('data-testid="legacy-activity"'));
    expect(nativeSection).toContain('data-testid="native-history-row"');
    expect(nativeSection).not.toContain('Deposit — simple21Day');
    expect(legacySection).toContain('Deposit — simple21Day');
    expect(legacySection).not.toContain('data-testid="native-history-row"');
    expect(legacySection).not.toContain('Native program activity');
  });

  it('intake rows are absent from the whole tab', () => {
    const html = render(<HouseLedgerTab isSol />);
    for (const seq of [20, 19, 18, 17]) expect(html).not.toContain(SIG(seq).slice(-8));
  });

  it('a build with no bridge configured shows the disabled state next to the legacy ledger (and calls nothing)', () => {
    h.history = { enabled: false, reason: 'not-configured' };
    const html = render(<HouseLedgerTab isSol />);
    expect(html).toContain('data-testid="native-history-disabled"');
    expect(html).toContain('Legacy (Chain Fusion) activity');
  });

  it('non-Solana sessions keep the single ledger with no native feed', () => {
    const html = render(<HouseLedgerTab isSol={false} />);
    expect(html).not.toContain('Native program activity');
    expect(html).not.toContain('data-testid="legacy-activity"');
  });
});
