import React from 'react';
import type { ChatItem } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName } from '../useDisplayName';

interface Props {
  item: ChatItem;
  currentUserName?: string;
  onBlock?: (principalText: string) => void;
  onReact?: (itemId: bigint) => void;
  isAdmin?: boolean;
  onDelete?: (itemId: bigint) => void;
  blocked: string[];
}

export default function UserMessageRow({ item, currentUserName, onBlock, onReact, isAdmin, onDelete, blocked }: Props) {
  const kind = item.kind;
  if (!('userMessage' in kind)) return null;
  const { body } = kind.userMessage;
  const authorName = useDisplayName(item.author);
  const mentioned = !!currentUserName && body.includes(`@${currentUserName}`);

  if (item.deleted) {
    return <div className="px-3 py-2 text-zinc-500 italic text-sm">[removed by Management]</div>;
  }

  return (
    <div className={`flex gap-2 px-3 py-2 ${mentioned ? 'border-l-2 border-amber-400' : ''}`}>
      <div className="flex h-6 w-6 items-center justify-center rounded-full bg-zinc-700 text-xs font-medium text-zinc-200 shrink-0">
        {authorName.charAt(0).toUpperCase()}
      </div>
      <div className="flex-1 min-w-0">
        <div className="flex items-baseline gap-2">
          <span className="text-sm font-medium text-zinc-200 truncate">{authorName}</span>
          <span className="text-xs text-zinc-500">{formatTimestamp(item.timestamp)}</span>
        </div>
        <div className="text-sm text-zinc-300 break-words whitespace-pre-wrap">{renderBodyWithMentions(body)}</div>
        <ReactionsRow item={item} onReact={onReact} blocked={blocked} />
      </div>
      {(onBlock || (isAdmin && onDelete)) && (
        <RowMenu
          onBlock={onBlock ? () => onBlock(item.author.toText()) : undefined}
          onDelete={isAdmin && onDelete ? () => onDelete(item.id) : undefined}
        />
      )}
    </div>
  );
}

function formatTimestamp(ns: bigint): string {
  const ms = Number(ns / 1_000_000n);
  return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function renderBodyWithMentions(body: string): React.ReactNode {
  const parts = body.split(/(@\w+)/g);
  return parts.map((part, i) =>
    part.startsWith('@')
      ? <span key={i} className="text-amber-400 font-medium">{part}</span>
      : <React.Fragment key={i}>{part}</React.Fragment>
  );
}

function ReactionsRow({ item, onReact, blocked }: { item: ChatItem; onReact?: (id: bigint) => void; blocked: string[] }) {
  if (item.reactions.length === 0) return null;
  return (
    <div className="mt-1 flex flex-wrap gap-1">
      {item.reactions.map((r, i) => {
        const visibleReactors = r.reactors.filter(p => !blocked.includes(p.toText()));
        if (visibleReactors.length === 0 && r.karmaPpBurned === 0n) return null;
        return (
          <button
            key={i}
            onClick={() => onReact?.(item.id)}
            className="rounded-full bg-zinc-800 px-2 py-0.5 text-xs text-zinc-300 hover:bg-zinc-700"
          >
            {r.emoji} {visibleReactors.length}{r.karmaPpBurned > 0n ? ` 💰${formatKarma(r.karmaPpBurned)}` : ''}
          </button>
        );
      })}
    </div>
  );
}

function formatKarma(units: bigint): string {
  const pp = units / 100_000_000n;
  if (pp >= 1000n) return `${(Number(pp) / 1000).toFixed(1)}k`;
  return pp.toString();
}

function RowMenu({ onBlock, onDelete }: { onBlock?: () => void; onDelete?: () => void }) {
  return (
    <div className="opacity-0 group-hover:opacity-100 flex flex-col gap-1">
      {onBlock && <button onClick={onBlock} className="text-xs text-zinc-500 hover:text-zinc-200">block</button>}
      {onDelete && <button onClick={onDelete} className="text-xs text-red-400 hover:text-red-300">🗑️</button>}
    </div>
  );
}
