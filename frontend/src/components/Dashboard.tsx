import React, { useState, useEffect } from 'react';
import GamePlans from './GamePlans';
import GameTracking from './GameTracking';
import ReferralSection from './ReferralSection';
import HouseDashboard from './HouseDashboard';
import Shenanigans from './Shenanigans';
import { DollarSign, Rocket, Landmark, Users, Dice5 } from 'lucide-react';

type TabType = 'profitCenter' | 'invest' | 'seedRound' | 'mlm' | 'shenanigans';

interface NavItem {
  id: TabType;
  label: string;
  mobileLabel: string;
  icon: React.ReactNode;
  activeClass?: string;
  glowClass?: string;
}

const navItems: NavItem[] = [
  { id: 'profitCenter', label: 'Profit Center', mobileLabel: 'Profit', icon: <DollarSign className="h-5 w-5" /> },
  { id: 'invest', label: '\u201CInvest\u201D', mobileLabel: '\u201CInvest\u201D', icon: <Rocket className="h-5 w-5" /> },
  { id: 'seedRound', label: 'Seed Round', mobileLabel: 'Seed', icon: <Landmark className="h-5 w-5" /> },
  { id: 'mlm', label: 'MLM', mobileLabel: 'MLM', icon: <Users className="h-5 w-5" /> },
  { id: 'shenanigans', label: 'Shenanigans', mobileLabel: 'Tricks', icon: <Dice5 className="h-5 w-5" />, activeClass: 'active-green', glowClass: 'mc-icon-glow-green' },
];

const sectionSubtitles: Record<TabType, string> = {
  profitCenter: "Don't get too attached",
  invest: "Choose your own adventure (all roads lead to losses)",
  seedRound: "Become a VC — put your money in someone else's scheme and call it strategy",
  mlm: "More than just a Ponzi — it's also a Pyramid Scheme",
  shenanigans: "Pure chaos, zero value",
};

export default function Dashboard() {
  const [activeTab, setActiveTab] = useState<TabType>('profitCenter');
  const [isAnimating, setIsAnimating] = useState(false);
  const [isMobile, setIsMobile] = useState(false);

  useEffect(() => {
    const check = () => setIsMobile(window.innerWidth < 769);
    check();
    window.addEventListener('resize', check);
    return () => window.removeEventListener('resize', check);
  }, []);

  const handleTabChange = (newTab: TabType) => {
    if (newTab === activeTab) return;
    setIsAnimating(true);
    setActiveTab(newTab);
    setTimeout(() => setIsAnimating(false), 250);
  };

  const handleNavigateToGameSetup = () => handleTabChange('invest');
  const handleNavigateToProfitCenter = () => handleTabChange('profitCenter');

  const activeItem = navItems.find(n => n.id === activeTab)!;

  const renderContent = () => {
    const cls = isAnimating ? 'mc-enter' : '';
    switch (activeTab) {
      case 'profitCenter': return <div className={cls}><GameTracking onNavigateToGameSetup={handleNavigateToGameSetup} /></div>;
      case 'invest': return <div className={cls}><GamePlans onNavigateToProfitCenter={handleNavigateToProfitCenter} /></div>;
      case 'seedRound': return <div className={cls}><HouseDashboard /></div>;
      case 'mlm': return <div className={cls}><ReferralSection /></div>;
      case 'shenanigans': return <div className={cls}><Shenanigans /></div>;
      default: return <div className={cls}><GameTracking onNavigateToGameSetup={handleNavigateToGameSetup} /></div>;
    }
  };

  return (
    <div className="flex min-h-[calc(100vh-80px)]">
      {/* === Desktop Left Rail — fixed 200px, always labeled === */}
      {!isMobile && (
        <nav className="mc-rail">
          {navItems.map((item) => {
            const isActive = activeTab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => handleTabChange(item.id)}
                className={`mc-rail-item ${isActive ? (item.activeClass || 'active') : ''}`}
              >
                <span className={`mc-rail-icon ${!isActive && item.glowClass ? item.glowClass : ''}`}>
                  {item.icon}
                </span>
                <span className="mc-rail-label">{item.label}</span>
              </button>
            );
          })}
        </nav>
      )}

      {/* === Main Content === */}
      <div className={`flex-1 ${!isMobile ? 'mc-content-offset' : ''} ${isMobile ? 'pb-20' : ''}`}>
        {/* Section header */}
        <div className="max-w-5xl mx-auto px-4 pt-6 md:pt-8">
          <div className="mc-section-header">
            <h1 className="mc-section-title">{activeItem.label}</h1>
            <span className="mc-section-subtitle">{sectionSubtitles[activeTab]}</span>
          </div>
        </div>

        {/* Content */}
        <div className="max-w-5xl mx-auto px-4 pb-8">
          {renderContent()}
        </div>
      </div>

      {/* === Mobile Bottom Tabs — all 5 tabs, no More sheet === */}
      {isMobile && (
        <nav className="mc-bottom-tabs">
          {navItems.map((item) => {
            const isActive = activeTab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => handleTabChange(item.id)}
                className={`mc-bottom-tab ${isActive ? (item.activeClass || 'active') : ''}`}
              >
                <span className={`tab-icon ${!isActive && item.glowClass ? item.glowClass : ''}`}>
                  {item.icon}
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
