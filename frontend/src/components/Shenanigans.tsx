import React, { useState, useEffect, useMemo, useRef } from 'react';
import { toast } from 'sonner';
import { useQueryClient } from '@tanstack/react-query';
import { Principal } from '@dfinity/principal';
import { useCastShenanigan, useGetShenaniganStats, useGetRecentShenanigans, useGetPonziPoints, useGetShenaniganConfigs, useSetPendingRenameName, useGetPendingRenameForCaller, useCancelPendingRename, useGetSpellCooldowns, useGetActiveSpellEffects } from '../hooks/useQueries';
import { renderTemplate } from '../lib/renderTemplate';
import { useSpellFlavorPool } from './trollbox/useSpellFlavorPool';
import LoadingSpinner from './LoadingSpinner';
import { ShenaniganType, ShenaniganRecord } from '../backend';
import { Shield, Coins, Waves, Pencil, Building2, Target, FlipHorizontal2, ArrowUp, Scissors, Fish, TrendingUp, Sparkles, Dices, LayoutGrid, List } from 'lucide-react';
import HallOfFameRail from './hall-of-fame/HallOfFameRail';
import HallOfFameMobileBlock from './hall-of-fame/HallOfFameMobileBlock';
import LiveFeedPanel from './Shenanigans/LiveFeedPanel';
import GuardrailsTooltip from './Shenanigans/GuardrailsTooltip';
import TargetPicker from './TargetPicker';
import WhitelistedFanfare from './WhitelistedFanfare';
import { useDisplayName, useIsGolden } from './trollbox/useDisplayName';
import GoldenName from './GoldenName';

// Spell ids that REQUIRE a target. Mirrors the trap in shenanigans/main.mo
// castShenanigan — backend rejects null target for these.
const TARGETED_SPELL_IDS = new Set([0, 2, 3, 4, 7]); // moneyTrickster, renameSpell, mintTaxSiphon, downlineHeist, purseCutter

// Poison Pill (magicMirror) shield charge ceiling. Mirrors the hardcoded
// cap at shenanigans/main.mo:2178 — keep in sync until that cap is promoted
// to a config field (tracked in TUNING_NOTES → "Promote the Poison Pill
// charge cap to a tunable"). Used to gray out the Cast button when the
// player's shield is already at max so they don't burn PP for a silent
// no-op (the backend still charges them; only the charge count is capped).
const POISON_PILL_ID = 5;
const POISON_PILL_CHARGE_CAP = 3;


interface ShenaniganConfig {
  id: number;
  type: ShenaniganType;
  name: string;
  icon: React.ReactNode;
  costSuccess: number;
  costFailure: number;
  costBackfire: number;
  description: string;
  /// Admin-editable backfire copy (templated). Null = use hardcoded fallback.
  backfireDescription: string | null;
  /// Raw effect values used by the templater (numeric percentages, caps, etc.).
  effectValues: number[];
  /// Duration in hours for templating {dur_h}/{dur_d} placeholders.
  durationHours: number;
  odds: { success: number; fail: number; backfire: number };
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

function getShenaniganCategory(idx: number): FilterCategory {
  if (offenseTypes.includes(idx)) return 'offense';
  if (defenseTypes.includes(idx)) return 'defense';
  return 'chaos';
}

// Variant tags are objects like { success: null }; extract the single key.
const variantKey = (v: unknown): string =>
  v && typeof v === 'object' ? Object.keys(v as Record<string, unknown>)[0] ?? '' : '';

function OutcomeTargetName({ principalText }: { principalText: string }) {
  const principal = Principal.fromText(principalText);
  const name = useDisplayName(principal);
  const isGolden = useIsGolden(principal);
  if (isGolden) return <GoldenName name={name || 'them'} isGolden={true} />;
  return <>{name || 'them'}</>;
}

// Compact "Xh Ym" / "Xm" / "<1m" formatter for active-effect expiries.
// Always shows a value while the effect is live; never negative.
function formatRemaining(expiresAtNs: bigint): string {
  const nowMs = Date.now();
  const expiresMs = Number(expiresAtNs) / 1_000_000;
  const remainingMs = expiresMs - nowMs;
  if (remainingMs <= 0) return 'expiring';
  const totalMin = Math.ceil(remainingMs / 60_000);
  if (totalMin < 60) return `${totalMin}m`;
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  return m === 0 ? `${h}h` : `${h}h ${m}m`;
}

// Strip of badges shown above the spell grid. Surfaces every live buff or
// debuff returned by getActiveSpellEffects so a player can see at a glance
// what's protecting them (Shield), helping them (Yield Boost, Override Bonus,
// Whitelisted), or being done to them (Renamed, Siphoned). Returns null
// (renders nothing) when no effects are active — no empty placeholder.
function ActiveEffectsStrip({ effects }: { effects: import('../backend').ActiveSpellEffects | null }) {
  if (!effects) return null;

  type Badge = {
    key: string;
    icon: React.ReactNode;
    label: string;
    tone: 'good' | 'bad' | 'neutral';
  };
  const badges: Badge[] = [];

  const shield = effects.shield[0];
  if (shield && shield.chargesRemaining > 0n) {
    badges.push({
      key: 'shield',
      icon: <Shield className="w-3.5 h-3.5" />,
      label: `Shield × ${Number(shield.chargesRemaining)} · ${formatRemaining(shield.expiresAt)}`,
      tone: 'good',
    });
  }

  const mult = effects.mintMultiplier[0];
  if (mult) {
    const pctBonus = (Number(mult.multiplierBps) - 10_000) / 100;
    badges.push({
      key: 'yield',
      icon: <ArrowUp className="w-3.5 h-3.5" />,
      label: `Yield Boost +${pctBonus.toFixed(0)}% · ${formatRemaining(mult.expiresAt)}`,
      tone: 'good',
    });
  }

  const boost = effects.cascadeBoost[0];
  if (boost) {
    const mx = Number(boost.multiplierBps) / 10_000;
    badges.push({
      key: 'override',
      icon: <TrendingUp className="w-3.5 h-3.5" />,
      label: `Override ${mx.toFixed(2)}× · ${formatRemaining(boost.expiresAt)}`,
      tone: 'good',
    });
  }

  if (effects.golden) {
    badges.push({
      key: 'golden',
      icon: <Sparkles className="w-3.5 h-3.5" />,
      label: 'Whitelisted',
      tone: 'good',
    });
  }

  const name = effects.displayName[0];
  if (name) {
    badges.push({
      key: 'renamed',
      icon: <Pencil className="w-3.5 h-3.5" />,
      label: `Renamed "${name.name}" · ${formatRemaining(name.expiresAt)}`,
      tone: 'bad',
    });
  }

  const siphon = effects.mintSiphon[0];
  if (siphon) {
    const pct = Number(siphon.pctTimes100) / 100;
    badges.push({
      key: 'siphoned',
      icon: <Building2 className="w-3.5 h-3.5" />,
      label: `Siphoned ${pct.toFixed(0)}% · ${formatRemaining(siphon.expiresAt)}`,
      tone: 'bad',
    });
  }

  if (badges.length === 0) return null;

  const toneClass = (tone: Badge['tone']) =>
    tone === 'good'
      ? 'bg-[var(--mc-green)]/15 mc-text-green border-[var(--mc-green)]/30'
      : tone === 'bad'
      ? 'bg-[var(--mc-danger)]/15 mc-text-danger border-[var(--mc-danger)]/30'
      : 'bg-white/5 mc-text-dim border-white/10';

  return (
    <div className="mc-card p-2">
      <div className="flex items-center gap-2 flex-wrap">
        <span className="mc-label flex-shrink-0">Active Effects:</span>
        {badges.map(b => (
          <span
            key={b.key}
            className={`text-xs px-2 py-1 rounded-full font-bold inline-flex items-center gap-1.5 border ${toneClass(b.tone)}`}
          >
            {b.icon}
            {b.label}
          </span>
        ))}
      </div>
    </div>
  );
}

export default function Shenanigans() {
  const { isLoading: statsLoading } = useGetShenaniganStats();
  const { data: recentShenanigans, isLoading: recentLoading } = useGetRecentShenanigans();
  const { data: ponziData, isLoading: ponziLoading } = useGetPonziPoints();
  const { data: backendConfigs, isLoading: configsLoading } = useGetShenaniganConfigs();
  const { data: cooldownsRaw } = useGetSpellCooldowns();
  const { data: activeEffects } = useGetActiveSpellEffects();
  const castShenanigan = useCastShenanigan();
  const queryClient = useQueryClient();
  const successFlavor = useSpellFlavorPool('spellFlavor.success');
  const failFlavor = useSpellFlavorPool('spellFlavor.fail');
  const backfireFlavor = useSpellFlavorPool('spellFlavor.backfire');
  const [filterCategory, setFilterCategory] = useState<FilterCategory>('all');
  const [viewMode, setViewMode] = useState<'cards' | 'compact'>('cards');
  const [animatingTrick, setAnimatingTrick] = useState<string | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [targetPickerOpen, setTargetPickerOpen] = useState(false);
  const [selectedShenanigan, setSelectedShenanigan] = useState<{ id: number; type: ShenaniganType; name: string; costSuccess: number; costFailure: number; costBackfire: number; icon: React.ReactNode } | null>(null);
  const [selectedTarget, setSelectedTarget] = useState<Principal | null>(null);
  const [whitelistedFanfareOpen, setWhitelistedFanfareOpen] = useState(false);
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

  // Toast when the player's Poison Pill shield absorbs an incoming attack.
  // Detects this by watching chargesRemaining via the 10s active-effects poll
  // — when charges drop between observations, infer absorption. We DON'T toast
  // when the shield naturally expired (previous expiresAt has passed by the
  // time we re-poll): that's not an attack-block, just the duration running
  // out. Edge case we accept: if both an absorption AND natural expiry happen
  // inside the same 10s poll window, the toast gets suppressed. Rare; can be
  // fixed by emitting a chat event from the backend in a follow-up.
  const prevShieldRef = useRef<{ charges: number; expiresMs: number } | null>(null);
  useEffect(() => {
    const cur = activeEffects?.shield[0] ?? null;
    const curCharges = cur ? Number(cur.chargesRemaining) : 0;
    const prev = prevShieldRef.current;
    if (prev && curCharges < prev.charges) {
      const naturallyExpired = !cur && prev.expiresMs <= Date.now();
      if (!naturallyExpired) {
        const absorbed = prev.charges - curCharges;
        const tail = curCharges > 0
          ? `${curCharges} charge${curCharges === 1 ? '' : 's'} left.`
          : 'Shield depleted.';
        toast.success(
          absorbed > 1
            ? `🛡 Your shield absorbed ${absorbed} attacks — ${tail}`
            : `🛡 Your shield absorbed an attack — ${tail}`,
          { duration: 5000 }
        );
      }
    }
    prevShieldRef.current = cur ? { charges: curCharges, expiresMs: Number(cur.expiresAt) / 1_000_000 } : null;
  }, [activeEffects]);

  useEffect(() => {
    if (backendConfigs) {
      setAvailableShenanigans(backendConfigs.map(config => {
        const id = Number(config.id);
        return {
          id, type: shenaniganTypes[id], name: config.name, icon: shenaniganIcons[id],
          costSuccess: config.costSuccess,
          costFailure: config.costFailure,
          costBackfire: config.costBackfire,
          description: config.description,
          backfireDescription: config.backfireDescription.length > 0 ? (config.backfireDescription[0] ?? null) : null,
          effectValues: config.effectValues,
          durationHours: Number(config.duration),
          odds: { success: Number(config.successOdds), fail: Number(config.failureOdds), backfire: Number(config.backfireOdds) },
          auraColor: auraColors[id] || auraColors[0],
        };
      }));
    }
  }, [backendConfigs]);

  // Admin-panel saves flow through React Query invalidation in the mutation
  // hooks → useGetShenaniganConfigs refetches → the useEffect above rebuilds
  // availableShenanigans with the fresh data. No CustomEvent shuttle needed
  // (the previous version overwrote unlisted fields with undefined, which
  // would have broken any new schema field added later).

  // Map spell id → expiry timestamp (ms since epoch). Spells not on
  // cooldown are absent. Memoized so card render doesn't churn on
  // unrelated state updates.
  const cooldownExpiresAt = useMemo(() => {
    const m = new Map<number, number>();
    (cooldownsRaw ?? []).forEach(([id, expiresNs]) => {
      m.set(Number(id), Number(expiresNs) / 1_000_000); // ns → ms
    });
    return m;
  }, [cooldownsRaw]);

  const handleCastClick = (
    id: number,
    type: ShenaniganType,
    costSuccess: number,
    costFailure: number,
    costBackfire: number,
    name: string,
    icon: React.ReactNode,
  ) => {
    // Pre-cast gate = costSuccess (the minimum the caster commits to paying).
    // A worse outcome may charge more; if they can't afford it the backend
    // clamps the burn to balance and zeros them out — no trap.
    if ((ponziData?.totalPoints || 0) < costSuccess) {
      setOutcomeToast({
        name,
        outcome: 'error',
        flavor: `Insufficient PP. Need ${costSuccess}, have ${(ponziData?.totalPoints || 0).toLocaleString()}.`,
        cost: 0,
      });
      return;
    }
    setSelectedShenanigan({ id, type, name, costSuccess, costFailure, costBackfire, icon });
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
      // A success just set a cooldown on the backend — refresh the
      // cooldown query so the spell card flips to "Cooldown — Xm" without
      // waiting for the 30s poll.
      if (outcome === 'success') {
        queryClient.invalidateQueries({ queryKey: ['spellCooldowns'] });
      }
      setTimeout(() => {
        const isRenameSuccess = outcome === 'success' && selectedShenanigan.id === 2 /* renameSpell */;
        const isWhitelistedSuccess = outcome === 'success' && selectedShenanigan.id === 10 /* goldenName */;
        const targetPrincipalText = detail.affectedTarget && detail.affectedTarget.length > 0
          ? detail.affectedTarget[0]?.toText() ?? null
          : null;
        if (isRenameSuccess && targetPrincipalText) {
          // Skip the success toast — the rename modal IS the success
          // affirmation, and otherwise the toast would sit hidden behind
          // the rename modal's backdrop.
          setRenamePrompt({ targetPrincipal: targetPrincipalText });
        } else if (isWhitelistedSuccess) {
          // Skip the success toast — the fanfare card IS the affirmation,
          // and stacking the small green toast under a confetti overlay
          // looks ridiculous. Failure / backfire still go through the
          // normal toast below.
          setWhitelistedFanfareOpen(true);
        } else {
          // Pick the cost matching the rolled outcome. (If the caster's
          // balance was below this, the backend clamped to balance — the
          // toast still reports the nominal cost for the outcome; the
          // actual debited amount is whatever they had.)
          const costForOutcome =
            outcome === 'success' ? selectedShenanigan.costSuccess
            : outcome === 'fail' ? selectedShenanigan.costFailure
            : selectedShenanigan.costBackfire;
          setOutcomeToast({
            name: selectedShenanigan.name,
            outcome,
            flavor: getFlavorText(outcome),
            cost: costForOutcome,
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
        // Cost on error path is unknown (trap before outcome roll) — show
        // the upfront commitment so the toast still has a number.
        cost: selectedShenanigan.costSuccess,
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
      {/* Mobile-only: Hall of Fame at top of page. Hidden on lg+. */}
      <div className="block lg:hidden">
        <HallOfFameMobileBlock />
      </div>

      <ActiveEffectsStrip effects={activeEffects ?? null} />

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
              <GuardrailsTooltip />
            </div>
          </div>

          {/* Shenanigan cards grid / compact list */}
          {viewMode === 'cards' ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mc-stagger">
              {availableShenanigans.filter((_, idx) => filterCategory === 'all' || getShenaniganCategory(idx) === filterCategory).map((trick, idx) => {
                const trickKey = variantKey(trick.type);
                const cooldownExpiresMs = cooldownExpiresAt.get(trick.id) ?? 0;
                const onCooldown = cooldownExpiresMs > Date.now();
                const minutesLeft = onCooldown ? Math.ceil((cooldownExpiresMs - Date.now()) / 60_000) : 0;
                // Shield-aware state for the Poison Pill card only. Charges
                // already at cap = casting is a silent no-op on charges (still
                // costs PP, just refreshes expiry), so we gray the button.
                const shield = trick.id === POISON_PILL_ID ? activeEffects?.shield[0] ?? null : null;
                const shieldCharges = shield ? Number(shield.chargesRemaining) : 0;
                const shieldFull = trick.id === POISON_PILL_ID && shieldCharges >= POISON_PILL_CHARGE_CAP;
                const isDisabled = castShenanigan.isPending || userPoints < trick.costSuccess || animatingTrick === trickKey || onCooldown || shieldFull;
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

                    {/* Title + per-outcome costs (success/fail/backfire). Pre-cast
                        gate is costSuccess — the upfront commitment. */}
                    <h3 className="font-display text-sm mc-text-primary text-center mb-1">{trick.name}</h3>
                    <div className="text-center mb-3 flex items-center justify-center gap-2 flex-wrap">
                      <span
                        className="text-xs px-2 py-0.5 rounded-full font-bold bg-[var(--mc-purple)]/20 mc-text-purple"
                        title="Cost on success / failure / backfire"
                      >
                        {trick.costSuccess}/{trick.costFailure}/{trick.costBackfire} PP
                      </span>
                      {/* Shield charge badge — Poison Pill only, when shield is live. */}
                      {trick.id === POISON_PILL_ID && shield && shieldCharges > 0 && (
                        <span
                          className="text-xs px-2 py-0.5 rounded-full font-bold bg-[var(--mc-green)]/15 mc-text-green border border-[var(--mc-green)]/30 inline-flex items-center gap-1"
                          title={`Shield active — ${shieldCharges} charge${shieldCharges === 1 ? '' : 's'} remaining, expires in ${formatRemaining(shield.expiresAt)}`}
                        >
                          <Shield className="w-3 h-3" />
                          {shieldCharges}/{POISON_PILL_CHARGE_CAP} · {formatRemaining(shield.expiresAt)}
                        </span>
                      )}
                    </div>

                    {/* Description (rendered through templater so admin
                        edits to effectValues flow into the copy). */}
                    <p className="text-xs mc-text-dim leading-relaxed mb-3">
                      {renderTemplate(trick.description, trick.effectValues, trick.durationHours)}
                    </p>

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

                    {/* Cast button — gate is costSuccess (the upfront
                        commitment). A worse outcome may charge up to
                        costBackfire; if balance is short the backend clamps
                        the burn and zeros the caster. */}
                    <button
                      onClick={() => !isDisabled && handleCastClick(trick.id, trick.type, trick.costSuccess, trick.costFailure, trick.costBackfire, trick.name, trick.icon)}
                      disabled={isDisabled}
                      className={`w-full py-2 rounded-lg text-xs font-bold uppercase transition-all ${
                        isDisabled ? 'bg-white/5 text-white/30 cursor-not-allowed border border-white/5' : 'mc-btn-primary'
                      }`}
                    >
                      {animatingTrick === trickKey ? (
                        <><span className="inline-block animate-spin mr-2">🎲</span>Casting…</>
                      ) : shieldFull ? 'Shield full'
                        : onCooldown ? `Cooldown — ${minutesLeft}m`
                        : userPoints < trick.costSuccess ? `Need ${trick.costSuccess} PP`
                        : `Cast (${trick.costSuccess} PP)`}
                    </button>
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="divide-y mc-border-subtle">
              {availableShenanigans.filter((_, idx) => filterCategory === 'all' || getShenaniganCategory(idx) === filterCategory).map((trick, idx) => {
                const trickKey = variantKey(trick.type);
                const cooldownExpiresMs = cooldownExpiresAt.get(trick.id) ?? 0;
                const onCooldown = cooldownExpiresMs > Date.now();
                const minutesLeft = onCooldown ? Math.ceil((cooldownExpiresMs - Date.now()) / 60_000) : 0;
                const shield = trick.id === POISON_PILL_ID ? activeEffects?.shield[0] ?? null : null;
                const shieldCharges = shield ? Number(shield.chargesRemaining) : 0;
                const shieldFull = trick.id === POISON_PILL_ID && shieldCharges >= POISON_PILL_CHARGE_CAP;
                const isDisabled = castShenanigan.isPending || userPoints < trick.costSuccess || animatingTrick === trickKey || onCooldown || shieldFull;
                return (
                  <div key={`compact-${idx}`} className="py-2 flex items-center gap-3">
                    <span className="flex-1 font-medium mc-text-primary text-sm">{trick.name}</span>
                    {trick.id === POISON_PILL_ID && shield && shieldCharges > 0 && (
                      <span
                        className="text-xs mc-text-green inline-flex items-center gap-1"
                        title={`Shield active — ${shieldCharges} charge${shieldCharges === 1 ? '' : 's'} remaining`}
                      >
                        <Shield className="w-3 h-3" />
                        {shieldCharges}/{POISON_PILL_CHARGE_CAP}
                      </span>
                    )}
                    <span className="text-xs mc-text-muted" title="Cost on success / failure / backfire">
                      {trick.costSuccess}/{trick.costFailure}/{trick.costBackfire} PP
                    </span>
                    <span className="text-xs mc-text-dim">{trick.odds.success}% win</span>
                    <button
                      onClick={() => !isDisabled && handleCastClick(trick.id, trick.type, trick.costSuccess, trick.costFailure, trick.costBackfire, trick.name, trick.icon)}
                      disabled={isDisabled}
                      className={`text-xs font-bold px-3 py-1 rounded-lg transition-all ${
                        isDisabled ? 'bg-white/5 text-white/30 cursor-not-allowed border border-white/5' : 'mc-btn-primary'
                      }`}
                    >
                      {animatingTrick === trickKey ? (
                        <><span className="inline-block animate-spin mr-1">🎲</span>Casting…</>
                      ) : shieldFull ? 'Full'
                        : onCooldown ? `${minutesLeft}m` : 'Cast'}
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

        </div>

        {/* Right column (desktop): HoF rail + Live Feed — sticky via .mc-shenanigans-sidebar */}
        <div className="mc-shenanigans-sidebar space-y-4">
          <HallOfFameRail />
          <LiveFeedPanel
            records={recentShenanigans ?? []}
            resolveSpell={(s) => {
              const config = availableShenanigans.find(a => variantKey(a.type) === variantKey(s.shenaniganType));
              return { name: config?.name ?? 'Unknown', icon: config?.icon ?? null };
            }}
          />
        </div>
      </div>

      {/* Mobile-only Live Feed: collapsed by default. Hidden on lg+. */}
      <div className="block lg:hidden">
        <LiveFeedPanel
          records={recentShenanigans ?? []}
          resolveSpell={(s) => {
            const config = availableShenanigans.find(a => variantKey(a.type) === variantKey(s.shenaniganType));
            return { name: config?.name ?? 'Unknown', icon: config?.icon ?? null };
          }}
          defaultCollapsed
        />
      </div>

      {/* Compact footer */}
      <div className="text-center text-xs mc-text-muted mt-2">
        PP &amp; cosmetics only · pure entertainment · no refunds
      </div>

      <WhitelistedFanfare
        open={whitelistedFanfareOpen}
        onClose={() => setWhitelistedFanfareOpen(false)}
      />

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
                      return <p className="text-xs mc-text-green mb-3">Your referral cascade pays 1.3× for the rest of the round.</p>;
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
                Costs <span className="mc-toast-accent">{selectedShenanigan.costSuccess} PP</span> on success,{' '}
                <span className="mc-toast-accent">{selectedShenanigan.costFailure} PP</span> on failure,{' '}
                <span className="mc-toast-accent">{selectedShenanigan.costBackfire} PP</span> on backfire.
              </p>
              <p className="text-xs mc-text-muted mb-4">Outcome is random. No refunds. (If a backfire exceeds your balance, you zero out.)</p>
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
