import React, { useState, useEffect } from 'react';
import { Principal } from '@dfinity/principal';
import { useCastShenanigan, useGetShenaniganStats, useGetRecentShenanigans, useGetPonziPoints, useGetShenaniganConfigs, useSetPendingRenameName, useGetPendingRenameForCaller, useCancelPendingRename } from '../hooks/useQueries';
import { useSpellFlavorPool } from './trollbox/useSpellFlavorPool';
import LoadingSpinner from './LoadingSpinner';
import { ShenaniganType, ShenaniganRecord } from '../backend';
import { Info, Shield, Zap, AlertTriangle, Coins, Waves, Pencil, Building2, Target, FlipHorizontal2, ArrowUp, Scissors, Fish, TrendingUp, Sparkles, Dices, RefreshCw, Trophy, LayoutGrid, List } from 'lucide-react';
import HallOfFame from './HallOfFame';
import TargetPicker from './TargetPicker';
import { useDisplayName } from './trollbox/useDisplayName';

// Spell ids that REQUIRE a target. Mirrors the trap in shenanigans/main.mo
// castShenanigan — backend rejects null target for these.
const TARGETED_SPELL_IDS = new Set([0, 2, 3, 4, 7]); // moneyTrickster, renameSpell, mintTaxSiphon, downlineHeist, purseCutter


interface ShenaniganConfig {
  id: number;
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

// User-facing copy describing what happens to the caster on backfire.
// Keep in sync with applyBackfireEffect in shenanigans/main.mo.
const backfireDescriptions: Record<number, string> = {
  0: 'You pay the target 2-8% of your PP (max 250).',   // moneyTrickster
  1: 'You burn 1-3% of your own PP.',                    // aoeSkim
  2: 'You get renamed for 7 days.',                      // renameSpell
  3: 'The target siphons 5% of your mints for 3 days (cap 1000 PP).', // mintTaxSiphon
  4: 'You lose your deepest downline to the target.',    // downlineHeist
  5: 'Cannot backfire.',                                 // magicMirror
  6: 'Cannot backfire.',                                 // ppBoosterAura
  7: 'You burn 25-50% of your own PP (max 800).',        // purseCutter
  8: 'You pay each of the top 3 whales (caps at ~49% loss).', // whaleRebalance
  9: 'Cannot backfire.',                                 // downlineBoost
  10: 'Cannot backfire.',                                // goldenName
};

type FilterCategory = 'all' | 'offense' | 'defense' | 'chaos';

const offenseTypes = [0, 1, 3, 4, 7, 8]; // moneyTrickster, aoeSkim, mintTaxSiphon, downlineHeist, purseCutter, whaleRebalance
const defenseTypes = [5, 6, 9]; // magicMirror, ppBoosterAura, downlineBoost

function getShenaniganCategory(idx: number): FilterCategory {
  if (offenseTypes.includes(idx)) return 'offense';
  if (defenseTypes.includes(idx)) return 'defense';
  return 'chaos';
}

// Variant tags are objects like { success: null }; extract the single key.
const variantKey = (v: unknown): string =>
  v && typeof v === 'object' ? Object.keys(v as Record<string, unknown>)[0] ?? '' : '';

function LiveFeedRow({
  record,
  spellName,
  spellIcon,
}: {
  record: ShenaniganRecord;
  spellName: string;
  spellIcon: React.ReactNode;
}) {
  const casterName = useDisplayName(record.user);
  const target = record.target[0] ?? null;
  const targetName = useDisplayName(target);
  const outcomeKey = variantKey(record.outcome);
  const outcomeColor =
    outcomeKey === 'success' ? 'mc-text-green' :
    outcomeKey === 'fail' ? 'mc-text-danger' :
    'mc-text-purple';
  return (
    <div className="mc-card p-2 text-xs space-y-1">
      <div className="flex items-center justify-between gap-2">
        <span className="font-bold mc-text-primary truncate">{casterName || 'Anon'}</span>
        <span className={`font-bold flex-shrink-0 ${outcomeColor}`}>{outcomeKey.toUpperCase()}</span>
      </div>
      <div className="mc-text-dim flex items-center gap-1 min-w-0">
        <span className="flex-shrink-0">{spellIcon}</span>
        <span className="truncate">{spellName}</span>
        {target ? (
          <span className="mc-text-muted truncate"> → {targetName}</span>
        ) : null}
      </div>
    </div>
  );
}

function OutcomeTargetName({ principalText }: { principalText: string }) {
  const principal = Principal.fromText(principalText);
  const name = useDisplayName(principal);
  return <>{name || 'them'}</>;
}

export default function Shenanigans() {
  const { data: stats, isLoading: statsLoading } = useGetShenaniganStats();
  const { data: recentShenanigans, isLoading: recentLoading } = useGetRecentShenanigans();
  const { data: ponziData, isLoading: ponziLoading } = useGetPonziPoints();
  const { data: backendConfigs, isLoading: configsLoading } = useGetShenaniganConfigs();
  const castShenanigan = useCastShenanigan();
  const successFlavor = useSpellFlavorPool('spellFlavor.success');
  const failFlavor = useSpellFlavorPool('spellFlavor.fail');
  const backfireFlavor = useSpellFlavorPool('spellFlavor.backfire');
  const [filterCategory, setFilterCategory] = useState<FilterCategory>('all');
  const [viewMode, setViewMode] = useState<'cards' | 'compact'>('cards');
  const [animatingTrick, setAnimatingTrick] = useState<string | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [targetPickerOpen, setTargetPickerOpen] = useState(false);
  const [selectedShenanigan, setSelectedShenanigan] = useState<{ id: number; type: ShenaniganType; name: string; cost: number; icon: React.ReactNode } | null>(null);
  const [selectedTarget, setSelectedTarget] = useState<Principal | null>(null);
  const [outcomeToast, setOutcomeToast] = useState<{
    name: string;
    outcome: string;
    flavor: string;
    cost: number;
    spellId?: number;                  // 0-10 per shenaniganTypes; drives per-spell copy
    ppDelta?: number;                  // PP units / 10^8 — display-ready number
    targetPrincipalText?: string | null;
    affectedCount?: number;
  } | null>(null);
  const [renamePrompt, setRenamePrompt] = useState<{ targetPrincipal: string } | null>(null);
  const [renameInput, setRenameInput] = useState('');
  const setRenameName = useSetPendingRenameName();
  const cancelRenameName = useCancelPendingRename();
  const { data: pendingRename } = useGetPendingRenameForCaller();
  const [availableShenanigans, setAvailableShenanigans] = useState<ShenaniganConfig[]>([]);

  // If the user cast Rename, navigated away, then came back within 5 minutes,
  // reopen the modal so they can still pick the name. Only triggers when the
  // backend reports a non-null pending slot AND no modal is currently open.
  useEffect(() => {
    if (pendingRename && !renamePrompt) {
      setRenamePrompt({ targetPrincipal: pendingRename.target.toText() });
    }
  }, [pendingRename, renamePrompt]);

  useEffect(() => {
    if (backendConfigs) {
      setAvailableShenanigans(backendConfigs.map(config => {
        const id = Number(config.id);
        return {
          id, type: shenaniganTypes[id], name: config.name, icon: shenaniganIcons[id],
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

  const handleCastClick = (id: number, type: ShenaniganType, cost: number, name: string, icon: React.ReactNode) => {
    if ((ponziData?.totalPoints || 0) < cost) {
      setOutcomeToast({
        name,
        outcome: 'error',
        flavor: `Insufficient PP. Need ${cost}, have ${(ponziData?.totalPoints || 0).toLocaleString()}.`,
        cost: 0,
      });
      return;
    }
    setSelectedShenanigan({ id, type, name, cost, icon });
    setSelectedTarget(null);
    if (TARGETED_SPELL_IDS.has(id)) {
      setTargetPickerOpen(true);
    } else {
      setConfirmOpen(true);
    }
  };

  const handleTargetSelected = (target: Principal) => {
    setSelectedTarget(target);
    setTargetPickerOpen(false);
    setConfirmOpen(true);
  };

  const handleTargetCancel = () => {
    setTargetPickerOpen(false);
    setSelectedShenanigan(null);
  };

  const getFlavorText = (outcome: string) => {
    const pool = outcome === 'success' ? successFlavor : outcome === 'fail' ? failFlavor : backfireFlavor;
    return pool[Math.floor(Math.random() * pool.length)];
  };

  const handleConfirmCast = async () => {
    if (!selectedShenanigan) return;
    setConfirmOpen(false);
    setAnimatingTrick(variantKey(selectedShenanigan.type));
    try {
      const detail = await castShenanigan.mutateAsync({ shenaniganType: selectedShenanigan.type, target: selectedTarget });
      const outcome = variantKey(detail.outcome);
      setTimeout(() => {
        const isRenameSuccess = outcome === 'success' && selectedShenanigan.id === 2 /* renameSpell */;
        const targetPrincipalText = detail.affectedTarget && detail.affectedTarget.length > 0
          ? detail.affectedTarget[0]?.toText() ?? null
          : null;
        if (isRenameSuccess && targetPrincipalText) {
          // Skip the success toast — the rename modal IS the success
          // affirmation, and otherwise the toast would sit hidden behind
          // the rename modal's backdrop.
          setRenamePrompt({ targetPrincipal: targetPrincipalText });
        } else {
          setOutcomeToast({
            name: selectedShenanigan.name,
            outcome,
            flavor: getFlavorText(outcome),
            cost: selectedShenanigan.cost,
            spellId: selectedShenanigan.id,
            ppDelta: Number(detail.ppDeltaCaster) / 100_000_000,
            targetPrincipalText,
            affectedCount: Number(detail.affectedCount),
          });
        }
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
      const key = variantKey(s.shenaniganType);
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
          {/* Filter tabs + view toggle */}
          <div className="flex items-center gap-2">
            <div className="flex flex-1 rounded-lg bg-white/5 p-0.5">
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
            <div className="flex items-center gap-1 ml-auto">
              <button
                type="button"
                onClick={() => setViewMode('cards')}
                className={viewMode === 'cards' ? 'mc-bg-elev-2 rounded p-1' : 'p-1 opacity-60 hover:opacity-100'}
                aria-label="Card view"
              >
                <LayoutGrid className="w-4 h-4" />
              </button>
              <button
                type="button"
                onClick={() => setViewMode('compact')}
                className={viewMode === 'compact' ? 'mc-bg-elev-2 rounded p-1' : 'p-1 opacity-60 hover:opacity-100'}
                aria-label="List view"
              >
                <List className="w-4 h-4" />
              </button>
            </div>
          </div>

          {/* Shenanigan cards grid / compact list */}
          {viewMode === 'cards' ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mc-stagger">
              {availableShenanigans.filter((_, idx) => filterCategory === 'all' || getShenaniganCategory(idx) === filterCategory).map((trick, idx) => {
                const trickKey = variantKey(trick.type);
                const isDisabled = castShenanigan.isPending || userPoints < trick.cost || animatingTrick === trickKey;
                return (
                  <div
                    key={`shenanigan-${idx}`}
                    className="mc-shenanigan-card"
                    style={{ '--aura-color': trick.auraColor } as React.CSSProperties}
                  >
                    {/* Popular badge */}
                    {mostPopularType !== null && trickKey === mostPopularType && (
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

                    {/* Mechanical effect */}
                    <div className="text-xs mc-text-muted mt-1 italic mb-1">
                      Effect: {trick.effects || 'see docs'}
                    </div>
                    <div className="text-xs mc-text-danger/80 italic mb-3">
                      Backfire: {backfireDescriptions[trick.id] ?? 'see docs'}
                    </div>

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
                      onClick={() => !isDisabled && handleCastClick(trick.id, trick.type, trick.cost, trick.name, trick.icon)}
                      disabled={isDisabled}
                      className={`w-full py-2 rounded-lg text-xs font-bold uppercase transition-all ${
                        isDisabled ? 'bg-white/5 text-white/30 cursor-not-allowed border border-white/5' : 'mc-btn-primary'
                      }`}
                    >
                      {animatingTrick === trickKey ? (
                        <><span className="inline-block animate-spin mr-2">🎲</span>Casting…</>
                      ) : userPoints < trick.cost ? `Need ${trick.cost} PP` : `Cast (${trick.cost} PP)`}
                    </button>
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="divide-y mc-border-subtle">
              {availableShenanigans.filter((_, idx) => filterCategory === 'all' || getShenaniganCategory(idx) === filterCategory).map((trick, idx) => {
                const trickKey = variantKey(trick.type);
                const isDisabled = castShenanigan.isPending || userPoints < trick.cost || animatingTrick === trickKey;
                return (
                  <div key={`compact-${idx}`} className="py-2 flex items-center gap-3">
                    <span className="flex-1 font-medium mc-text-primary text-sm">{trick.name}</span>
                    <span className="text-xs mc-text-muted">{trick.cost} PP</span>
                    <span className="text-xs mc-text-dim">{trick.odds.success}% win</span>
                    <button
                      onClick={() => !isDisabled && handleCastClick(trick.id, trick.type, trick.cost, trick.name, trick.icon)}
                      disabled={isDisabled}
                      className={`text-xs font-bold px-3 py-1 rounded-lg transition-all ${
                        isDisabled ? 'bg-white/5 text-white/30 cursor-not-allowed border border-white/5' : 'mc-btn-primary'
                      }`}
                    >
                      {animatingTrick === trickKey ? (
                        <><span className="inline-block animate-spin mr-1">🎲</span>Casting…</>
                      ) : 'Cast'}
                    </button>
                  </div>
                );
              })}
            </div>
          )}

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
                <span><strong className="mc-text-primary">Zero Floor</strong> — No player goes below 0 PP</span>
              </div>
              <div className="flex items-start gap-2">
                <Zap className="h-3 w-3 mc-text-purple mt-0.5 flex-shrink-0" />
                <span><strong className="mc-text-primary">Cooldowns</strong> — 2-min global cooldown, 3-min per-target cooldown</span>
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
                recentShenanigans.slice(0, 20).map(s => {
                  const config = availableShenanigans.find(a => variantKey(a.type) === variantKey(s.shenaniganType));
                  return (
                    <LiveFeedRow
                      key={s.id.toString()}
                      record={s}
                      spellName={config?.name ?? 'Unknown'}
                      spellIcon={config?.icon ?? null}
                    />
                  );
                })
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
        <>
          <div className="mc-modal-backdrop" onClick={() => setOutcomeToast(null)} aria-hidden="true" />
          <div className="fixed top-28 md:top-36 left-1/2 -translate-x-1/2 z-[9999]" role="dialog" aria-modal="true">
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
              <p className="font-accent text-xs mc-text-dim italic mb-2">
                {outcomeToast.flavor}
              </p>
              {(() => {
                const d = outcomeToast.ppDelta ?? 0;
                const cnt = outcomeToast.affectedCount ?? 0;
                const id = outcomeToast.spellId;
                const target = outcomeToast.targetPrincipalText
                  ? <OutcomeTargetName principalText={outcomeToast.targetPrincipalText} />
                  : 'them';
                // Per-spell IDs (mirror shenaniganTypes array order in main.mo):
                // 0 moneyTrickster, 1 aoeSkim, 2 renameSpell, 3 mintTaxSiphon,
                // 4 downlineHeist, 5 magicMirror, 6 ppBoosterAura, 7 purseCutter,
                // 8 whaleRebalance, 9 downlineBoost, 10 goldenName

                if (outcomeToast.outcome === 'success') {
                  // Numeric wins first (Money Trickster, AoE Skim, Whale Rebalance theft).
                  if (d > 0 && cnt === 1) return <p className="text-xs mc-text-green mb-3">Stole {Math.round(d)} PP from {target}.</p>;
                  if (d > 0 && cnt > 1)  return <p className="text-xs mc-text-green mb-3">Stole {Math.round(d)} PP from {cnt} players.</p>;
                  // Spells with no PP delta — explain what actually happened.
                  switch (id) {
                    case 3: // mintTaxSiphon
                      if (cnt === 1) return <p className="text-xs mc-text-green mb-3">{target} is now siphoned. You'll skim 5% of their next 1000 PP minted (over 7 days).</p>;
                      return <p className="text-xs mc-text-green mb-3">{target} was shielded. No siphon.</p>;
                    case 4: // downlineHeist
                      if (cnt === 1) return <p className="text-xs mc-text-green mb-3">Stole a downline member from {target}.</p>;
                      return <p className="text-xs mc-text-green mb-3">{target} had no downline to steal.</p>;
                    case 5: // magicMirror
                      return <p className="text-xs mc-text-green mb-3">Shield up. Next hostile spell aimed at you gets blocked.</p>;
                    case 6: // ppBoosterAura
                      return <p className="text-xs mc-text-green mb-3">PP booster active for the rest of the round.</p>;
                    case 7: // purseCutter
                      if (cnt === 1) return <p className="text-xs mc-text-green mb-3">Burned {target}'s PP.</p>;
                      return <p className="text-xs mc-text-green mb-3">{target} was shielded. Purse intact.</p>;
                    case 9: // downlineBoost
                      return <p className="text-xs mc-text-green mb-3">Your referral cascade pays 1.3× for 24 hours.</p>;
                    case 10: // goldenName
                      return <p className="text-xs mc-text-green mb-3">You're golden — name glows on the leaderboard.</p>;
                    case 0: // moneyTrickster — no theft, shielded target (success path)
                      return <p className="text-xs mc-text-green mb-3">{target} was shielded. No PP stolen.</p>;
                    case 1: // aoeSkim — all victims shielded
                    case 8: // whaleRebalance — all whales shielded
                      return <p className="text-xs mc-text-green mb-3">Every target was shielded. Nothing skimmed.</p>;
                    default:
                      return <p className="text-xs mc-text-green mb-3">It worked.</p>;
                  }
                }

                if (outcomeToast.outcome === 'backfire') {
                  // Numeric losses first.
                  if (d < 0 && cnt === 1)  return <p className="text-xs mc-text-purple mb-3">Paid {Math.abs(Math.round(d))} PP to {target}.</p>;
                  if (d < 0 && cnt > 1)   return <p className="text-xs mc-text-purple mb-3">Paid {Math.abs(Math.round(d))} PP to {cnt} whales.</p>;
                  if (d < 0 && cnt === 0) return <p className="text-xs mc-text-purple mb-3">You burned {Math.abs(Math.round(d))} PP.</p>;
                  // State-change backfires with no PP delta — name them.
                  switch (id) {
                    case 2: // renameSpell backfire — caster gets renamed
                      return <p className="text-xs mc-text-purple mb-3">You got renamed for 7 days.</p>;
                    case 3: // mintTaxSiphon backfire — target becomes the siphoner
                      return <p className="text-xs mc-text-purple mb-3">{target} now siphons 5% of YOUR mints for 3 days.</p>;
                    case 4: // downlineHeist backfire — caster loses a downline
                      if (cnt === 1) return <p className="text-xs mc-text-purple mb-3">{target} stole a downline member from you.</p>;
                      return <p className="text-xs mc-text-purple mb-3">Backfired — but you had no downline to lose.</p>;
                    default:
                      return <p className="text-xs mc-text-purple mb-3">Backfired — but no observable effect.</p>;
                  }
                }

                if (outcomeToast.outcome === 'fail') {
                  return <p className="text-xs mc-text-muted mb-3">Nothing happened. The PP is still gone.</p>;
                }
                return null;
              })()}
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
        </>
      )}

      {/* Rename Spell — name picker modal */}
      {renamePrompt && (
        <>
          <div className="mc-modal-backdrop" aria-hidden="true" />
          <div className="fixed top-28 md:top-36 left-1/2 -translate-x-1/2 z-[9999]" role="dialog" aria-modal="true">
            <div className="mc-toast text-center max-w-sm">
              <div className="font-display text-xl mc-text-primary mb-2">
                Name them.
              </div>
              <p className="text-sm mc-text-dim mb-3">
                You have 5 minutes. 1-32 characters. Letters, numbers, space, dash, underscore.
              </p>
              <input
                type="text"
                value={renameInput}
                onChange={(e) => setRenameInput(e.target.value)}
                maxLength={32}
                autoFocus
                className="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-sm mc-text-primary mb-3"
                placeholder="e.g., Liquidation Larry"
              />
              <div className="flex gap-3 justify-center">
                <button
                  onClick={async () => {
                    setRenamePrompt(null);
                    setRenameInput('');
                    // Tell the backend to clear the slot so it doesn't
                    // re-trigger the mount-time prompt. Best-effort —
                    // ignore network errors; the slot lapses in 5 min anyway.
                    try { await cancelRenameName.mutateAsync(); } catch {}
                  }}
                  disabled={cancelRenameName.isPending}
                  className="mc-btn-secondary px-5 py-2 rounded-full text-sm"
                >
                  Skip (no rename)
                </button>
                <button
                  onClick={async () => {
                    try {
                      await setRenameName.mutateAsync(renameInput);
                      setRenamePrompt(null);
                      setRenameInput('');
                    } catch (e: any) {
                      alert(e.message || 'Rename failed');
                    }
                  }}
                  disabled={renameInput.trim().length === 0 || setRenameName.isPending}
                  className="mc-btn-primary px-5 py-2 rounded-full text-sm"
                >
                  {setRenameName.isPending ? 'Committing…' : 'Lock it in'}
                </button>
              </div>
            </div>
          </div>
        </>
      )}

      {/* Target picker */}
      {selectedShenanigan && (
        <TargetPicker
          open={targetPickerOpen}
          spellName={selectedShenanigan.name}
          onSelect={handleTargetSelected}
          onCancel={handleTargetCancel}
        />
      )}

      {/* Confirm dialog */}
      {confirmOpen && selectedShenanigan && (
        <>
          <div className="mc-modal-backdrop" onClick={() => setConfirmOpen(false)} aria-hidden="true" />
          <div className="fixed top-28 md:top-36 left-1/2 -translate-x-1/2 z-[9999]" role="dialog" aria-modal="true">
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
        </>
      )}
    </div>
  );
}
