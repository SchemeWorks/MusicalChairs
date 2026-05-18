import React from 'react';
import type { ChatItem } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName } from '../useDisplayName';

export default function RankUpRow({ item }: { item: ChatItem }) {
  const user = 'rankUp' in item.kind ? item.kind.rankUp.user : null;
  const name = useDisplayName(user);
  if (!('rankUp' in item.kind)) return null;
  const { newRank } = item.kind.rankUp;
  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;
  return (
    <div className="px-3 py-1 text-xs text-emerald-300">
      📈 <span className="text-zinc-100 font-medium">{name}</span> promoted to <span className="font-semibold">{newRank}</span>.
    </div>
  );
}
