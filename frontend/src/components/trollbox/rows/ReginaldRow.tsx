import React from 'react';
import type { ChatItem } from '../../../declarations/shenanigans/shenanigans.did';

export default function ReginaldRow({ item }: { item: ChatItem }) {
  if (!('reginald' in item.kind)) return null;
  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;
  return (
    <div className="px-3 py-1 flex items-start gap-2 italic text-sm text-zinc-400">
      <span className="rounded bg-zinc-800 px-1.5 py-0.5 text-[10px] font-semibold not-italic text-zinc-300">Reginald</span>
      <span>{item.kind.reginald.line}</span>
    </div>
  );
}
