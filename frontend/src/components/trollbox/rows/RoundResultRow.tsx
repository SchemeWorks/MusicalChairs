import React from 'react';
import type { ChatItem } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName } from '../useDisplayName';

export default function RoundResultRow({ item }: { item: ChatItem }) {
  if (!('roundResult' in item.kind)) return null;
  const { gameId, winner, pot } = item.kind.roundResult;
  const name = useDisplayName(winner);
  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;
  const ppWon = Number(pot) / 100_000_000;
  return (
    <div className="px-3 py-1 text-xs text-amber-300">
      🎰 Round #{gameId.toString()} — <span className="text-zinc-100 font-medium">{name}</span> took the chair. Won: {ppWon.toFixed(0)} PP.
    </div>
  );
}
