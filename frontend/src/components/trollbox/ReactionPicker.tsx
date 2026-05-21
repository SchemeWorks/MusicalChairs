import React, { useState } from 'react';
import {
  FREE_EMOJIS,
  KARMA_EMOJIS,
  KARMA_MIN_PP,
  KARMA_SPLIT_BURN_PCT,
  KARMA_SPLIT_MGMT_PCT,
  KARMA_SPLIT_RECIPIENT_PCT,
} from './trollboxConstants';
import { useAddReaction, useKarmaReact } from '../../hooks/useQueries';

interface Props {
  itemId: bigint;
  onClose: () => void;
}

export default function ReactionPicker({ itemId, onClose }: Props) {
  const addReact = useAddReaction();
  const karmaReact = useKarmaReact();
  const [mode, setMode] = useState<'free' | 'karma'>('free');
  const [karmaAmount, setKarmaAmount] = useState<number>(KARMA_MIN_PP);

  const handleFree = async (emoji: string) => {
    try { await addReact.mutateAsync({ itemId, emoji }); } catch { /* hook toasts */ }
    onClose();
  };
  const handleKarma = async (emoji: string) => {
    const amount = Number.isFinite(karmaAmount) && karmaAmount >= KARMA_MIN_PP
      ? karmaAmount
      : KARMA_MIN_PP;
    try { await karmaReact.mutateAsync({ itemId, emoji, ppToBurn: BigInt(amount) }); } catch { /* hook toasts */ }
    onClose();
  };

  const recipientPp = Math.floor((karmaAmount * KARMA_SPLIT_RECIPIENT_PCT) / 100);
  const burnPp = Math.floor((karmaAmount * KARMA_SPLIT_BURN_PCT) / 100);
  const mgmtPp = karmaAmount - recipientPp - burnPp;

  return (
    <div className="w-72 rounded-lg border border-zinc-700 bg-zinc-900 p-3 shadow-lg">
      <div className="mb-2 inline-flex rounded-md bg-zinc-800 p-0.5 text-xs">
        <button
          onClick={() => setMode('free')}
          className={
            'rounded px-3 py-1 transition ' +
            (mode === 'free'
              ? 'bg-zinc-700 text-zinc-100 font-medium shadow-sm'
              : 'text-zinc-400 hover:text-zinc-200')
          }
        >
          Free
        </button>
        <button
          onClick={() => setMode('karma')}
          className={
            'rounded px-3 py-1 transition ' +
            (mode === 'karma'
              ? 'bg-zinc-700 text-zinc-100 font-medium shadow-sm'
              : 'text-zinc-400 hover:text-zinc-200')
          }
        >
          Karma · burn PP
        </button>
      </div>
      <div className="flex flex-wrap gap-1">
        {(mode === 'free' ? FREE_EMOJIS : KARMA_EMOJIS).map((e) => (
          <button
            key={e}
            onClick={() => mode === 'free' ? handleFree(e) : handleKarma(e)}
            className="text-lg p-1 hover:bg-zinc-800 rounded"
          >
            {e}
          </button>
        ))}
      </div>
      {mode === 'karma' && (
        <div className="mt-3 space-y-2 border-t border-zinc-800 pt-2 text-xs text-zinc-400">
          <div className="flex items-center gap-2">
            <label className="text-zinc-400">PP to spend:</label>
            <input
              type="number"
              min={KARMA_MIN_PP}
              value={karmaAmount}
              onChange={(e) => {
                const n = Number(e.target.value);
                setKarmaAmount(Number.isFinite(n) ? Math.max(KARMA_MIN_PP, n) : KARMA_MIN_PP);
              }}
              className="w-20 rounded bg-zinc-800 px-2 py-1 text-base md:text-sm text-zinc-100"
            />
            <span className="text-zinc-500">(min {KARMA_MIN_PP})</span>
          </div>
          <div className="text-[11px] leading-relaxed">
            <span className="text-emerald-400">{recipientPp} PP</span> to author
            {' · '}
            <span className="text-amber-400">{burnPp} PP</span> burned
            {' · '}
            <span className="text-zinc-300">{mgmtPp} PP</span> to Management
          </div>
        </div>
      )}
    </div>
  );
}
