import React from 'react';
import { Principal } from '@dfinity/principal';
import { X, BellOff, Bell } from 'lucide-react';
import PinnedBanner from './PinnedBanner';
import ChatStream from './ChatStream';
import Composer from './Composer';
import BlockedUsersMenu from './BlockedUsersMenu';
import { getChimeMuted, setChimeMuted } from './trollboxState';
import { DESKTOP_PANEL_HEIGHT_PX, DESKTOP_PANEL_WIDTH_PX, MOBILE_BREAKPOINT_PX } from './trollboxConstants';

interface Props {
  authenticated: boolean;
  principal: Principal | null;
  currentUserName?: string;
  isAdmin: boolean;
  onClose: () => void;
}

export default function TrollboxPanel({ authenticated, principal, currentUserName, isAdmin, onClose }: Props) {
  const [isMobile, setIsMobile] = React.useState(() =>
    typeof window !== 'undefined' && window.innerWidth < MOBILE_BREAKPOINT_PX
  );
  const [chimeMuted, setChimeMutedState] = React.useState(getChimeMuted());
  React.useEffect(() => {
    const update = () => setIsMobile(window.innerWidth < MOBILE_BREAKPOINT_PX);
    window.addEventListener('resize', update);
    return () => window.removeEventListener('resize', update);
  }, []);

  // Lock body scroll while the full-screen mobile panel is mounted so swipes
  // can't bleed through to the dashboard underneath. Desktop panel floats over
  // content and doesn't need the lock.
  React.useEffect(() => {
    if (!isMobile) return;
    const prevOverflow = document.body.style.overflow;
    const prevOverscroll = document.body.style.overscrollBehavior;
    document.body.style.overflow = 'hidden';
    document.body.style.overscrollBehavior = 'contain';
    return () => {
      document.body.style.overflow = prevOverflow;
      document.body.style.overscrollBehavior = prevOverscroll;
    };
  }, [isMobile]);

  const toggleChime = () => {
    const next = !chimeMuted;
    setChimeMuted(next);
    setChimeMutedState(next);
  };

  const containerClass = isMobile
    ? 'fixed inset-0 z-50 flex flex-col bg-zinc-950'
    : 'fixed bottom-4 right-4 z-50 flex flex-col rounded-xl border border-zinc-800 bg-zinc-950 shadow-2xl';
  const containerStyle = isMobile
    ? undefined
    : { width: DESKTOP_PANEL_WIDTH_PX, height: DESKTOP_PANEL_HEIGHT_PX };

  return (
    <div className={containerClass} style={containerStyle}>
      <div className="flex items-center justify-between border-b border-zinc-800 px-3 py-2">
        <div className="text-sm font-medium text-zinc-200">Trollbox</div>
        <div className="flex items-center gap-2">
          <BlockedUsersMenu />
          <button
            onClick={toggleChime}
            aria-label={chimeMuted ? 'Unmute mentions' : 'Mute mentions'}
            className="flex h-8 w-8 items-center justify-center rounded text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100"
          >
            {chimeMuted ? <BellOff className="h-5 w-5" /> : <Bell className="h-5 w-5" />}
          </button>
          <button
            onClick={onClose}
            aria-label="Close chat"
            className="flex h-8 w-8 items-center justify-center rounded text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100"
          >
            <X className="h-5 w-5" />
          </button>
        </div>
      </div>
      <PinnedBanner />
      <ChatStream currentUserName={currentUserName} isAdmin={isAdmin} />
      <Composer authenticated={authenticated} principal={principal} />
    </div>
  );
}
