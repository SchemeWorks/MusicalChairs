import React, { useState } from 'react';
import { useSaveUserProfile } from '../hooks/useQueries';
import { Dices, AlertTriangle } from 'lucide-react';

export default function ProfileSetup() {
  const [name, setName] = useState('');
  const saveProfile = useSaveUserProfile();

  const isNameValid = name.trim().length > 0;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (name.trim()) {
      saveProfile.mutate({ name: name.trim() });
    }
  };

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
          <p className="mc-text-dim text-sm">Every great partnership starts with a name.</p>
        </div>

        <form onSubmit={handleSubmit} className="space-y-6">
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="mc-input w-full text-center text-lg"
            placeholder="What should we call you?"
            required
          />

          <button
            type="submit"
            disabled={!isNameValid || saveProfile.isPending}
            className={`w-full py-4 text-lg font-bold rounded-xl transition-all ${
              isNameValid
                ? 'mc-btn-primary pulse'
                : 'mc-btn-primary opacity-50 cursor-not-allowed'
            }`}
          >
            {saveProfile.isPending ? 'Joining...' : 'JOIN THE GAME'}
          </button>
        </form>

        {saveProfile.isError && (
          <div className="mc-status-red p-3 mt-4 text-center text-sm">
            Failed to create profile. Please try again.
          </div>
        )}

        {/* Gambling warning */}
        <div className="mc-status-red p-4 mt-6 text-center">
          <p className="font-bold text-sm flex items-center justify-center gap-2"><AlertTriangle className="h-4 w-4" /> THIS IS A GAMBLING GAME</p>
          <p className="text-xs mt-1 opacity-80">Real ICP. Real risk. Only play with what you can afford to lose.</p>
        </div>
      </div>
    </div>
  );
}
