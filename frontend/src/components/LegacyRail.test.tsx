// IMPL-46c P1: native vs legacy are explicit views (DR-22). The three states of native availability map to:
//   loading      -> a verification state; NO legacy action is offered as if native had failed
//   unavailable  -> the legacy rail only, under a visible "Legacy (Chain Fusion)" label, no native UI
//   native       -> native first, legacy positions/backers in their own labeled section (never merged)
// Rendered to static markup (no jsdom) with the data hooks stubbed; only the rail decision is real.
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { testPool } from '../solana/native/panelsTestFixtures';

const PUBKEY = 'Bswb3UyeD1pUTaGiE6WvqwFpJZsQSEY1xhJePCDTHdvp';
const h = vi.hoisted(() => ({
  availability: { state: 'unavailable', pool: null, reason: 'cluster-not-configured', error: null } as Record<string, unknown>,
  solGames: [] as unknown[],
  solBackers: [] as unknown[],
  repayments: [] as unknown[],
  portfolio: null as unknown,
  localMode: false,
}));

// The unit tests run as a DEV build with VITE_NATIVE_LOCAL_CLUSTER=1 from .env.local, which would turn the
// legacy fallback into the local-mode "not available" card. Pin the mode per test instead.
vi.mock('../solana/native/localModeFlag', async (importOriginal) => {
  const real = await importOriginal<typeof import('../solana/native/localModeFlag')>();
  return {
    ...real,
    get LOCAL_CLUSTER_MODE() {
      return h.localMode;
    },
  };
});

vi.mock('../solana/native/panelsHooks', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../solana/native/panelsHooks')>()),
  useNativeAvailability: () => h.availability,
  useNativePortfolio: () => h.portfolio,
}));
vi.mock('../hooks/useWallet', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../hooks/useWallet')>()),
  useWallet: () => ({ walletType: 'siws', solanaPubkey: PUBKEY, principal: 'aaaaa-aa', identity: null, isConnected: true }),
}));
vi.mock('../hooks/usePonziMathSolActor', () => ({
  usePonziMathSolActor: () => ({ actor: {}, isFetching: false, error: null }),
  default: () => ({ actor: {}, isFetching: false, error: null }),
}));
vi.mock('./AddBackerMoney', () => ({ default: () => <div>fund-the-project-stub</div> }));
vi.mock('../hooks/useQueries', async (importOriginal) => {
  const real = await importOriginal<typeof import('../hooks/useQueries')>();
  const query = (data: unknown) => () => ({ data, isLoading: false, isFetching: false, error: null, refetch: async () => undefined });
  const mutation = () => ({ mutate: () => undefined, mutateAsync: async () => 0, isPending: false, isError: false, error: null, reset: () => undefined, variables: undefined });
  return {
    ...real,
    // GameTracking
    useGetUserGames: query([]),
    useGetPonziPoints: query({ totalPoints: 0, chipPoints: 0, walletPoints: 0 }),
    useGetShenaniganConfigs: query([]),
    useGetMintConfig: query({ backerPpPerSol: 4000n, backerPpPerIcp: 4000n }),
    useGetUserSolGames: () => ({ data: h.solGames, isLoading: false, isFetching: false, error: null }),
    useWithdrawGameEarnings: mutation,
    useSettleCompoundingGame: mutation,
    useWithdrawSolGameEarnings: mutation,
    // invest / backer legacy forms
    useSolBalance: query(5),
    useIsSelfServeBackingEnabled: query(true),
    // HouseDashboard
    useGetGeneralLedger: query([]),
    useGetGeneralLedgerStats: query(undefined),
    useGetBackerPositions: query([]),
    useGetGameStats: query(undefined),
    useGetAllBackerRepayments: query([]),
    useClaimDealerRepayment: mutation,
    useGetUserNames: query(undefined),
    useGetSolBackerPositions: () => ({ data: h.solBackers, isLoading: false, error: null, refetch: async () => undefined }),
    useGetSolAllBackerRepayments: () => ({ data: h.repayments, isLoading: false, error: null }),
    useGetSolGeneralLedger: query([]),
    useGetSolGeneralLedgerStats: query({ totalInflows: 0, totalOutflows: 0, netFlow: 0, entryCount: 0n }),
    useClaimSolBackerRepayment: mutation,
  };
});

import SolInvestPanel from './SolInvestPanel';
import SolBackerPanel from './SolBackerPanel';
import GameTracking from './GameTracking';
import HouseDashboard from './HouseDashboard';

const NOW_NS = BigInt(Date.now()) * 1_000_000n;
const legacyGame = { id: 1n, plan: { simple21Day: null }, amount: 1, startTime: NOW_NS, lastUpdateTime: NOW_NS, isCompounding: false, isActive: true };
const me = { toString: () => 'aaaaa-aa' };
const legacyBacker = { owner: me, backerType: { seriesA: null }, amount: 1, entitlement: 1.24, startTime: 0n };

const emptyPortfolio = (pool: ReturnType<typeof testPool> | null) => ({
  pool,
  rows: [],
  active: [],
  closed: [],
  now: BigInt(Math.floor(Date.now() / 1000)),
  totalDeposits: 0n,
  totalEarned: 0n,
  isLoading: false,
});

function setRail(state: 'loading' | 'unavailable' | 'native') {
  const pool = state === 'native' ? testPool() : null;
  h.availability = { state, pool, reason: state === 'unavailable' ? 'cluster-not-configured' : null, error: null };
  h.portfolio = emptyPortfolio(pool);
}

const render = (node: React.ReactElement) =>
  renderToStaticMarkup(<QueryClientProvider client={new QueryClient()}>{node}</QueryClientProvider>);

beforeEach(() => {
  h.localMode = false;
  h.solGames = [legacyGame];
  h.solBackers = [legacyBacker];
  h.repayments = [[[me, { seriesA: null }], 0.5]];
  setRail('unavailable');
});

const LEGACY_LABEL = 'Legacy (Chain Fusion)';
const VERIFYING = 'Verifying the native program on this cluster';
const before = (html: string, a: string, b: string) => html.indexOf(a) !== -1 && html.indexOf(b) !== -1 && html.indexOf(a) < html.indexOf(b);

describe('Invest panel', () => {
  it('loading: a verification state, no legacy deposit form', () => {
    setRail('loading');
    const html = render(<SolInvestPanel planId="21-day-simple" />);
    expect(html).toContain(VERIFYING);
    expect(html).not.toContain('DEPOSIT WITH PHANTOM');
    expect(html).not.toContain(LEGACY_LABEL);
  });

  it('unavailable: the legacy form under a visible "Legacy (Chain Fusion)" label, and no native UI', () => {
    setRail('unavailable');
    const html = render(<SolInvestPanel planId="21-day-simple" />);
    expect(html).toContain('data-testid="legacy-rail-banner"');
    expect(before(html, LEGACY_LABEL, 'DEPOSIT WITH PHANTOM')).toBe(true);
    expect(html).not.toContain('DEPOSIT FROM YOUR WALLET');
    expect(html).not.toContain('Front-End Load (4%)');
    expect(html).not.toContain(VERIFYING);
  });
});

describe('local-cluster mode has no legacy rail to fall back to', () => {
  it('unavailable says why instead of showing a legacy form (invest and Series A)', () => {
    h.localMode = true;
    setRail('unavailable');
    for (const html of [render(<SolInvestPanel planId="21-day-simple" />), render(<SolBackerPanel />)]) {
      expect(html).toContain('data-testid="native-unavailable"');
      expect(html).not.toContain('DEPOSIT WITH PHANTOM');
      expect(html).not.toContain('Back with Phantom');
      expect(html).not.toContain('data-testid="legacy-rail-banner"');
    }
  });
});

describe('Series A / backer panel', () => {
  it('loading: a verification state, no legacy funding form', () => {
    setRail('loading');
    const html = render(<SolBackerPanel />);
    expect(html).toContain(VERIFYING);
    expect(html).not.toContain('Back with Phantom');
    expect(html).not.toContain(LEGACY_LABEL);
  });

  it('unavailable: the legacy form under the "Legacy (Chain Fusion)" label, no native funding', () => {
    setRail('unavailable');
    const html = render(<SolBackerPanel />);
    expect(before(html, LEGACY_LABEL, 'Back with Phantom')).toBe(true);
    expect(html).not.toContain('Back with your wallet');
    expect(html).not.toContain(VERIFYING);
  });
});

describe('Profit Center (positions)', () => {
  const tracking = () => render(<GameTracking />);

  it('loading: verification state, and NO legacy position or withdraw action is offered', () => {
    setRail('loading');
    const html = tracking();
    expect(html).toContain('data-testid="native-rail-verifying"');
    expect(html).toContain('data-testid="tally-verifying-caption"');
    expect(html).not.toContain('data-testid="legacy-positions"');
    expect(html).not.toContain('Withdraw');
    expect(html).not.toContain('Native positions');
  });

  it('unavailable: the legacy positions under a "Legacy (Chain Fusion)" banner, tally captioned legacy-only, no native section', () => {
    setRail('unavailable');
    const html = tracking();
    expect(html).toContain('data-testid="legacy-positions"');
    expect(before(html, LEGACY_LABEL, 'Withdraw')).toBe(true);
    expect(html).toContain('data-testid="tally-legacy-caption"');
    expect(html).not.toContain('data-testid="native-positions"');
    expect(html).not.toContain('data-testid="native-rail-verifying"');
  });

  it('native: native section first, legacy in its own labeled section (never merged), no legacy-only banner', () => {
    setRail('native');
    const html = tracking();
    expect(before(html, 'data-testid="native-positions"', 'data-testid="legacy-positions"')).toBe(true);
    expect(html).toContain('Legacy (Chain Fusion) positions');
    expect(html).not.toContain('data-testid="legacy-rail-banner"');
    expect(html).not.toContain('data-testid="native-rail-verifying"');
  });
});

describe('House (backers and ledger)', () => {
  const house = () => render(<HouseDashboard />);

  it('loading: verification state; no legacy backer list and no claim button', () => {
    setRail('loading');
    const html = house();
    expect(html).toContain('data-testid="native-rail-verifying"');
    expect(html).not.toContain('data-testid="legacy-backers"');
    expect(html).not.toContain('Claim Repayment');
    expect(html).not.toContain('Current Backers');
  });

  it('unavailable: banner, and the list is headed "Legacy (Chain Fusion) backers", never a generic "Current Backers"', () => {
    setRail('unavailable');
    const html = house();
    expect(html).toContain('data-testid="legacy-rail-banner"');
    expect(html).toContain('Legacy (Chain Fusion) backers');
    expect(html).not.toContain('Current Backers');
    expect(html).toContain('Claim Repayment'); // the legacy claim keeps working
    expect(html).not.toContain('data-testid="native-backers"');
  });

  it('native: the native Backer section first, the legacy list separately labeled', () => {
    setRail('native');
    const html = house();
    expect(before(html, 'data-testid="native-backers"', 'data-testid="legacy-backers"')).toBe(true);
    expect(html).toContain('Legacy (Chain Fusion) backers');
    expect(html).not.toContain('data-testid="legacy-rail-banner"');
  });
});
