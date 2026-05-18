import React from 'react';
import { MessageCircle } from 'lucide-react';

interface Props {
  unread: number;
  onClick: () => void;
}

export default function TrollboxFab({ unread, onClick }: Props) {
  return (
    <button
      onClick={onClick}
      aria-label="Open chat"
      className="fixed bottom-4 right-4 z-50 flex h-12 w-12 items-center justify-center rounded-full bg-amber-500 text-zinc-900 shadow-lg hover:bg-amber-400"
    >
      <MessageCircle className="h-6 w-6" />
      {unread > 0 && (
        <span className="absolute -top-1 -right-1 flex h-5 min-w-5 items-center justify-center rounded-full bg-red-500 px-1 text-[11px] font-medium text-white">
          {unread > 99 ? '99+' : unread}
        </span>
      )}
    </button>
  );
}
