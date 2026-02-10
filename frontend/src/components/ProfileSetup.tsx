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
    <div className="max-w-md mx-auto">
      {/* Welcome Header matching dashboard style */}
      <div className="text-center mb-8">
        <div className="dashboard-title-panel mb-2">
          <h2 className="text-4xl font-black dashboard-title-stroked">
            🎪 Welcome to Musical Chairs! 🎪
          </h2>
        </div>
      </div>

      {/* Single Animated Gradient Frosted-Glass Outer Card */}
      <div className="profile-outer-card">
        {/* Slot Machine Icon - Top Center */}
        <div className="text-center mb-6">
          <div className="text-6xl mb-4 slot-icon">🎰</div>
        </div>

        <form onSubmit={handleSubmit} className="space-y-6">
          <div>
            <input
              type="text"
              id="name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="name-input-field"
              placeholder="Choose a name"
              required
            />
            <p className="help-text">
              Pick a fun nickname — this is what others will see!
            </p>
          </div>

          <button
            type="submit"
            disabled={!isNameValid || saveProfile.isPending}
            className={isNameValid ? 'join-game-button-active-with-glow' : 'join-game-button-neutral'}
          >
            {saveProfile.isPending ? '🎰 Joining...' : '🎟️ JOIN THE GAME!'}
          </button>
        </form>

        {saveProfile.isError && (
          <div className="mt-4 p-4 bg-red-50 border-2 border-red-200 rounded-xl">
            <p className="text-sm text-red-600 font-bold text-center">
              Failed to save profile. Please try again.
            </p>
          </div>
        )}

        {/* Red Gambling Warning Box - Separate Card matching Login Page */}
        <div className="profile-inner-red-card">
          <div className="flex items-center justify-center">
            <div className="text-center">
              <p className="text-red-800 font-bold text-sm">
                ⚠️ THIS IS A GAMBLING GAME! ⚠️<br />
                Only play with money you can afford to lose!
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
