import React, { useState, useEffect } from 'react';
import { useCastShenanigan, useGetShenaniganStats, useGetRecentShenanigans, useGetPonziPoints, useGetShenaniganConfigs } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { toast } from 'sonner';
import { ShenaniganType } from '../backend';
import { Info, Shield, Zap, AlertTriangle } from 'lucide-react';

interface ShenaniganConfig {
  type: ShenaniganType;
  name: string;
  icon: string;
  cost: number;
  description: string;
  odds: { success: number; fail: number; backfire: number };
  effects: string;
  gradient: string;
}

// Icon mapping
const shenaniganIcons: Record<number, string> = {
  0: '💰',
  1: '🌊',
  2: '✏️',
  3: '🏦',
  4: '🎯',
  5: '🪞',
  6: '⬆️',
  7: '✂️',
  8: '🐋',
  9: '📈',
  10: '✨',
};

// Type mapping
const shenaniganTypes: ShenaniganType[] = [
  ShenaniganType.moneyTrickster,
  ShenaniganType.aoeSkim,
  ShenaniganType.renameSpell,
  ShenaniganType.mintTaxSiphon,
  ShenaniganType.downlineHeist,
  ShenaniganType.magicMirror,
  ShenaniganType.ppBoosterAura,
  ShenaniganType.purseCutter,
  ShenaniganType.whaleRebalance,
  ShenaniganType.downlineBoost,
  ShenaniganType.goldenName,
];

// Gradient mapping
const shenaniganGradients: Record<number, string> = {
  0: 'linear-gradient(135deg, #ffd75a 0%, #ffb673 100%)',
  1: 'linear-gradient(135deg, #8ed4ff 0%, #b8f5ff 100%)',
  2: 'linear-gradient(135deg, #ff9ff3 0%, #f368e0 100%)',
  3: 'linear-gradient(135deg, #a78bfa 0%, #c084fc 100%)',
  4: 'linear-gradient(135deg, #34d399 0%, #6ee7b7 100%)',
  5: 'linear-gradient(135deg, #fbbf24 0%, #fcd34d 100%)',
  6: 'linear-gradient(135deg, #60a5fa 0%, #93c5fd 100%)',
  7: 'linear-gradient(135deg, #f87171 0%, #fca5a5 100%)',
  8: 'linear-gradient(135deg, #a855f7 0%, #c084fc 100%)',
  9: 'linear-gradient(135deg, #10b981 0%, #34d399 100%)',
  10: 'linear-gradient(135deg, #f59e0b 0%, #fbbf24 100%)',
};

export default function Shenanigans() {
  const { data: stats, isLoading: statsLoading } = useGetShenaniganStats();
  const { data: recentShenanigans, isLoading: recentLoading } = useGetRecentShenanigans();
  const { data: ponziData, isLoading: ponziLoading } = useGetPonziPoints();
  const { data: backendConfigs, isLoading: configsLoading } = useGetShenaniganConfigs();
  const castShenanigan = useCastShenanigan();
  const [animatingTrick, setAnimatingTrick] = useState<string | null>(null);
  const [confirmDialogOpen, setConfirmDialogOpen] = useState(false);
  const [selectedShenanigan, setSelectedShenanigan] = useState<{
    type: ShenaniganType;
    name: string;
    cost: number;
    icon: string;
  } | null>(null);
  
  // State for shenanigans that can be updated in real-time
  const [availableShenanigans, setAvailableShenanigans] = useState<ShenaniganConfig[]>([]);

  // Load configs from backend
  useEffect(() => {
    if (backendConfigs) {
      const mappedConfigs = backendConfigs.map(config => {
        const id = Number(config.id);
        return {
          type: shenaniganTypes[id],
          name: config.name,
          icon: shenaniganIcons[id],
          cost: config.cost,
          description: config.description,
          odds: {
            success: Number(config.successOdds),
            fail: Number(config.failureOdds),
            backfire: Number(config.backfireOdds)
          },
          effects: config.effectValues.join(', '),
          gradient: shenaniganGradients[id],
        };
      });
      setAvailableShenanigans(mappedConfigs);
    }
  }, [backendConfigs]);

  // Listen for shenanigan updates from admin panel
  useEffect(() => {
    const handleShenaniganUpdate = (event: CustomEvent) => {
      const updatedConfig = event.detail;
      
      // Update the shenanigans list
      setAvailableShenanigans(prev => 
        prev.map(shen => {
          const shenId = shenaniganTypes.indexOf(shen.type);
          if (shenId === updatedConfig.id) {
            return {
              ...shen,
              name: updatedConfig.name,
              icon: updatedConfig.icon,
              cost: updatedConfig.cost,
              description: updatedConfig.description,
              odds: {
                success: updatedConfig.successOdds,
                fail: updatedConfig.failOdds,
                backfire: updatedConfig.backfireOdds
              },
              effects: updatedConfig.effectValues,
            };
          }
          return shen;
        })
      );
    };

    window.addEventListener('shenaniganUpdated', handleShenaniganUpdate as EventListener);
    
    return () => {
      window.removeEventListener('shenaniganUpdated', handleShenaniganUpdate as EventListener);
    };
  }, []);

  const handleCastClick = (shenaniganType: ShenaniganType, cost: number, name: string, icon: string) => {
    const userPoints = ponziData?.totalPoints || 0;
    
    if (userPoints < cost) {
      toast.error(`Insufficient Ponzi Points! You need ${cost} but only have ${userPoints.toLocaleString()}.`);
      return;
    }

    setSelectedShenanigan({ type: shenaniganType, name, cost, icon });
    setConfirmDialogOpen(true);
  };

  const handleConfirmCast = async () => {
    if (!selectedShenanigan) return;

    setConfirmDialogOpen(false);
    setAnimatingTrick(selectedShenanigan.type);
    
    try {
      const outcome = await castShenanigan.mutateAsync({ 
        shenaniganType: selectedShenanigan.type, 
        target: null 
      });
      
      // Show outcome with animation
      setTimeout(() => {
        const outcomeEmoji = outcome === 'success' ? '✨' : outcome === 'fail' ? '💥' : '🔄';
        const outcomeText = outcome === 'success' ? 'Success!' : outcome === 'fail' ? 'Failed!' : 'Backfired!';
        
        toast.success(`${outcomeEmoji} Shenanigan ${outcomeText}`, {
          description: `Your ${selectedShenanigan.name} ${outcome}! ${selectedShenanigan.cost} Ponzi Points spent.`
        });
        
        setAnimatingTrick(null);
      }, 1500);
    } catch (error: any) {
      const errorMessage = error.message || 'Failed to cast shenanigan!';
      toast.error(errorMessage);
      setAnimatingTrick(null);
    }
  };

  const getShenaniganName = (type: ShenaniganType): string => {
    return availableShenanigans.find(s => s.type === type)?.name || 'Unknown';
  };

  const getOutcomeColor = (outcome: string): string => {
    switch (outcome) {
      case 'success': return 'text-green-600';
      case 'fail': return 'text-red-600';
      case 'backfire': return 'text-purple-600';
      default: return 'text-gray-600';
    }
  };

  // Mock data for previous round recap
  const mockPreviousRoundData = {
    totalShenanigans: 47,
    totalPointsBurned: 8950,
    outcomes: {
      good: 28,
      bad: 12,
      backfire: 7
    }
  };

  if (statsLoading || recentLoading || configsLoading || ponziLoading) {
    return (
      <div className="space-y-6">
        <div className="text-center">
          <h2 className="text-2xl font-black text-white text-with-backdrop">
            🃏 Shenanigans — Pure Chaos, Zero Value
          </h2>
          <p className="text-center text-white text-with-backdrop mt-2">
            Burn your Ponzi Points on ridiculous tricks that don't change the math, only the madness.
          </p>
        </div>
        
        <div className="rewards-single-container">
          <div className="flex justify-center py-8">
            <LoadingSpinner />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header outside the container */}
      <div className="text-center">
        <h2 className="text-2xl font-black text-white text-with-backdrop">
          🃏 Shenanigans — Pure Chaos, Zero Value
        </h2>
        <p className="text-center text-white text-with-backdrop mt-2 opacity-80">
          Burn your Ponzi Points on ridiculous tricks that don't change the math, only the madness.
          <br />
          <span className="text-yellow-300 font-bold">Your Ponzi Points: {ponziData?.totalPoints.toLocaleString() || '0'}</span>
        </p>
      </div>
      
      {/* Available Shenanigans Shop - Wrapped in unified frosted glass panel */}
      <div className="rewards-single-container">
        <div className="space-y-4">
          <div className="text-center space-y-2">
            <h3 className="text-2xl font-bold text-white text-with-backdrop">
              Available Shenanigans
            </h3>
            <div 
              className="w-full h-px mx-auto"
              style={{
                background: 'linear-gradient(90deg, transparent 0%, rgba(255,255,255,0.3) 20%, rgba(255,255,255,0.6) 50%, rgba(255,255,255,0.3) 80%, transparent 100%)',
                boxShadow: '0 0 8px rgba(255,255,255,0.4)'
              }}
            />
          </div>
          
          {/* Redesigned Shenanigans cards with enhanced readability and no effect values */}
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {availableShenanigans.map((trick) => {
              const isDisabled = castShenanigan.isPending || (ponziData?.totalPoints || 0) < trick.cost || animatingTrick === trick.type;
              
              return (
                <div 
                  key={trick.type} 
                  className="shenanigan-card-gradient group"
                  style={{
                    background: trick.gradient,
                    borderRadius: '1rem',
                    boxShadow: '0 8px 20px rgba(0, 0, 0, 0.15), inset 0 1px 0 rgba(255, 255, 255, 0.3)',
                    border: '1px solid rgba(255, 255, 255, 0.15)',
                    transition: 'all 200ms ease',
                    cursor: isDisabled ? 'not-allowed' : 'pointer',
                    opacity: isDisabled ? 0.5 : 1,
                    padding: '20px',
                    minHeight: '280px',
                    display: 'flex',
                    flexDirection: 'column',
                    position: 'relative',
                    overflow: 'hidden'
                  }}
                  onMouseEnter={(e) => {
                    if (!isDisabled) {
                      e.currentTarget.style.transform = 'translateY(-4px)';
                      e.currentTarget.style.boxShadow = '0 0 24px rgba(255, 255, 255, 0.4), 0 12px 28px rgba(0, 0, 0, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.4)';
                    }
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.transform = 'translateY(0)';
                    e.currentTarget.style.boxShadow = '0 8px 20px rgba(0, 0, 0, 0.15), inset 0 1px 0 rgba(255, 255, 255, 0.3)';
                  }}
                >
                  {/* Dark overlay for text readability */}
                  <div 
                    style={{
                      position: 'absolute',
                      top: 0,
                      left: 0,
                      right: 0,
                      bottom: 0,
                      background: 'rgba(0, 0, 0, 0.10)',
                      borderRadius: '1rem',
                      pointerEvents: 'none'
                    }}
                  />

                  {/* Content with relative positioning to appear above overlay */}
                  <div style={{ position: 'relative', zIndex: 1, display: 'flex', flexDirection: 'column', height: '100%' }}>
                    {/* Icon and Title */}
                    <div className="flex items-center gap-2 mb-3">
                      <span className="text-2xl">{trick.icon}</span>
                      <h3 
                        style={{
                          fontSize: '20px',
                          lineHeight: '1.2',
                          color: '#ffffff',
                          textShadow: '1px 1px 3px rgba(0, 0, 0, 0.5)',
                          fontWeight: '700',
                          fontFamily: '"Nunito", "Poppins", sans-serif'
                        }}
                      >
                        {trick.name}
                      </h3>
                    </div>

                    {/* PP Cost Badge */}
                    <div 
                      className="inline-block px-3 py-1 rounded-full mb-3 self-start"
                      style={{
                        background: 'rgba(0, 0, 0, 0.3)',
                        backdropFilter: 'blur(8px)',
                        border: '1px solid rgba(255, 255, 255, 0.2)',
                        fontSize: '14px',
                        fontWeight: '700',
                        color: '#ffd75a',
                        textShadow: '1px 1px 3px rgba(0, 0, 0, 0.6)'
                      }}
                    >
                      {trick.cost} PP
                    </div>

                    {/* Description */}
                    <p 
                      className="mb-3 flex-grow"
                      style={{
                        fontSize: '16px',
                        color: '#ffffff',
                        lineHeight: '1.5',
                        textShadow: '1px 1px 3px rgba(0, 0, 0, 0.5)',
                        fontWeight: '400'
                      }}
                    >
                      {trick.description}
                    </p>

                    {/* Odds with improved readability */}
                    <div 
                      className="mb-4"
                      style={{
                        fontSize: '14px',
                        color: '#ffffff',
                        lineHeight: '1.5',
                        textShadow: '1px 1px 4px rgba(0, 0, 0, 0.7)',
                        background: 'rgba(0, 0, 0, 0.15)',
                        padding: '8px 12px',
                        borderRadius: '8px',
                        backdropFilter: 'blur(4px)'
                      }}
                    >
                      <div className="font-semibold mb-1" style={{ fontWeight: '600' }}>Odds:</div>
                      <div>Success: {trick.odds.success}% | Fail: {trick.odds.fail}% | Backfire: {trick.odds.backfire}%</div>
                    </div>

                    {/* Cast Button */}
                    <button
                      onClick={() => !isDisabled && handleCastClick(trick.type, trick.cost, trick.name, trick.icon)}
                      disabled={isDisabled}
                      className="w-full py-2.5 rounded-xl font-bold transition-all duration-200 uppercase text-sm"
                      style={{
                        background: 'linear-gradient(135deg, #ffd75a 0%, #ff9f40 100%)',
                        boxShadow: '0 4px 12px rgba(0, 0, 0, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.4)',
                        color: '#ffffff',
                        textShadow: '1px 1px 2px rgba(0, 0, 0, 0.3)',
                        cursor: isDisabled ? 'not-allowed' : 'pointer',
                        border: 'none',
                        fontWeight: '700'
                      }}
                      onMouseEnter={(e) => {
                        if (!isDisabled) {
                          e.currentTarget.style.background = 'linear-gradient(135deg, #ffe270 0%, #ffb055 100%)';
                          e.currentTarget.style.boxShadow = '0 0 16px rgba(255, 215, 90, 0.6), 0 6px 16px rgba(0, 0, 0, 0.25), inset 0 1px 0 rgba(255, 255, 255, 0.5)';
                          e.currentTarget.style.transform = 'translateY(-2px) scale(1.02)';
                        }
                      }}
                      onMouseLeave={(e) => {
                        e.currentTarget.style.background = 'linear-gradient(135deg, #ffd75a 0%, #ff9f40 100%)';
                        e.currentTarget.style.boxShadow = '0 4px 12px rgba(0, 0, 0, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.4)';
                        e.currentTarget.style.transform = 'translateY(0) scale(1)';
                      }}
                    >
                      {animatingTrick === trick.type ? '✨ Casting...' : 'CAST'}
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {/* Single frosted glass container wrapping all content below shop */}
      <div className="rewards-single-container">
        <div className="space-y-6">
          {/* Current Round Stats - Full Width with 4 Stats Side by Side */}
          <Card className="border-4 border-orange-300 bg-gradient-to-r from-orange-100 to-red-100">
            <CardHeader>
              <CardTitle className="text-lg text-center text-orange-800">
                📊 Current Round Stats 📊
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div className="bg-blue-100 rounded-lg p-3 border border-blue-300 text-center">
                  <div className="text-sm font-bold text-blue-800">💸 PP Spent</div>
                  <div className="text-xl font-black text-blue-900">
                    {stats?.totalSpent.toLocaleString() || '0'}
                  </div>
                  <div className="text-xs text-blue-600">This Round</div>
                </div>
                <div className="bg-green-100 rounded-lg p-3 border border-green-300 text-center">
                  <div className="text-sm font-bold text-green-800">🎪 Total Cast</div>
                  <div className="text-xl font-black text-green-900">
                    {stats?.totalCast.toString() || '0'}
                  </div>
                  <div className="text-xs text-green-600">Shenanigans</div>
                </div>
                <div className="bg-purple-100 rounded-lg p-3 border border-purple-300 text-center">
                  <div className="text-sm font-bold text-purple-800">📈 Outcomes</div>
                  <div className="text-xs text-purple-900 space-y-1">
                    <div>✅ Good: {stats?.goodOutcomes.toString() || '0'}</div>
                    <div>❌ Bad: {stats?.badOutcomes.toString() || '0'}</div>
                    <div>🔄 Backfire: {stats?.backfires.toString() || '0'}</div>
                  </div>
                </div>
                <div className="bg-yellow-100 rounded-lg p-3 border border-yellow-300 text-center">
                  <div className="text-sm font-bold text-yellow-800">🏰 Dealer Cut</div>
                  <div className="text-xl font-black text-yellow-900">
                    {stats?.dealerCut.toLocaleString() || '0'}
                  </div>
                  <div className="text-xs text-yellow-600">PP Skimmed</div>
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Guardrails Info Card */}
          <Card className="border-2 border-cyan-400 bg-gradient-to-r from-cyan-50 to-blue-50">
            <CardHeader>
              <CardTitle className="text-lg text-center text-cyan-800 flex items-center justify-center gap-2">
                <Shield className="h-5 w-5" />
                Shenanigans Guardrails
                <Shield className="h-5 w-5" />
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
                <div className="bg-white/80 rounded-lg p-3 border border-cyan-200">
                  <div className="flex items-start gap-2">
                    <Info className="h-4 w-4 text-cyan-600 flex-shrink-0 mt-0.5" />
                    <div>
                      <div className="font-bold text-cyan-800">PP & Cosmetics Only</div>
                      <div className="text-cyan-900 text-xs">Never affects ICP, pot size, dealer selection, payout math, or round structure</div>
                    </div>
                  </div>
                </div>
                <div className="bg-white/80 rounded-lg p-3 border border-green-200">
                  <div className="flex items-start gap-2">
                    <Shield className="h-4 w-4 text-green-600 flex-shrink-0 mt-0.5" />
                    <div>
                      <div className="font-bold text-green-800">Loss Protection</div>
                      <div className="text-green-900 text-xs">Targets under 200 PP protected from loss; no one goes below 0 PP</div>
                    </div>
                  </div>
                </div>
                <div className="bg-white/80 rounded-lg p-3 border border-purple-200">
                  <div className="flex items-start gap-2">
                    <Zap className="h-4 w-4 text-purple-600 flex-shrink-0 mt-0.5" />
                    <div>
                      <div className="font-bold text-purple-800">Cooldowns</div>
                      <div className="text-purple-900 text-xs">2-min global cooldown per caster; 3-min per-target; 24-hr protection after negative effects</div>
                    </div>
                  </div>
                </div>
                <div className="bg-white/80 rounded-lg p-3 border border-orange-200">
                  <div className="flex items-start gap-2">
                    <AlertTriangle className="h-4 w-4 text-orange-600 flex-shrink-0 mt-0.5" />
                    <div>
                      <div className="font-bold text-orange-800">No Refunds</div>
                      <div className="text-orange-900 text-xs">All shenanigans are final - no refunds or appeals for outcomes</div>
                    </div>
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Live Shenanigans Feed and Previous Round Recap - Side by Side */}
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            {/* Live Activity Feed */}
            <Card className="border-2 border-cyan-400 bg-gradient-to-r from-cyan-50 to-blue-50">
              <CardHeader>
                <CardTitle className="text-lg text-center text-cyan-800">
                  📺 Live Shenanigans Feed 📺
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="casino-ticker space-y-2 max-h-48 overflow-y-auto">
                  {recentShenanigans && recentShenanigans.length > 0 ? (
                    recentShenanigans.slice(0, 12).map((shenanigan) => (
                      <div 
                        key={shenanigan.id.toString()} 
                        className="ticker-item bg-white rounded-lg p-2 border border-gray-200 text-sm transition-all duration-300"
                      >
                        <div className="flex justify-between items-center">
                          <span className="font-semibold">
                            {getShenaniganName(shenanigan.shenaniganType)} {availableShenanigans.find(s => s.type === shenanigan.shenaniganType)?.icon}
                          </span>
                          <span className={`font-bold ${getOutcomeColor(shenanigan.outcome)}`}>
                            {shenanigan.outcome.toUpperCase()}
                          </span>
                        </div>
                        <div className="text-xs text-gray-600">
                          Cost: {shenanigan.cost} PP • {new Date(Number(shenanigan.timestamp) / 1000000).toLocaleTimeString()}
                        </div>
                      </div>
                    ))
                  ) : (
                    <div className="text-center text-gray-500 py-4">
                      No shenanigans cast yet! Be the first to cause some chaos! 🎭
                    </div>
                  )}
                </div>
              </CardContent>
            </Card>

            {/* Previous Round Recap */}
            <Card className="border-2 border-indigo-400 bg-gradient-to-r from-indigo-100 to-purple-100">
              <CardHeader>
                <CardTitle className="text-lg text-center text-indigo-800">
                  🎭 Previous Round Recap 🎭
                </CardTitle>
                <CardDescription className="text-center text-indigo-700 text-sm">
                  Stats from the previous round
                </CardDescription>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-1 gap-3 text-sm">
                  <div className="bg-white rounded-lg p-3 border border-indigo-300">
                    <div className="font-bold text-indigo-800">Total Shenanigans Cast</div>
                    <div className="text-2xl font-black text-indigo-900">
                      {mockPreviousRoundData.totalShenanigans}
                    </div>
                    <div className="text-xs text-indigo-600">Previous Round</div>
                  </div>
                  <div className="bg-white rounded-lg p-3 border border-purple-300">
                    <div className="font-bold text-purple-800">PP Burned</div>
                    <div className="text-2xl font-black text-purple-900">
                      {mockPreviousRoundData.totalPointsBurned.toLocaleString()}
                    </div>
                    <div className="text-xs text-purple-600">Total Spent</div>
                  </div>
                  <div className="bg-white rounded-lg p-3 border border-pink-300">
                    <div className="font-bold text-pink-800">📈 Outcomes</div>
                    <div className="text-xs text-pink-900 space-y-1">
                      <div>✅ Good: {mockPreviousRoundData.outcomes.good}</div>
                      <div>❌ Bad: {mockPreviousRoundData.outcomes.bad}</div>
                      <div>🔄 Backfire: {mockPreviousRoundData.outcomes.backfire}</div>
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>
          </div>

          {/* Footer Message */}
          <Card className="border-2 border-purple-300 bg-purple-50">
            <CardContent className="pt-4">
              <div className="text-center text-purple-800 font-semibold">
                <div className="text-lg mb-2">🎪 Remember: Shenanigans are pure entertainment! 🎪</div>
                <div className="text-sm">
                  They don't affect the actual game math, just the madness! Burn those Ponzi Points for glory!
                </div>
                <div className="text-xs mt-2 text-purple-600 italic">
                  ⚠️ All effects are limited to Ponzi Points and cosmetics only - never touching ICP, pot size, dealer selection, payout math, or round structure.
                </div>
              </div>
            </CardContent>
          </Card>
        </div>
      </div>

      {/* Confirmation Dialog - Warning removed */}
      {confirmDialogOpen && selectedShenanigan && (
        <div
          className="fixed top-8 left-1/2 transform -translate-x-1/2 z-[9999] transition-all duration-300 ease-out opacity-100 translate-y-0"
          style={{ pointerEvents: 'auto' }}
        >
          {/* Toast card */}
          <div className="house-money-toast-card relative">
            {/* Content */}
            <div className="text-center space-y-3">
              {/* Title */}
              <div className="text-2xl font-black text-white">
                {selectedShenanigan.icon} Cast {selectedShenanigan.name}?
              </div>
              
              {/* Subtitle */}
              <div className="text-base text-white/90 leading-relaxed">
                This will cost{' '}
                <span className="house-toast-accent font-black">{selectedShenanigan.cost} Ponzi Points</span>
              </div>
              
              {/* Description */}
              <div className="text-sm text-white/80">
                Are you sure you want to cast this shenanigan? The outcome is random and there are no refunds!
              </div>
            </div>
            
            {/* Buttons */}
            <div className="flex justify-center space-x-4 mt-4">
              <button
                onClick={() => setConfirmDialogOpen(false)}
                className="px-6 py-3 rounded-full bg-gray-600 hover:bg-gray-700 text-white font-bold transition-all"
              >
                Cancel
              </button>
              <button
                onClick={handleConfirmCast}
                className="house-toast-button"
              >
                Cast It! 🎲
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
