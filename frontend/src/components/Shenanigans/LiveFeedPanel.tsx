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

function LiveFeedRow({ record, spellName, spellIcon }: LiveFeedRowProps) {
  const casterName = useDisplayName(record.user);
  const isCasterGolden = useIsGolden(record.user);
  const target = record.target[0] ?? null;
  const targetName = useDisplayName(target);
  const isTargetGolden = useIsGolden(target);
  const outcomeKey = variantKey(record.outcome);
  const outcomeColor =
    outcomeKey === 'success' ? 'mc-text-green' :
    outcomeKey === 'fail' ? 'mc-text-danger' :
    'mc-text-purple';
  return (
    <div className="mc-card p-2 text-xs space-y-1">
      <div className="flex items-center justify-between gap-2">
        {isCasterGolden ? (
          <GoldenName name={casterName || 'Anon'} isGolden={true} className="font-bold truncate" />
        ) : (
          <span className="font-bold mc-text-primary truncate">{casterName || 'Anon'}</span>
        )}
        <span className={`font-bold flex-shrink-0 ${outcomeColor}`}>{outcomeKey.toUpperCase()}</span>
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
