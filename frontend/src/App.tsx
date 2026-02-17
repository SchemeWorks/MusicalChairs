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
import { Wallet, Dices, AlertTriangle, Users, Wrench, Tent, DollarSign, Rocket, Landmark, Dice5, ChevronDown, HelpCircle, BookOpen } from 'lucide-react';
import DocsPage from './components/DocsPage';
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

const splashQuotes = [
  "You look like someone who understands opportunity.",
  "The best time to get in was yesterday. The second best time is right now.",
  "I've never lied to you. That's more than most can say.",
  "Smart money moves fast. Scared money doesn't move at all.",
  "The only guarantee is that there are no guarantees. But the odds are... interesting.",
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
 * Spring-physics drop animation. Uses damped harmonic oscillator:
 *   y(t) = A · e^(-ζt) · cos(ωt)
 * Produces perfectly smooth motion — no keyframe seams.
 */
function useSpringDrop(
  ref: React.RefObject<HTMLElement | null>,
  enabled: boolean = true,
  {
    startY = -350,       // px above resting position
    damping = 4.2,       // ζ — how fast bounces decay
    frequency = 8.5,     // ω — bounce frequency (rad/s)
    duration = 2.0,      // total animation seconds
    delay = 700,         // ms before animation starts
    restRotation = -3,   // final rotation in degrees
  } = {}
) {
  useEffect(() => {
    if (!enabled) return;
    const el = ref.current;
    if (!el) return;

    // Respect reduced-motion preference
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      el.style.opacity = '1';
      el.style.transform = `rotate(${restRotation}deg)`;
      return;
    }

    // Hide initially
    el.style.opacity = '0';
    el.style.transform = `translateY(${startY}px) rotate(0deg)`;

    const delayTimer = setTimeout(() => {
      const startTime = performance.now();
      const durationMs = duration * 1000;

      function tick(now: number) {
        const elapsed = now - startTime;
        const t = Math.min(elapsed / durationMs, 1);
        const tSec = t * duration;

        // Damped spring: displacement from rest
        const springVal = Math.exp(-damping * tSec) * Math.cos(frequency * tSec);
        // Y position: startY * springVal (at t=0, springVal≈1 → full offset; at t=∞ → 0)
        const y = startY * springVal;

        // Rotation eases in as it settles
        const rotationProgress = 1 - Math.exp(-damping * tSec * 0.6);
        const rotation = restRotation * rotationProgress;

        // Subtle squash/stretch on vertical velocity
        const velocity = -damping * springVal + Math.exp(-damping * tSec) * (-frequency * Math.sin(frequency * tSec));
        const squash = 1 + Math.abs(velocity) * 0.012;
        const scaleX = 1 + (squash - 1) * 0.4;
        const scaleY = 1 / scaleX; // preserve area

        // Fade in quickly
        const opacity = Math.min(1, tSec * 6);

        el.style.opacity = String(opacity);
        el.style.transform = `translateY(${y}px) rotate(${rotation}deg) scaleX(${scaleX.toFixed(4)}) scaleY(${scaleY.toFixed(4)})`;

        if (t < 1) {
          requestAnimationFrame(tick);
        } else {
          // Snap to exact final values
          el.style.opacity = '1';
          el.style.transform = `translateY(0px) rotate(${restRotation}deg) scale(1)`;
        }
      }

      requestAnimationFrame(tick);
    }, delay);

    return () => clearTimeout(delayTimer);
  }, [ref, enabled]);
}

export default function App() {
  const { identity, principal, isInitializing } = useInternetIdentity();
  const { data: userProfile, isLoading: profileLoading, isFetched } = useGetCallerUserProfile();
  const { data: balanceData } = useGetInternalWalletBalance();
  const [isWalletDropdownOpen, setIsWalletDropdownOpen] = useState(false);
  const [showAdminPanel, setShowAdminPanel] = useState(false);
  const [activeTab, setActiveTab] = useState<TabType>('profitCenter');
  const walletButtonRef = useRef<HTMLButtonElement>(null);
  const [splashQuote] = useState(() => splashQuotes[Math.floor(Math.random() * splashQuotes.length)]);
  const [showHowItWorks, setShowHowItWorks] = useState(false);
  const [showDocsPage, setShowDocsPage] = useState(false);
  const { data: publicStats } = useGetPublicStats();

  // Scroll-triggered animation refs
  const cardsRef = useRef<HTMLDivElement>(null);
  const ribbonRef = useRef<HTMLDivElement>(null);
  const howItWorksRef = useRef<HTMLDivElement>(null);
  const taglineRef = useRef<HTMLSpanElement>(null);
  const splashVisible = !isInitializing && !identity;
  useScrollAnimate(cardsRef, splashVisible);
  useScrollAnimate(ribbonRef, splashVisible);
  useScrollAnimate(howItWorksRef, splashVisible);
  useSpringDrop(taglineRef, splashVisible);

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
                {!showDashboard && (
                  <span className="mc-tagline text-sm md:text-base leading-none">
                    It's a Ponzi!
                  </span>
                )}
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
              <div className="flex items-center gap-3">
                {/* Docs — always visible */}
                <button
                  onClick={() => setShowDocsPage(true)}
                  className="text-xs mc-text-muted hover:mc-text-primary transition-colors hidden sm:flex items-center gap-1.5"
                  title="Documentation"
                >
                  <BookOpen className="h-3.5 w-3.5" />
                  Docs
                </button>

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
                  <LoginButton compact />
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
            {!isAuthenticated ? (
              /* === SPLASH / LOGIN PAGE === */
              <div className="mc-hero max-w-2xl mx-auto px-4 py-8 md:py-16">
                {/* Animated gradient background */}
                <div className="mc-splash-bg" />

                {/* Hero: logo → tagline → Charles hook */}
                <div className="mc-stagger mc-hero-entrance">
                  <div className="mc-hero-logo">
                    Musical Chairs
                  </div>
                  <div className="mc-tagline text-2xl md:text-3xl mb-4">
                    <span ref={taglineRef} style={{ display: 'inline-block', opacity: 0 }}>It's a Ponzi!</span>
                  </div>
                  <div className="mb-10" />
                </div>

                {/* Three info cards */}
                <div ref={cardsRef} className="grid grid-cols-1 md:grid-cols-3 gap-4 text-center mc-splash-cards mc-scroll-animate">
                  {/* Card 1: The Hook */}
                  <div className="mc-card mc-accent-green p-6 mc-card-hook">
                    <div className="mc-icon-disc mc-icon-disc-green mx-auto mb-4">
                      <Dices className="h-6 w-6" />
                    </div>
                    <p className="text-sm mc-text-dim leading-relaxed">
                      Up to 12% daily. Withdraw anytime, or lock it in and let compound interest do its thing.
                    </p>
                  </div>

                  {/* Card 2: The Warning */}
                  <div className="mc-card mc-accent-danger p-5">
                    <div className="mc-icon-disc mc-icon-disc-danger mx-auto mb-4">
                      <AlertTriangle className="h-6 w-6" />
                    </div>
                    <p className="text-sm mc-text-dim leading-relaxed">
                      This is literally a Ponzi scheme. Only put in what you'd comfortably light on fire.
                    </p>
                  </div>

                  {/* Card 3: The Payoff */}
                  <div className="mc-card-elevated mc-accent-gold p-5 mc-card-payoff">
                    <div className="mc-icon-disc mc-icon-disc-gold mx-auto mb-4">
                      <Dices className="h-6 w-6" />
                    </div>
                    <p className="text-sm mc-text-dim leading-relaxed">
                      When the pot empties, the whole thing starts over. If you're still in when that happens — Loss.
                    </p>
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

                {/* How It Works — left-aligned accordion */}
                <div ref={howItWorksRef} className="mt-6 mc-scroll-animate">
                  <div className={`mc-card overflow-hidden ${showHowItWorks ? 'border-white/15' : ''}`}>
                    <button
                      onClick={() => setShowHowItWorks(!showHowItWorks)}
                      className="w-full p-4 flex items-center justify-between hover:bg-white/[0.02] transition-colors"
                    >
                      <div className="flex items-center gap-3">
                        <HelpCircle className="h-5 w-5 mc-text-cyan" />
                        <span className="text-sm font-bold mc-text-primary">How does it work?</span>
                      </div>
                      <ChevronDown className={`h-4 w-4 mc-text-muted transition-transform ${showHowItWorks ? 'rotate-180' : ''}`} />
                    </button>
                    {showHowItWorks && (
                      <div className="px-4 pb-4 grid grid-cols-1 md:grid-cols-2 gap-3 text-left mc-stagger">
                        <div className="mc-card p-4">
                          <h4 className="font-display text-sm mc-text-green mb-2">Deposit ICP</h4>
                          <p className="text-xs mc-text-dim">Choose a plan. Simple earns 11%/day for 21 days. Compounding earns more but locks your money.</p>
                        </div>
                        <div className="mc-card p-4">
                          <h4 className="font-display text-sm mc-text-gold mb-2">Earn Daily</h4>
                          <p className="text-xs mc-text-dim">Your position earns interest from the pot. Withdraw anytime — earlier exits pay a higher toll.</p>
                        </div>
                        <div className="mc-card p-4">
                          <h4 className="font-display text-sm mc-text-purple mb-2">Cast Shenanigans</h4>
                          <p className="text-xs mc-text-dim">Earn Ponzi Points. Spend them on cosmetic chaos — rename other players, skim their earnings, boost your referrals.</p>
                        </div>
                        <div className="mc-card p-4">
                          <h4 className="font-display text-sm mc-text-danger mb-2">The Catch</h4>
                          <p className="text-xs mc-text-dim">When the pot empties, the game resets. If you're still in — total loss. That's the Ponzi part.</p>
                        </div>
                      </div>
                    )}
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

                {/* Charles quote — below the warning */}
                <div className="mt-8 text-center">
                  <p className="font-accent text-sm mc-text-dim italic">
                    &ldquo;{splashQuote}&rdquo;
                  </p>
                  <span className="text-xs mc-text-muted font-bold">&mdash; Charles</span>
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
        </main>

        {/* Wallet Dropdown */}
        <ErrorBoundary fallback={null}>
          <WalletDropdown
            isOpen={isWalletDropdownOpen}
            onClose={() => setIsWalletDropdownOpen(false)}
            buttonRef={walletButtonRef}
          />
        </ErrorBoundary>

        {/* Full docs page */}
        {showDocsPage && (
          <div className="fixed inset-0 z-50 mc-bg overflow-y-auto">
            <DocsPage onBack={() => setShowDocsPage(false)} />
          </div>
        )}

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
