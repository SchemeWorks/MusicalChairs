import React, { useState, useRef, useEffect } from 'react';
import { useInternetIdentity } from './hooks/useInternetIdentity';
import { useGetCallerUserProfile, useGetInternalWalletBalance, useGetUserGames, useGetPonziPoints, useGetPublicStats, useGetReferralStats } from './hooks/useQueries';
import { useLivePortfolio } from './hooks/useLiveEarnings';
import LoginButton from './components/LoginButton';
import ProfileSetup from './components/ProfileSetup';
import Dashboard from './components/Dashboard';
import LoadingSpinner from './components/LoadingSpinner';
import ConfettiCanvas from './components/ConfettiCanvas';
import WalletDropdown from './components/WalletDropdown';
import LogoutButton from './components/LogoutButton';
import ErrorBoundary from './components/ErrorBoundary';
import ShenanigansAdminPanel from './components/ShenanigansAdminPanel';
import GameStatusBar from './components/GameStatusBar';
import { Toaster } from '@/components/ui/sonner';
import { Wallet, Dices, AlertTriangle, Users, Wrench, Tent, DollarSign, Rocket, Landmark, Dice5, BookOpen, CircleDollarSign } from 'lucide-react';
import DocsPage from './components/DocsPage';
import { Footer } from './components/Footer';
import { formatICP } from './lib/formatICP';
import { isCharles, CharlesIcon } from './lib/charles';

export type TabType = 'profitCenter' | 'invest' | 'seedRound' | 'mlm' | 'shenanigans';

const headerNavItems: Array<{ id: TabType; label: string; icon: React.ReactNode; glowClass?: string }> = [
  { id: 'profitCenter', label: 'Profit Center', icon: <DollarSign className="h-4 w-4" /> },
  { id: 'invest', label: '\u201CInvest\u201D', icon: <Rocket className="h-4 w-4" /> },
  { id: 'seedRound', label: 'Seed Round', icon: <Landmark className="h-4 w-4" /> },
  { id: 'mlm', label: 'MLM', icon: <Users className="h-4 w-4" /> },
  { id: 'shenanigans', label: 'Shenanigans', icon: <Dice5 className="h-4 w-4" />, glowClass: 'mc-icon-glow-green' },
];

const howItWorks: { step: string; title: string; body: string; color: string; fine?: string }[] = [
  {
    step: '1',
    title: 'Pick Your Plan',
    body: `Choose your "investment" wisely. This is the best decision you'll ever make.`,
    color: 'green',
  },
  {
    step: '2',
    title: 'Collect Returns',
    body: 'Withdraw anytime, or lock in until maturity for life-changing returns.',
    color: 'gold',
  },
  {
    step: '3',
    title: 'The Reset',
    body: 'Pot empties, round resets, whole thing starts over. You knew this going in.',
    color: 'danger',
  },
  {
    step: '4',
    title: 'Consolation Prizes',
    body: `Losers get mostly worthless tokens. Play some games, pull yourself together.`,
    color: 'purple',
  },
  {
    step: '5',
    title: 'Loyalty Rewards',
    body: 'Top exit-liquidity providers win an all-expense-paid trip* to Cancun!',
    color: 'cyan',
    fine: '*Expenses do not include food, transportation, or lodging.',
  },
];

function useScrollAnimate(ref: React.RefObject<HTMLElement | null>, enabled: boolean = true) {
  useEffect(() => {
    if (!enabled) return;
    const el = ref.current;
    if (!el) return;

    // Small delay to let layout settle, then check if already visible
    const timer = setTimeout(() => {
      const rect = el.getBoundingClientRect();
      if (rect.top < window.innerHeight && rect.bottom > 0) {
        el.classList.add('mc-scroll-visible');
        return;
      }
      // Otherwise, observe for scroll
      const observer = new IntersectionObserver(
        ([entry]) => {
          if (entry.isIntersecting) {
            el.classList.add('mc-scroll-visible');
            observer.unobserve(el);
          }
        },
        { threshold: 0.1 }
      );
      observer.observe(el);
    }, 100);
    return () => clearTimeout(timer);
  }, [ref, enabled]);
}

/**
 * Letter-by-letter spring-drop animation with idle ripple.
 * Each letter drops from above with damped harmonic oscillator physics,
 * staggered sequentially. After settling, a gentle wave ripples through
 * the letters every few seconds.
 */
function LetterReveal({
  text,
  enabled,
  delay = 700,            // ms before first letter drops
  letterDelay = 80,       // ms stagger between each letter's drop
  startY = -200,          // px above resting position
  damping = 5.0,          // ζ — how fast bounces decay (higher = less bouncy)
  frequency = 9.0,        // ω — bounce frequency (rad/s)
  duration = 1.2,         // seconds each letter animates
  restRotation = -3,      // rotation of the whole text (applied immediately)
  rippleInterval = 3000,  // ms between ripple waves
  rippleAmplitude = 4,    // px max displacement per letter
  rippleStagger = 40,     // ms stagger between letters in ripple
}: {
  text: string;
  enabled: boolean;
  delay?: number;
  letterDelay?: number;
  startY?: number;
  damping?: number;
  frequency?: number;
  duration?: number;
  restRotation?: number;
  rippleInterval?: number;
  rippleAmplitude?: number;
  rippleStagger?: number;
}) {
  const lettersRef = useRef<(HTMLSpanElement | null)[]>([]);
  const dropDoneRef = useRef(false);

  // Phase 1: Drop-in animation
  useEffect(() => {
    if (!enabled) return;

    const els = lettersRef.current;
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    // Respect reduced-motion
    if (reducedMotion) {
      els.forEach(el => {
        if (!el) return;
        el.style.opacity = '1';
        el.style.transform = 'translateY(0px)';
      });
      dropDoneRef.current = true;
      return;
    }

    // Hide all letters initially
    els.forEach(el => {
      if (!el) return;
      el.style.opacity = '0';
      el.style.transform = `translateY(${startY}px)`;
    });

    const timers: number[] = [];
    const rafs: number[] = [];
    let settled = 0;

    // Stagger each letter's spring animation
    const nonSpaceCount = els.filter(Boolean).length;
    els.forEach((el, i) => {
      if (!el) return;
      const letterEl = el;

      const timer = window.setTimeout(() => {
        const startTime = performance.now();
        const durationMs = duration * 1000;

        function tick(now: number) {
          const elapsed = now - startTime;
          const t = Math.min(elapsed / durationMs, 1);
          const tSec = t * duration;

          // Damped spring: displacement from rest
          const springVal = Math.exp(-damping * tSec) * Math.cos(frequency * tSec);
          const y = startY * springVal;

          // Fade in quickly
          const opacity = Math.min(1, tSec * 8);

          letterEl.style.opacity = String(opacity);
          letterEl.style.transform = `translateY(${y}px)`;

          if (t < 1) {
            rafs.push(requestAnimationFrame(tick));
          } else {
            letterEl.style.opacity = '1';
            letterEl.style.transform = 'translateY(0px)';
            settled++;
            if (settled >= nonSpaceCount) dropDoneRef.current = true;
          }
        }

        rafs.push(requestAnimationFrame(tick));
      }, delay + i * letterDelay);

      timers.push(timer);
    });

    return () => {
      timers.forEach(t => clearTimeout(t));
      rafs.forEach(r => cancelAnimationFrame(r));
    };
  }, [enabled]);

  // Phase 2: Idle ripple — gentle wave every rippleInterval ms
  useEffect(() => {
    if (!enabled) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

    // Wait for drop to finish before starting ripple loop
    const totalDropTime = delay + (text.length - 1) * letterDelay + duration * 1000 + 500;
    const rafs: number[] = [];

    function runRipple() {
      const els = lettersRef.current;
      els.forEach((el, i) => {
        if (!el) return;
        const letterEl = el;
        // Each letter does a quick mini-bounce: up then back
        const letterStart = performance.now() + i * rippleStagger;
        const rippleDuration = 400; // ms for one letter's bounce

        function rippleTick(now: number) {
          const elapsed = now - letterStart;
          if (elapsed < 0) { rafs.push(requestAnimationFrame(rippleTick)); return; }
          const t = Math.min(elapsed / rippleDuration, 1);
          // Sine wave: up then back to 0
          const y = -rippleAmplitude * Math.sin(Math.PI * t);
          letterEl.style.transform = `translateY(${y}px)`;
          if (t < 1) {
            rafs.push(requestAnimationFrame(rippleTick));
          } else {
            letterEl.style.transform = 'translateY(0px)';
          }
        }
        rafs.push(requestAnimationFrame(rippleTick));
      });
    }

    let interval: number;
    const startTimer = window.setTimeout(() => {
      runRipple(); // first ripple right after drop settles
      interval = window.setInterval(runRipple, rippleInterval);
    }, totalDropTime);

    return () => {
      clearTimeout(startTimer);
      if (interval) clearInterval(interval);
      rafs.forEach(r => cancelAnimationFrame(r));
    };
  }, [enabled]);

  if (!enabled) return null;

  return (
    <span
      style={{
        display: 'inline-block',
        transform: `rotate(${restRotation}deg)`,
      }}
    >
      {text.split('').map((char, i) => (
        <span
          key={i}
          ref={el => { lettersRef.current[i] = el; }}
          style={{
            display: 'inline-block',
            opacity: 0,
            // preserve whitespace width even when hidden
            width: char === ' ' ? '0.3em' : undefined,
          }}
        >
          {char}
        </span>
      ))}
    </span>
  );
}

export default function App() {
  const { identity, principal, isInitializing } = useInternetIdentity();
  const { data: userProfile, isLoading: profileLoading, isFetched } = useGetCallerUserProfile();
  const { data: balanceData } = useGetInternalWalletBalance();
  const [isWalletDropdownOpen, setIsWalletDropdownOpen] = useState(false);
  const [showAdminPanel, setShowAdminPanel] = useState(false);
  const [activeTab, setActiveTab] = useState<TabType>('profitCenter');
  const walletButtonRef = useRef<HTMLButtonElement>(null);
  const [showDocsPage, setShowDocsPage] = useState(false);
  const { data: publicStats } = useGetPublicStats();

  // Scroll-triggered animation refs
  const cardsRef = useRef<HTMLDivElement>(null);
  const ribbonRef = useRef<HTMLDivElement>(null);
  const splashVisible = !isInitializing && !identity;
  useScrollAnimate(cardsRef, splashVisible);
  useScrollAnimate(ribbonRef, splashVisible);

  // Badge data — only fetched when authenticated (React Query caches these)
  const { data: games } = useGetUserGames();
  const { data: ponziData } = useGetPonziPoints();
  const { data: referralStats } = useGetReferralStats();
  const portfolio = useLivePortfolio(games);

  // Profit Center badge: any position has positive earnings (withdrawable)
  const hasWithdrawableEarnings = portfolio.games.some(g => g.earnings > 0);
  // Shenanigans badge: user has enough PP to cast the cheapest shenanigan (500 PP)
  const canCastShenanigan = (ponziData?.totalPoints || 0) >= 500;
  // MLM badge: referral earnings increased since last visit (gold dot)
  const currentReferralEarnings = referralStats?.totalEarnings || 0;
  const lastSeenReferralEarnings = parseInt(localStorage.getItem('mc_last_seen_referral_earnings') || '0');
  const hasNewReferralActivity = currentReferralEarnings > lastSeenReferralEarnings;

  // Clear MLM badge when user visits the tab
  useEffect(() => {
    if (activeTab === 'mlm' && referralStats) {
      localStorage.setItem('mc_last_seen_referral_earnings', String(referralStats.totalEarnings || 0));
    }
  }, [activeTab, referralStats]);

  // Footer "Docs" link — mirrors the header Docs button
  useEffect(() => {
    const handler = (_e: Event) => setShowDocsPage(true);
    window.addEventListener('mc:open-docs', handler);
    return () => window.removeEventListener('mc:open-docs', handler);
  }, []);

  const badges: Record<TabType, 'red' | 'purple' | 'gold' | null> = {
    profitCenter: hasWithdrawableEarnings ? 'red' : null,
    invest: null,
    seedRound: null,
    mlm: hasNewReferralActivity ? 'gold' : null,
    shenanigans: canCastShenanigan ? 'purple' : null,
  };

  const isAuthenticated = !!identity;
  const showProfileSetup = isAuthenticated && !profileLoading && isFetched && userProfile === null;
  const showDashboard = isAuthenticated && !showProfileSetup && !profileLoading;
  const isOnLandingHero = !identity && !showDocsPage && !showProfileSetup && !showAdminPanel && !profileLoading;

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
              <button onClick={() => { setShowAdminPanel(false); window.scrollTo(0, 0); }} className="flex flex-col text-left hover:opacity-80 transition-opacity shrink-0">
                <span className="mc-logo text-xl md:text-2xl leading-none whitespace-nowrap">
                  <span className="hidden md:inline">Musical Chairs</span>
                  <span className="md:hidden">MC</span>
                </span>
                <span className="mc-tagline text-sm md:text-base leading-none">
                  It's a Ponzi!
                </span>
              </button>

              {/* Desktop header tabs — only when on dashboard */}
              {showDashboard && !showAdminPanel && (
                <nav className="mc-header-tabs">
                  {headerNavItems.map(item => {
                    const isActive = activeTab === item.id;
                    const badge = badges[item.id];
                    return (
                      <button
                        key={item.id}
                        onClick={() => setActiveTab(item.id)}
                        className={`mc-header-tab ${isActive ? (item.id === 'shenanigans' ? 'active-green' : 'active') : ''}`}
                      >
                        <span className={`relative ${!isActive && item.glowClass ? item.glowClass : ''}`}>
                          {item.icon}
                          {badge && !isActive && (
                            <span className={`mc-badge-dot ${badge === 'red' ? 'mc-badge-red' : badge === 'gold' ? 'mc-badge-gold' : 'mc-badge-purple'}`} />
                          )}
                        </span>
                        <span>{item.label}</span>
                      </button>
                    );
                  })}
                </nav>
              )}

              <div className="flex-1" />

              {/* Right controls */}
              <div className="flex items-center gap-2 sm:gap-3">
                {/* Docs — always visible, visually distinct */}
                <button
                  onClick={() => setShowDocsPage(true)}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-display mc-text-dim hover:mc-text-primary hover:bg-white/5 transition-all border border-white/10 hover:border-white/20"
                >
                  <BookOpen className="h-4 w-4" />
                  <span>Docs</span>
                </button>

                {/* Separator dot */}
                <div className="w-px h-4 bg-white/10 hidden sm:block" />

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
                  !isOnLandingHero && <LoginButton compact />
                )}
              </div>
            </div>
          </div>
        </header>

        {/* Status Bar — persistent game stats below header */}
        {showDashboard && <GameStatusBar />}

        {/* Main Content */}
        <main className={`flex-1 ${showDashboard ? 'pt-[calc(4rem+44px)] md:pt-[calc(5rem+44px)]' : 'pt-16 md:pt-20'}`}>
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
            {showDocsPage ? (
              /* === DOCS PAGE === */
              <div className="max-w-3xl mx-auto px-4 py-8 md:py-12">
                <DocsPage onBack={() => setShowDocsPage(false)} />
              </div>
            ) : !isAuthenticated ? (
              /* === SPLASH / LOGIN PAGE === */
              <div className="mc-hero max-w-2xl mx-auto px-4 py-8 md:py-16">
                {/* Animated gradient background */}
                <div className="mc-splash-bg" />

                {/* Hero: logo → tagline → Charles hook */}
                <div className="mc-stagger mc-hero-entrance">
                  <div className="mc-hero-logo">
                    Musical Chairs
                  </div>
                  <div className="mc-tagline text-3xl md:text-4xl mb-4">
                    <LetterReveal text="It's a Ponzi!" enabled={splashVisible} />
                  </div>
                  <div className="mb-10" />
                </div>

                {/* Three info cards — icon discs float above the top rail */}
                <div ref={cardsRef} className="grid grid-cols-1 md:grid-cols-3 gap-8 text-center mc-splash-cards mc-scroll-animate">
                  {/* Card 1: The Hook */}
                  <div className="flex flex-col items-center">
                    <div className="mc-icon-disc mc-icon-disc-green mb-[-20px] z-10">
                      <Dices className="h-7 w-7 mc-text-green" />
                    </div>
                    <div className="mc-card mc-accent-green pt-8 pb-5 px-5 mc-card-hook w-full flex-1">
                      <p className="text-sm mc-text-dim leading-relaxed">
                        Up to 12% daily. Withdraw anytime, or lock &amp; compound for a face-melting ROI.
                      </p>
                    </div>
                  </div>

                  {/* Card 2: The Warning */}
                  <div className="flex flex-col items-center">
                    <div className="mc-icon-disc mc-icon-disc-danger mb-[-20px] z-10">
                      <AlertTriangle className="h-7 w-7 mc-text-danger" />
                    </div>
                    <div className="mc-card mc-accent-danger pt-8 pb-5 px-5 w-full flex-1">
                      <p className="text-sm mc-text-dim leading-relaxed">
                        This is literally a Ponzi scheme. The smart money gets in early and gets out earlier.
                      </p>
                    </div>
                  </div>

                  {/* Card 3: The Payoff */}
                  <div className="flex flex-col items-center">
                    <div className="mc-icon-disc mc-icon-disc-gold mb-[-20px] z-10">
                      <Dices className="h-7 w-7 mc-text-gold" />
                    </div>
                    <div className="mc-card-elevated mc-accent-gold pt-8 pb-5 px-5 mc-card-payoff w-full flex-1">
                      <p className="text-sm mc-text-dim leading-relaxed">
                        When the music stops, whoever's still standing loses. Then it starts all over again.
                      </p>
                    </div>
                  </div>
                </div>

                {/* Live stats ribbon */}
                <div ref={ribbonRef} className="mt-8 mc-card p-4 mc-scroll-animate">
                  <div className="grid grid-cols-3 md:grid-cols-6 gap-4 text-center text-xs">
                    <div>
                      <div className="mc-text-muted mb-1 uppercase tracking-wider" style={{ fontSize: '10px' }}>Pot Balance</div>
                      <div className="font-bold mc-text-gold text-sm">
                        {publicStats ? `${formatICP(publicStats.potBalance)} ICP` : '—'}
                      </div>
                    </div>
                    <div>
                      <div className="mc-text-muted mb-1 uppercase tracking-wider" style={{ fontSize: '10px' }}>Active Positions</div>
                      <div className="font-bold mc-text-green text-sm">
                        {publicStats ? Number(publicStats.activeGames) : '—'}
                      </div>
                    </div>
                    <div>
                      <div className="mc-text-muted mb-1 uppercase tracking-wider" style={{ fontSize: '10px' }}>Total Deposited</div>
                      <div className="font-bold mc-text-purple text-sm">
                        {publicStats ? `${formatICP(publicStats.totalDeposits)} ICP` : '—'}
                      </div>
                    </div>
                    <div>
                      <div className="mc-text-muted mb-1 uppercase tracking-wider" style={{ fontSize: '10px' }}>Total Withdrawn</div>
                      <div className="font-bold mc-text-cyan text-sm">
                        {publicStats ? `${formatICP(publicStats.totalWithdrawals)} ICP` : '—'}
                      </div>
                    </div>
                    <div>
                      <div className="mc-text-muted mb-1 uppercase tracking-wider" style={{ fontSize: '10px' }}>Days Active</div>
                      <div className="font-bold mc-text-primary text-sm">
                        {publicStats ? Number(publicStats.daysActive) : '—'}
                      </div>
                    </div>
                    <div>
                      <div className="mc-text-muted mb-1 uppercase tracking-wider" style={{ fontSize: '10px' }}>Status</div>
                      <div className="font-bold mc-text-cyan text-sm">Live on ICP</div>
                    </div>
                  </div>
                </div>

                {/* How It Works — 3×2 grid */}
                <div className="mt-8">
                  <div className="text-center mb-4">
                    <span className="font-display text-xs mc-text-muted uppercase tracking-widest">How it works</span>
                  </div>
                  <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                    {howItWorks.map((step, i) => (
                      <div key={i} className={`mc-step-card mc-accent-${step.color}`}>
                        <span className={`mc-step-number mc-text-${step.color}`}>{step.step}</span>
                        <span className={`font-display text-xs mc-text-${step.color} uppercase tracking-wider`}>{step.title}</span>
                        <p className="text-xs mc-text-dim leading-snug mt-1">{step.body}</p>
                        {step.fine && (
                          <p className="text-[9px] mc-text-muted mt-1 opacity-50">{step.fine}</p>
                        )}
                      </div>
                    ))}
                    {/* 6th slot — the punchline */}
                    <div className="mc-step-card mc-accent-pink flex flex-col items-center justify-center">
                      <CircleDollarSign className="h-8 w-8 mc-text-pink mb-2 opacity-70" />
                      <span className="font-display text-xs mc-text-pink uppercase tracking-wider">Profit</span>
                      <p className="text-[10px] mc-text-muted mt-1 opacity-60">Repeat until music stops.</p>
                    </div>
                  </div>
                </div>

                {/* CTA */}
                <div className="mt-8 flex justify-center">
                  <LoginButton />
                </div>

                {/* Responsible gambling warning — always prominent, no jokes */}
                <div className="mt-10 mx-auto max-w-lg mc-warning-box text-center">
                  <div className="flex items-center justify-center gap-2 mb-2">
                    <AlertTriangle className="h-5 w-5 text-red-400 shrink-0" />
                    <span className="text-base font-bold text-red-300 uppercase tracking-wide">This is a gambling game</span>
                    <AlertTriangle className="h-5 w-5 text-red-400 shrink-0" />
                  </div>
                  <p className="text-sm text-red-300/80">All positions carry risk of total loss.</p>
                  <p className="text-sm text-red-300/80">Please play responsibly.</p>
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
                <Dashboard activeTab={activeTab} onTabChange={setActiveTab} badges={badges} />
              </ErrorBoundary>
            )}
          </ErrorBoundary>
          <Footer />
        </main>

        {/* Wallet Dropdown */}
        <ErrorBoundary fallback={null}>
          <WalletDropdown
            isOpen={isWalletDropdownOpen}
            onClose={() => setIsWalletDropdownOpen(false)}
            buttonRef={walletButtonRef}
          />
        </ErrorBoundary>

        {/* DocsPage is rendered inline in main content — see showDocsPage conditional above */}

        {/* Toast */}
        <Toaster
          position="top-center"
          toastOptions={{
            style: {
              background: 'var(--mc-felt-raised)',
              color: 'var(--mc-white)',
              border: '1px solid var(--mc-border)',
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
