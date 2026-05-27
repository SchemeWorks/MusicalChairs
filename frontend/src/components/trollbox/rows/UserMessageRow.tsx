import React from 'react';
import { minidenticon } from 'minidenticons';
import type { ChatItem } from '../../../declarations/shenanigans/shenanigans.did';
import { useDisplayName, useIsGolden, useIsStrategicReserve, useHasVoiceOfGod, useCustomTitle } from '../useDisplayName';
import GoldenName, { PurpleName } from '../../GoldenName';

interface Props {
  item: ChatItem;
  currentUserName?: string;
  onBlock?: (principalText: string, displayName: string) => void;
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
  const isGolden = useIsGolden(item.author);
  const isPurple = useIsStrategicReserve(item.author);
  const hasVoiceOfGod = useHasVoiceOfGod(item.author);
  const customTitle = useCustomTitle(item.author);
  const mentioned = !!currentUserName && body.includes(`@${currentUserName}`);

  if (item.deleted) {
    return <div className="px-3 py-2 text-zinc-500 italic text-sm">[removed by Management]</div>;
  }

  return (
    <div
      className={`relative flex gap-2 px-3 py-2 ${hasVoiceOfGod ? 'mc-voice-of-god' : ''}${
        isGolden ? ' border-l-2 border-[var(--mc-gold)]' : mentioned ? ' border-l-2 border-amber-400' : ''
      }`}
      style={isGolden ? { backgroundImage: 'linear-gradient(90deg, rgba(255,215,0,0.08), transparent 60%)' } : undefined}
    >
      {isGolden ? (
        <div className="rounded-full p-[2px] bg-[var(--mc-gold)]/40 shrink-0" style={{ boxShadow: '0 0 8px rgba(255, 215, 0, 0.4)' }}>
          <Identicon seed={item.author.toText()} />
        </div>
      ) : (
        <Identicon seed={item.author.toText()} />
      )}

      <div className="flex-1 min-w-0">
        <div className="flex items-baseline gap-2 flex-wrap">
          {isGolden ? (
            <GoldenName name={authorName} isGolden={true} className="text-sm font-medium truncate" />
          ) : isPurple ? (
            <PurpleName name={authorName} isPurple={true} className="text-sm font-medium truncate" />
          ) : (
            <span className="text-sm font-medium text-zinc-200 truncate">{authorName}</span>
          )}
          {customTitle && (
            <span className="mc-text-custom-title-bracket">⟨{customTitle}⟩</span>
          )}
          <span className="text-xs text-zinc-500">{formatTimestamp(item.timestamp)}</span>
        </div>
        <div className="text-sm text-zinc-300 break-words whitespace-pre-wrap">{renderBodyWithMentions(body)}</div>
        <ReactionsRow item={item} onReact={onReact} blocked={blocked} />
      </div>
      {(onReact || onBlock || (isAdmin && onDelete)) && (
        <RowMenu
          onReact={onReact ? () => onReact(item.id) : undefined}
          onBlock={onBlock ? () => onBlock(item.author.toText(), authorName) : undefined}
          onDelete={isAdmin && onDelete ? () => onDelete(item.id) : undefined}
        />
      )}
    </div>
  );
}

const Identicon = React.memo(function Identicon({ seed }: { seed: string }) {
  const svg = React.useMemo(() => minidenticon(seed, 60, 50), [seed]);
  const dataUri = `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
  return (
    <img
      src={dataUri}
      alt=""
      className="h-6 w-6 rounded-full bg-zinc-800 shrink-0"
    />
  );
});

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

function RowMenu({ onReact, onBlock, onDelete }: { onReact?: () => void; onBlock?: () => void; onDelete?: () => void }) {
  const [overflowOpen, setOverflowOpen] = React.useState(false);
  const hasOverflow = !!onBlock || !!onDelete;

  React.useEffect(() => {
    if (!overflowOpen) return;
    const close = () => setOverflowOpen(false);
    window.addEventListener('click', close);
    return () => window.removeEventListener('click', close);
  }, [overflowOpen]);

  return (
    <div className="absolute right-2 top-1 opacity-0 group-hover:opacity-100 focus-within:opacity-100 flex items-center gap-1 rounded-md border border-zinc-800 bg-zinc-900/90 px-1 py-0.5 shadow-sm">
      {onReact && (
        <button
          onClick={onReact}
          aria-label="React"
          className="rounded px-1 text-sm hover:bg-zinc-800"
        >
          😊
        </button>
      )}
      {hasOverflow && (
        <div className="relative">
          <button
            onClick={(e) => { e.stopPropagation(); setOverflowOpen((v) => !v); }}
            aria-label="More actions"
            className="rounded px-1 text-sm text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200"
          >
            ⋯
          </button>
          {overflowOpen && (
            <div
              onClick={(e) => e.stopPropagation()}
              className="absolute right-0 top-6 z-10 flex w-28 flex-col rounded-md border border-zinc-700 bg-zinc-900 py-1 shadow-lg"
            >
              {onBlock && (
                <button
                  onClick={() => { setOverflowOpen(false); onBlock(); }}
                  className="px-2 py-1 text-left text-xs text-zinc-300 hover:bg-zinc-800"
                >
                  Block user
                </button>
              )}
              {onDelete && (
                <button
                  onClick={() => { setOverflowOpen(false); onDelete(); }}
                  className="px-2 py-1 text-left text-xs text-red-400 hover:bg-zinc-800"
                >
                  Delete (admin)
                </button>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
