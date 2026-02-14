import React, { useState } from 'react';
import { useSaveUserProfile } from '../hooks/useQueries';

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
        <div className="mc-tagline text-xl mb-2">It's a Ponzi!</div>
      </div>

      {/* Setup card */}
      <div className="mc-card-elevated">
        <div className="text-center mb-8">
          <div className="text-5xl mb-4">🎰</div>
          <p className="mc-text-dim text-sm">Pick a name — this is what others will see.</p>
        </div>

        <form onSubmit={handleSubmit} className="space-y-6">
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="mc-input w-full text-center text-lg"
            placeholder="Choose a name"
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
          <p className="font-bold text-sm">⚠️ THIS IS A GAMBLING GAME</p>
          <p className="text-xs mt-1 opacity-80">Only play with money you can afford to lose.</p>
        </div>
      </div>
    </div>
  );
}
