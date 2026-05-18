import React from 'react';
import type { ChatItem } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName } from '../useDisplayName';

export default function RankUpRow({ item }: { item: ChatItem }) {
  if (!('rankUp' in item.kind)) return null;
  const { user, newRank } = item.kind.rankUp;
  const name = useDisplayName(user);
  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;
  return (
    <div className="px-3 py-1 text-xs text-emerald-300">
      📈 <span className="text-zinc-100 font-medium">{name}</span> promoted to <span className="font-semibold">{newRank}</span>.
    </div>
  );
}
