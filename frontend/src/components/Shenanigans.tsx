import React, { useState, useEffect, useMemo, useRef } from 'react';
import { toast } from 'sonner';
import { useQueryClient } from '@tanstack/react-query';
import { Principal } from '@dfinity/principal';
import { useCastShenanigan, useGetShenaniganStats, useGetRecentShenanigans, useGetPonziPoints, useGetShenaniganConfigs, useSetPendingRenameName, useGetPendingRenameForCaller, useCancelPendingRename, useRerollPendingRename, useGetSpellCooldowns, useGetActiveSpellEffects, useSetCustomTitle, useGetPendingCustomTitleForCaller, useGetConfettiCannonStatus } from '../hooks/useQueries';
import { renderTemplate } from '../lib/renderTemplate';
import { prettifyCanisterError, ErrorKind } from '../lib/errorMessages';
import { useSpellFlavorPool } from './trollbox/useSpellFlavorPool';
import LoadingSpinner from './LoadingSpinner';
import { ShenaniganType, ShenaniganRecord } from '../backend';
import { Shield, Coins, Waves, Pencil, Building2, Target, FlipHorizontal2, ArrowUp, Scissors, Fish, TrendingUp, TrendingDown, Sparkles, Dices, DollarSign, LayoutGrid, List, Briefcase, Crown, Gift, Lightbulb, Megaphone, BadgeCheck, Quote, PartyPopper } from 'lucide-react';
import HallOfFameRail from './hall-of-fame/HallOfFameRail';
import HallOfFameMobileBlock from './hall-of-fame/HallOfFameMobileBlock';
import LiveFeedPanel from './Shenanigans/LiveFeedPanel';
import BuyPPWidget from './Shenanigans/BuyPPWidget';
import BuyPPFab from './Shenanigans/BuyPPFab';
import GuardrailsTooltip from './Shenanigans/GuardrailsTooltip';
import TargetPicker from './TargetPicker';
import WhitelistedFanfare from './WhitelistedFanfare';
import { useDisplayName, useIsGolden } from './trollbox/useDisplayName';
import GoldenName from './GoldenName';
import { triggerConfetti } from './ConfettiCanvas';
import { useWallet } from '../hooks/useWallet';

// Spell ids that REQUIRE a target. Mirrors the trap in shenanigans/main.mo
// castShenanigan — backend rejects null target for these.
const TARGETED_SPELL_IDS = new Set([0, 2, 3, 4, 7, 11, 16, 17]); // moneyTrickster, renameSpell, mintTaxSiphon, downlineHeist, purseCutter, tenderOffer, slushFund, insiderTip

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
  9: <TrendingUp className="h-5 w-5" />, 10: <Sparkles className="h-5 w-5" />, 11: <Dices className="h-5 w-5" />,
  12: <DollarSign className="h-5 w-5" />, 13: <TrendingDown className="h-5 w-5" />,
  14: <Briefcase className="h-5 w-5" />, 15: <Crown className="h-5 w-5" />,
  16: <Gift className="h-5 w-5" />, 17: <Lightbulb className="h-5 w-5" />,
  18: <Megaphone className="h-5 w-5" />, 19: <BadgeCheck className="h-5 w-5" />,
  20: <Quote className="h-5 w-5" />, 21: <PartyPopper className="h-5 w-5" />,
};

const shenaniganTypes: ShenaniganType[] = [
  ShenaniganType.moneyTrickster, ShenaniganType.aoeSkim, ShenaniganType.renameSpell,
  ShenaniganType.mintTaxSiphon, ShenaniganType.downlineHeist, ShenaniganType.magicMirror,
  ShenaniganType.ppBoosterAura, ShenaniganType.purseCutter, ShenaniganType.whaleRebalance,
  ShenaniganType.downlineBoost, ShenaniganType.goldenName, ShenaniganType.tenderOffer,
  ShenaniganType.stimulusCheck, ShenaniganType.bearRaid,
  ShenaniganType.foundersRound, ShenaniganType.strategicReserve,
  ShenaniganType.slushFund, ShenaniganType.insiderTip,
  ShenaniganType.voiceOfGod, ShenaniganType.customTitle,
  ShenaniganType.echo, ShenaniganType.confettiCannon,
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
  11: 'rgba(255, 100, 50, 0.3)',
  12: 'rgba(80, 200, 120, 0.3)',
  13: 'rgba(220, 50, 90, 0.3)',
  14: 'rgba(160, 160, 255, 0.3)',   // light blue-purple (Founder's Round #e6e6ff)
  15: 'rgba(180, 120, 255, 0.3)',   // light purple (Strategic Reserve #e6d4ff)
  16: 'rgba(100, 220, 100, 0.3)',   // light green (Slush Fund #e6ffd9)
  17: 'rgba(80, 230, 130, 0.3)',    // lighter green (Insider Tip #d9ffe6)
  18: 'rgba(255, 220, 100, 0.3)',   // warm gold (Voice of God #fff4d6)
  19: 'rgba(130, 130, 255, 0.3)',   // soft blue-purple (Custom Title #e6e6ff)
  20: 'rgba(200, 120, 255, 0.3)',   // soft purple (Echo #f0d6ff)
  21: 'rgba(255, 140, 160, 0.3)',   // soft pink (Confetti Cannon #ffe6ec)
};

type FilterCategory = 'all' | 'offense' | 'defense' | 'chaos';

const offenseTypes = [0, 1, 3, 4, 7, 8, 11, 13]; // moneyTrickster, aoeSkim, mintTaxSiphon, downlineHeist, purseCutter, whaleRebalance, tenderOffer, bearRaid
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
  const { principal } = useWallet();
  const callerPrincipal = principal ? (() => { try { return Principal.fromText(principal); } catch { return null; } })() : null;
  const { data: confettiCannonDeadlineNs } = useGetConfettiCannonStatus(callerPrincipal);
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
    errorKind?: ErrorKind;             // populated when outcome === 'error'; drives contextual CTAs
    errorRaw?: string;                 // raw error blob behind a Details disclosure
  } | null>(null);
  const [showErrorDetails, setShowErrorDetails] = useState(false);
  const [renamePrompt, setRenamePrompt] = useState<{ targetPrincipal: string } | null>(null);
  const [renameInput, setRenameInput] = useState('');
  const [renameInputError, setRenameInputError] = useState('');
  const setRenameName = useSetPendingRenameName();
  const cancelRenameName = useCancelPendingRename();
  const rerollRename = useRerollPendingRename();
  const { data: pendingRename } = useGetPendingRenameForCaller();
  const [customTitlePrompt, setCustomTitlePrompt] = useState(false);
  const [customTitleInput, setCustomTitleInput] = useState('');
  const [customTitleInputError, setCustomTitleInputError] = useState('');
  const setCustomTitleMutation = useSetCustomTitle();
  const { data: pendingCustomTitle } = useGetPendingCustomTitleForCaller();
  const [availableShenanigans, setAvailableShenanigans] = useState<ShenaniganConfig[]>([]);

  // If the user cast Rename, navigated away, then came back within 5 minutes,
  // reopen the modal so they can still pick the name. Only triggers when the
  // backend reports a non-null pending slot AND no modal is currently open.
  useEffect(() => {
    if (pendingRename && !renamePrompt) {
      setRenamePrompt({ targetPrincipal: pendingRename.target.toText() });
    }
  }, [pendingRename, renamePrompt]);

  // Same pattern for Custom Title — reopen if navigated away mid-window.
  useEffect(() => {
    if (pendingCustomTitle && !customTitlePrompt) {
      setCustomTitlePrompt(true);
    }
  }, [pendingCustomTitle, customTitlePrompt]);

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
    // Pre-cast gate = costSuccess against SIDE-POCKET balance (not total).
    // The side pocket is the spendable bucket for shenanigans; wallet PP
    // requires a deposit step first. If the side pocket is short but wallet
    // PP would cover it, the toast gets an insufficient_chips errorKind
    // which renders the "Deposit PP →" CTA — mirroring the runtime trap
    // path so the UX is consistent whether the gate catches it here or on
    // the backend. (errorKind name retained because backend trap message
    // still uses "chips" — see errorMessages.ts.)
    const chips = ponziData?.chipPoints || 0;
    const wallet = ponziData?.walletPoints || 0;
    if (chips < costSuccess) {
      const canTopUp = chips + wallet >= costSuccess;
      setOutcomeToast({
        name,
        outcome: 'error',
        flavor: canTopUp
          ? `Need ${costSuccess.toLocaleString()} in side pocket. You have ${chips.toLocaleString()} in side pocket and ${wallet.toLocaleString()} in wallet — deposit some to play.`
          : `Need ${costSuccess.toLocaleString()} PP. You have ${(chips + wallet).toLocaleString()} total.`,
        cost: 0,
        errorKind: canTopUp ? 'insufficient_chips' : undefined,
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
      const detail = await castShenanigan.mutateAsync({
        shenaniganType: selectedShenanigan.type,
        target: selectedTarget,
      });
      const outcome = variantKey(detail.outcome);
      // A success just set a cooldown on the backend — refresh the
      // cooldown query so the spell card flips to "Cooldown — Xm" without
      // waiting for the 30s poll.
      if (outcome === 'success') {
        queryClient.invalidateQueries({ queryKey: ['spellCooldowns'] });
        // Fire confetti if the caster has an active Confetti Cannon buff.
        const cannonActive = confettiCannonDeadlineNs !== null && confettiCannonDeadlineNs !== undefined
          && Number(confettiCannonDeadlineNs) / 1_000_000 > Date.now();
        if (cannonActive) triggerConfetti();
      }
      setTimeout(() => {
        const isRenameSuccess = outcome === 'success' && selectedShenanigan.id === 2 /* renameSpell */;
        const isWhitelistedSuccess = outcome === 'success' && selectedShenanigan.id === 10 /* goldenName */;
        const isCustomTitleSuccess = outcome === 'success' && selectedShenanigan.id === 19 /* customTitle */;
        const targetPrincipalText = detail.affectedTarget && detail.affectedTarget.length > 0
          ? detail.affectedTarget[0]?.toText() ?? null
          : null;
        if (isRenameSuccess && targetPrincipalText) {
          // Skip the success toast — the rename modal IS the success
          // affirmation, and otherwise the toast would sit hidden behind
          // the rename modal's backdrop.
          setRenameInput('');
          setRenameInputError('');
          setRenamePrompt({ targetPrincipal: targetPrincipalText });
        } else if (isCustomTitleSuccess) {
          // Skip the generic toast — the Custom Title modal is the success
          // affirmation. The player must commit their title within 5 minutes.
          setCustomTitleInput('');
          setCustomTitleInputError('');
          setCustomTitlePrompt(true);
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
    } catch (error: unknown) {
      // Trap errors come back as a wall of IC debug text. prettifyCanisterError
      // pulls out the actual trap message and classifies known patterns so the
      // toast can show a contextual CTA (e.g. "Deposit PP →" for insufficient_chips).
      const pretty = prettifyCanisterError(error);
      // The cast trapped — NO PP was deducted. Setting cost to 0 prevents the
      // toast from showing the misleading "X PP spent" line. (Previously we
      // showed selectedShenanigan.costSuccess here, which was actively wrong
      // and confused users into thinking they'd been charged for a failed cast.)
      setOutcomeToast({
        name: selectedShenanigan.name,
        outcome: 'error',
        flavor: pretty.message,
        cost: 0,
        errorKind: pretty.kind,
        errorRaw: pretty.raw,
      });
      setShowErrorDetails(false);
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

  // Side pocket vs wallet PP — only side-pocket PP can be spent on
  // shenanigans. Wallet PP must be deposited first (Bank → BridgeCard, or
  // BuyPP widget's post-buy prompt). Comparing against totalPoints was the
  // legacy bug that caused the "Insufficient chips" trap to surface as a
  // wall-of-text error.
  const userChips = ponziData?.chipPoints || 0;
  const userWallet = ponziData?.walletPoints || 0;

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
                const isDisabled = castShenanigan.isPending || userChips < trick.costSuccess || animatingTrick === trickKey || onCooldown || shieldFull;
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
                        : userChips < trick.costSuccess ? (userChips + userWallet >= trick.costSuccess ? `Need ${trick.costSuccess} in side pocket` : `Need ${trick.costSuccess} PP`)
                        : `Cast (${trick.costSuccess} PP)`}
                    </button>
                    {/* Per-card top-up pill — shown only when the side
                        pocket is short but wallet PP would cover the cast.
                        Clicking jumps to the Bank page where the bridge +
                        BuyPP widget live. */}
                    {!onCooldown && !shieldFull && !castShenanigan.isPending && userChips < trick.costSuccess && userChips + userWallet >= trick.costSuccess && (
                      <button
                        type="button"
                        onClick={() => { window.location.hash = '#side-pocket'; }}
                        className="mc-shenanigan-topup-pill"
                      >
                        Top up {(trick.costSuccess - userChips).toLocaleString()} PP →
                      </button>
                    )}
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
                const isDisabled = castShenanigan.isPending || userChips < trick.costSuccess || animatingTrick === trickKey || onCooldown || shieldFull;
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
                    {!onCooldown && !shieldFull && !castShenanigan.isPending && userChips < trick.costSuccess && userChips + userWallet >= trick.costSuccess && (
                      <button
                        type="button"
                        onClick={() => { window.location.hash = '#side-pocket'; }}
                        className="mc-shenanigan-topup-pill mc-shenanigan-topup-pill-compact"
                        title={`Need ${(trick.costSuccess - userChips).toLocaleString()} more in side pocket`}
                      >
                        Top up →
                      </button>
                    )}
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

        {/* Right column (desktop): HoF rail + Buy PP widget + Live Feed — sticky via .mc-shenanigans-sidebar */}
        <div className="mc-shenanigans-sidebar space-y-4">
          <HallOfFameRail />
          <BuyPPWidget />
          <LiveFeedPanel
            records={recentShenanigans ?? []}
            resolveSpell={(s) => {
              const config = availableShenanigans.find(a => variantKey(a.type) === variantKey(s.shenaniganType));
              return { name: config?.name ?? 'Unknown', icon: config?.icon ?? null };
            }}
          />
        </div>
      </div>

      {/* Mobile-only: Buy PP FAB (bottom-left, mirrors trollbox). Hidden on lg+. */}
      <BuyPPFab />

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
                // 8 whaleRebalance, 9 downlineBoost, 10 goldenName,
                // 11 tenderOffer, 12 stimulusCheck, 13 bearRaid,
                // 14 foundersRound, 15 strategicReserve, 16 slushFund, 17 insiderTip

                if (outcomeToast.outcome === 'success') {
                  // Spell-specific copy for spells whose generic "stole" framing
                  // doesn't fit (Stimulus is hero, Bear Raid voice differs from
                  // a 1v1 theft, Tender Offer is an acquisition not a theft).
                  if (id === 12) { // stimulusCheck — hero, mints to everyone
                    return <p className="text-xs mc-text-green mb-3">Pulled strings at the Fed. {cnt} player{cnt === 1 ? '' : 's'} got paid. You took {Math.round(d)} PP off the top.</p>;
                  }
                  if (id === 13) { // bearRaid — drain everyone, caster keeps up to 100, excess burns
                    return <p className="text-xs mc-text-green mb-3">Coordinated short. {cnt} player{cnt === 1 ? '' : 's'} took a haircut. You netted {Math.round(d)} PP (the rest burned).</p>;
                  }
                  if (id === 11) { // tenderOffer — acquired the target's whole stack
                    // Shielded targets return cnt=0 and the acquired-lock isn't set.
                    if (cnt === 0) return <p className="text-xs mc-text-green mb-3">{target} was shielded. Acquisition deflected.</p>;
                    return <p className="text-xs mc-text-green mb-3">Acquired {target}. Their {Math.round(d)} PP is yours. They're locked out of casting for 24h.</p>;
                  }
                  if (id === 14) { // foundersRound — self-buff, mint rate +15%
                    return <p className="text-xs mc-text-green mb-3">Locked in a flat round. Mint rate +15% for 24h.</p>;
                  }
                  if (id === 15) { // strategicReserve — cosmetic purple name
                    return <p className="text-xs mc-text-green mb-3">Strategic Reserve secured. Purple name for 7 days. (You've made it.)</p>;
                  }
                  if (id === 16) { // slushFund — minted PP to target
                    return <p className="text-xs mc-text-green mb-3">Slipped a wad of PP to {target}. They'll wonder who their secret admirer is.</p>;
                  }
                  if (id === 17) { // insiderTip — target gets +10% mint rate
                    return <p className="text-xs mc-text-green mb-3">{target}'s mint rate just jumped +10% for 12h. They're going to be insufferable.</p>;
                  }
                  if (id === 18) { // voiceOfGod — bolder chat posts for 6h
                    return <p className="text-xs mc-text-green mb-3">Voice of God active for 6h. Your posts carry institutional authority now. Try not to abuse it.</p>;
                  }
                  if (id === 19) { // customTitle — modal handles the affirmation; this branch is unreachable but kept for safety
                    return <p className="text-xs mc-text-green mb-3">Title slot secured. You have 5 minutes to choose your designation.</p>;
                  }
                  if (id === 20) { // echo — Reginald footnotes on posts for 6h
                    return <p className="text-xs mc-text-green mb-3">Echo active for 6h. Reginald will be appending editorial commentary to your contributions. He has opinions.</p>;
                  }
                  if (id === 21) { // confettiCannon — confetti on successful casts for 24h
                    return <p className="text-xs mc-text-green mb-3">Confetti Cannon loaded. Your next 24h of successful casts will be appropriately celebrated.</p>;
                  }
                  // Numeric wins first (MEV Attack, Contagion, Wealth Tax theft).
                  if (d > 0 && cnt === 1) return <p className="text-xs mc-text-green mb-3">Stole {Math.round(d)} PP from {target}.</p>;
                  if (d > 0 && cnt > 1)  return <p className="text-xs mc-text-green mb-3">Stole {Math.round(d)} PP from {cnt} players.</p>;
                  // Spells with no PP delta — explain what actually happened.
                  switch (id) {
                    case 3: // mintTaxSiphon
                      if (cnt === 1) return <p className="text-xs mc-text-green mb-3">{target} is now siphoned. You'll skim 5% of their next 1000 PP minted (over 7 days).</p>;
                      return <p className="text-xs mc-text-green mb-3">{target} was shielded. No siphon.</p>;
                    case 4: // downlineHeist — affectedTarget is the poached member, NOT the
                            // original target. Reword so the sentence reads correctly.
                      if (cnt === 1) return <p className="text-xs mc-text-green mb-3">Poached {target} — they're in your downline now.</p>;
                      return <p className="text-xs mc-text-green mb-3">No downline member to poach.</p>;
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
                  // Spell-specific backfires for spells whose handlers do work
                  // but return ppDeltaCaster = 0 (the toast can't infer the
                  // burn/payout from the delta alone).
                  if (id === 12) { // stimulusCheck — caster burned extra 200 PP
                    return <p className="text-xs mc-text-purple mb-3">The bill didn't pass. You ate the lobbying budget — 200 PP gone.</p>;
                  }
                  if (id === 13) { // bearRaid — karmic inversion, everyone got paid
                    return <p className="text-xs mc-text-purple mb-3">You misread the cycle. {cnt} player{cnt === 1 ? '' : 's'} got paid; you burned 100 PP.</p>;
                  }
                  if (id === 11) { // tenderOffer — d < 0 (paid target 3x cost) + 7d Tender Offer lockout
                    return <p className="text-xs mc-text-purple mb-3">{target} got {Math.abs(Math.round(d))} PP as poison-pill compensation. Tender Offer locked for 7 days.</p>;
                  }
                  if (id === 14) { // foundersRound — down round, mint rate -10%
                    return <p className="text-xs mc-text-purple mb-3">Down round. Mint rate -10% for 24h. Investors are "concerned."</p>;
                  }
                  if (id === 15) { // strategicReserve — reserves frozen (0% default odds but admin-tunable)
                    return <p className="text-xs mc-text-purple mb-3">Reserves frozen pending audit. PP gone.</p>;
                  }
                  if (id === 16) { // slushFund — they found you out, paid d < 0 to target
                    return <p className="text-xs mc-text-purple mb-3">{target} found you out. You owe them an extra 200 PP for being annoying.</p>;
                  }
                  if (id === 17) { // insiderTip — SEC settlement, d < 0 burn
                    return <p className="text-xs mc-text-purple mb-3">Whisper got out. SEC settlement, no admission of wrongdoing. -50 PP.</p>;
                  }
                  if (id === 18) { // voiceOfGod — cost burn, no effect
                    return <p className="text-xs mc-text-purple mb-3">Voice of God went to voicemail. PP burned, pulpit empty.</p>;
                  }
                  if (id === 19) { // customTitle — cost burn, no slot opened
                    return <p className="text-xs mc-text-purple mb-3">Title application returned to sender. HR says the form was incomplete.</p>;
                  }
                  if (id === 20) { // echo — cost burn, no effect
                    return <p className="text-xs mc-text-purple mb-3">Echo chamber collapsed inward. Reginald is fine. You are not.</p>;
                  }
                  if (id === 21) { // confettiCannon — cost burn, no effect
                    return <p className="text-xs mc-text-purple mb-3">Cannon misfired. Confetti went sideways. PP gone, dignity unclear.</p>;
                  }
                  // Numeric losses first.
                  if (d < 0 && cnt === 1)  return <p className="text-xs mc-text-purple mb-3">Paid {Math.abs(Math.round(d))} PP to {target}.</p>;
                  if (d < 0 && cnt > 1)   return <p className="text-xs mc-text-purple mb-3">Paid {Math.abs(Math.round(d))} PP to {cnt} whales.</p>;
                  if (d < 0 && cnt === 0) return <p className="text-xs mc-text-purple mb-3">You burned {Math.abs(Math.round(d))} PP.</p>;
                  // State-change backfires with no PP delta — name them.
                  // Spells 5/6/9/10 default to 100% success in config but admin
                  // can tune odds, so every spell needs backfire copy.
                  switch (id) {
                    case 2: // renameSpell backfire — caster gets renamed
                      return <p className="text-xs mc-text-purple mb-3">You got renamed for 7 days.</p>;
                    case 3: // mintTaxSiphon backfire — target becomes the siphoner
                      return <p className="text-xs mc-text-purple mb-3">{target} now siphons 5% of YOUR mints for 3 days.</p>;
                    case 4: // downlineHeist backfire — affectedTarget is the downline member
                            // who switched to the (original) target. Reword for clarity.
                      if (cnt === 1) return <p className="text-xs mc-text-purple mb-3">{target} bolted — they're in someone else's downline now.</p>;
                      return <p className="text-xs mc-text-purple mb-3">Backfired — but you had no downline to lose.</p>;
                    case 5: // magicMirror (Poison Pill) — no extra effect, just the cost burn
                      return <p className="text-xs mc-text-purple mb-3">Pill landed in your own pocket. PP burned, no shield.</p>;
                    case 6: // ppBoosterAura (Yield Boost) — no extra effect
                      return <p className="text-xs mc-text-purple mb-3">Booster jammed. No yield, just a smoldering injector.</p>;
                    case 9: // downlineBoost (Override Bonus) — no extra effect
                      return <p className="text-xs mc-text-purple mb-3">Your downline filed a grievance. PP went to HR; no override.</p>;
                    case 10: // goldenName (Whitelisted) — no extra effect
                      return <p className="text-xs mc-text-purple mb-3">Application leaked to the press. PP burned in PR scramble.</p>;
                    default:
                      return <p className="text-xs mc-text-purple mb-3">Backfired — but no observable effect.</p>;
                  }
                }

                if (outcomeToast.outcome === 'fail') {
                  // Most spells share the generic "nothing happened" — the
                  // 4 buff/cosmetic spells (5/6/9/10) default to 100% success
                  // but admin-tunable odds mean a fail can still fire. Give
                  // them flavor so the toast doesn't read as a no-op.
                  switch (id) {
                    case 5: // magicMirror (Poison Pill)
                      return <p className="text-xs mc-text-muted mb-3">The pill dissolved. No shield, no refund.</p>;
                    case 6: // ppBoosterAura (Yield Boost)
                      return <p className="text-xs mc-text-muted mb-3">Booster refused to engage. No yield change.</p>;
                    case 9: // downlineBoost (Override Bonus)
                      return <p className="text-xs mc-text-muted mb-3">Downline didn't hear you. Cascade unchanged.</p>;
                    case 10: // goldenName (Whitelisted)
                      return <p className="text-xs mc-text-muted mb-3">Whitelist application rejected. No gold name today.</p>;
                    case 18: // voiceOfGod
                      return <p className="text-xs mc-text-muted mb-3">Broadcast signal lost. No voice, no authority, no refund.</p>;
                    case 19: // customTitle
                      return <p className="text-xs mc-text-muted mb-3">Title pending review. Rejected. No explanation provided.</p>;
                    case 20: // echo
                      return <p className="text-xs mc-text-muted mb-3">Reginald declined to participate. Echo silent.</p>;
                    case 21: // confettiCannon
                      return <p className="text-xs mc-text-muted mb-3">Cannon failed inspection. No confetti, no celebration, no refund.</p>;
                    default:
                      return <p className="text-xs mc-text-muted mb-3">Nothing happened. The PP is still gone.</p>;
                  }
                }
                return null;
              })()}
              {outcomeToast.cost > 0 && outcomeToast.outcome !== 'error' && (
                <p className="text-xs mc-text-muted mb-3">{outcomeToast.cost} PP spent</p>
              )}
              {outcomeToast.outcome === 'error' && outcomeToast.errorKind === 'insufficient_chips' && (
                <p className="text-xs mc-text-muted mb-3">
                  Your PP needs to be deposited into your side pocket before you can cast.
                </p>
              )}
              {outcomeToast.outcome === 'error' && (
                <div className="text-xs mb-3">
                  <button
                    type="button"
                    onClick={() => setShowErrorDetails(v => !v)}
                    className="mc-text-muted underline hover:mc-text-dim"
                  >
                    {showErrorDetails ? 'Hide details' : 'Show details'}
                  </button>
                  {showErrorDetails && outcomeToast.errorRaw && (
                    <pre className="mt-2 p-2 rounded bg-black/40 text-[10px] mc-text-muted text-left overflow-auto max-h-32 whitespace-pre-wrap break-words">
                      {outcomeToast.errorRaw}
                    </pre>
                  )}
                </div>
              )}
              <div className="flex items-center justify-center gap-2 flex-wrap">
                {outcomeToast.outcome === 'error' && outcomeToast.errorKind === 'insufficient_chips' && (
                  <button
                    onClick={() => {
                      setOutcomeToast(null);
                      window.location.hash = '#side-pocket';
                    }}
                    className="mc-btn-primary px-5 py-2 rounded-full text-sm"
                  >
                    Deposit PP →
                  </button>
                )}
                <button
                  onClick={() => setOutcomeToast(null)}
                  className="mc-btn-secondary px-5 py-2 rounded-full text-sm"
                >
                  {outcomeToast.outcome === 'error' ? 'Close' : 'Noted'}
                </button>
              </div>
            </div>
          </div>
        </>
      )}

      {/* Rename Spell — post-success reveal modal */}
      {renamePrompt && (
        <>
          <div className="mc-modal-backdrop" aria-hidden="true" />
          <div className="fixed top-28 md:top-36 left-1/2 -translate-x-1/2 z-[9999]" role="dialog" aria-modal="true">
            <div className="mc-toast text-center max-w-sm">
              <div className="font-display text-xl mc-text-primary mb-2">
                Cease &amp; Desist landed.
              </div>
              <p className="text-sm mc-text-dim mb-3">
                Their new name:{' '}
                <strong className="mc-text-primary">
                  <OutcomeTargetName principalText={renamePrompt.targetPrincipal} />
                </strong>
              </p>
              {/* Accept */}
              <div className="flex flex-col gap-2 items-center">
                <button
                  onClick={async () => {
                    setRenamePrompt(null);
                    setRenameInput('');
                    setRenameInputError('');
                    try { await cancelRenameName.mutateAsync(); } catch {}
                  }}
                  disabled={cancelRenameName.isPending}
                  className="mc-btn-primary px-5 py-2 rounded-full text-sm w-full"
                >
                  Accept
                </button>
                {/* Re-roll */}
                <button
                  onClick={async () => {
                    setRenameInputError('');
                    try {
                      await rerollRename.mutateAsync();
                    } catch (e: any) {
                      setRenameInputError(e.message || 'Re-roll failed');
                    }
                  }}
                  disabled={rerollRename.isPending || setRenameName.isPending}
                  className="mc-btn-secondary px-5 py-2 rounded-full text-sm w-full"
                >
                  {rerollRename.isPending ? 'Rolling…' : 'Pick a different one (–50 PP)'}
                </button>
                {/* Type your own */}
                <div className="w-full mt-1">
                  <p className="text-xs mc-text-muted mb-1">Type your own (–500 PP)</p>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={renameInput}
                      onChange={(e) => { setRenameInput(e.target.value); setRenameInputError(''); }}
                      onKeyDown={async (e) => {
                        if (e.key === 'Enter' && renameInput.trim().length > 0 && !setRenameName.isPending) {
                          setRenameInputError('');
                          try {
                            await setRenameName.mutateAsync(renameInput);
                            setRenamePrompt(null);
                            setRenameInput('');
                          } catch (err: any) {
                            setRenameInputError(err.message || 'Rename failed');
                          }
                        }
                      }}
                      maxLength={32}
                      className="flex-1 px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-sm mc-text-primary"
                      placeholder="e.g., Liquidation Larry"
                    />
                    <button
                      onClick={async () => {
                        setRenameInputError('');
                        try {
                          await setRenameName.mutateAsync(renameInput);
                          setRenamePrompt(null);
                          setRenameInput('');
                        } catch (err: any) {
                          setRenameInputError(err.message || 'Rename failed');
                        }
                      }}
                      disabled={renameInput.trim().length === 0 || setRenameName.isPending || rerollRename.isPending}
                      className="mc-btn-primary px-4 py-2 rounded-full text-sm whitespace-nowrap"
                    >
                      {setRenameName.isPending ? 'Committing…' : 'Lock it in'}
                    </button>
                  </div>
                  {renameInputError && (
                    <p className="text-xs text-red-400 mt-1">{renameInputError}</p>
                  )}
                </div>
              </div>
            </div>
          </div>
        </>
      )}

      {/* Custom Title — post-success commit modal */}
      {customTitlePrompt && (
        <>
          <div className="mc-modal-backdrop" aria-hidden="true" />
          <div className="fixed top-28 md:top-36 left-1/2 -translate-x-1/2 z-[9999]" role="dialog" aria-modal="true">
            <div className="mc-toast text-center max-w-sm">
              <div className="font-display text-xl mc-text-primary mb-2">
                Designation Approved.
              </div>
              <p className="text-sm mc-text-dim mb-3">
                Choose a title (1–32 characters). It will appear beside your name in chat for 7 days.
              </p>
              <div className="flex flex-col gap-2 items-center w-full">
                <div className="flex gap-2 w-full">
                  <input
                    type="text"
                    value={customTitleInput}
                    onChange={(e) => { setCustomTitleInput(e.target.value); setCustomTitleInputError(''); }}
                    onKeyDown={async (e) => {
                      if (e.key === 'Enter' && customTitleInput.trim().length > 0 && !setCustomTitleMutation.isPending) {
                        setCustomTitleInputError('');
                        try {
                          await setCustomTitleMutation.mutateAsync(customTitleInput.trim());
                          setCustomTitlePrompt(false);
                          setCustomTitleInput('');
                        } catch (err: any) {
                          setCustomTitleInputError(err.message || 'Failed to set title');
                        }
                      }
                    }}
                    maxLength={32}
                    className="flex-1 px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-sm mc-text-primary"
                    placeholder="e.g., Chief Visionary Officer"
                    autoFocus
                  />
                  <button
                    onClick={async () => {
                      setCustomTitleInputError('');
                      try {
                        await setCustomTitleMutation.mutateAsync(customTitleInput.trim());
                        setCustomTitlePrompt(false);
                        setCustomTitleInput('');
                      } catch (err: any) {
                        setCustomTitleInputError(err.message || 'Failed to set title');
                      }
                    }}
                    disabled={customTitleInput.trim().length === 0 || setCustomTitleMutation.isPending}
                    className="mc-btn-primary px-4 py-2 rounded-full text-sm whitespace-nowrap"
                  >
                    {setCustomTitleMutation.isPending ? 'Filing…' : 'Commit'}
                  </button>
                </div>
                {customTitleInputError && (
                  <p className="text-xs text-red-400 self-start">{customTitleInputError}</p>
                )}
                <button
                  onClick={() => { setCustomTitlePrompt(false); setCustomTitleInput(''); setCustomTitleInputError(''); }}
                  disabled={setCustomTitleMutation.isPending}
                  className="mc-btn-secondary px-5 py-2 rounded-full text-sm w-full"
                >
                  Skip (slot expires in 5 min)
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
              <div className="flex gap-3 justify-center mt-4">
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
