import React, { useEffect, useRef, useState } from 'react';
import { Principal } from '@dfinity/principal';
import TrollboxFab from './TrollboxFab';
import TrollboxPanel from './TrollboxPanel';
import { getOpen, setOpen, getLastSeenId, setLastSeenId, getChimeMuted } from './trollboxState';
import { CHIME_DEBOUNCE_MS } from './trollboxConstants';
import { useRecentChatItems } from '../../hooks/useQueries';

interface Props {
  authenticated: boolean;
  principal: Principal | null;
  currentUserName?: string;
  isAdmin: boolean;
}

export default function Trollbox({ authenticated, principal, currentUserName, isAdmin }: Props) {
  const [open, setOpenState] = useState<boolean>(() => getOpen());
  const { data: items = [] } = useRecentChatItems();

  const topId = items.length > 0 ? items[0].id : 0n;
  const [lastSeen, setLastSeen] = useState<bigint>(() => getLastSeenId());
  const lastChimeRef = useRef<number>(0);
  const seenIdsRef = useRef<Set<string>>(new Set());
  const seededRef = useRef<boolean>(false);

  const unread = React.useMemo(() => {
    let count = 0;
    for (const item of items) {
      if (item.id <= lastSeen) break;
      if ('userMessage' in item.kind) count++;
    }
    return count;
  }, [items, lastSeen]);

  useEffect(() => {
    if (open && topId > lastSeen) {
      setLastSeen(topId);
      setLastSeenId(topId);
    }
  }, [open, topId, lastSeen]);

  // @-mention chime — only fires for NEW items observed while the panel is
  // closed and the user is authenticated. On the first listen tick after a
  // listen-start transition (panel-close or auth-arrive), seed the seen set
  // from current items without firing — those are historical, not "new."
  useEffect(() => {
    if (open || !currentUserName) {
      // Not listening — clear the seed flag so a future listen-start path seeds again.
      seededRef.current = false;
      return;
    }
    if (!seededRef.current) {
      for (const item of items) {
        seenIdsRef.current.add(item.id.toString());
      }
      seededRef.current = true;
      return;
    }
    if (getChimeMuted()) return;
    const needle = `@${currentUserName}`;
    let triggered = false;
    for (const item of items) {
      const key = item.id.toString();
      if (seenIdsRef.current.has(key)) continue;
      seenIdsRef.current.add(key);
      if ('userMessage' in item.kind && item.kind.userMessage.body.includes(needle)) {
        triggered = true;
      }
    }
    if (triggered) {
      const now = Date.now();
      if (now - lastChimeRef.current >= CHIME_DEBOUNCE_MS) {
        lastChimeRef.current = now;
        try { new Audio('/trollbox-mention.mp3').play(); } catch { /* ignore */ }
      }
    }
  }, [items, open, currentUserName]);

  const handleOpen = () => {
    setOpenState(true);
    setOpen(true);
  };
  const handleClose = () => {
    setOpenState(false);
    setOpen(false);
  };

  if (!open) return <TrollboxFab unread={unread} onClick={handleOpen} />;
  return (
    <TrollboxPanel
      authenticated={authenticated}
      principal={principal}
      currentUserName={currentUserName}
      isAdmin={isAdmin}
      onClose={handleClose}
    />
  );
}
