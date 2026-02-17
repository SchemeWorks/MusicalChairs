import React, { useState, useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useSaveUserProfile } from '../hooks/useQueries';
import { triggerConfetti } from './ConfettiCanvas';
import { Dices, AlertTriangle, PartyPopper, CreditCard } from 'lucide-react';

const MAX_NAME_LENGTH = 20;

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
        // Explicitly refetch user profile so App.tsx detects it and redirects
        queryClient.invalidateQueries({ queryKey: ['userProfile'] });
      },
    });
  };

  // Fallback: if React Query hasn't redirected after 3 seconds, force another refetch
  useEffect(() => {
    if (showCelebration) {
      const timer = setTimeout(() => {
        queryClient.invalidateQueries({ queryKey: ['userProfile'] });
      }, 3000);
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
    <div className="max-w-md mx-auto px-4 py-12 md:py-20">
      {/* Hero */}
      <div className="text-center mb-10">
        <div className="mc-hero-logo text-3xl md:text-4xl">Musical Chairs</div>
        <div className="mc-tagline text-xl mb-4">It's a Ponzi!</div>
        <p className="font-accent text-sm mc-text-dim italic leading-relaxed max-w-xs mx-auto">
          &ldquo;I&rsquo;m glad you&rsquo;re here. Truly.
          Let me show you something special.&rdquo;
        </p>
        <span className="text-xs mc-text-muted font-bold">&mdash; Charles</span>
      </div>

      {/* Decorative casino registration icon */}
      <div className="flex justify-center mb-6 opacity-40">
        <div className="relative">
          <CreditCard className="h-12 w-12 mc-text-gold absolute -rotate-12 -translate-x-2" />
          <CreditCard className="h-12 w-12 mc-text-purple rotate-6 translate-x-2" />
        </div>
      </div>

      {/* Setup card */}
      <div className="mc-card-elevated mc-registration-glow">
        <div className="text-center mb-8">
          <Dices className="h-12 w-12 mc-text-purple mb-4 mx-auto" />
          <p className="mc-text-dim text-sm">Everyone who walks through that door gets a seat at the table.</p>
        </div>

        <form onSubmit={handleSubmit} className="space-y-6">
          <div>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className={`mc-input w-full text-center text-lg ${shakeInput ? 'mc-shake' : ''}`}
              placeholder="Your name, future millionaire"
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

        {/* Ponzi disclaimer — Charles voice, normal text */}
        <div className="mt-6 text-center">
          <p className="font-bold text-sm mc-text-dim flex items-center justify-center gap-2">
            <AlertTriangle className="h-4 w-4 mc-text-danger" /> THIS IS A REAL PONZI SCHEME
          </p>
          <p className="text-xs mc-text-muted mt-1">Real ICP. Real risk. Real fun. Only put in what you'd comfortably set on fire.</p>
        </div>

        {/* Gambling disclaimer — straight-faced, red warning box */}
        <div className="mc-status-red p-3 mt-4 text-center">
          <p className="text-xs flex items-center justify-center gap-1.5">
            <AlertTriangle className="h-3 w-3" /> This is a gambling game. Please play responsibly.
          </p>
        </div>
      </div>
    </div>
  );
}
