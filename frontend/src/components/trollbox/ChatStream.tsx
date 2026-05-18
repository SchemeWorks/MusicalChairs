import React, { useState, useEffect } from 'react';
import type { ShenaniganRecord } from '../../declarations/shenanigans/shenanigans.did';
import ChatItemRow from './ChatItemRow';
import ReactionPicker from './ReactionPicker';
import { useRecentChatItems, useGetRecentShenanigans, useAdminDeleteChatItem } from '../../hooks/useQueries';
import { getBlocked, addBlocked, subscribeBlocked } from './trollboxState';

interface Props {
  currentUserName?: string;
  isAdmin: boolean;
}

export default function ChatStream({ currentUserName, isAdmin }: Props) {
  const { data: items = [] } = useRecentChatItems();
  const { data: spells = [] } = useGetRecentShenanigans();
  const adminDelete = useAdminDeleteChatItem();
  const [picker, setPicker] = useState<bigint | null>(null);
  const [blocked, setBlockedLocal] = useState<string[]>(() => getBlocked());

  useEffect(() => {
    const unsub = subscribeBlocked((list) => setBlockedLocal(list));
    return unsub;
  }, []);

  const spellLookup = React.useMemo(() => {
    const m = new Map<string, ShenaniganRecord>();
    for (const r of spells) m.set(r.id.toString(), r);
    return m;
  }, [spells]);

  const visible = items.filter((item) => {
    if ('userMessage' in item.kind && blocked.includes(item.author.toText())) return false;
    return true;
  });

  const handleBlock = (principalText: string) => {
    addBlocked(principalText);
    setBlockedLocal(getBlocked());
  };
  const handleDelete = (itemId: bigint) => {
    adminDelete.mutate(itemId);
  };

  return (
    <div className="flex-1 overflow-y-auto">
      {[...visible].reverse().map((item) => (
        <div key={item.id.toString()} className="group relative">
          <ChatItemRow
            item={item}
            currentUserName={currentUserName}
            spellLookup={spellLookup}
            isAdmin={isAdmin}
            onBlock={handleBlock}
            onReact={(id) => setPicker(id)}
            onDelete={handleDelete}
            blocked={blocked}
          />
          {picker === item.id && (
            <div className="absolute right-2 top-2 z-10">
              <ReactionPicker itemId={item.id} onClose={() => setPicker(null)} />
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
