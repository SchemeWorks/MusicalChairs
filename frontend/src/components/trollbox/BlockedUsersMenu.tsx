import React from 'react';
import { Principal } from '@dfinity/principal';
import { UserX } from 'lucide-react';
import { getBlocked, removeBlocked, subscribeBlocked } from './trollboxState';
import { useDisplayName } from './useDisplayName';

export default function BlockedUsersMenu() {
  const [open, setOpen] = React.useState(false);
  const [blocked, setBlocked] = React.useState<string[]>(() => getBlocked());

  React.useEffect(() => {
    const unsub = subscribeBlocked(setBlocked);
    return unsub;
  }, []);

  const refresh = () => setBlocked(getBlocked());
  const handleUnblock = (p: string) => {
    removeBlocked(p);
    refresh();
  };

  return (
    <div className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        aria-label={`Blocked users (${blocked.length})`}
        title={`Blocked users (${blocked.length})`}
        className="relative flex h-8 w-8 items-center justify-center rounded text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100"
      >
        <UserX className="h-5 w-5" />
        {blocked.length > 0 && (
          <span className="absolute top-0 right-0 rounded-full bg-amber-500 px-1 text-[10px] font-medium leading-none text-zinc-950">
            {blocked.length}
          </span>
        )}
      </button>
      {open && (
        <div className="absolute right-0 top-10 z-20 w-64 rounded-lg border border-zinc-700 bg-zinc-900 p-2 shadow-xl">
          <div className="mb-1 px-1 text-xs font-medium text-zinc-400">Blocked users</div>
          {blocked.length === 0 ? (
            <div className="px-1 py-2 text-xs text-zinc-500">No one is blocked.</div>
          ) : (
            <ul className="max-h-64 overflow-y-auto">
              {blocked.map((p) => (
                <BlockedRow key={p} principalText={p} onUnblock={() => handleUnblock(p)} />
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

function BlockedRow({ principalText, onUnblock }: { principalText: string; onUnblock: () => void }) {
  const principal = React.useMemo(() => {
    try { return Principal.fromText(principalText); } catch { return null; }
  }, [principalText]);
  const name = useDisplayName(principal);
  return (
    <li className="flex items-center justify-between gap-2 rounded px-1 py-1 hover:bg-zinc-800">
      <span className="truncate text-xs text-zinc-200">{name || principalText}</span>
      <button
        onClick={onUnblock}
        className="shrink-0 rounded bg-zinc-800 px-2 py-0.5 text-xs text-zinc-300 hover:bg-zinc-700"
      >
        unblock
      </button>
    </li>
  );
}
