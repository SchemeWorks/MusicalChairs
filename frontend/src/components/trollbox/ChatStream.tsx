import React, { useState, useEffect, useLayoutEffect, useRef } from 'react';
import ChatItemRow from './ChatItemRow';
import ReactionPicker from './ReactionPicker';
import { useRecentChatItems, useAdminDeleteChatItem } from '../../hooks/useQueries';
import { getBlocked, addBlocked, subscribeBlocked } from './trollboxState';

const STICK_TO_BOTTOM_THRESHOLD_PX = 64;

interface Props {
  currentUserName?: string;
  isAdmin: boolean;
}

export default function ChatStream({ currentUserName, isAdmin }: Props) {
  const { data: items = [] } = useRecentChatItems();
  const adminDelete = useAdminDeleteChatItem();
  const [picker, setPicker] = useState<bigint | null>(null);
  const [blocked, setBlockedLocal] = useState<string[]>(() => getBlocked());

  useEffect(() => {
    const unsub = subscribeBlocked((list) => setBlockedLocal(list));
    return unsub;
  }, []);

  // Spell-cast chat items now embed caster/spell/target/outcome inline, so the
  // join against useGetRecentShenanigans was removed — SpellRow renders from
  // item.kind.spellCast directly. The Live Feed still uses that query.

  const visible = items.filter((item) => {
    if ('userMessage' in item.kind && blocked.includes(item.author.toText())) return false;
    return true;
  });

  const scrollerRef = useRef<HTMLDivElement>(null);
  // Default true so the first non-empty render snaps to the newest message.
  const stickToBottomRef = useRef(true);

  const handleScroll = () => {
    const el = scrollerRef.current;
    if (!el) return;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    stickToBottomRef.current = distanceFromBottom < STICK_TO_BOTTOM_THRESHOLD_PX;
  };

  useLayoutEffect(() => {
    const el = scrollerRef.current;
    if (!el) return;
    if (stickToBottomRef.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [visible.length]);

  const handleBlock = (principalText: string, displayName: string) => {
    const ok = window.confirm(
      `Block ${displayName}?\n\nTheir messages and reactions will be hidden from you. You can unblock them from the Trollbox header (Blocked users).`
    );
    if (!ok) return;
    addBlocked(principalText);
    setBlockedLocal(getBlocked());
  };
  const handleDelete = (itemId: bigint) => {
    adminDelete.mutate(itemId);
  };

  return (
    <>
      {picker !== null && (
        <div className="fixed inset-0 z-[9]" onClick={() => setPicker(null)} aria-hidden="true" />
      )}
      <div
        ref={scrollerRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto overscroll-contain"
      >
        {[...visible].reverse().map((item) => (
          <div key={item.id.toString()} className="group relative">
            <ChatItemRow
              item={item}
              currentUserName={currentUserName}
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
    </>
  );
}
