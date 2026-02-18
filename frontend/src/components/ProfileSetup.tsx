import React, { useState, useEffect, useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useSaveUserProfile } from '../hooks/useQueries';
import { triggerConfetti } from './ConfettiCanvas';
import { Dices, AlertTriangle, PartyPopper } from 'lucide-react';

const MAX_NAME_LENGTH = 20;

/** Idle ripple — letters are visible immediately, gentle wave every 3s */
function RippleText({ text, interval = 3000, amplitude = 4, stagger = 40 }: {
  text: string; interval?: number; amplitude?: number; stagger?: number;
}) {
  const lettersRef = useRef<(HTMLSpanElement | null)[]>([]);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

    function runRipple() {
      const els = lettersRef.current;
      const rafs: number[] = [];
      els.forEach((el, i) => {
        if (!el) return;
        const letterStart = performance.now() + i * stagger;
        const dur = 400;
        function tick(now: number) {
          const elapsed = now - letterStart;
          if (elapsed < 0) { rafs.push(requestAnimationFrame(tick)); return; }
          const t = Math.min(elapsed / dur, 1);
          el.style.transform = `translateY(${-amplitude * Math.sin(Math.PI * t)}px)`;
          if (t < 1) rafs.push(requestAnimationFrame(tick));
          else el.style.transform = 'translateY(0px)';
        }
        rafs.push(requestAnimationFrame(tick));
      });
    }

    const timer = setInterval(runRipple, interval);
    const firstRipple = setTimeout(runRipple, 1000); // first ripple after 1s
    return () => { clearInterval(timer); clearTimeout(firstRipple); };
  }, []);

  return (
    <span className="mc-tagline" style={{ display: 'inline-block' }}>
      {text.split('').map((char, i) => (
        <span
          key={i}
          ref={el => { lettersRef.current[i] = el; }}
          style={{ display: 'inline-block', width: char === ' ' ? '0.3em' : undefined }}
        >
          {char}
        </span>
      ))}
    </span>
  );
}

export default function ProfileSetup() {
  const [name, setName] = useState('');
  const [showCelebration, setShowCelebration] = useState(false);
  const [savedName, setSavedName] = useState('');
  const [shakeInput, setShakeInput] = useState(false);
  const queryClient = useQueryClient();
  const saveProfile = useSaveUserProfile();

  const trimmedName = name.trim();
  const isNameValid = trimmedName.length > 0 && trimmedName.length <= MAX_NAME_LENGTH;

  const triggerShake = () => {
    setShakeInput(true);
    setTimeout(() => setShakeInput(false), 400);
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!trimmedName) { triggerShake(); return; }
    if (trimmedName.length > MAX_NAME_LENGTH) { triggerShake(); return; }
    setSavedName(trimmedName);
    saveProfile.mutate({ name: trimmedName }, {
      onSuccess: () => {
        setShowCelebration(true);
        triggerConfetti();
        // Delay redirect so the celebration screen lingers for the user to read
        // The query invalidation triggers App.tsx to swap ProfileSetup → Dashboard
      },
    });
  };

  // Let the celebration screen breathe — redirect after 5 seconds
  useEffect(() => {
    if (showCelebration) {
      const timer = setTimeout(() => {
        queryClient.invalidateQueries({ queryKey: ['userProfile'] });
      }, 5000);
      return () => clearTimeout(timer);
    }
  }, [showCelebration, queryClient]);

  if (showCelebration) {
    return (
      <div className="max-w-md mx-auto px-4 py-20 text-center mc-hero-entrance">
        <PartyPopper className="h-16 w-16 mc-text-gold mx-auto mb-6" />
        <h1 className="font-display text-2xl sm:text-3xl mc-text-primary mb-4">
          Welcome to Musical Chairs, {savedName}!
        </h1>
        <p className="font-accent text-sm mc-text-dim italic mb-2">
          &ldquo;I knew you had it in you.&rdquo;
        </p>
        <span className="text-xs mc-text-muted font-bold">&mdash; Charles</span>

        <div className="mt-10">
          <div className="mc-spinner mx-auto mb-3" />
          <p className="text-xs mc-text-muted">Setting up your table...</p>
          <button
            onClick={() => queryClient.invalidateQueries({ queryKey: ['userProfile'] })}
            className="mc-btn-primary mt-4 px-6 py-2 text-sm"
          >
            TAKE ME TO THE TABLE
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-2xl mx-auto px-4 py-8 md:py-16">
      {/* Hero — matches splash page sizing, no stagger (avoids transform override) */}
      <div className="text-center">
        <div className="mc-hero-logo">
          Musical Chairs
        </div>
        <div className="text-3xl md:text-4xl mb-4">
          <RippleText text="It's a Ponzi!" />
        </div>
        <div className="mb-10" />
      </div>

      {/* Setup card — dice icon floats above the top rail */}
      <div className="flex flex-col items-center max-w-md mx-auto">
        <div className="mc-icon-disc mc-icon-disc-purple mb-[-20px] z-10">
          <Dices className="h-7 w-7 mc-text-purple" />
        </div>
        <div className="mc-card-elevated mc-registration-glow pt-8 w-full">
          <div className="text-center mb-8">
            <p className="mc-text-primary text-sm font-display tracking-wide">
              Everyone who walks through that door gets a seat at the table.
            </p>
          </div>

          <form onSubmit={handleSubmit} className="space-y-6 px-1">
            <div>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className={`mc-input w-full text-center text-lg ${shakeInput ? 'mc-shake' : ''}`}
                placeholder="What do they call you?"
                maxLength={MAX_NAME_LENGTH + 5} /* soft limit — let them type a bit over to see the counter turn red */
                required
              />
              {/* Character count & validation feedback */}
              <div className="flex justify-between mt-1.5 text-xs">
                <span className={name.length > MAX_NAME_LENGTH ? 'mc-text-danger' : 'mc-text-muted'}>
                  {name.length > 0 ? `${name.length}/${MAX_NAME_LENGTH} characters` : ''}
                </span>
                {name.length > MAX_NAME_LENGTH && (
                  <span className="mc-text-danger">Too long</span>
                )}
              </div>
              {trimmedName && trimmedName.length <= MAX_NAME_LENGTH && (
                <p className="text-xs mc-text-muted mt-1 text-center">
                  Players will see you as: <span className="text-white font-bold">{trimmedName}</span>
                </p>
              )}
            </div>

            <button
              type="submit"
              disabled={!isNameValid || saveProfile.isPending}
              className={`w-full py-4 text-lg font-bold rounded-xl transition-all ${
                isNameValid
                  ? 'mc-btn-primary pulse'
                  : 'mc-btn-primary opacity-50 cursor-not-allowed'
              }`}
            >
              {saveProfile.isPending ? 'Pulling up a chair...' : isNameValid ? 'TAKE YOUR SEAT' : 'JOIN THE GAME'}
            </button>
          </form>

          {saveProfile.isError && (
            <div className="mc-status-red p-3 mt-4 text-center text-sm">
              Failed to create profile. Please try again.
            </div>
          )}
        </div>
      </div>

      {/* Warning box — matches splash page exactly */}
      <div className="mt-10 mx-auto max-w-lg mc-warning-box text-center">
        <div className="flex items-center justify-center gap-2 mb-2">
          <AlertTriangle className="h-5 w-5 text-red-400 shrink-0" />
          <span className="text-base font-bold text-red-300 uppercase tracking-wide">This is a gambling game</span>
          <AlertTriangle className="h-5 w-5 text-red-400 shrink-0" />
        </div>
        <p className="text-sm text-red-300/80">All positions carry risk of total loss.</p>
        <p className="text-sm text-red-300/80">Please play responsibly.</p>
      </div>

      {/* Charles quote — below the warning */}
      <div className="mt-6 text-center">
        <p className="font-accent text-sm mc-text-dim italic leading-relaxed max-w-xs mx-auto">
          &ldquo;I&rsquo;m glad you&rsquo;re here. Truly.
          Let me show you something special.&rdquo;
        </p>
        <span className="text-xs mc-text-muted font-bold">&mdash; Charles</span>
      </div>
    </div>
  );
}
