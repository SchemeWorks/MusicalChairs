import React from 'react';
import type { ChatItem } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName } from '../useDisplayName';

export default function SignupRow({ item }: { item: ChatItem }) {
  if (!('signup' in item.kind)) return null;
  const name = useDisplayName(item.kind.signup.newUser);
  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;
  return (
    <div className="px-3 py-1 text-xs text-zinc-400">
      🆕 <span className="text-zinc-200 font-medium">{name}</span> just signed the dotted line.
    </div>
  );
}
