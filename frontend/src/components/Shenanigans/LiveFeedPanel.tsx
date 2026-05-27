import React from 'react';
import { useDisplayName, useIsGolden } from '../trollbox/useDisplayName';
import GoldenName from '../GoldenName';
import type { ShenaniganRecord } from '../../backend';

interface LiveFeedRowProps {
  record: ShenaniganRecord;
  spellName: string;
  spellIcon: React.ReactNode;
}

const variantKey = (v: unknown): string =>
  v && typeof v === 'object' ? Object.keys(v as Record<string, unknown>)[0] ?? '' : '';

// Convert PP units (1 PP = 100_000_000 units) to a display string with up
// to 2 decimals when needed. Used by the live feed to render the ppDelta
// captured on each cast record.
function formatPp(units: bigint | number): string {
  const n = typeof units === 'bigint' ? Number(units) : units;
  const pp = n / 100_000_000;
  if (Number.isInteger(pp)) return pp.toLocaleString();
  return pp.toLocaleString(undefined, { maximumFractionDigits: 2 });
}

function LiveFeedRow({ record, spellName, spellIcon }: LiveFeedRowProps) {
  const casterName = useDisplayName(record.user);
  const isCasterGolden = useIsGolden(record.user);
  const target = record.target[0] ?? null;
  const targetName = useDisplayName(target);
  const isTargetGolden = useIsGolden(target);
  const outcomeKey = variantKey(record.outcome);

  // Forward-only detail fields. Old records have empty arrays here and
  // render in the terse format below the spell name. New records show
  // the rich detail line.
  const shieldDeflected = record.shieldDeflected?.[0] ?? false;
  const ppDelta = record.ppDelta?.[0] ?? null;
  const affected = record.affectedCount?.[0] ?? null;
  const renameDetail = record.renameDetail?.[0] ?? null;

  const outcomeLabel =
    shieldDeflected ? 'DEFLECTED' : outcomeKey.toUpperCase();
  const outcomeColor =
    shieldDeflected ? 'mc-text-muted' :
    outcomeKey === 'success' ? 'mc-text-green' :
    outcomeKey === 'fail' ? 'mc-text-danger' :
    'mc-text-purple';

  let detailLine: React.ReactNode = null;
  if (renameDetail) {
    detailLine = (
      <div className="mc-text-muted truncate">
        renamed <span className="mc-text-primary">{renameDetail.oldName}</span>
        {' → '}
        <span className="mc-text-primary">{renameDetail.newName}</span>
      </div>
    );
  } else if (shieldDeflected) {
    detailLine = <div className="mc-text-muted">shield blocked the effect</div>;
  } else if (ppDelta !== null && ppDelta !== 0n) {
    const ppNum = typeof ppDelta === 'bigint' ? ppDelta : BigInt(ppDelta);
    const sign = ppNum > 0n ? '+' : '';
    const sourceText = affected !== null && Number(affected) > 1
      ? ` across ${Number(affected)} players`
      : '';
    detailLine = (
      <div className="mc-text-muted">
        <span className={ppNum > 0n ? 'mc-text-green' : 'mc-text-danger'}>
          {sign}{formatPp(ppNum)} PP
        </span>
        {sourceText}
      </div>
    );
  }

  return (
    <div className="mc-card p-2 text-xs space-y-1">
      <div className="flex items-center justify-between gap-2">
        {isCasterGolden ? (
          <GoldenName name={casterName || 'Anon'} isGolden={true} className="font-bold truncate" />
        ) : (
          <span className="font-bold mc-text-primary truncate">{casterName || 'Anon'}</span>
        )}
        <span className={`font-bold flex-shrink-0 ${outcomeColor}`}>{outcomeLabel}</span>
      </div>
      <div className="mc-text-dim flex items-center gap-1 min-w-0">
        <span className="flex-shrink-0">{spellIcon}</span>
        <span className="truncate">{spellName}</span>
        {target ? (
          isTargetGolden ? (
            <span className="mc-text-muted truncate"> → <GoldenName name={targetName} isGolden={true} /></span>
          ) : (
            <span className="mc-text-muted truncate"> → {targetName}</span>
          )
        ) : null}
      </div>
      {detailLine}
    </div>
  );
}

export interface LiveFeedPanelProps {
  records: ShenaniganRecord[];
  resolveSpell: (record: ShenaniganRecord) => { name: string; icon: React.ReactNode };
  defaultCollapsed?: boolean;
}

export default function LiveFeedPanel({ records, resolveSpell, defaultCollapsed = false }: LiveFeedPanelProps) {
  const [open, setOpen] = React.useState(!defaultCollapsed);
  const trimmed = records.slice(0, 20);
  return (
    <div className="mc-card-elevated p-3">
      <button
        type="button"
        onClick={() => setOpen(v => !v)}
        aria-expanded={open}
        className="flex items-center justify-between w-full mb-2"
      >
        <h3 className="font-display text-sm mc-text-primary">Live Feed</h3>
        <span className="text-xs mc-text-muted">{open ? '▴' : 'latest casts ▾'}</span>
      </button>
      {open && (
        <div className="space-y-2 max-h-72 overflow-y-auto">
          {trimmed.length > 0 ? (
            trimmed.map(s => {
              const { name, icon } = resolveSpell(s);
              return <LiveFeedRow key={s.id.toString()} record={s} spellName={name} spellIcon={icon} />;
            })
          ) : (
            <p className="text-center mc-text-muted text-xs py-4">No shenanigans cast yet. Be the first!</p>
          )}
        </div>
      )}
    </div>
  );
}
