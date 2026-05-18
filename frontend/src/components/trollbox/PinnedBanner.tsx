import React from 'react';
import { Megaphone } from 'lucide-react';
import { useCurrentPin } from '../../hooks/useQueries';

export default function PinnedBanner() {
  const { data: pin } = useCurrentPin();
  if (!pin || !('pinUpdate' in pin.kind)) return null;
  const body = pin.kind.pinUpdate.body;
  if (!body) return null;
  return (
    <div className="flex items-start gap-2 border-b border-zinc-800 bg-zinc-900/60 px-3 py-2 text-xs">
      <Megaphone className="mt-0.5 h-3 w-3 shrink-0 text-amber-400" />
      <div className="text-zinc-200">{body}</div>
    </div>
  );
}
