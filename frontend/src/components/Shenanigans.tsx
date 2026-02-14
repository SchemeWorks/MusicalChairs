import React, { useState, useEffect } from 'react';
import { useCastShenanigan, useGetShenaniganStats, useGetRecentShenanigans, useGetPonziPoints, useGetShenaniganConfigs } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';
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
  auraColor: string;
}

const shenaniganIcons: Record<number, string> = {
  0: '💰', 1: '🌊', 2: '✏️', 3: '🏦', 4: '🎯',
  5: '🪞', 6: '⬆️', 7: '✂️', 8: '🐋', 9: '📈', 10: '✨',
};

const shenaniganTypes: ShenaniganType[] = [
  ShenaniganType.moneyTrickster, ShenaniganType.aoeSkim, ShenaniganType.renameSpell,
  ShenaniganType.mintTaxSiphon, ShenaniganType.downlineHeist, ShenaniganType.magicMirror,
  ShenaniganType.ppBoosterAura, ShenaniganType.purseCutter, ShenaniganType.whaleRebalance,
  ShenaniganType.downlineBoost, ShenaniganType.goldenName,
];

// Dark-themed aura colors for each shenanigan
const auraColors: Record<number, string> = {
  0: 'rgba(255, 215, 90, 0.3)',
  1: 'rgba(100, 200, 255, 0.3)',
  2: 'rgba(255, 130, 200, 0.3)',
  3: 'rgba(168, 85, 247, 0.3)',
  4: 'rgba(57, 255, 20, 0.3)',
  5: 'rgba(255, 215, 0, 0.3)',
  6: 'rgba(100, 165, 255, 0.3)',
  7: 'rgba(255, 100, 100, 0.3)',
  8: 'rgba(168, 85, 247, 0.3)',
  9: 'rgba(16, 185, 129, 0.3)',
  10: 'rgba(245, 158, 11, 0.3)',
};

export default function Shenanigans() {
  const { data: stats, isLoading: statsLoading } = useGetShenaniganStats();
  const { data: recentShenanigans, isLoading: recentLoading } = useGetRecentShenanigans();
  const { data: ponziData, isLoading: ponziLoading } = useGetPonziPoints();
  const { data: backendConfigs, isLoading: configsLoading } = useGetShenaniganConfigs();
  const castShenanigan = useCastShenanigan();
  const [animatingTrick, setAnimatingTrick] = useState<string | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [selectedShenanigan, setSelectedShenanigan] = useState<{ type: ShenaniganType; name: string; cost: number; icon: string } | null>(null);
  const [availableShenanigans, setAvailableShenanigans] = useState<ShenaniganConfig[]>([]);

  useEffect(() => {
    if (backendConfigs) {
      setAvailableShenanigans(backendConfigs.map(config => {
        const id = Number(config.id);
        return {
          type: shenaniganTypes[id], name: config.name, icon: shenaniganIcons[id],
          cost: config.cost, description: config.description,
          odds: { success: Number(config.successOdds), fail: Number(config.failureOdds), backfire: Number(config.backfireOdds) },
          effects: config.effectValues.join(', '), auraColor: auraColors[id] || auraColors[0],
        };
      }));
    }
  }, [backendConfigs]);

  // Listen for admin panel live updates
  useEffect(() => {
    const handler = (event: CustomEvent) => {
      const u = event.detail;
      setAvailableShenanigans(prev => prev.map(s => {
        if (shenaniganTypes.indexOf(s.type) === u.id) {
          return { ...s, name: u.name, icon: u.icon, cost: u.cost, description: u.description,
            odds: { success: u.successOdds, fail: u.failOdds, backfire: u.backfireOdds }, effects: u.effectValues };
        }
        return s;
      }));
    };
    window.addEventListener('shenaniganUpdated', handler as EventListener);
    return () => window.removeEventListener('shenaniganUpdated', handler as EventListener);
  }, []);

  const handleCastClick = (type: ShenaniganType, cost: number, name: string, icon: string) => {
    if ((ponziData?.totalPoints || 0) < cost) {
      toast.error(`Insufficient PP! Need ${cost}, have ${(ponziData?.totalPoints || 0).toLocaleString()}.`);
      return;
    }
    setSelectedShenanigan({ type, name, cost, icon });
    setConfirmOpen(true);
  };

  const handleConfirmCast = async () => {
    if (!selectedShenanigan) return;
    setConfirmOpen(false);
    setAnimatingTrick(selectedShenanigan.type);
    try {
      const outcome = await castShenanigan.mutateAsync({ shenaniganType: selectedShenanigan.type, target: null });
      setTimeout(() => {
        const emoji = outcome === 'success' ? '✨' : outcome === 'fail' ? '💥' : '🔄';
        toast.success(`${emoji} ${selectedShenanigan.name} ${outcome}!`, {
          description: `${selectedShenanigan.cost} PP spent.`
        });
        setAnimatingTrick(null);
      }, 1500);
    } catch (error: any) {
      toast.error(error.message || 'Failed to cast shenanigan!');
      setAnimatingTrick(null);
    }
  };

  if (statsLoading || recentLoading || configsLoading || ponziLoading) return <LoadingSpinner />;

  const userPoints = ponziData?.totalPoints || 0;

  return (
    <div className="space-y-6">
      {/* PP balance bar */}
      <div className="mc-card p-3 flex items-center justify-center gap-3">
        <span className="mc-label">Your Ponzi Points:</span>
        <span className="text-lg font-bold mc-text-purple mc-glow-purple">{userPoints.toLocaleString()} PP</span>
      </div>

      {/* Shenanigan cards grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 mc-stagger">
        {availableShenanigans.map((trick, idx) => {
          const isDisabled = castShenanigan.isPending || userPoints < trick.cost || animatingTrick === trick.type;
          return (
            <div
              key={`shenanigan-${idx}`}
              className="mc-shenanigan-card"
              style={{ '--aura-color': trick.auraColor } as React.CSSProperties}
            >
              {/* Icon */}
              <div
                className="mc-shenanigan-icon"
                style={{ background: `linear-gradient(135deg, ${trick.auraColor}, transparent)` }}
              >
                {trick.icon}
              </div>

              {/* Title + cost */}
              <h3 className="font-bold text-sm mc-text-primary text-center mb-1">{trick.name}</h3>
              <div className="text-center mb-3">
                <span className="text-xs px-2 py-0.5 rounded-full font-bold bg-purple-500/20 mc-text-purple">
                  {trick.cost} PP
                </span>
              </div>

              {/* Description */}
              <p className="text-xs mc-text-dim leading-relaxed mb-3">{trick.description}</p>

              {/* Odds bar */}
              <div className="mb-4">
                <div className="flex h-2 rounded-full overflow-hidden mb-1">
                  <div className="bg-green-500" style={{ width: `${trick.odds.success}%` }} />
                  <div className="bg-red-500" style={{ width: `${trick.odds.fail}%` }} />
                  <div className="bg-purple-500" style={{ width: `${trick.odds.backfire}%` }} />
                </div>
                <div className="flex justify-between text-xs mc-text-muted">
                  <span className="mc-text-green">{trick.odds.success}%</span>
                  <span className="mc-text-danger">{trick.odds.fail}%</span>
                  <span className="mc-text-purple">{trick.odds.backfire}%</span>
                </div>
              </div>

              {/* Cast button */}
              <button
                onClick={() => !isDisabled && handleCastClick(trick.type, trick.cost, trick.name, trick.icon)}
                disabled={isDisabled}
                className={`w-full py-2 rounded-lg text-xs font-bold uppercase transition-all ${
                  isDisabled ? 'bg-gray-700 text-gray-500 cursor-not-allowed' : 'mc-btn-primary'
                }`}
              >
                {animatingTrick === trick.type ? '✨ Casting...' : 'Cast'}
              </button>
            </div>
          );
        })}
      </div>

      {/* Stats + Feed */}
      <div className="mc-card-elevated">
        {/* Current round stats */}
        <h3 className="font-display text-base mc-text-primary mb-4">Current Round Stats</h3>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3 mb-6">
          {[
            { label: 'PP Spent', value: stats?.totalSpent?.toLocaleString() || '0', color: 'mc-text-cyan' },
            { label: 'Total Cast', value: stats?.totalCast?.toString() || '0', color: 'mc-text-green' },
            { label: 'Outcomes', value: `${stats?.goodOutcomes || 0}/${stats?.badOutcomes || 0}/${stats?.backfires || 0}`, sub: 'good/bad/backfire', color: 'mc-text-purple' },
            { label: 'Dealer Cut', value: stats?.dealerCut?.toLocaleString() || '0', color: 'mc-text-gold' },
          ].map(s => (
            <div key={s.label} className="mc-card p-3 text-center">
              <div className="mc-label mb-1">{s.label}</div>
              <div className={`text-lg font-bold ${s.color}`}>{s.value}</div>
              {s.sub && <div className="text-xs mc-text-muted">{s.sub}</div>}
            </div>
          ))}
        </div>

        {/* Live feed */}
        <h3 className="font-display text-base mc-text-primary mb-3">Live Feed</h3>
        <div className="space-y-2 max-h-48 overflow-y-auto">
          {recentShenanigans && recentShenanigans.length > 0 ? (
            recentShenanigans.slice(0, 12).map(s => (
              <div key={s.id.toString()} className="mc-card p-2 flex items-center justify-between text-xs">
                <span className="font-bold mc-text-primary">
                  {availableShenanigans.find(a => a.type === s.shenaniganType)?.name || 'Unknown'}{' '}
                  {availableShenanigans.find(a => a.type === s.shenaniganType)?.icon}
                </span>
                <span className={`font-bold ${
                  s.outcome === 'success' ? 'mc-text-green' : s.outcome === 'fail' ? 'mc-text-danger' : 'mc-text-purple'
                }`}>
                  {s.outcome.toUpperCase()}
                </span>
              </div>
            ))
          ) : (
            <p className="text-center mc-text-muted text-sm py-4">No shenanigans cast yet. Be the first!</p>
          )}
        </div>
      </div>

      {/* Guardrails */}
      <div className="mc-card p-5">
        <h3 className="font-display text-sm mc-text-primary mb-3 flex items-center gap-2">
          <Shield className="h-4 w-4 mc-text-cyan" /> Guardrails
        </h3>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs mc-text-dim">
          <div className="flex items-start gap-2">
            <Info className="h-3 w-3 mc-text-cyan mt-0.5 flex-shrink-0" />
            <span><strong className="mc-text-primary">PP & Cosmetics Only</strong> — Never affects ICP, pot, dealer selection, or payout math</span>
          </div>
          <div className="flex items-start gap-2">
            <Shield className="h-3 w-3 mc-text-green mt-0.5 flex-shrink-0" />
            <span><strong className="mc-text-primary">Loss Protection</strong> — Targets under 200 PP protected; no one goes below 0</span>
          </div>
          <div className="flex items-start gap-2">
            <Zap className="h-3 w-3 mc-text-purple mt-0.5 flex-shrink-0" />
            <span><strong className="mc-text-primary">Cooldowns</strong> — 2-min global, 3-min per-target, 24-hr protection after negative effects</span>
          </div>
          <div className="flex items-start gap-2">
            <AlertTriangle className="h-3 w-3 mc-text-gold mt-0.5 flex-shrink-0" />
            <span><strong className="mc-text-primary">No Refunds</strong> — All shenanigans are final</span>
          </div>
        </div>
      </div>

      {/* Footer */}
      <div className="mc-status-blue p-4 text-center text-xs">
        <span className="font-bold">Shenanigans are pure entertainment.</span>
        <span className="mc-text-dim"> They don't affect game math — just the madness. Effects limited to PP and cosmetics only.</span>
      </div>

      {/* Confirm dialog */}
      {confirmOpen && selectedShenanigan && (
        <div className="fixed top-8 left-1/2 -translate-x-1/2 z-[9999]">
          <div className="mc-toast text-center">
            <div className="font-display text-xl mc-text-primary mb-2">
              {selectedShenanigan.icon} Cast {selectedShenanigan.name}?
            </div>
            <p className="text-sm mc-text-dim mb-1">
              This costs <span className="mc-toast-accent">{selectedShenanigan.cost} PP</span>
            </p>
            <p className="text-xs mc-text-muted mb-4">Outcome is random. No refunds.</p>
            <div className="flex gap-3 justify-center">
              <button onClick={() => setConfirmOpen(false)} className="mc-btn-secondary px-5 py-2 rounded-full text-sm">Cancel</button>
              <button onClick={handleConfirmCast} className="mc-btn-primary px-5 py-2 rounded-full text-sm">Cast It! 🎲</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
