import React, { useEffect, useRef, useState } from 'react';
import { Principal } from '@dfinity/principal';
import TrollboxFab from './TrollboxFab';
import TrollboxPanel from './TrollboxPanel';
import { getOpen, setOpen, getLastSeenId, setLastSeenId, getChimeMuted } from './trollboxState';
import { CHIME_DEBOUNCE_MS } from './trollboxConstants';
import { useRecentChatItems, useListChimeSounds } from '../../hooks/useQueries';
import { useReadShenaniganActor } from '../../hooks/useShenaniganActor';

interface Props {
  authenticated: boolean;
  principal: Principal | null;
  currentUserName?: string;
  isAdmin: boolean;
}

export default function Trollbox({ authenticated, principal, currentUserName, isAdmin }: Props) {
  const [open, setOpenState] = useState<boolean>(() => getOpen());
  const { data: items = [] } = useRecentChatItems();
  const { data: chimeList = [] } = useListChimeSounds();
  const actor = useReadShenaniganActor();

  const topId = items.length > 0 ? items[0].id : 0n;
  const [lastSeen, setLastSeen] = useState<bigint>(() => getLastSeenId());
  const lastChimeRef = useRef<number>(0);
  const seenIdsRef = useRef<Set<string>>(new Set());
  const seededRef = useRef<boolean>(false);
  const audioCacheRef = useRef<Map<string, HTMLAudioElement | null>>(new Map());

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

  // Sync audio cache with the chime list. Drop removed sounds; fetch new ones lazily.
  useEffect(() => {
    if (!actor) return;
    const desired = new Set(chimeList.map((m: any) => m.name as string));
    // Drop removed
    for (const name of Array.from(audioCacheRef.current.keys())) {
      if (!desired.has(name)) {
        const a = audioCacheRef.current.get(name);
        if (a) {
          try { URL.revokeObjectURL(a.src); } catch { /* ignore */ }
        }
        audioCacheRef.current.delete(name);
      }
    }
    // Fetch new
    for (const meta of chimeList) {
      if (audioCacheRef.current.has(meta.name)) continue;
      // Mark a placeholder to avoid double-fetch races.
      audioCacheRef.current.set(meta.name, null);
      (async () => {
        try {
          const opt = await actor.getChimeSound(meta.name);
          if (opt.length === 0) {
            audioCacheRef.current.delete(meta.name);
            return;
          }
          const sound = opt[0];
          const bytes = sound.bytes instanceof Uint8Array
            ? sound.bytes
            : new Uint8Array(sound.bytes as ArrayLike<number>);
          const blob = new Blob([bytes], { type: sound.mimeType });
          const url = URL.createObjectURL(blob);
          const audio = new Audio(url);
          audio.preload = 'auto';
          audioCacheRef.current.set(meta.name, audio);
        } catch {
          audioCacheRef.current.delete(meta.name);
        }
      })();
    }
  }, [chimeList, actor]);

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
        // Prefer the uploaded pool; fall back to the static path.
        const available = Array.from(audioCacheRef.current.entries())
          .filter(([_, a]) => !!a)
          .map(([_, a]) => a as HTMLAudioElement);
        try {
          if (available.length > 0) {
            const pick = available[Math.floor(Math.random() * available.length)];
            const cloned = pick.cloneNode() as HTMLAudioElement;
            cloned.play();
          } else {
            new Audio('/trollbox-mention.mp3').play();
          }
        } catch { /* ignore */ }
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
