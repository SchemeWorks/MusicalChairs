import React, { useState, useEffect, useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import GamePlans from './GamePlans';
import GameTracking from './GameTracking';
import ReferralSection from './ReferralSection';
import HouseDashboard from './HouseDashboard';
import Shenanigans from './Shenanigans';
import { DollarSign, Rocket, Landmark, Users, Dice5, RefreshCw } from 'lucide-react';
import type { TabType } from '../App';
import { usePullToRefresh } from '../hooks/usePullToRefresh';
import OnboardingTour from './OnboardingTour';

interface NavItem {
  id: TabType;
  mobileLabel: string;
  icon: React.ReactNode;
  activeClass?: string;
  glowClass?: string;
}

const navItems: NavItem[] = [
  { id: 'profitCenter', mobileLabel: 'Profit', icon: <DollarSign className="h-5 w-5" /> },
  { id: 'invest', mobileLabel: '\u201CInvest\u201D', icon: <Rocket className="h-5 w-5" /> },
  { id: 'seedRound', mobileLabel: 'Seed', icon: <Landmark className="h-5 w-5" /> },
  { id: 'mlm', mobileLabel: 'MLM', icon: <Users className="h-5 w-5" /> },
  { id: 'shenanigans', mobileLabel: 'Tricks', icon: <Dice5 className="h-5 w-5" />, activeClass: 'active-green', glowClass: 'mc-icon-glow-green' },
];

const sectionSubtitles: Record<TabType, string> = {
  profitCenter: "Don't get too attached",
  invest: "Choose your own adventure (all roads lead to losses)",
  seedRound: "Become a VC — put your money in someone else's scheme and call it strategy",
  mlm: "More than just a Ponzi — it's also a Pyramid Scheme",
  shenanigans: "Pure chaos, zero value",
};

const sectionLabels: Record<TabType, string> = {
  profitCenter: 'Profit Center',
  invest: '\u201CInvest\u201D',
  seedRound: 'Seed Round',
  mlm: 'MLM',
  shenanigans: 'Shenanigans',
};

interface DashboardProps {
  activeTab: TabType;
  onTabChange: (tab: TabType) => void;
  badges?: Record<TabType, 'red' | 'purple' | 'gold' | null>;
}

export default function Dashboard({ activeTab, onTabChange, badges }: DashboardProps) {
  const [isAnimating, setIsAnimating] = useState(false);
  const [isMobile, setIsMobile] = useState(false);
  const queryClient = useQueryClient();

  useEffect(() => {
    const check = () => setIsMobile(window.innerWidth < 769);
    check();
    window.addEventListener('resize', check);
    return () => window.removeEventListener('resize', check);
  }, []);

  const handleRefresh = useCallback(async () => {
    await queryClient.invalidateQueries();
  }, [queryClient]);

  const { containerRef, pulling, pullDistance, refreshing, isTriggered } = usePullToRefresh({
    onRefresh: handleRefresh,
    disabled: !isMobile,
  });

  const handleTabChange = (newTab: TabType) => {
    if (newTab === activeTab) return;
    setIsAnimating(true);
    onTabChange(newTab);
    setTimeout(() => setIsAnimating(false), 250);
  };

  const handleNavigateToGameSetup = () => handleTabChange('invest');
  const handleNavigateToProfitCenter = () => handleTabChange('profitCenter');

  const renderContent = () => {
    const cls = isAnimating ? 'mc-enter' : '';
    switch (activeTab) {
      case 'profitCenter': return <div className={cls}><GameTracking onNavigateToGameSetup={handleNavigateToGameSetup} onTabChange={handleTabChange} visible={activeTab === 'profitCenter'} /></div>;
      case 'invest': return <div className={cls}><GamePlans onNavigateToProfitCenter={handleNavigateToProfitCenter} /></div>;
      case 'seedRound': return <div className={cls}><HouseDashboard /></div>;
      case 'mlm': return <div className={cls}><ReferralSection onTabChange={handleTabChange} /></div>;
      case 'shenanigans': return <div className={cls}><Shenanigans /></div>;
      default: return <div className={cls}><GameTracking onNavigateToGameSetup={handleNavigateToGameSetup} onTabChange={handleTabChange} visible={activeTab === 'profitCenter'} /></div>;
    }
  };

  return (
    <div ref={containerRef} className={`min-h-[calc(100vh-80px)] ${isMobile ? 'pb-20' : ''}`}>
      {/* Pull-to-refresh indicator (mobile only) */}
      {isMobile && pulling && (
        <div
          className="flex items-center justify-center overflow-hidden transition-all"
          style={{ height: `${pullDistance}px` }}
        >
          <RefreshCw className={`h-5 w-5 mc-text-muted transition-transform ${refreshing ? 'animate-spin' : ''} ${isTriggered ? 'mc-text-green scale-110' : ''}`} />
        </div>
      )}

      {/* Section header */}
      <div className="max-w-5xl mx-auto px-4 pt-6 md:pt-8">
        <div className="mc-section-header">
          <h1 className="mc-section-title">{sectionLabels[activeTab]}</h1>
          <span className="mc-section-subtitle">{sectionSubtitles[activeTab]}</span>
        </div>
      </div>

      {/* Content */}
      <div className="max-w-5xl mx-auto px-4 pb-8">
        {renderContent()}
      </div>

      {/* Onboarding tour — first visit only */}
      <OnboardingTour onTabChange={handleTabChange} isMobile={isMobile} />

      {/* === Mobile Bottom Tabs — all 5 tabs === */}
      {isMobile && (
        <nav className="mc-bottom-tabs">
          {navItems.map((item) => {
            const isActive = activeTab === item.id;
            const badge = badges?.[item.id];
            return (
              <button
                key={item.id}
                onClick={() => handleTabChange(item.id)}
                className={`mc-bottom-tab ${isActive ? (item.activeClass || 'active') : ''}`}
              >
                <span className={`tab-icon relative ${!isActive && item.glowClass ? item.glowClass : ''}`}>
                  {item.icon}
                  {badge && !isActive && (
                    <span className={`mc-badge-dot ${badge === 'red' ? 'mc-badge-red' : badge === 'gold' ? 'mc-badge-gold' : 'mc-badge-purple'}`} />
                  )}
                </span>
                <span>{item.mobileLabel}</span>
              </button>
            );
          })}
        </nav>
      )}
    </div>
  );
}
