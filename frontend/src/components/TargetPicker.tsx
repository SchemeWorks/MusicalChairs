import React, { useMemo } from 'react';
import { Principal } from '@dfinity/principal';
import { Shuffle, X } from 'lucide-react';
import MobileSheet from './MobileSheet';
import { useGetKnownPpHolders, useGetUserNames } from '../hooks/useQueries';
import { useWallet } from '../hooks/useWallet';

interface TargetPickerProps {
  open: boolean;
  spellName: string;
  onSelect: (target: Principal) => void;
  onCancel: () => void;
}

function shortenPrincipal(text: string): string {
  if (text.length <= 12) return text;
  return `${text.slice(0, 5)}…${text.slice(-3)}`;
}

export default function TargetPicker({ open, spellName, onSelect, onCancel }: TargetPickerProps) {
  const { principal: callerPrincipal } = useWallet();
  const { data: holders = [] } = useGetKnownPpHolders();

  const candidateTexts = useMemo(() => {
    return holders
      .map(p => p.toString())
      .filter(p => p !== callerPrincipal);
  }, [holders, callerPrincipal]);

  const { data: nameByPrincipal } = useGetUserNames(candidateTexts);

  const handleRandom = () => {
    if (candidateTexts.length === 0) return;
    const pick = candidateTexts[Math.floor(Math.random() * candidateTexts.length)];
    onSelect(Principal.fromText(pick));
  };

  const handlePick = (principalText: string) => {
    onSelect(Principal.fromText(principalText));
  };

  return (
    <MobileSheet open={open} onOpenChange={(v) => { if (!v) onCancel(); }}>
      <div className="space-y-4">
        <div className="flex items-start justify-between gap-3">
          <div>
            <div className="font-display text-base mc-text-primary">Pick a Target</div>
            <div className="text-xs mc-text-muted mt-0.5">For <span className="font-bold mc-text-primary">{spellName}</span></div>
          </div>
          <button onClick={onCancel} className="mc-text-muted hover:mc-text-primary" aria-label="Close">
            <X className="h-4 w-4" />
          </button>
        </div>

        <button
          onClick={handleRandom}
          disabled={candidateTexts.length === 0}
          className="mc-btn-primary w-full flex items-center justify-center gap-2 py-2 rounded-full text-sm disabled:opacity-40 disabled:cursor-not-allowed"
        >
          <Shuffle className="h-4 w-4" />
          Pick One For Me
        </button>

        <div className="space-y-1 max-h-72 overflow-y-auto pr-1">
          {candidateTexts.length === 0 ? (
            <p className="text-center mc-text-muted text-sm py-6">No other players found yet. Try again once someone else has earned PP.</p>
          ) : (
            candidateTexts.map(p => {
              const name = nameByPrincipal?.get(p)?.trim();
              return (
                <button
                  key={p}
                  onClick={() => handlePick(p)}
                  className="w-full mc-card p-3 flex items-center justify-between text-left hover:bg-white/[0.04] transition-colors"
                >
                  <span className="font-bold mc-text-primary text-sm truncate">
                    {name && name.length > 0 ? name : shortenPrincipal(p)}
                  </span>
                  {name && name.length > 0 && (
                    <span className="text-xs mc-text-muted font-mono ml-3 shrink-0">{shortenPrincipal(p)}</span>
                  )}
                </button>
              );
            })
          )}
        </div>
      </div>
    </MobileSheet>
  );
}
