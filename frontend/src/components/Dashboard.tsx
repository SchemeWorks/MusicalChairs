import React, { useState, useEffect } from 'react';
import GamePlans from './GamePlans';
import GameTracking from './GameTracking';
import ReferralSection from './ReferralSection';
import HouseDashboard from './HouseDashboard';
import PonziPointsDashboard from './PonziPointsDashboard';
import HallOfFame from './HallOfFame';
import Shenanigans from './Shenanigans';
import { Button } from '@/components/ui/button';
import { X, ChevronLeft, ChevronRight } from 'lucide-react';

type TabType = 'positions' | 'setup' | 'casino' | 'rewards' | 'shenanigans' | 'marketing' | 'halloffame';

interface DashboardProps {
  isMobileMenuOpen: boolean;
  setIsMobileMenuOpen: (open: boolean) => void;
}

export default function Dashboard({ isMobileMenuOpen, setIsMobileMenuOpen }: DashboardProps) {
  const [activeTab, setActiveTab] = useState<TabType>('positions');
  const [isAnimating, setIsAnimating] = useState(false);
  const [isMobile, setIsMobile] = useState(false);
  const [isDockCollapsed, setIsDockCollapsed] = useState(false);
  const [hoveredItem, setHoveredItem] = useState<TabType | null>(null);

  // Check if mobile on mount and resize
  useEffect(() => {
    const checkMobile = () => {
      setIsMobile(window.innerWidth < 768);
    };
    
    checkMobile();
    window.addEventListener('resize', checkMobile);
    
    return () => window.removeEventListener('resize', checkMobile);
  }, []);

  // Close mobile menu when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (isMobileMenuOpen && isMobile) {
        const target = event.target as Element;
        if (!target.closest('.mobile-drawer') && !target.closest('.mobile-menu-button')) {
          setIsMobileMenuOpen(false);
        }
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [isMobileMenuOpen, isMobile, setIsMobileMenuOpen]);

  const handleTabChange = (newTab: TabType) => {
    if (newTab === activeTab) return;
    
    setIsAnimating(true);
    setActiveTab(newTab);
    setIsMobileMenuOpen(false); // Close mobile menu when tab changes
    
    setTimeout(() => setIsAnimating(false), 200);
  };

  const handleNavigateToGameSetup = () => {
    handleTabChange('setup');
  };

  const sidebarItems = [
    { id: 'positions' as TabType, label: 'Profit Center', icon: '💰', group: 'core' },
    { id: 'setup' as TabType, label: 'Pick Your Plan', icon: '🎯', group: 'core' },
    { id: 'casino' as TabType, label: 'House Ledger', icon: '📊', group: 'core' },
    { id: 'rewards' as TabType, label: 'Rewards', icon: '🎁', group: 'extras' },
    { id: 'marketing' as TabType, label: 'Multi-Level Marketing', icon: '👥', group: 'extras' },
    { id: 'shenanigans' as TabType, label: 'Shenanigans', icon: '🎲', group: 'fun' },
    { id: 'halloffame' as TabType, label: 'Hall of Fame', icon: '🏆', group: 'fun' }
  ];

  const renderTabContent = () => {
    const contentClass = isAnimating ? 'animate-tab-bounce' : '';
    
    switch (activeTab) {
      case 'positions':
        return <div className={contentClass}><GameTracking onNavigateToGameSetup={handleNavigateToGameSetup} /></div>;
      case 'setup':
        return <div className={contentClass}><GamePlans /></div>;
      case 'casino':
        return <div className={contentClass}><HouseDashboard /></div>;
      case 'marketing':
        return <div className={contentClass}><ReferralSection /></div>;
      case 'rewards':
        return <div className={contentClass}><PonziPointsDashboard /></div>;
      case 'halloffame':
        return <div className={contentClass}><HallOfFame /></div>;
      case 'shenanigans':
        return <div className={contentClass}><Shenanigans /></div>;
      default:
        return <div className={contentClass}><GameTracking onNavigateToGameSetup={handleNavigateToGameSetup} /></div>;
    }
  };

  const getTooltipBorderClass = (itemId: TabType) => {
    if (itemId === 'shenanigans') return 'border-neon-green';
    if (itemId === 'halloffame') return 'border-golden';
    return 'border-white';
  };

  const renderNavItems = (isMobileDrawer = false) => {
    const groups = [
      { name: 'core', items: sidebarItems.filter(item => item.group === 'core') },
      { name: 'extras', items: sidebarItems.filter(item => item.group === 'extras') },
      { name: 'fun', items: sidebarItems.filter(item => item.group === 'fun') }
    ];

    return groups.map((group, groupIndex) => (
      <div key={group.name}>
        {group.items.map((item, itemIndex) => (
          <div key={item.id} className="relative">
            <button
              onClick={() => handleTabChange(item.id)}
              onMouseEnter={() => !isMobileDrawer && setHoveredItem(item.id)}
              onMouseLeave={() => !isMobileDrawer && setHoveredItem(null)}
              className={`
                w-full flex items-center gap-4 p-3 rounded-xl transition-all duration-200 relative
                ${isMobileDrawer ? 'py-4' : ''}
                ${activeTab === item.id 
                  ? item.id === 'shenanigans'
                    ? 'bg-gradient-to-r from-neon-green to-purple-500 text-white border-2 border-neon-green shadow-neon-glow'
                    : item.id === 'halloffame'
                    ? 'bg-gradient-to-r from-golden to-yellow-500 text-black border-2 border-golden shadow-golden-glow'
                    : 'bg-gradient-to-r from-purple-500 to-green-500 text-white border-2 border-purple-500 shadow-purple-glow'
                  : 'text-gray-300 hover:text-white hover:bg-white/10'
                }
                ${isMobileDrawer ? 'mobile-nav-item' : ''}
              `}
              style={isMobileDrawer ? { animationDelay: `${(groupIndex * 3 + itemIndex) * 50}ms` } : {}}
            >
              <span className={`
                text-xl min-w-[1.5rem] text-center
                ${activeTab === item.id && item.id === 'shenanigans' ? 'animate-neon-pulse' : ''}
                ${activeTab === item.id && item.id === 'halloffame' ? 'animate-golden-pulse' : ''}
                ${item.id === 'shenanigans' && activeTab !== item.id ? 'shenanigans-icon-glow' : ''}
                ${item.id === 'halloffame' && activeTab !== item.id ? 'hall-of-fame-icon-glow' : ''}
              `}>
                {item.icon}
              </span>
              {(!isDockCollapsed || isMobileDrawer) && (
                <span className={`
                  font-medium whitespace-nowrap
                  ${item.id === 'shenanigans' ? 'font-bold text-lg' : ''}
                  ${item.id === 'halloffame' ? 'font-bold italic' : ''}
                `}>
                  {item.label}
                </span>
              )}
            </button>

            {/* Tooltip for collapsed desktop state */}
            {!isMobileDrawer && isDockCollapsed && hoveredItem === item.id && (
              <div className={`
                absolute left-full ml-2 top-1/2 -translate-y-1/2 z-50
                bg-black text-white px-3 py-2 rounded-lg text-sm whitespace-nowrap
                border-2 ${getTooltipBorderClass(item.id)}
                shadow-lg animate-fade-in
              `}>
                {item.label}
              </div>
            )}
          </div>
        ))}
        
        {/* Neon divider lines for mobile drawer */}
        {isMobileDrawer && groupIndex < groups.length - 1 && (
          <div className="my-4 px-4">
            <div className="h-px bg-gradient-to-r from-transparent via-purple-500 to-transparent opacity-60"></div>
          </div>
        )}
      </div>
    ));
  };

  return (
    <div className="flex min-h-screen relative">
      {/* Mobile Overlay */}
      {isMobile && isMobileMenuOpen && (
        <div className="fixed inset-0 bg-black/50 z-40 transition-opacity duration-300" />
      )}

      {/* Desktop Floating Dock / Mobile Drawer */}
      {isMobile ? (
        /* Enhanced Mobile Drawer */
        <div className={`
          mobile-drawer fixed left-0 top-0 h-full z-50 transition-transform duration-300 ease-in-out
          ${isMobileMenuOpen ? 'translate-x-0' : '-translate-x-full'}
        `}
        style={{ width: '75%', maxWidth: '280px' }}
        >
          <div className="h-full mobile-drawer-bg overflow-y-auto">
            {/* Mobile Close Button with glowing outline */}
            <div className="flex justify-start p-4">
              <Button
                onClick={() => setIsMobileMenuOpen(false)}
                className="bg-transparent hover:bg-white/10 p-2 border-2 border-white/30 rounded-lg glow-outline transition-all duration-200"
                variant="ghost"
                size="icon"
              >
                <X className="h-6 w-6 text-white" />
              </Button>
            </div>

            {/* Mobile Navigation Items with staggered animation */}
            <nav className="px-4 pb-4 space-y-2">
              {renderNavItems(true)}
            </nav>
          </div>
        </div>
      ) : (
        /* Desktop Floating Dock */
        <div 
          className={`
            floating-dock fixed z-40 transition-all duration-300 ease-in-out
            ${isDockCollapsed ? 'w-20' : 'w-64'}
          `}
          style={{ 
            left: '60px', 
            top: '50%', 
            transform: 'translateY(-50%)',
          }}
          onMouseLeave={() => isDockCollapsed && setHoveredItem(null)}
        >
          <div className="bg-black/60 backdrop-blur-md rounded-2xl shadow-2xl p-4">
            {/* Navigation Items */}
            <nav className="space-y-2 mb-4">
              {renderNavItems(false)}
            </nav>

            {/* Collapse/Expand Toggle */}
            <div className="border-t border-white/20 pt-4">
              <button
                onClick={() => setIsDockCollapsed(!isDockCollapsed)}
                className="w-full flex items-center justify-center p-2 rounded-lg text-gray-300 hover:text-white hover:bg-white/10 transition-all duration-200"
              >
                {isDockCollapsed ? (
                  <ChevronRight className="h-5 w-5" />
                ) : (
                  <ChevronLeft className="h-5 w-5" />
                )}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Main Content Area - offset for sidebar on desktop */}
      <div className={`flex-1 min-h-screen transition-all duration-300 ${
        !isMobile ? (isDockCollapsed ? 'md:ml-[140px]' : 'md:ml-[340px]') : ''
      }`}>
        {/* Dashboard Header */}
        <div className="dashboard-header">
          <div className="dashboard-title-panel">
            <h2 className="text-4xl font-black mb-2 dashboard-title-stroked">
              🎪 Musical Chairs Dashboard 🎪
            </h2>
            <div className="ponzi-tagline text-yellow-300 text-2xl font-bold mb-4">
              It's a Ponzi!
            </div>
            <p className="text-white font-bold text-sm mt-2 text-with-backdrop">Please gamble responsibly</p>
          </div>
        </div>

        {/* Tab Content */}
        <div className="content-container">
          {renderTabContent()}
        </div>
      </div>
    </div>
  );
}
