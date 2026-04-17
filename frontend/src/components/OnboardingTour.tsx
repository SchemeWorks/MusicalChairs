import React, { useState, useEffect, useCallback, useRef } from 'react';
import { DollarSign, Rocket, Landmark, Users, Dice5, ChevronRight, X } from 'lucide-react';
import type { TabType } from '../App';

interface TourStep {
  tab: TabType;
  title: string;
  desc: string;
  icon: React.ReactNode;
  targetSelector?: string;
}

const TOUR_STEPS: TourStep[] = [
  { tab: 'profitCenter', title: 'Profit Center', desc: 'Track your positions and see your P/L.', icon: <DollarSign className="h-5 w-5 mc-text-green" />, targetSelector: '[data-tour-id="tab-profitCenter"]' },
  { tab: 'invest', title: '\u201CInvest\u201D', desc: 'Choose a plan and deposit ICP to start earning.', icon: <Rocket className="h-5 w-5 mc-text-purple" />, targetSelector: '[data-tour-id="tab-invest"]' },
  { tab: 'seedRound', title: 'Seed Round', desc: 'Back the house as a dealer. Earn 12%.', icon: <Landmark className="h-5 w-5 mc-text-gold" />, targetSelector: '[data-tour-id="tab-seedRound"]' },
  { tab: 'mlm', title: 'MLM', desc: 'Recruit friends. Three-level pyramid. Charles approves.', icon: <Users className="h-5 w-5 mc-text-cyan" />, targetSelector: '[data-tour-id="tab-mlm"]' },
  { tab: 'shenanigans', title: 'Shenanigans', desc: 'Spend Ponzi Points on cosmetic chaos.', icon: <Dice5 className="h-5 w-5 mc-text-green" />, targetSelector: '[data-tour-id="tab-shenanigans"]' },
];

const STORAGE_KEY = 'mc_tour_completed';

interface OnboardingTourProps {
  onTabChange: (tab: TabType) => void;
  isMobile: boolean;
}

export default function OnboardingTour({ onTabChange, isMobile }: OnboardingTourProps) {
  const [step, setStep] = useState(0);
  const [visible, setVisible] = useState(false);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const [spotlight, setSpotlight] = useState<{ top: number; left: number; width: number; height: number } | null>(null);

  useEffect(() => {
    if (!localStorage.getItem(STORAGE_KEY)) {
      // Small delay so the dashboard renders first
      const t = setTimeout(() => setVisible(true), 600);
      return () => clearTimeout(t);
    }
  }, []);

  const complete = useCallback(() => {
    localStorage.setItem(STORAGE_KEY, 'true');
    setVisible(false);
  }, []);

  const next = useCallback(() => {
    if (step < TOUR_STEPS.length - 1) {
      const nextStep = step + 1;
      setStep(nextStep);
      onTabChange(TOUR_STEPS[nextStep].tab);
    } else {
      complete();
      // Return to first tab after tour
      onTabChange('profitCenter');
    }
  }, [step, onTabChange, complete]);

  // Navigate to the first step's tab when tour starts
  useEffect(() => {
    if (visible && step === 0) {
      onTabChange(TOUR_STEPS[0].tab);
    }
  }, [visible]); // eslint-disable-line react-hooks/exhaustive-deps

  // Spotlight: track the target tab element position
  useEffect(() => {
    if (!visible) {
      setSpotlight(null);
      return;
    }
    const currentStep = TOUR_STEPS[step];
    if (!currentStep?.targetSelector) {
      setSpotlight(null);
      return;
    }
    const update = () => {
      const els = document.querySelectorAll<HTMLElement>(currentStep.targetSelector!);
      // Use the first element that has a non-zero bounding rect (i.e. is visible)
      let el: HTMLElement | null = null;
      els.forEach(candidate => {
        if (!el) {
          const r = candidate.getBoundingClientRect();
          if (r.width > 0 && r.height > 0) el = candidate;
        }
      });
      if (!el) return;
      const rect = (el as HTMLElement).getBoundingClientRect();
      setSpotlight({ top: rect.top - 6, left: rect.left - 6, width: rect.width + 12, height: rect.height + 12 });
    };
    update();
    window.addEventListener('resize', update);
    const t = setTimeout(update, 200);
    return () => {
      window.removeEventListener('resize', update);
      clearTimeout(t);
    };
  }, [step, visible]);

  if (!visible) return null;

  const current = TOUR_STEPS[step];
  const isLast = step === TOUR_STEPS.length - 1;
  const progress = ((step + 1) / TOUR_STEPS.length) * 100;

  return (
    <>
      {/* Backdrop overlay */}
      <div className="fixed inset-0 bg-black/40 z-[60]" onClick={complete} />

      {/* Spotlight ring around the active tab */}
      {spotlight && (
        <div
          className="fixed pointer-events-none rounded-lg ring-2 ring-yellow-400 ring-offset-2 ring-offset-black transition-all duration-300"
          style={{
            top: spotlight.top,
            left: spotlight.left,
            width: spotlight.width,
            height: spotlight.height,
            zIndex: 62,
          }}
        />
      )}

      {/* Tooltip */}
      <div
        ref={tooltipRef}
        className={`fixed z-[61] mc-card-elevated p-5 rounded-xl shadow-2xl max-w-xs w-[calc(100vw-2rem)] mc-enter ${
          isMobile ? 'bottom-24 left-1/2 -translate-x-1/2' : 'top-[140px] left-1/2 -translate-x-1/2'
        }`}
      >
        {/* Progress bar */}
        <div className="absolute top-0 left-0 right-0 h-0.5 rounded-t-xl overflow-hidden bg-white/5">
          <div
            className="h-full bg-gradient-to-r from-[var(--mc-neon-green)] to-[var(--mc-purple)] transition-all duration-300"
            style={{ width: `${progress}%` }}
          />
        </div>

        {/* Close button */}
        <button
          onClick={complete}
          className="absolute top-2 right-2 p-1 mc-text-muted hover:mc-text-primary transition-colors"
        >
          <X className="h-4 w-4" />
        </button>

        {/* Step content */}
        <div className="flex items-start gap-3 mb-4">
          <div className="mt-0.5">{current.icon}</div>
          <div>
            <h3 className="font-display text-sm text-white mb-1">{current.title}</h3>
            <p className="text-xs mc-text-dim leading-relaxed">{current.desc}</p>
          </div>
        </div>

        {/* Navigation */}
        <div className="flex items-center justify-between">
          <button
            onClick={complete}
            className="text-xs mc-text-muted hover:mc-text-primary transition-colors"
          >
            Skip Tour
          </button>

          <div className="flex items-center gap-3">
            <span className="text-xs mc-text-muted">{step + 1}/{TOUR_STEPS.length}</span>
            <button
              onClick={next}
              className="mc-btn-primary text-xs px-4 py-1.5 rounded-lg flex items-center gap-1"
            >
              {isLast ? 'Done' : 'Next'}
              {!isLast && <ChevronRight className="h-3 w-3" />}
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
