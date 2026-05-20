import React, { useState } from 'react';
import { Principal } from '@dfinity/principal';
import { Send } from 'lucide-react';
import { usePostChatMessage, useIsMuted } from '../../hooks/useQueries';
import { TROLLBOX_MAX_MESSAGE } from './trollboxConstants';
import { toast } from 'sonner';

interface Props {
  authenticated: boolean;
  principal: Principal | null;
}

export default function Composer({ authenticated, principal }: Props) {
  const [body, setBody] = useState('');
  const post = usePostChatMessage();
  const { data: muteExpiry } = useIsMuted(principal);

  if (!authenticated) {
    return (
      <div className="border-t border-zinc-800 px-3 py-2 text-xs text-zinc-500">
        Sign in to join the conversation.
      </div>
    );
  }
  if (muteExpiry) {
    const date = new Date(Number(muteExpiry / 1_000_000n));
    return (
      <div className="border-t border-zinc-800 px-3 py-2 text-xs text-red-400">
        You've been muted by the Management until {date.toLocaleString()}.
      </div>
    );
  }

  const remaining = TROLLBOX_MAX_MESSAGE - [...body].length;
  const submit = async () => {
    if (!body.trim()) return;
    try {
      await post.mutateAsync({ body, replyTo: null });
      setBody('');
    } catch (e) {
      toast.error((e as Error).message);
    }
  };
  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      submit();
    }
  };

  return (
    <div className="border-t border-zinc-800 p-2">
      <div className="flex items-end gap-2">
        <textarea
          value={body}
          onChange={(e) => setBody([...e.target.value].slice(0, TROLLBOX_MAX_MESSAGE).join(''))}
          onKeyDown={onKeyDown}
          placeholder="Say something compromising…"
          rows={2}
          className="flex-1 resize-none rounded bg-zinc-800 px-2 py-1 text-base md:text-sm text-zinc-100 placeholder:text-zinc-500 focus:outline-none focus:ring-1 focus:ring-amber-400/40"
        />
        <button
          onClick={submit}
          disabled={!body.trim() || post.isPending}
          className="rounded bg-amber-500 px-2 py-1.5 text-zinc-900 disabled:opacity-40"
        >
          <Send className="h-4 w-4" />
        </button>
      </div>
      <div className="mt-1 flex justify-end text-[10px] text-zinc-500">
        {remaining} left
      </div>
    </div>
  );
}
