import { ArrowDownLeft, Ban } from 'lucide-react';
import { useWallet } from '../hooks/useWallet';
import { useNativeHistory, type NativeHistoryItem, type NativeHistoryKind, type NativeHistoryResult } from '../hooks/useNativeHistory';
import { formatSOL } from '../solana/lamports';
import { PLAN_LABEL } from '../solana/native/panelsQuote';
import { shortSignature } from '../solana/native/panelsStatus';

/**
 * Verified native program activity, as recorded by the ICP bridge (hooks/useNativeHistory.ts). Shown beside, never
 * merged with, the legacy (Chain Fusion) ledger. Informational only: balances, payouts and positions are read from
 * Solana. Only verified-delivery records (delivered or permanently rejected) can appear; intake rows never do.
 */

const KIND_LABEL: Record<NativeHistoryKind, string> = {
  PositionOpened: 'Position opened',
  SeriesAFunded: 'Series A funded',
  PromotionApplied: 'Emergency Equity Conversion',
};

const BASIS_LABEL: Record<NativeHistoryKind, string> = {
  PositionOpened: 'Deposit',
  SeriesAFunded: 'Funded',
  PromotionApplied: 'Loss basis',
};

function formatTime(nanoseconds: bigint): string {
  try {
    return new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(
      new Date(Number(nanoseconds / 1_000_000n)),
    );
  } catch {
    return '';
  }
}

function HistoryRow({ item }: { item: NativeHistoryItem }) {
  const delivered = item.outcome === 'delivered';
  return (
    <li
      className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 py-3 border-b border-white/5 last:border-0"
      data-testid="native-history-row"
      data-outcome={item.outcome}
      data-kind={item.kind}
    >
      <div className="flex items-center gap-2 min-w-0">
        {delivered ? <ArrowDownLeft className="h-3.5 w-3.5 mc-text-green flex-shrink-0" /> : <Ban className="h-3.5 w-3.5 mc-text-danger flex-shrink-0" />}
        <div className="min-w-0">
          <div className="text-sm font-bold mc-text-primary truncate">
            {KIND_LABEL[item.kind]}
            {item.plan ? <span className="font-normal mc-text-muted"> · {PLAN_LABEL[item.plan]}</span> : null}
          </div>
          <div className="text-xs mc-text-muted">
            Round {item.sourceRound.toString()}
            {item.sourceCloseId !== null ? ` · close ${item.sourceCloseId.toString()}` : ''} · {formatTime(item.deliveredAt)}
          </div>
        </div>
      </div>
      <div className="text-xs text-right space-y-0.5">
        <div className="mc-text-primary font-medium">
          {BASIS_LABEL[item.kind]} {formatSOL(item.basisLamports)} SOL
        </div>
        <div className="font-mono mc-text-muted" title={item.signature}>
          {shortSignature(item.signature)}
        </div>
        <div className={delivered ? 'mc-text-green' : 'mc-text-danger'} title={item.reason}>
          {delivered ? 'Delivered' : `Rejected${item.reason ? `: ${item.reason}` : ''}`}
        </div>
      </div>
    </li>
  );
}

/** Why the feed is off, in words: clusters by label only (never a key, path or endpoint). */
export function disabledCopy(history: Extract<NativeHistoryResult, { enabled: false }>): string {
  const app = history.appCluster ?? 'this app';
  switch (history.reason) {
    case 'cluster-mismatch':
      return `Native activity is hidden: the ICP bridge follows ${history.bridgeCluster ?? 'another cluster'}, but this app is on ${app}. Nothing from another cluster is listed.`;
    case 'program-mismatch':
      return `Native activity is hidden: the ICP bridge follows another program deployment on ${app}. Nothing from another deployment is listed.`;
    case 'cluster-unresolved':
      return "Native activity can't be listed yet: this build's Solana cluster isn't set up for the native game, so the ICP bridge's cluster can't be checked.";
    case 'config-unavailable':
      return `Native activity can't be listed right now: the ICP bridge's configuration couldn't be read to check that it follows ${app}.`;
    case 'bridge-uninitialized':
      return "The ICP bridge isn't configured for any cluster yet, so there is no verified native activity to list.";
    default:
      return "The ICP bridge isn't configured for this build, so verified native activity can't be listed here.";
  }
}

/** The feed for a given history state (presentational: the static-render tests pass fixtures). */
export function NativeHistoryList({ history }: { history: NativeHistoryResult }) {
  return (
    <section className="mc-card-elevated space-y-3" data-testid="native-history-feed" data-rail="native-activity">
      <div>
        <h3 className="font-display text-base mc-text-primary">Native program activity · verified by the ICP bridge</h3>
        <p className="text-xs mc-text-muted">
          Informational. Balances, positions and payouts are read from Solana, not from this feed.
        </p>
      </div>
      {!history.enabled ? (
        <p className="text-sm mc-text-dim text-center py-4" data-testid="native-history-disabled" data-reason={history.reason}>
          {disabledCopy(history)}
        </p>
      ) : history.error ? (
        <div className="mc-status-red p-3 text-center text-sm" data-testid="native-history-error">
          <p className="mb-2">Couldn't load native activity from the ICP bridge.</p>
          <button onClick={history.refetch} className="mc-btn-secondary px-4 py-2 text-xs rounded-lg">Retry</button>
        </div>
      ) : history.isLoading ? (
        <p className="text-sm mc-text-dim text-center py-4" data-testid="native-history-loading">Loading native activity…</p>
      ) : (
        <>
          {history.items.length === 0 ? (
            <p className="text-sm mc-text-dim text-center py-4" data-testid="native-history-empty">No verified native activity yet.</p>
          ) : (
            <ul className="max-h-96 overflow-y-auto">
              {history.items.map((item) => (
                <HistoryRow key={item.seq.toString()} item={item} />
              ))}
            </ul>
          )}
          {history.hasMore && (
            <div className="text-center">
              <button onClick={history.loadMore} disabled={history.isLoadingMore} className="mc-btn-secondary px-4 py-2 text-xs rounded-lg disabled:opacity-50">
                {history.isLoadingMore ? 'Loading…' : 'Load more'}
              </button>
            </div>
          )}
        </>
      )}
    </section>
  );
}

function NativeHistoryFeedBody() {
  return <NativeHistoryList history={useNativeHistory()} />;
}

/** Solana sessions only: ICP wallets have no native activity, and make no bridge call. */
export default function NativeHistoryFeed() {
  const { walletType } = useWallet();
  if (walletType !== 'siws') return null;
  return <NativeHistoryFeedBody />;
}
