import React from 'react';
import { toast } from 'sonner';
import { AlertTriangle } from 'lucide-react';
import {
  usePendingCashOuts,
  useClaimCashOut,
  useCancelCashOut,
  useGetPonziPoints,
} from '../hooks/useQueries';

function formatWhen(date: Date): string {
  const now = Date.now();
  const diffMs = date.getTime() - now;
  if (diffMs <= 0) return 'ready now';
  const hours = Math.round(diffMs / (1000 * 60 * 60));
  if (hours < 24) return `unlocks in ${hours}h`;
  const days = Math.round(hours / 24);
  return `unlocks in ${days}d`;
}

export default function PendingQueueCard() {
  const { data: pending } = usePendingCashOuts();
  const { data: pp } = useGetPonziPoints();
  const claim = useClaimCashOut();
  const cancel = useCancelCashOut();

  const position = pp?.chipPoints ?? 0;

  if (!pending || pending.length === 0) {
    return (
      <section className="mc-card p-4">
        <h2 className="font-display text-xl mb-1">Pending redemptions</h2>
        <p className="text-sm mc-text-muted">No pending redemptions.</p>
      </section>
    );
  }

  // FIFO ordering: oldest claimableAfter first.
  const sorted = [...pending].sort(
    (a, b) => a.claimableAfter.getTime() - b.claimableAfter.getTime(),
  );

  // FIFO shortfall attribution: draw down Position in order.
  let remaining = position;
  const rows = sorted.map((entry) => {
    const paid = Math.min(entry.amount, remaining);
    remaining = Math.max(0, remaining - entry.amount);
    const shortfall = paid < entry.amount;
    return { entry, paid, shortfall };
  });

  return (
    <section className="mc-card p-4">
      <h2 className="font-display text-xl mb-3">Pending redemptions</h2>
      <ul className="space-y-3">
        {rows.map(({ entry, paid, shortfall }) => {
          const ready = entry.claimableAfter.getTime() <= Date.now();
          return (
            <li key={String(entry.id)} className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <div>
                  <b>{entry.amount.toLocaleString()} PP</b>
                  <span className="mc-text-muted"> · {ready ? 'ready now' : formatWhen(entry.claimableAfter)}</span>
                </div>
                {shortfall && (
                  <div className="text-xs text-amber-500 flex items-center gap-1 mt-0.5">
                    <AlertTriangle className="h-3 w-3" />
                    spells reduced this to ~{paid.toLocaleString()} PP
                  </div>
                )}
              </div>
              <div className="flex gap-2">
                {ready ? (
                  <button
                    className="mc-btn mc-btn-success text-xs px-3 py-1"
                    disabled={claim.isPending}
                    onClick={async () => {
                      try {
                        await claim.mutateAsync(entry.id);
                        toast.success('Redeemed');
                      } catch (e: any) {
                        toast.error(e.message);
                      }
                    }}
                  >
                    Redeem
                  </button>
                ) : (
                  <button
                    className="mc-btn-secondary text-xs px-3 py-1"
                    disabled={cancel.isPending}
                    onClick={async () => {
                      try {
                        await cancel.mutateAsync(entry.id);
                        toast.success('Cancelled');
                      } catch (e: any) {
                        toast.error(e.message);
                      }
                    }}
                  >
                    Cancel
                  </button>
                )}
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
