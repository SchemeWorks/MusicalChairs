// IMPL-46c: renders the native panels' presentational pieces to static markup (no jsdom, no effects):
// the shared confirmation-ladder card (headline, saved signature and nonce, which buttons each status
// offers) and the deposit projection (gross interest, 4 % Front-End Load).
import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { NativeInvestProjection, NativeTxStatusCard } from './SolInvestPanel';
import { NativePositionCard } from './GameTracking';
import { quotePosition } from '../solana/native/panelsQuote';
import type { NativePositionRow } from '../solana/native/panelsHooks';
import type { NativeTxResult, NativeTxStatus, StatusUpdate } from '../solana/native/types';
import { OPENED_AT, testConfig, testPool, testPosition } from '../solana/native/panelsTestFixtures';

const SIG = '5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW';

function tx(status: NativeTxStatus, nonce: bigint | null = 123456789012345678n): NativeTxResult {
  return {
    signature: SIG,
    nonce,
    status,
    status$: (async function* () {
      yield { signature: SIG, status } as StatusUpdate;
    })(),
    origin: { clusterKey: 'k', cluster: { expectedClusterDomain: 'localnet', programId: 'p' }, rpcEndpoint: 'http://127.0.0.1:8899', generation: 0 },
    retry: async () => ({ signature: SIG, status: 'confirmed' }),
    reconcile: async () => ({ signature: SIG, status: 'confirmed' }),
    safeToRebuild: status === 'failed' || status === 'expired',
  } as unknown as NativeTxResult;
}

function render(status: NativeTxStatus, extra: Partial<{ nonce: bigint | null; withTryAgain: boolean }> = {}) {
  const update: StatusUpdate = { signature: SIG, status };
  return renderToStaticMarkup(
    <QueryClientProvider client={new QueryClient()}>
      <NativeTxStatusCard
        tx={tx(status, extra.nonce)}
        status={update}
        label="Your deposit"
        onTryAgain={extra.withTryAgain === false ? undefined : () => undefined}
      />
    </QueryClientProvider>,
  );
}

describe('NativeTxStatusCard', () => {
  it('keeps the signature and the open_position nonce on screen at every status', () => {
    for (const status of ['submitted', 'confirmed', 'finalized', 'unknown', 'failed', 'expired'] as const) {
      const html = render(status);
      expect(html, status).toContain(`${SIG.slice(0, 8)}…${SIG.slice(-8)}`);
      expect(html, status).toContain('Nonce 123456789012345678');
      expect(html, status).toContain(`data-status="${status}"`);
    }
  });

  it('omits the nonce line for transactions that have none', () => {
    expect(render('confirmed', { nonce: null })).not.toContain('Nonce');
  });

  it('unknown offers Check status (read-only) and a separate guarded resend, and never Try again', () => {
    const html = render('unknown');
    expect(html).toContain('Check status');
    expect(html).toContain('Resend the same transaction');
    expect(html).not.toContain('Try again');
    expect(html).toContain('Outcome unknown');
    // the resend is behind a confirmation: its explanation is not shown until it is asked for
    expect(html).not.toContain('data-testid="tx-resend-confirm"');
  });

  it('shows where the transaction was sent (cluster and RPC host only)', () => {
    for (const status of ['submitted', 'unknown', 'finalized'] as const) {
      expect(render(status), status).toContain('Sent on localnet (127.0.0.1:8899)');
    }
  });

  it('only an unknown outcome can be resent; every other status offers no resend', () => {
    for (const status of ['submitted', 'confirmed', 'finalized', 'failed', 'expired'] as const) {
      expect(render(status), status).not.toContain('Resend the same transaction');
    }
  });

  it('the card never calls the mutating retry() itself: Check status is reconcile, resend is the guarded handler', async () => {
    const { readFileSync } = await import('node:fs');
    const source = readFileSync(new URL('./SolInvestPanel.tsx', import.meta.url), 'utf8');
    const card = source.slice(source.indexOf('export function NativeTxStatusCard'), source.indexOf('function NativeInvestProjection') > 0 ? source.indexOf('export function NativeInvestProjection') : undefined);
    expect(card).not.toMatch(/\.retry\(\)/);
    expect(card).not.toMatch(/\.reconcile\(\)/);
    expect(card).toMatch(/checkTxStatus\(tx\)/);
    expect(card).toMatch(/resendSameTx\(tx\)/);
  });

  it('failed and expired offer Try again and no Check status', () => {
    for (const status of ['failed', 'expired'] as const) {
      const html = render(status);
      expect(html, status).toContain('Try again');
      expect(html, status).not.toContain('Check status');
    }
  });

  it('submitted, confirmed and finalized offer neither', () => {
    for (const status of ['submitted', 'confirmed', 'finalized'] as const) {
      const html = render(status);
      expect(html, status).not.toContain('Try again');
      expect(html, status).not.toContain('Check status');
    }
  });
});

describe('NativeInvestProjection (the Invest panel right-hand column)', () => {
  const html = (plan: 'Simple' | 'Compound15' | 'Compound30', planId: string, lamports: bigint, config = testConfig()) =>
    renderToStaticMarkup(<NativeInvestProjection plan={plan} planId={planId} lamports={lamports} config={config} />);

  it('1 SOL on the Simple plan: 0.11 SOL a day on the GROSS deposit, 4 % Front-End Load shown as the entry fee', () => {
    const out = html('Simple', '21-day-simple', 1_000_000_000n);
    expect(out).toContain('Front-End Load (4%)');
    expect(out).toContain('-0.0400 SOL');
    expect(out).toContain('Deposit into AUM');
    expect(out).toContain('0.9600 SOL');
    expect(out).toContain('0.1100 SOL/day');
    expect(out).not.toContain('0.1056'); // the legacy panel projects on the NET deposit
    expect(out).toContain('gross deposit of 1.0000 SOL');
    expect(out).toContain('Interest Payout');
    expect(out).toContain('Carried Interest applies on withdrawal');
  });

  it('compounding plans name the Jackpot Fee', () => {
    const out = html('Compound15', '15-day-compounding', 1_000_000_000n);
    expect(out).toContain('Compounded Interest');
    expect(out).toContain('9% Jackpot Fee applies at maturity');
    expect(html('Compound30', '30-day-compounding', 1_000_000_000n)).toContain('13% Jackpot Fee');
  });

  it('shows the placeholder until an amount is entered', () => {
    expect(html('Simple', '21-day-simple', 0n)).toContain('Enter an amount to see projected returns');
  });

  it('uses the config\'s load rate, not a hard-coded 4 %', () => {
    const out = html('Simple', '21-day-simple', 1_000_000_000n, testConfig({ frontEndLoadBps: 500n }));
    expect(out).toContain('Front-End Load (5%)');
    expect(out).toContain('-0.0500 SOL');
  });
});

describe('NativePositionCard (a native position in the Profit Center)', () => {
  const config = testConfig();
  const BIG_POT = 1_000_000_000_000n;
  const render = (position = testPosition(), opts: { now?: bigint; pool?: ReturnType<typeof testPool> } = {}) => {
    const pool = opts.pool ?? testPool({ roundId: 1n, pot: BIG_POT });
    const now = opts.now ?? OPENED_AT + 86_400n;
    const row: NativePositionRow = {
      position,
      config,
      quote: quotePosition({ position, config, pot: pool.pot, now }),
      active: true,
    };
    return renderToStaticMarkup(<NativePositionCard row={row} pool={pool} busy={false} onAct={() => undefined} />);
  };

  it('Simple, one day in: owed, the 12 % Carried Interest tier, the net you receive, and a Withdraw button', () => {
    const out = render();
    expect(out).toContain('21-Day Simple');
    expect(out).toContain('Carried Interest');
    expect(out).toContain('12%');
    expect(out).toContain('1.0000 SOL'); // principal
    expect(out).toContain('0.1100 SOL'); // one day owed (gross)
    expect(out).toContain('0.0968 SOL'); // golden vector: net 96,800,000
    expect(out).toContain('Withdraw');
    expect(out).not.toContain('closes the round');
    expect(out).toContain('to 7.5%'); // countdown to the next tier
  });

  it('labels a payout that exceeds AUM as one that closes the round', () => {
    const out = render(testPosition(), { pool: testPool({ roundId: 1n, pot: 50_000_000n }) });
    expect(out).toContain('This payout closes the round.');
  });

  it('disables the payout while an allocation run is in progress, with the reason as the title', () => {
    const out = render(testPosition(), { pool: testPool({ roundId: 1n, pot: BIG_POT, activeTicket: 4n }) });
    expect(out).toMatch(/disabled=""[^>]*title="An allocation run is in progress\. Resume processing first\."|title="An allocation run is in progress\. Resume processing first\."[^>]*disabled=""/);
  });

  it('a compounding position is locked until maturity and names the Jackpot Fee', () => {
    const out = render(testPosition({ kind: 1n, maturityAt: OPENED_AT + 15n * 86_400n }));
    expect(out).toContain('Jackpot Fee');
    expect(out).toContain('9%');
    expect(out).toContain('15-Day Compounding');
    expect(out).not.toContain('Withdraw');
    expect(out).toMatch(/13d \d+h \d+m|14d \d+h \d+m/);
  });

  it('a matured Simple position with nothing owed offers Close', () => {
    const paid = testPosition({ accruedGross: 2_310_000_000n, paidGross: 2_310_000_000n, lastAccruedAt: OPENED_AT + 1_814_400n });
    expect(render(paid, { now: OPENED_AT + 30n * 86_400n })).toContain('Close');
  });
});
