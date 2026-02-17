import React, { useState, useEffect, useCallback, useRef } from 'react';
import { DollarSign, Rocket, Landmark, Users, Dice5, ChevronRight, X } from 'lucide-react';
import type { TabType } from '../App';

interface TourStep {
  tab: TabType;
  title: string;
  desc: string;
  icon: React.ReactNode;
}

const TOUR_STEPS: TourStep[] = [
  { tab: 'profitCenter', title: 'Profit Center', desc: 'Track your positions and see your P/L.', icon: <DollarSign className="h-5 w-5 mc-text-green" /> },
  { tab: 'invest', title: '\u201CInvest\u201D', desc: 'Choose a plan and deposit ICP to start earning.', icon: <Rocket className="h-5 w-5 mc-text-purple" /> },
  { tab: 'seedRound', title: 'Seed Round', desc: 'Back the house as a dealer. Earn 12%*.', icon: <Landmark className="h-5 w-5 mc-text-gold" /> },
  { tab: 'mlm', title: 'MLM', desc: 'Recruit friends. Three-level pyramid. Charles approves.', icon: <Users className="h-5 w-5 mc-text-cyan" /> },
  { tab: 'shenanigans', title: 'Shenanigans', desc: 'Spend Ponzi Points on cosmetic chaos.', icon: <Dice5 className="h-5 w-5 mc-text-green" /> },
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

  if (!visible) return null;

  const current = TOUR_STEPS[step];
  const isLast = step === TOUR_STEPS.length - 1;
  const progress = ((step + 1) / TOUR_STEPS.length) * 100;

  return (
    <>
      {/* Backdrop overlay */}
      <div className="fixed inset-0 bg-black/40 z-[60]" onClick={complete} />

      {/* Tooltip */}
      <div
        ref={tooltipRef}
        className={`fixed z-[61] mc-card-elevated p-5 rounded-xl shadow-2xl max-w-xs w-[calc(100vw-2rem)] mc-enter ${
          isMobile ? 'bottom-24 left-1/2 -translate-x-1/2' : 'top-24 left-1/2 -translate-x-1/2'
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
