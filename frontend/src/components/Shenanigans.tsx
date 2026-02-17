import React, { useState, useEffect } from 'react';
import { useCastShenanigan, useGetShenaniganStats, useGetRecentShenanigans, useGetPonziPoints, useGetShenaniganConfigs } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';
import { ShenaniganType } from '../backend';
import { Info, Shield, Zap, AlertTriangle, Coins, Waves, Pencil, Building2, Target, FlipHorizontal2, ArrowUp, Scissors, Fish, TrendingUp, Sparkles, Dices, RefreshCw, Trophy } from 'lucide-react';
import HallOfFame from './HallOfFame';

const successFlavor = [
  "The house smiles upon you.",
  "Clean hit. Charles would be proud.",
  "Flawless execution. You're a natural.",
  "They never saw it coming.",
  "That's how it's done in this business.",
];

const failFlavor = [
  "The universe said no.",
  "Not your day. It happens to everyone. Mostly to you.",
  "Swing and a miss. The PP is still gone, though.",
  "Nothing happened. Except you're poorer now.",
  "Better luck next time. Or not. Who knows.",
];

const backfireFlavor = [
  "Oh no. It hit you instead.",
  "Karma works fast around here.",
  "You played yourself. Literally.",
  "That's what they call a learning experience.",
  "Charles is laughing somewhere.",
];

interface ShenaniganConfig {
  type: ShenaniganType;
  name: string;
  icon: React.ReactNode;
  cost: number;
  description: string;
  odds: { success: number; fail: number; backfire: number };
  effects: string;
  auraColor: string;
}

const shenaniganIcons: Record<number, React.ReactNode> = {
  0: <Coins className="h-5 w-5" />, 1: <Waves className="h-5 w-5" />, 2: <Pencil className="h-5 w-5" />,
  3: <Building2 className="h-5 w-5" />, 4: <Target className="h-5 w-5" />, 5: <FlipHorizontal2 className="h-5 w-5" />,
  6: <ArrowUp className="h-5 w-5" />, 7: <Scissors className="h-5 w-5" />, 8: <Fish className="h-5 w-5" />,
  9: <TrendingUp className="h-5 w-5" />, 10: <Sparkles className="h-5 w-5" />,
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

type FilterCategory = 'all' | 'offense' | 'defense' | 'chaos';

const offenseTypes = [0, 1, 3, 4, 7, 8]; // moneyTrickster, aoeSkim, mintTaxSiphon, downlineHeist, purseCutter, whaleRebalance
const defenseTypes = [5, 6, 9]; // magicMirror, ppBoosterAura, downlineBoost
const chaosTypes = [2, 10]; // renameSpell, goldenName

function getShenaniganCategory(idx: number): FilterCategory {
  if (offenseTypes.includes(idx)) return 'offense';
  if (defenseTypes.includes(idx)) return 'defense';
  return 'chaos';
}

export default function Shenanigans() {
  const { data: stats, isLoading: statsLoading } = useGetShenaniganStats();
  const { data: recentShenanigans, isLoading: recentLoading } = useGetRecentShenanigans();
  const { data: ponziData, isLoading: ponziLoading } = useGetPonziPoints();
  const { data: backendConfigs, isLoading: configsLoading } = useGetShenaniganConfigs();
  const castShenanigan = useCastShenanigan();
  const [filterCategory, setFilterCategory] = useState<FilterCategory>('all');
  const [animatingTrick, setAnimatingTrick] = useState<string | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [selectedShenanigan, setSelectedShenanigan] = useState<{ type: ShenaniganType; name: string; cost: number; icon: string } | null>(null);
  const [outcomeToast, setOutcomeToast] = useState<{ name: string; outcome: string; flavor: string; cost: number } | null>(null);
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
      setOutcomeToast({
        name,
        outcome: 'error',
        flavor: `Insufficient PP. Need ${cost}, have ${(ponziData?.totalPoints || 0).toLocaleString()}.`,
        cost: 0,
      });
      return;
    }
    setSelectedShenanigan({ type, name, cost, icon });
    setConfirmOpen(true);
  };

  const getFlavorText = (outcome: string) => {
    const pool = outcome === 'success' ? successFlavor : outcome === 'fail' ? failFlavor : backfireFlavor;
    return pool[Math.floor(Math.random() * pool.length)];
  };

  const handleConfirmCast = async () => {
    if (!selectedShenanigan) return;
    setConfirmOpen(false);
    setAnimatingTrick(selectedShenanigan.type);
    try {
      const rawOutcome = await castShenanigan.mutateAsync({ shenaniganType: selectedShenanigan.type, target: null });
      const outcome = String(rawOutcome);
      setTimeout(() => {
        setOutcomeToast({
          name: selectedShenanigan.name,
          outcome,
          flavor: getFlavorText(outcome),
          cost: selectedShenanigan.cost,
        });
        setAnimatingTrick(null);
      }, 1500);
    } catch (error: any) {
      setOutcomeToast({
        name: selectedShenanigan.name,
        outcome: 'error',
        flavor: error.message || 'Something went wrong. The PP is still gone.',
        cost: selectedShenanigan.cost,
      });
      setAnimatingTrick(null);
    }
  };

  // Compute most popular shenanigan from recent feed
  const mostPopularType = (() => {
    if (!recentShenanigans || recentShenanigans.length < 3) return null;
    const counts: Record<string, number> = {};
    for (const s of recentShenanigans) {
      const key = String(s.shenaniganType);
      counts[key] = (counts[key] || 0) + 1;
    }
    const sorted = Object.entries(counts).sort((a, b) => b[1] - a[1]);
    return sorted[0]?.[0] ?? null;
  })();

  if (statsLoading || recentLoading || configsLoading || ponziLoading) return <LoadingSpinner />;

  const userPoints = ponziData?.totalPoints || 0;

  return (
    <div className="space-y-6">
      {/* PP balance bar */}
      <div className="mc-card p-3 flex items-center justify-center gap-3">
        <span className="mc-label">Your Ponzi Points:</span>
        <span className="text-lg font-bold mc-text-purple mc-glow-purple">{userPoints.toLocaleString()} PP</span>
      </div>

      {/* Desktop 2-column layout: cards left, feed right */}
      <div className="mc-shenanigans-layout">
        {/* Left column: filter + cards + guardrails */}
        <div className="space-y-6">
          {/* Filter tabs */}
          <div className="flex rounded-lg bg-white/5 p-0.5">
            {([
              { key: 'all' as FilterCategory, label: 'All' },
              { key: 'offense' as FilterCategory, label: 'Offense' },
              { key: 'defense' as FilterCategory, label: 'Defense' },
              { key: 'chaos' as FilterCategory, label: 'Chaos' },
            ]).map(tab => (
              <button
                key={tab.key}
                onClick={() => setFilterCategory(tab.key)}
                className={`flex-1 py-2 text-xs font-bold rounded-md transition-all ${
                  filterCategory === tab.key ? 'bg-[var(--mc-purple)]/25 mc-text-primary border border-[var(--mc-purple)]/30' : 'mc-text-muted hover:mc-text-dim hover:bg-white/5'
                }`}
              >
                {tab.label}
              </button>
            ))}
          </div>

          {/* Shenanigan cards grid */}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mc-stagger">
            {availableShenanigans.filter((_, idx) => filterCategory === 'all' || getShenaniganCategory(idx) === filterCategory).map((trick, idx) => {
              const isDisabled = castShenanigan.isPending || userPoints < trick.cost || animatingTrick === trick.type;
              return (
                <div
                  key={`shenanigan-${idx}`}
                  className="mc-shenanigan-card"
                  style={{ '--aura-color': trick.auraColor } as React.CSSProperties}
                >
                  {/* Popular badge */}
                  {mostPopularType !== null && String(trick.type) === mostPopularType && (
                    <span className="absolute -top-2 -right-2 text-xs bg-orange-500/20 text-orange-400 px-2 py-0.5 rounded-full font-bold z-10">
                      🔥 Popular
                    </span>
                  )}
                  {/* Icon */}
                  <div
                    className="mc-shenanigan-icon"
                    style={{ background: `linear-gradient(135deg, ${trick.auraColor}, transparent)` }}
                  >
                    {trick.icon}
                  </div>

                  {/* Title + cost */}
                  <h3 className="font-display text-sm mc-text-primary text-center mb-1">{trick.name}</h3>
                  <div className="text-center mb-3">
                    <span className="text-xs px-2 py-0.5 rounded-full font-bold bg-[var(--mc-purple)]/20 mc-text-purple">
                      {trick.cost} PP
                    </span>
                  </div>

                  {/* Description */}
                  <p className="text-xs mc-text-dim leading-relaxed mb-3">{trick.description}</p>

                  {/* Odds bar */}
                  <div className="mb-4">
                    <div className="flex h-2 rounded-full overflow-hidden mb-1">
                      <div className="mc-bg-green" style={{ width: `${trick.odds.success}%` }} />
                      <div className="mc-bg-danger" style={{ width: `${trick.odds.fail}%` }} />
                      <div className="mc-bg-purple" style={{ width: `${trick.odds.backfire}%` }} />
                    </div>
                    <div className="flex justify-between text-xs mc-text-muted">
                      <span className="mc-text-green">✓ {trick.odds.success}%</span>
                      <span className="mc-text-danger">✗ {trick.odds.fail}%</span>
                      <span className="mc-text-purple">↩ {trick.odds.backfire}%</span>
                    </div>
                  </div>

                  {/* Cast button */}
                  <button
                    onClick={() => !isDisabled && handleCastClick(trick.type, trick.cost, trick.name, trick.icon)}
                    disabled={isDisabled}
                    className={`w-full py-2 rounded-lg text-xs font-bold uppercase transition-all ${
                      isDisabled ? 'bg-white/5 text-white/30 cursor-not-allowed border border-white/5' : 'mc-btn-primary'
                    }`}
                  >
                    {animatingTrick === trick.type ? 'Casting...' : userPoints < trick.cost ? `Need ${trick.cost} PP` : `Cast (${trick.cost} PP)`}
                  </button>
                </div>
              );
            })}
          </div>

          {/* Empty state when filter matches nothing */}
          {availableShenanigans.filter((_, idx) => filterCategory === 'all' || getShenaniganCategory(idx) === filterCategory).length === 0 && (
            <div className="text-center py-12">
              <Dices className="h-10 w-10 mc-text-muted mb-3 mx-auto" />
              <p className="mc-text-dim text-sm">No shenanigans in this category.</p>
              <button onClick={() => setFilterCategory('all')} className="mc-text-purple text-xs mt-2 hover:underline">
                Show all
              </button>
            </div>
          )}

          {/* Guardrails */}
          <div className="mc-card p-5">
            <h3 className="font-display text-sm mc-text-primary mb-3 flex items-center gap-2">
              <Shield className="h-4 w-4 mc-text-cyan" /> Guardrails
            </h3>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs mc-text-dim">
              <div className="flex items-start gap-2">
                <Info className="h-3 w-3 mc-text-cyan mt-0.5 flex-shrink-0" />
                <span><strong className="mc-text-primary">PP & Cosmetics Only</strong> — Never affects ICP, pot, backer selection, or payout math</span>
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
        </div>

        {/* Right column (desktop): Stats + Live Feed — sticky */}
        <div className="mc-shenanigans-sidebar">
          <div className="mc-card-elevated">
            {/* Current round stats */}
            <h3 className="font-display text-base mc-text-primary mb-4">Current Round Stats</h3>
            <div className="grid grid-cols-2 gap-3 mb-6">
              {[
                { label: 'PP Spent', value: stats?.totalSpent?.toLocaleString() || '0', color: 'mc-text-cyan' },
                { label: 'Total Cast', value: stats?.totalCast?.toString() || '0', color: 'mc-text-green' },
                { label: 'Outcomes', value: `${stats?.goodOutcomes || 0}/${stats?.badOutcomes || 0}/${stats?.backfires || 0}`, sub: 'good/bad/backfire', color: 'mc-text-purple' },
                { label: 'VC Royalties', value: stats?.dealerCut?.toLocaleString() || '0', color: 'mc-text-gold' },
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
            <div className="space-y-2 max-h-72 overflow-y-auto">
              {recentShenanigans && recentShenanigans.length > 0 ? (
                recentShenanigans.slice(0, 20).map(s => (
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
        </div>
      </div>

      {/* Hall of Fame */}
      <div className="mt-2">
        <div className="flex items-center gap-2 mb-4">
          <Trophy className="h-5 w-5 mc-text-gold" />
          <h2 className="font-display text-lg mc-text-primary">Hall of Fame</h2>
        </div>
        <HallOfFame />
      </div>

      {/* Trollbox teaser — coming soon */}
      <div className="mc-card mc-accent-cyan p-5 text-center">
        <div className="flex items-center justify-center gap-2 mb-2">
          <RefreshCw className="h-4 w-4 mc-text-cyan animate-spin" style={{ animationDuration: '3s' }} />
          <span className="font-display text-sm mc-text-cyan">Trollbox</span>
        </div>
        <p className="text-xs mc-text-dim">Live chat where everyone can trash-talk, flex, and watch the chaos unfold in real time.</p>
        <p className="text-xs mc-text-muted mt-1 font-accent italic">Coming soon.</p>
      </div>

      {/* Footer */}
      <div className="mc-status-blue p-4 text-center text-xs">
        <span className="font-bold">Shenanigans are pure entertainment.</span>
        <span className="mc-text-dim"> They don't affect game math — just the madness. Effects limited to PP and cosmetics only.</span>
      </div>

      {/* Outcome toast */}
      {outcomeToast && (
        <div className="fixed top-8 left-1/2 -translate-x-1/2 z-[9999]">
          <div className="mc-toast text-center">
            <div className={`font-display text-xl mb-2 ${
              outcomeToast.outcome === 'success' ? 'mc-text-green' :
              outcomeToast.outcome === 'fail' ? 'mc-text-danger' :
              outcomeToast.outcome === 'backfire' ? 'mc-text-purple' :
              'mc-text-danger'
            }`}>
              {outcomeToast.outcome === 'success' ? 'Success!' :
               outcomeToast.outcome === 'fail' ? 'Failed.' :
               outcomeToast.outcome === 'backfire' ? 'Backfire!' :
               'Error'}
            </div>
            <p className="font-bold text-sm mc-text-primary mb-1">{outcomeToast.name}</p>
            <p className="font-accent text-xs mc-text-dim italic mb-3">
              &ldquo;{outcomeToast.flavor}&rdquo;
            </p>
            {outcomeToast.cost > 0 && (
              <p className="text-xs mc-text-muted mb-3">{outcomeToast.cost} PP spent</p>
            )}
            <button
              onClick={() => setOutcomeToast(null)}
              className="mc-btn-secondary px-5 py-2 rounded-full text-sm"
            >
              Noted
            </button>
          </div>
        </div>
      )}

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
              <button onClick={handleConfirmCast} className="mc-btn-primary px-5 py-2 rounded-full text-sm">Cast It!</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
