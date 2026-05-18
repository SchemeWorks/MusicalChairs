import React, { useState } from 'react';
import { FREE_EMOJIS, KARMA_EMOJIS, KARMA_MIN_PP } from './trollboxConstants';
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
    await addReact.mutateAsync({ itemId, emoji });
    onClose();
  };
  const handleKarma = async (emoji: string) => {
    const amount = Number.isFinite(karmaAmount) && karmaAmount >= KARMA_MIN_PP
      ? karmaAmount
      : KARMA_MIN_PP;
    try {
      await karmaReact.mutateAsync({ itemId, emoji, ppToBurn: BigInt(amount) });
      onClose();
    } catch (e) {
      // Let the mutation toast handle it via the hook's error path; just close.
      onClose();
    }
  };

  return (
    <div className="rounded-lg border border-zinc-700 bg-zinc-900 p-2 shadow-lg">
      <div className="mb-2 flex gap-2 text-xs">
        <button
          onClick={() => setMode('free')}
          className={mode === 'free' ? 'text-zinc-100 font-medium' : 'text-zinc-500'}
        >
          Free
        </button>
        <button
          onClick={() => setMode('karma')}
          className={mode === 'karma' ? 'text-zinc-100 font-medium' : 'text-zinc-500'}
        >
          Karma (burn PP)
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
        <div className="mt-2 flex items-center gap-2">
          <label className="text-xs text-zinc-400">PP:</label>
          <input
            type="number"
            min={KARMA_MIN_PP}
            value={karmaAmount}
            onChange={(e) => {
              const n = Number(e.target.value);
              setKarmaAmount(Number.isFinite(n) ? Math.max(KARMA_MIN_PP, n) : KARMA_MIN_PP);
            }}
            className="w-20 rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100"
          />
        </div>
      )}
    </div>
  );
}
