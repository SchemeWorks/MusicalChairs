import React, { useState, useRef } from 'react';
import { useInternetIdentity } from './hooks/useInternetIdentity';
import { useGetCallerUserProfile, useGetInternalWalletBalance } from './hooks/useQueries';
import LoginButton from './components/LoginButton';
import ProfileSetup from './components/ProfileSetup';
import Dashboard from './components/Dashboard';
import LoadingSpinner from './components/LoadingSpinner';
import ConfettiCanvas from './components/ConfettiCanvas';
import WalletDropdown from './components/WalletDropdown';
import LogoutButton from './components/LogoutButton';
import ErrorBoundary from './components/ErrorBoundary';
import ShenanigansAdminPanel from './components/ShenanigansAdminPanel';
import { Toaster } from '@/components/ui/sonner';
import { Wallet, Dices, AlertTriangle, Users, Wrench, Tent } from 'lucide-react';
import { isCharles, CharlesIcon } from './lib/charles';

const splashQuotes = [
  "You look like someone who understands opportunity.",
  "The best time to get in was yesterday. The second best time is right now.",
  "I've never lied to you. That's more than most can say.",
  "Smart money moves fast. Scared money doesn't move at all.",
  "The only guarantee is that there are no guarantees. But the odds are... interesting.",
];

export default function App() {
  const { identity, principal, isInitializing } = useInternetIdentity();
  const { data: userProfile, isLoading: profileLoading, isFetched } = useGetCallerUserProfile();
  const { data: balanceData } = useGetInternalWalletBalance();
  const [isWalletDropdownOpen, setIsWalletDropdownOpen] = useState(false);
  const [showAdminPanel, setShowAdminPanel] = useState(false);
  const walletButtonRef = useRef<HTMLButtonElement>(null);
  const [splashQuote] = useState(() => splashQuotes[Math.floor(Math.random() * splashQuotes.length)]);

  const isAuthenticated = !!identity;
  const showProfileSetup = isAuthenticated && !profileLoading && isFetched && userProfile === null;
  const showDashboard = isAuthenticated && !showProfileSetup && !profileLoading;

  if (isInitializing) {
    return (
      <div className="mc-bg min-h-screen flex flex-col items-center justify-center gap-6">
        <div className="font-display text-4xl mc-text-primary mc-glow-gold animate-pulse">
          Musical Chairs
        </div>
        <div className="font-display text-base mc-text-gold opacity-60">
          It's a Ponzi!
        </div>
        <LoadingSpinner />
      </div>
    );
  }

  return (
    <ErrorBoundary>
      <div className="mc-bg min-h-screen flex flex-col">
        <ConfettiCanvas />

        {/* Header */}
        <header className="mc-header fixed top-0 left-0 right-0 z-40">
          <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
            <div className="flex items-center h-16 md:h-20">

              {/* Logo */}
              <button onClick={() => { setShowAdminPanel(false); window.scrollTo(0, 0); }} className="flex flex-col text-left hover:opacity-80 transition-opacity">
                <span className="mc-logo text-xl md:text-2xl leading-none">
                  <span className="hidden md:inline">Musical Chairs</span>
                  <span className="md:hidden">MC</span>
                </span>
                <span className="mc-tagline text-sm md:text-base leading-none">
                  It's a Ponzi!
                </span>
              </button>

              <div className="flex-1" />

              {/* Right controls */}
              <div className="flex items-center gap-3">
                {isAuthenticated ? (
                  <>
                    {/* Charles's Office — locked to Charles principals */}
                    {principal && isCharles(principal) && (
                      <button
                        onClick={() => setShowAdminPanel(!showAdminPanel)}
                        className={`mc-btn-secondary flex items-center gap-2 px-3 py-2 text-xs rounded-lg ${
                          showAdminPanel ? 'border-yellow-500/50 text-yellow-400' : 'mc-text-gold'
                        }`}
                      >
                        <CharlesIcon className="h-4 w-4" />
                        <span className="hidden sm:inline">Charles</span>
                      </button>
                    )}

                    {/* Wallet */}
                    <button
                      ref={walletButtonRef}
                      onClick={() => setIsWalletDropdownOpen(!isWalletDropdownOpen)}
                      className="mc-btn-pill flex items-center gap-2"
                    >
                      <Wallet className="h-4 w-4" />
                      <span>Wallet</span>
                    </button>

                    <LogoutButton />
                  </>
                ) : (
                  <LoginButton />
                )}
              </div>
            </div>
          </div>
        </header>

        {/* Main Content */}
        <main className="flex-1 pt-16 md:pt-20">
          <ErrorBoundary fallback={
            <div className="text-center py-16 px-4">
              <Dices className="h-12 w-12 mc-text-purple mb-4 mx-auto" />
              <h2 className="font-display text-xl text-white mb-3">The Table Flipped</h2>
              <p className="mc-text-dim mb-6 text-sm">The house always wins, but the website doesn't always cooperate.</p>
              <button onClick={() => window.location.reload()} className="mc-btn-primary">
                Spin Again
              </button>
            </div>
          }>
            {!isAuthenticated ? (
              /* === SPLASH / LOGIN PAGE === */
              <div className="mc-hero max-w-2xl mx-auto px-4 py-8 md:py-16">
                {/* Hero: logo → tagline → Charles hook */}
                <div className="mc-stagger mc-hero-entrance">
                  <div className="mc-hero-logo">
                    Musical Chairs
                  </div>
                  <div className="mc-tagline text-2xl md:text-3xl mb-4">
                    It's a Ponzi!
                  </div>
                  <p className="font-accent text-sm mc-text-muted italic mb-10">
                    You know exactly what this is. That's what makes it fun.
                  </p>
                </div>

                {/* Three info cards — the pitch builds before the ask */}
                <div className="grid grid-cols-1 md:grid-cols-3 gap-4 text-left mc-stagger">
                  <div className="mc-card mc-accent-green p-5">
                    <Dices className="h-7 w-7 mc-text-green mb-3" />
                    <p className="text-sm mc-text-dim leading-relaxed">
                      Up to 12% daily. Withdraw anytime, or lock it in and let compound interest do its thing.
                    </p>
                  </div>

                  <div className="mc-card mc-accent-danger p-5">
                    <AlertTriangle className="h-7 w-7 mc-text-danger mb-3" />
                    <p className="text-sm mc-text-dim leading-relaxed">
                      This is literally a Ponzi scheme. Only put in what you'd comfortably light on fire.
                    </p>
                  </div>

                  <div className="mc-card mc-accent-gold p-5">
                    <Dices className="h-7 w-7 mc-text-gold mb-3" />
                    <p className="text-sm mc-text-dim leading-relaxed">
                      When the pot empties, the whole thing starts over. If you're still in when that happens — Loss.
                    </p>
                  </div>
                </div>

                {/* Charles quote */}
                <div className="mt-8 text-center">
                  <p className="font-accent text-sm mc-text-dim italic">
                    &ldquo;{splashQuote}&rdquo;
                  </p>
                  <span className="text-xs mc-text-muted font-bold">&mdash; Charles</span>
                </div>

                {/* CTA — after the pitch has landed */}
                <div className="mt-8 flex justify-center">
                  <LoginButton />
                </div>

                {/* Responsible gambling — straight-faced, not Charles */}
                <div className="mt-10 text-center">
                  <p className="text-xs mc-text-muted opacity-60">
                    <AlertTriangle className="h-3 w-3 inline-block mr-1 align-text-top" />
                    This is a gambling game. Please play responsibly.
                  </p>
                </div>
              </div>
            ) : showProfileSetup ? (
              <ErrorBoundary fallback={
                <div className="text-center py-16">
                  <Users className="h-12 w-12 mc-text-purple mb-4 mx-auto" />
                  <h2 className="font-display text-xl text-white mb-3">Onboarding Hit a Snag</h2>
                  <p className="mc-text-dim text-sm">Try logging out and back in. Charles apologizes for nothing.</p>
                </div>
              }>
                <ProfileSetup />
              </ErrorBoundary>
            ) : profileLoading ? (
              <div className="flex flex-col items-center justify-center py-24 gap-6">
                <div className="font-display text-3xl mc-text-primary mc-glow-gold animate-pulse">
                  Musical Chairs
                </div>
                <LoadingSpinner />
                <p className="mc-text-muted text-xs tracking-wider uppercase">Counting your chips...</p>
              </div>
            ) : showAdminPanel ? (
              <ErrorBoundary fallback={
                <div className="text-center py-16">
                  <Wrench className="h-12 w-12 mc-text-gold mb-4 mx-auto" />
                  <h2 className="font-display text-xl text-white mb-3">Charles's Office Is on Fire</h2>
                  <p className="mc-text-dim text-sm mb-4">The back office crashed. The front office is fine. Probably.</p>
                  <button onClick={() => window.location.reload()} className="mc-btn-primary mt-4">
                    Spin Again
                  </button>
                </div>
              }>
                <div className="max-w-7xl mx-auto px-4 py-8">
                  <button
                    onClick={() => setShowAdminPanel(false)}
                    className="mc-btn-secondary flex items-center gap-2 px-4 py-2 text-xs rounded-lg mb-6"
                  >
                    &larr; Leave Charles's Office
                  </button>
                  <ShenanigansAdminPanel />
                </div>
              </ErrorBoundary>
            ) : (
              <ErrorBoundary fallback={
                <div className="text-center py-16">
                  <Tent className="h-12 w-12 mc-text-purple mb-4 mx-auto" />
                  <h2 className="font-display text-xl text-white mb-3">The Dashboard Took a Hit</h2>
                  <p className="mc-text-dim text-sm mb-4">Your money's still there. Probably. Refresh and find out.</p>
                  <button onClick={() => window.location.reload()} className="mc-btn-primary mt-4">
                    Spin Again
                  </button>
                </div>
              }>
                <Dashboard />
              </ErrorBoundary>
            )}
          </ErrorBoundary>
        </main>

        {/* Wallet Dropdown */}
        <ErrorBoundary fallback={null}>
          <WalletDropdown
            isOpen={isWalletDropdownOpen}
            onClose={() => setIsWalletDropdownOpen(false)}
            buttonRef={walletButtonRef}
          />
        </ErrorBoundary>

        {/* Toast */}
        <Toaster
          position="top-center"
          toastOptions={{
            style: {
              background: 'var(--mc-felt-raised)',
              color: 'var(--mc-white)',
              border: '1px solid var(--mc-border)',
              fontFamily: "'Space Mono', monospace",
              fontSize: '14px',
              fontWeight: '700',
              padding: '12px 16px',
              borderRadius: 'var(--radius-md)',
              boxShadow: '0 8px 32px rgba(0,0,0,0.4)',
            },
            duration: 4000,
          }}
        />
      </div>
    </ErrorBoundary>
  );
}
