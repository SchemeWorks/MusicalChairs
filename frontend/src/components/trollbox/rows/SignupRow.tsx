import React from 'react';
import type { ChatItem } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName, useIsGolden } from '../useDisplayName';
import GoldenName from '../../GoldenName';

export default function SignupRow({ item }: { item: ChatItem }) {
  const newUser = 'signup' in item.kind ? item.kind.signup.newUser : null;
  const name = useDisplayName(newUser);
  const isGolden = useIsGolden(newUser);
  if (!('signup' in item.kind)) return null;
  if (item.deleted) return <div className="px-3 py-1 text-zinc-500 italic text-xs">[removed by Management]</div>;
  return (
    <div className="px-3 py-1 text-xs text-zinc-400">
      🆕{' '}
      {isGolden ? (
        <GoldenName name={name} isGolden={true} className="font-medium" />
      ) : (
        <span className="text-zinc-200 font-medium">{name}</span>
      )}
      {' '}just signed the dotted line.
    </div>
  );
}
