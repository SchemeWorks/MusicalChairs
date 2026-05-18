import React from 'react';
import type { ChatItem, ShenaniganRecord } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName } from '../useDisplayName';

interface Props {
  item: ChatItem;
  spellLookup: Map<string, ShenaniganRecord>;
}

export default function SpellRow({ item, spellLookup }: Props) {
  if (!('spellCast' in item.kind)) return null;
  const castId = item.kind.spellCast.castId;
  const record = spellLookup.get(castId.toString());
  const userName = useDisplayName(record?.user ?? null);

  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;
  if (!record) {
    return (
      <div className="px-3 py-1 text-xs text-zinc-500">
        ✨ Someone cast a spell.
      </div>
    );
  }
  const outcomeText = 'success' in record.outcome ? 'landed clean' : 'backfire' in record.outcome ? 'backfired' : 'fizzled';
  const outcomeColor = 'success' in record.outcome ? 'text-emerald-300' : 'backfire' in record.outcome ? 'text-red-400' : 'text-zinc-400';
  return (
    <div className="px-3 py-1 text-xs">
      <span className={`${outcomeColor} font-medium`}>✨ {userName}</span>
      <span className="text-zinc-400"> cast a spell — {outcomeText}.</span>
    </div>
  );
}
