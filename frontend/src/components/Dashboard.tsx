import React, { useState, useEffect } from 'react';
import GamePlans from './GamePlans';
import GameTracking from './GameTracking';
import ReferralSection from './ReferralSection';
import HouseDashboard from './HouseDashboard';
import PonziPointsDashboard from './PonziPointsDashboard';
import HallOfFame from './HallOfFame';
import Shenanigans from './Shenanigans';
import { DollarSign, Target, BarChart3, Gift, Users, Dice5, Trophy, MoreHorizontal, X } from 'lucide-react';

type TabType = 'positions' | 'setup' | 'casino' | 'rewards' | 'shenanigans' | 'marketing' | 'halloffame';

interface NavItem {
  id: TabType;
  label: string;
  icon: React.ReactNode;
  emoji?: string;
  group: 'core' | 'extras' | 'fun';
  activeClass?: string;
  glowClass?: string;
}

const navItems: NavItem[] = [
  { id: 'positions', label: 'Profit Center', icon: <DollarSign className="h-5 w-5" />, group: 'core' },
  { id: 'setup', label: 'Pick Your Plan', icon: <Target className="h-5 w-5" />, group: 'core' },
  { id: 'casino', label: 'House Ledger', icon: <BarChart3 className="h-5 w-5" />, group: 'core' },
  { id: 'rewards', label: 'Rewards', icon: <Gift className="h-5 w-5" />, group: 'extras' },
  { id: 'marketing', label: 'MLM', icon: <Users className="h-5 w-5" />, group: 'extras' },
  { id: 'shenanigans', label: 'Shenanigans', emoji: '🎲', icon: <Dice5 className="h-5 w-5" />, group: 'fun', activeClass: 'active-green', glowClass: 'mc-icon-glow-green' },
  { id: 'halloffame', label: 'Hall of Fame', emoji: '🏆', icon: <Trophy className="h-5 w-5" />, group: 'fun', activeClass: 'active-gold', glowClass: 'mc-icon-glow-gold' },
];

// Mobile bottom bar shows these 5; the rest go in "More"
const mobileMainTabs: TabType[] = ['positions', 'setup', 'casino', 'shenanigans'];
const mobileMoreTabs: TabType[] = ['rewards', 'marketing', 'halloffame'];

const sectionSubtitles: Record<TabType, string> = {
  positions: "Don't get too attached",
  setup: "Choose your own adventure (all roads lead to losses)",
  casino: "Spoiler: the house always wins",
  rewards: "Worthless tokens for YOLOing into a Ponzi",
  marketing: "More than just a Ponzi — it's also a Pyramid Scheme",
  shenanigans: "Pure chaos, zero value",
  halloffame: "Legends of the grift",
};

export default function Dashboard() {
  // No props needed — navigation is self-contained now
  const [activeTab, setActiveTab] = useState<TabType>('positions');
  const [isAnimating, setIsAnimating] = useState(false);
  const [isMobile, setIsMobile] = useState(false);
  const [moreSheetOpen, setMoreSheetOpen] = useState(false);

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
    setMoreSheetOpen(false);
    setTimeout(() => setIsAnimating(false), 250);
  };

  const handleNavigateToGameSetup = () => handleTabChange('setup');

  const activeItem = navItems.find(n => n.id === activeTab)!;

  const renderContent = () => {
    const cls = isAnimating ? 'mc-enter' : '';
    switch (activeTab) {
      case 'positions': return <div className={cls}><GameTracking onNavigateToGameSetup={handleNavigateToGameSetup} /></div>;
      case 'setup': return <div className={cls}><GamePlans /></div>;
      case 'casino': return <div className={cls}><HouseDashboard /></div>;
      case 'marketing': return <div className={cls}><ReferralSection /></div>;
      case 'rewards': return <div className={cls}><PonziPointsDashboard /></div>;
      case 'halloffame': return <div className={cls}><HallOfFame /></div>;
      case 'shenanigans': return <div className={cls}><Shenanigans /></div>;
      default: return <div className={cls}><GameTracking onNavigateToGameSetup={handleNavigateToGameSetup} /></div>;
    }
  };

  return (
    <div className="flex min-h-[calc(100vh-80px)]">
      {/* === Desktop Left Rail === */}
      {!isMobile && (
        <nav className="mc-rail">
          {navItems.map((item, i) => {
            const isActive = activeTab === item.id;
            const prevItem = navItems[i - 1];
            const showDivider = prevItem && prevItem.group !== item.group;

            return (
              <React.Fragment key={item.id}>
                {showDivider && <div className="mc-rail-divider" />}
                <button
                  onClick={() => handleTabChange(item.id)}
                  className={`mc-rail-item ${isActive ? (item.activeClass || 'active') : ''}`}
                >
                  <span className={`mc-rail-icon ${!isActive && item.glowClass ? item.glowClass : ''}`}>
                    {item.emoji || item.icon}
                  </span>
                  <span className="mc-rail-label">{item.label}</span>
                </button>
              </React.Fragment>
            );
          })}
        </nav>
      )}

      {/* === Main Content === */}
      <div className={`flex-1 ${!isMobile ? 'ml-16' : ''} ${isMobile ? 'pb-20' : ''}`}>
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

      {/* === Mobile Bottom Tabs === */}
      {isMobile && (
        <>
          <nav className="mc-bottom-tabs">
            {mobileMainTabs.map(tabId => {
              const item = navItems.find(n => n.id === tabId)!;
              const isActive = activeTab === tabId;
              return (
                <button
                  key={item.id}
                  onClick={() => handleTabChange(item.id)}
                  className={`mc-bottom-tab ${isActive ? (item.activeClass || 'active') : ''}`}
                >
                  <span className={`tab-icon ${!isActive && item.glowClass ? item.glowClass : ''}`}>
                    {item.emoji || item.icon}
                  </span>
                  <span>{item.label.split(' ')[0]}</span>
                </button>
              );
            })}
            {/* More button */}
            <button
              onClick={() => setMoreSheetOpen(!moreSheetOpen)}
              className={`mc-bottom-tab ${mobileMoreTabs.includes(activeTab) ? 'active' : ''}`}
            >
              <span className="tab-icon">
                {moreSheetOpen ? <X className="h-5 w-5" /> : <MoreHorizontal className="h-5 w-5" />}
              </span>
              <span>More</span>
            </button>
          </nav>

          {/* More sheet */}
          <div className={`mc-more-sheet ${moreSheetOpen ? 'open' : ''}`}>
            {mobileMoreTabs.map(tabId => {
              const item = navItems.find(n => n.id === tabId)!;
              const isActive = activeTab === tabId;
              return (
                <button
                  key={item.id}
                  onClick={() => handleTabChange(item.id)}
                  className={`w-full flex items-center gap-3 p-3 rounded-lg mb-1 transition-all ${
                    isActive ? 'mc-card bg-white/5' : 'hover:bg-white/5'
                  }`}
                >
                  <span className={`text-lg ${item.glowClass || ''}`}>{item.emoji || ''}</span>
                  <span className="font-bold text-sm">{item.label}</span>
                </button>
              );
            })}
          </div>

          {/* Overlay */}
          {moreSheetOpen && (
            <div className="fixed inset-0 z-30" onClick={() => setMoreSheetOpen(false)} />
          )}
        </>
      )}
    </div>
  );
}
