import React from 'react';
import PodiumCard, { HallOfFameEntry } from './PodiumCard';

interface PodiumProps {
  entries: HallOfFameEntry[];
}

/**
 * Top-3 podium for the Hall of Fame Diamond Tier. Three cards filling the
 * parent card's width via CSS grid; on narrow viewports the cards stack
 * vertically in natural rank order (#1, #2, #3 top-down).
 */
export default function Podium({ entries }: PodiumProps) {
  const top3 = entries.slice(0, 3);
  if (top3.length === 0) return null;

  // Desktop horizontal order: [#2, #1, #3] — classic podium left-center-right.
  // Mobile stacks vertically in natural rank order via `order-*` utilities.
  const desktopOrder: Array<{ entry: HallOfFameEntry; rank: 1 | 2 | 3; mobileOrderClass: string }> = [];
  if (top3[1]) desktopOrder.push({ entry: top3[1], rank: 2, mobileOrderClass: 'order-2 sm:order-none' });
  if (top3[0]) desktopOrder.push({ entry: top3[0], rank: 1, mobileOrderClass: 'order-1 sm:order-none' });
  if (top3[2]) desktopOrder.push({ entry: top3[2], rank: 3, mobileOrderClass: 'order-3 sm:order-none' });

  // Tailwind JIT needs class names literal in source — pick the right
  // grid-cols-N from a static set rather than templating with a number.
  const gridColsClass =
    top3.length === 1 ? 'sm:grid-cols-1'
    : top3.length === 2 ? 'sm:grid-cols-2'
    : 'sm:grid-cols-3';

  return (
    <div className={`grid grid-cols-1 ${gridColsClass} gap-3 sm:gap-4 mb-6`}>
      {desktopOrder.map(({ entry, rank, mobileOrderClass }) => (
        <div key={`podium-${rank}`} className={mobileOrderClass}>
          <PodiumCard entry={entry} rank={rank} />
        </div>
      ))}
    </div>
  );
}
