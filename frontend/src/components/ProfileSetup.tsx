import React, { useState, useEffect } from 'react';
import { useSaveUserProfile } from '../hooks/useQueries';
import { triggerConfetti } from './ConfettiCanvas';
import { Dices, AlertTriangle, PartyPopper } from 'lucide-react';

export default function ProfileSetup() {
  const [name, setName] = useState('');
  const [showCelebration, setShowCelebration] = useState(false);
  const [savedName, setSavedName] = useState('');
  const saveProfile = useSaveUserProfile();

  const isNameValid = name.trim().length > 0;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (name.trim()) {
      const trimmedName = name.trim();
      setSavedName(trimmedName);
      saveProfile.mutate({ name: trimmedName }, {
        onSuccess: () => {
          setShowCelebration(true);
          triggerConfetti();
        },
      });
    }
  };

  // Auto-dismiss after 4 seconds (React Query will refetch and redirect)
  useEffect(() => {
    if (showCelebration) {
      const timer = setTimeout(() => {
        // App.tsx will detect the profile exists and redirect automatically
      }, 4000);
      return () => clearTimeout(timer);
    }
  }, [showCelebration]);

  if (showCelebration) {
    return (
      <div className="max-w-md mx-auto px-4 py-20 text-center mc-hero-entrance">
        <PartyPopper className="h-16 w-16 mc-text-gold mx-auto mb-6" />
        <h1 className="font-display text-3xl mc-text-primary mb-4">
          Welcome to Musical Chairs, {savedName}!
        </h1>
        <p className="font-accent text-sm mc-text-dim italic mb-2">
          &ldquo;I knew you had it in you.&rdquo;
        </p>
        <span className="text-xs mc-text-muted font-bold">&mdash; Charles</span>

        <div className="mt-10">
          <div className="mc-spinner mx-auto mb-3" />
          <p className="text-xs mc-text-muted">Setting up your table...</p>
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

      {/* Setup card */}
      <div className="mc-card-elevated">
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
              className="mc-input w-full text-center text-lg"
              placeholder="Your name, future millionaire"
              required
            />
            {name.trim() && (
              <p className="text-xs mc-text-muted mt-2 text-center">
                Players will see you as: <span className="text-white font-bold">{name.trim()}</span>
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
