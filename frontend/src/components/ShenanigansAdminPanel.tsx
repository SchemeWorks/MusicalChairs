import React, { useState, useEffect } from 'react';
import { Principal } from '@dfinity/principal';
import { toast } from 'sonner';
import { Save, RotateCcw, AlertTriangle, CheckCircle, Info, ChevronRight, Coins, Waves, Pencil, Building2, Target, FlipHorizontal2, ArrowUp, Scissors, Fish, TrendingUp, Sparkles, SlidersHorizontal } from 'lucide-react';
import { CharlesIcon } from '../lib/charles';
import {
  useGetShenaniganConfigs,
  useUpdateShenaniganConfig,
  useSaveAllShenaniganConfigs,
  useResetShenaniganConfig,
  useGetMintConfig,
  useSetSimple21,
  useSetCompounding15,
  useSetCompounding30,
  useSetDealerMultiplier,
  useSetReferralBps,
  useSetMinDeposit,
  useSetCashOutDelay,
  useSetObserverInterval,
  useGetObserverStatus,
  useStopObserver,
  useResumeObserver,
  useAdminSetPin,
  useAdminMuteUser,
  useAdminUnmute,
  useAdminPostAsReginald,
  useCurrentPin,
  useListChimeSounds,
  useAdminUploadChimeSound,
  useAdminDeleteChimeSound,
  useListFlavorPools,
  useGetFlavorPoolDefaults,
  useAdminSetFlavorPool,
  useAdminClearFlavorPool,
} from '../hooks/useQueries';
import { SPELL_FLAVOR_DEFAULTS, type SpellFlavorKey } from './trollbox/spellFlavorDefaults';
import { useReadShenaniganActor } from '../hooks/useShenaniganActor';
import LoadingSpinner from './LoadingSpinner';

interface ShenaniganConfig {
  id: number;
  name: string;
  description: string;
  cost: number;
  successOdds: number;
  failureOdds: number;
  backfireOdds: number;
  duration: number;
  cooldown: number;
  effectValues: number[];
  castLimit: number;
  backgroundColor: string;
}

const shenaniganIcons: Record<number, React.ReactNode> = {
  0: <Coins className="h-5 w-5" />, 1: <Waves className="h-5 w-5" />, 2: <Pencil className="h-5 w-5" />,
  3: <Building2 className="h-5 w-5" />, 4: <Target className="h-5 w-5" />, 5: <FlipHorizontal2 className="h-5 w-5" />,
  6: <ArrowUp className="h-5 w-5" />, 7: <Scissors className="h-5 w-5" />, 8: <Fish className="h-5 w-5" />,
  9: <TrendingUp className="h-5 w-5" />, 10: <Sparkles className="h-5 w-5" />,
};

const auraColors: Record<number, string> = {
  0: 'rgba(255, 215, 90, 0.4)', 1: 'rgba(100, 200, 255, 0.4)', 2: 'rgba(255, 130, 200, 0.4)',
  3: 'rgba(168, 85, 247, 0.4)', 4: 'rgba(57, 255, 20, 0.4)', 5: 'rgba(255, 215, 0, 0.4)',
  6: 'rgba(100, 165, 255, 0.4)', 7: 'rgba(239, 68, 68, 0.4)', 8: 'rgba(59, 130, 246, 0.4)',
  9: 'rgba(16, 185, 129, 0.4)', 10: 'rgba(245, 158, 11, 0.4)',
};

/* ================================================================
   Dark-themed input component
   ================================================================ */
function AdminInput({ label, hint, type = 'text', value, onChange, min, max, placeholder, rows }: {
  label: string; hint?: string; type?: string; value: string | number;
  onChange: (v: string) => void; min?: string; max?: string; placeholder?: string; rows?: number;
}) {
  const baseClass = "w-full bg-white/5 border border-white/10 rounded-lg px-3 py-2.5 text-sm text-white placeholder-white/30 focus:outline-none focus:border-purple-500/50 focus:ring-1 focus:ring-purple-500/30 transition-all font-body";
  return (
    <div>
      <label className="block text-xs font-bold uppercase tracking-wider text-white/50 mb-1.5">{label}</label>
      {rows ? (
        <textarea className={baseClass} value={value} onChange={e => onChange(e.target.value)}
          rows={rows} placeholder={placeholder} />
      ) : (
        <input className={baseClass} type={type} value={value} onChange={e => onChange(e.target.value)}
          min={min} max={max} placeholder={placeholder} />
      )}
      {hint && <p className="text-[11px] text-white/30 mt-1">{hint}</p>}
    </div>
  );
}

/* ================================================================
   Odds validator badge
   ================================================================ */
function OddsBadge({ total }: { total: number }) {
  const valid = total === 100;
  return (
    <span className={`inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-xs font-bold border ${
      valid
        ? 'bg-[var(--mc-neon-green)]/10 mc-text-green border-[var(--mc-neon-green)]/30'
        : 'bg-[var(--mc-danger)]/10 mc-text-danger border-[var(--mc-danger)]/30 animate-pulse'
    }`}>
      {valid ? <CheckCircle className="h-3 w-3" /> : <AlertTriangle className="h-3 w-3" />}
      {total}%
    </span>
  );
}

/* ================================================================
   Main Component
   ================================================================ */
export default function ShenanigansAdminPanel() {
  const { data: backendConfigs, isLoading } = useGetShenaniganConfigs();
  const updateConfig = useUpdateShenaniganConfig();
  const saveAllConfigs = useSaveAllShenaniganConfigs();
  const resetConfig = useResetShenaniganConfig();

  const [shenanigans, setShenanigans] = useState<ShenaniganConfig[]>([]);
  const [selectedShenanigan, setSelectedShenanigan] = useState<ShenaniganConfig | null>(null);
  // Raw text buffer for the Effect Values input so mid-edit states (empty
  // entries, trailing commas) don't get clobbered by the array round-trip.
  const [effectValuesDraft, setEffectValuesDraft] = useState<string>('');

  useEffect(() => {
    if (backendConfigs) {
      const mappedConfigs = backendConfigs.map(config => ({
        id: Number(config.id),
        name: config.name,
        description: config.description,
        cost: config.cost,
        successOdds: Number(config.successOdds),
        failureOdds: Number(config.failureOdds),
        backfireOdds: Number(config.backfireOdds),
        duration: Number(config.duration),
        cooldown: Number(config.cooldown),
        effectValues: config.effectValues,
        castLimit: Number(config.castLimit),
        backgroundColor: config.backgroundColor,
      }));
      setShenanigans(mappedConfigs);
      if (selectedShenanigan) {
        const updated = mappedConfigs.find(c => c.id === selectedShenanigan.id);
        if (updated) {
          setSelectedShenanigan(updated);
          setEffectValuesDraft(updated.effectValues.join(', '));
        }
      }
    }
  }, [backendConfigs]);

  /* ---- Save single ---- */
  const handleSaveShenanigan = async () => {
    if (!selectedShenanigan) return;
    const oddsSum = selectedShenanigan.successOdds + selectedShenanigan.failureOdds + selectedShenanigan.backfireOdds;
    if (oddsSum !== 100) { toast.error('Odds must sum to 100%'); return; }
    if (selectedShenanigan.cost < 0 || selectedShenanigan.cooldown < 0 || selectedShenanigan.duration < 0 || selectedShenanigan.castLimit < 0) {
      toast.error('Numeric values cannot be negative'); return;
    }
    try {
      await updateConfig.mutateAsync({
        id: BigInt(selectedShenanigan.id), name: selectedShenanigan.name,
        description: selectedShenanigan.description, cost: selectedShenanigan.cost,
        successOdds: BigInt(selectedShenanigan.successOdds), failureOdds: BigInt(selectedShenanigan.failureOdds),
        backfireOdds: BigInt(selectedShenanigan.backfireOdds), duration: BigInt(selectedShenanigan.duration),
        cooldown: BigInt(selectedShenanigan.cooldown), effectValues: selectedShenanigan.effectValues,
        castLimit: BigInt(selectedShenanigan.castLimit), backgroundColor: selectedShenanigan.backgroundColor,
      });
      setShenanigans(prev => prev.map(s => s.id === selectedShenanigan.id ? selectedShenanigan : s));
      window.dispatchEvent(new CustomEvent('shenaniganUpdated', {
        detail: {
          id: selectedShenanigan.id, name: selectedShenanigan.name, icon: shenaniganIcons[selectedShenanigan.id],
          description: selectedShenanigan.description, cost: selectedShenanigan.cost,
          successOdds: selectedShenanigan.successOdds, failOdds: selectedShenanigan.failureOdds,
          backfireOdds: selectedShenanigan.backfireOdds, effectValues: selectedShenanigan.effectValues.join(', '),
        }
      }));
      toast.success(`${selectedShenanigan.name} saved`);
    } catch (error: any) {
      toast.error(`Save failed: ${error.message || 'Unknown error'}`);
    }
  };

  /* ---- Reset single ---- */
  const handleResetToDefaults = async () => {
    if (!selectedShenanigan) return;
    try {
      await resetConfig.mutateAsync(BigInt(selectedShenanigan.id));
      toast.success(`${selectedShenanigan.name} reset to defaults`);
    } catch (error: any) {
      toast.error(`Reset failed: ${error.message || 'Unknown error'}`);
    }
  };

  /* ---- Save all ---- */
  const handleSaveAllChanges = async () => {
    for (const shen of shenanigans) {
      const oddsSum = shen.successOdds + shen.failureOdds + shen.backfireOdds;
      if (oddsSum !== 100) { toast.error(`${shen.name}: Odds must sum to 100%`); return; }
      if (shen.cost < 0 || shen.cooldown < 0 || shen.duration < 0 || shen.castLimit < 0) {
        toast.error(`${shen.name}: Numeric values cannot be negative`); return;
      }
    }
    try {
      await saveAllConfigs.mutateAsync(shenanigans.map(shen => ({
        id: BigInt(shen.id), name: shen.name, description: shen.description, cost: shen.cost,
        successOdds: BigInt(shen.successOdds), failureOdds: BigInt(shen.failureOdds),
        backfireOdds: BigInt(shen.backfireOdds), duration: BigInt(shen.duration),
        cooldown: BigInt(shen.cooldown), effectValues: shen.effectValues,
        castLimit: BigInt(shen.castLimit), backgroundColor: shen.backgroundColor,
      })));
      shenanigans.forEach(shen => {
        window.dispatchEvent(new CustomEvent('shenaniganUpdated', {
          detail: {
            id: shen.id, name: shen.name, icon: shenaniganIcons[shen.id],
            description: shen.description, cost: shen.cost,
            successOdds: shen.successOdds, failOdds: shen.failureOdds,
            backfireOdds: shen.backfireOdds, effectValues: shen.effectValues.join(', '),
          }
        }));
      });
      toast.success('All shenanigans saved');
    } catch (error: any) {
      toast.error(`Save all failed: ${error.message || 'Unknown error'}`);
    }
  };

  /* ---- Update selected field helper ---- */
  const updateField = (field: keyof ShenaniganConfig, value: any) => {
    if (!selectedShenanigan) return;
    setSelectedShenanigan({ ...selectedShenanigan, [field]: value });
  };

  /* ---- Loading state ---- */
  if (isLoading) {
    return (
      <div className="space-y-6">
        <div className="text-center">
          <h2 className="font-display text-2xl mc-text-primary">
            <CharlesIcon className="inline h-5 w-5 mr-2 mc-text-gold" />
            Charles's Office
          </h2>
          <p className="mc-text-muted text-sm mt-1 font-accent italic">Charles is counting the money...</p>
        </div>
        <div className="mc-card-elevated p-8 flex justify-center"><LoadingSpinner /></div>
      </div>
    );
  }

  const oddsTotal = selectedShenanigan
    ? selectedShenanigan.successOdds + selectedShenanigan.failureOdds + selectedShenanigan.backfireOdds
    : 0;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="text-center">
        <h2 className="font-display text-2xl mc-text-primary">
          <CharlesIcon className="inline h-5 w-5 mr-2 mc-text-gold" />
          Charles's Office
        </h2>
        <p className="mc-text-muted text-sm mt-1 font-accent italic">Pull the strings. Tweak the odds. The alpha is whatever you say it is.</p>
      </div>

      {/* Instructions callout */}
      <div className="mc-card mc-accent-gold p-4">
        <div className="flex items-start gap-3">
          <Info className="h-5 w-5 mc-text-gold flex-shrink-0 mt-0.5" />
          <div className="text-sm mc-text-dim space-y-1">
            <p className="font-bold mc-text-gold">How This Works</p>
            <p>
              Changes take effect immediately and sync to the Shenanigans page in real-time.
              Odds must sum to 100%. Save per-item or batch save at bottom.
              Remember: you break it, you fix it. The players will notice.
            </p>
          </div>
        </div>
      </div>

      {/* Observer status */}
      <ObserverStatusSection />

      {/* Mint Rules & Economy */}
      <MintRulesSection />

      {/* Main layout: list + editor */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">

        {/* ============ Left: Shenanigan selector list ============ */}
        <div className="lg:col-span-1">
          <div className="mc-card p-4">
            <h3 className="font-display text-sm mc-text-primary mb-3">Shenanigans</h3>
            <p className="text-xs mc-text-muted mb-4">Pick your poison</p>
            <div className="space-y-1.5">
              {shenanigans.map(shen => {
                const isActive = selectedShenanigan?.id === shen.id;
                return (
                  <button
                    key={shen.id}
                    onClick={() => { setSelectedShenanigan(shen); setEffectValuesDraft(shen.effectValues.join(', ')); }}
                    className={`w-full text-left p-3 rounded-lg border transition-all group ${
                      isActive
                        ? 'border-[var(--mc-purple)]/50 bg-[var(--mc-purple)]/10 shadow-[0_0_15px_rgba(168,85,247,0.15)]'
                        : 'border-white/5 bg-white/[0.02] hover:border-white/15 hover:bg-white/5'
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-3">
                        <span className="text-xl" style={{ filter: isActive ? 'none' : 'grayscale(0.3)' }}>
                          {shenaniganIcons[shen.id]}
                        </span>
                        <div>
                          <div className={`text-sm font-bold ${isActive ? 'mc-text-primary' : 'mc-text-dim'}`}>
                            {shen.name}
                          </div>
                          <div className="text-[11px] mc-text-muted">
                            {shen.cost} PP · {shen.successOdds}/{shen.failureOdds}/{shen.backfireOdds}
                          </div>
                        </div>
                      </div>
                      <ChevronRight className={`h-4 w-4 transition-all ${
                        isActive ? 'mc-text-purple opacity-100' : 'opacity-0 group-hover:opacity-40'
                      }`} />
                    </div>
                  </button>
                );
              })}
            </div>
          </div>
        </div>

        {/* ============ Right: Editor panel ============ */}
        {selectedShenanigan ? (
          <div className="lg:col-span-2">
            <div className="mc-card overflow-hidden">
              {/* Editor header with aura */}
              <div className="p-5 border-b border-white/5 relative">
                <div className="absolute inset-0 opacity-20" style={{
                  background: `radial-gradient(ellipse at 30% 50%, ${auraColors[selectedShenanigan.id]}, transparent 70%)`
                }} />
                <div className="relative flex items-center gap-3">
                  <span className="text-3xl">{shenaniganIcons[selectedShenanigan.id]}</span>
                  <div>
                    <h3 className="font-display text-lg mc-text-primary">{selectedShenanigan.name}</h3>
                    <p className="text-xs mc-text-muted">ID: {selectedShenanigan.id} · Tweak everything. Nobody's watching.</p>
                  </div>
                </div>
              </div>

              {/* Editor body */}
              <div className="p-5 space-y-5" >

                {/* Identity section */}
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                  <AdminInput label="Name" value={selectedShenanigan.name}
                    onChange={v => updateField('name', v)} />
                  <AdminInput label="Cost (PP)" type="number" value={selectedShenanigan.cost}
                    onChange={v => updateField('cost', Math.max(0, parseFloat(v) || 0))} min="0"
                    hint="Higher cost = fewer casts = less chaos" />
                </div>

                <AdminInput label="Description" value={selectedShenanigan.description}
                  onChange={v => updateField('description', v)} rows={2} />

                {/* Divider */}
                <div className="border-t border-white/5" />

                {/* Odds section */}
                <div>
                  <div className="flex items-center justify-between mb-3">
                    <span className="text-xs font-bold uppercase tracking-wider text-white/50">
                      Odds Distribution
                    </span>
                    <OddsBadge total={oddsTotal} />
                  </div>
                  <div className="grid grid-cols-3 gap-3">
                    <AdminInput label="Success %" type="number" value={selectedShenanigan.successOdds}
                      onChange={v => updateField('successOdds', Math.max(0, Math.min(100, parseInt(v) || 0)))}
                      min="0" max="100" />
                    <AdminInput label="Failure %" type="number" value={selectedShenanigan.failureOdds}
                      onChange={v => updateField('failureOdds', Math.max(0, Math.min(100, parseInt(v) || 0)))}
                      min="0" max="100" />
                    <AdminInput label="Backfire %" type="number" value={selectedShenanigan.backfireOdds}
                      onChange={v => updateField('backfireOdds', Math.max(0, Math.min(100, parseInt(v) || 0)))}
                      min="0" max="100" />
                  </div>
                  {/* Visual odds bar */}
                  <div className="flex rounded-full h-2 overflow-hidden mt-3 bg-white/5">
                    <div className="mc-bg-green transition-all" style={{ width: `${selectedShenanigan.successOdds}%` }} />
                    <div className="mc-bg-gold transition-all" style={{ width: `${selectedShenanigan.failureOdds}%` }} />
                    <div className="mc-bg-danger transition-all" style={{ width: `${selectedShenanigan.backfireOdds}%` }} />
                  </div>
                  <div className="flex justify-between text-[10px] mt-1 mc-text-muted">
                    <span className="mc-text-green">Success</span>
                    <span className="mc-text-gold">Fail</span>
                    <span className="mc-text-danger">Backfire</span>
                  </div>
                </div>

                {/* Divider */}
                <div className="border-t border-white/5" />

                {/* Timing section */}
                <div className="grid grid-cols-2 gap-4">
                  <AdminInput label="Duration (hours)" type="number" value={selectedShenanigan.duration}
                    onChange={v => updateField('duration', Math.max(0, parseInt(v) || 0))}
                    min="0" hint="0 = instant. Otherwise, how long the effect lingers." />
                  <AdminInput label="Cooldown (hours)" type="number" value={selectedShenanigan.cooldown}
                    onChange={v => updateField('cooldown', Math.max(0, parseInt(v) || 0))}
                    min="0" hint="How long before they can cast again. 0 = spam city." />
                </div>

                {/* Effects section */}
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                  <AdminInput label="Effect Values" value={effectValuesDraft}
                    onChange={v => {
                      setEffectValuesDraft(v);
                      // Only commit fully-valid lists to the array; allow
                      // empty entries and trailing commas during editing.
                      const parts = v.split(',').map(s => s.trim());
                      if (parts.every(s => s !== '' && !isNaN(parseFloat(s)))) {
                        updateField('effectValues', parts.map(s => parseFloat(s)));
                      }
                    }}
                    placeholder="e.g., 2.0, 8.0, 250.0" hint="Comma-separated. What the shenanigan actually does." />
                  <AdminInput label="Cast Limit" type="number" value={selectedShenanigan.castLimit}
                    onChange={v => updateField('castLimit', Math.max(0, parseInt(v) || 0))}
                    min="0" hint="0 = unlimited. Set a cap or let anarchy reign." />
                </div>

                {/* Divider */}
                <div className="border-t border-white/5" />

                {/* Action buttons */}
                <div className="flex gap-3">
                  <button
                    onClick={handleSaveShenanigan}
                    disabled={updateConfig.isPending || oddsTotal !== 100}
                    className="flex-1 flex items-center justify-center gap-2 px-4 py-3 rounded-lg font-bold text-sm transition-all
                      bg-[var(--mc-neon-green)]/20 mc-text-green border border-[var(--mc-neon-green)]/30 hover:bg-[var(--mc-neon-green)]/30 hover:shadow-[0_0_20px_rgba(34,197,94,0.15)]
                      disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    <Save className="h-4 w-4" />
                    {updateConfig.isPending ? 'Saving...' : 'Save Changes'}
                  </button>
                  <button
                    onClick={handleResetToDefaults}
                    disabled={resetConfig.isPending}
                    className="flex items-center justify-center gap-2 px-4 py-3 rounded-lg font-bold text-sm transition-all
                      bg-white/5 text-white/60 border border-white/10 hover:bg-white/10 hover:text-white/80
                      disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    <RotateCcw className="h-4 w-4" />
                    {resetConfig.isPending ? 'Resetting...' : 'Reset'}
                  </button>
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="lg:col-span-2 flex items-center justify-center">
            <div className="mc-card p-12 text-center w-full">
              <SlidersHorizontal className="h-10 w-10 mc-text-muted mb-4 mx-auto" />
              <p className="mc-text-dim text-sm">Pick a shenanigan from the list.</p>
              <p className="mc-text-muted text-xs mt-1 font-accent italic">With great power comes great responsibility. Just kidding. Go nuts.</p>
            </div>
          </div>
        )}
      </div>

      {/* Save All */}
      <button
        onClick={handleSaveAllChanges}
        disabled={saveAllConfigs.isPending}
        className="w-full flex items-center justify-center gap-2 px-6 py-4 rounded-xl font-bold text-base transition-all
          bg-gradient-to-r from-purple-600/30 to-pink-600/30 text-white border border-purple-500/30
          hover:from-purple-600/40 hover:to-pink-600/40 hover:shadow-[0_0_30px_rgba(168,85,247,0.2)]
          disabled:opacity-40 disabled:cursor-not-allowed"
      >
        <Save className="h-5 w-5" />
        {saveAllConfigs.isPending ? 'Saving All...' : 'Save All Changes'}
      </button>

      <TrollboxAdminSection />
      <FlavorPoolsSection />
    </div>
  );
}

/* ================================================================
   Mint Rules & Economy section — tunable knobs backed by
   shenanigans' MintConfig state.
   ================================================================ */
function MintRulesSection() {
  const { data: config, isLoading } = useGetMintConfig();
  const setSimple = useSetSimple21();
  const setC15 = useSetCompounding15();
  const setC30 = useSetCompounding30();
  const setDealer = useSetDealerMultiplier();
  const setRefBps = useSetReferralBps();
  const setMinDep = useSetMinDeposit();
  const setCashDelay = useSetCashOutDelay();
  const setObserver = useSetObserverInterval();

  const [simple, setSimpleVal] = useState('');
  const [c15, setC15Val] = useState('');
  const [c30, setC30Val] = useState('');
  const [dealer, setDealerVal] = useState('');
  const [l1, setL1] = useState('');
  const [l2, setL2] = useState('');
  const [l3, setL3] = useState('');
  const [minDep, setMinDep_] = useState('');
  const [cashDelay, setCashDelay_] = useState('');
  const [observer, setObserver_] = useState('');

  useEffect(() => {
    if (!config) return;
    setSimpleVal(config.simple21DayPpPerIcp.toString());
    setC15Val(config.compounding15DayPpPerIcp.toString());
    setC30Val(config.compounding30DayPpPerIcp.toString());
    setDealerVal(config.backerPpPerIcp.toString());
    setL1(config.referralL1Bps.toString());
    setL2(config.referralL2Bps.toString());
    setL3(config.referralL3Bps.toString());
    setMinDep_(config.minDepositPp.toString());
    setCashDelay_(config.cashOutDelaySeconds.toString());
    setObserver_(config.observerIntervalSeconds.toString());
  }, [config]);

  const run = async (label: string, fn: () => Promise<unknown>) => {
    try {
      await fn();
      toast.success(`${label} updated`);
    } catch (e: any) {
      toast.error(e?.message ?? 'Update failed');
    }
  };

  const row = (
    label: string,
    hint: string,
    value: string,
    onChange: (v: string) => void,
    onSave: () => Promise<unknown>,
  ) => (
    <div className="grid grid-cols-[1fr_auto_auto] gap-2 items-end">
      <AdminInput label={label} hint={hint} type="number" value={value} onChange={onChange} />
      <button
        className="mc-btn mc-btn-secondary"
        onClick={() => run(label, onSave)}
      >
        <Save className="h-4 w-4" />
      </button>
    </div>
  );

  return (
    <div className="mc-card mc-accent-purple p-4">
      <div className="flex items-start gap-3 mb-3">
        <SlidersHorizontal className="h-5 w-5 mc-text-purple flex-shrink-0 mt-0.5" />
        <div className="text-sm mc-text-dim space-y-1">
          <p className="font-bold mc-text-purple">Mint Rules & Economy</p>
          <p>Tunable knobs for PP issuance, referral cascades, deposit floor, and cash-out windows. Changes apply to future events only — past mints are final.</p>
        </div>
      </div>
      {isLoading || !config ? (
        <div className="flex justify-center p-4"><LoadingSpinner /></div>
      ) : (
        <div className="space-y-3">
          {row('Simple 21-day PP per ICP', 'Default 1000', simple, setSimpleVal,
            () => setSimple.mutateAsync([BigInt(simple)]))}
          {row('Compounding 15-day PP per ICP', 'Default 2000', c15, setC15Val,
            () => setC15.mutateAsync([BigInt(c15)]))}
          {row('Compounding 30-day PP per ICP', 'Default 3000', c30, setC30Val,
            () => setC30.mutateAsync([BigInt(c30)]))}
          {row('Dealer (Seed Round) PP per ICP', 'Default 4000', dealer, setDealerVal,
            () => setDealer.mutateAsync([BigInt(dealer)]))}
          <div className="grid grid-cols-[1fr_1fr_1fr_auto] gap-2 items-end">
            <AdminInput label="Referral L1 BPS" hint="Default 800" type="number" value={l1} onChange={setL1} />
            <AdminInput label="Referral L2 BPS" hint="Default 500" type="number" value={l2} onChange={setL2} />
            <AdminInput label="Referral L3 BPS" hint="Default 200" type="number" value={l3} onChange={setL3} />
            <button
              className="mc-btn mc-btn-secondary"
              onClick={() => run('Referral BPS',
                () => setRefBps.mutateAsync([BigInt(l1), BigInt(l2), BigInt(l3)]))}
            >
              <Save className="h-4 w-4" />
            </button>
          </div>
          {row('Min chip deposit (whole PP)', 'Default 5000', minDep, setMinDep_,
            () => setMinDep.mutateAsync([BigInt(minDep)]))}
          {row('Cash-out delay (seconds)', 'Default 604800 = 7 days', cashDelay, setCashDelay_,
            () => setCashDelay.mutateAsync([BigInt(cashDelay)]))}
          {row('Observer interval (seconds)', 'Default 10', observer, setObserver_,
            () => setObserver.mutateAsync([BigInt(observer)]))}
        </div>
      )}
    </div>
  );
}

/* ================================================================
   Observer status — minting pipeline health indicator.
   ================================================================ */
function ObserverStatusSection() {
  const { data: status, isLoading } = useGetObserverStatus();
  const stop = useStopObserver();
  const resume = useResumeObserver();

  const handleStop = async () => {
    try { await stop.mutateAsync(); toast.success('Observer paused'); }
    catch (e: any) { toast.error(e?.message ?? 'Stop failed'); }
  };
  const handleResume = async () => {
    try { await resume.mutateAsync(); toast.success('Observer resumed'); }
    catch (e: any) { toast.error(e?.message ?? 'Resume failed'); }
  };

  const running = status?.running ?? false;
  const badge = running
    ? <span className="mc-text-green font-bold">● Running</span>
    : <span className="mc-text-danger font-bold">● Paused</span>;

  return (
    <div className="mc-card mc-accent-cyan p-4">
      <div className="flex items-start gap-3 mb-3">
        <Waves className="h-5 w-5 mc-text-cyan flex-shrink-0 mt-0.5" />
        <div className="text-sm mc-text-dim space-y-1 flex-1">
          <div className="flex items-center justify-between">
            <p className="font-bold mc-text-cyan">Observer</p>
            {isLoading ? <span className="mc-text-muted">…</span> : badge}
          </div>
          <p>Polls the backend for new games and dealer deposits, then mints PP into chip subaccounts.</p>
        </div>
      </div>
      {status && (
        <div className="grid grid-cols-3 gap-2 text-xs mb-3">
          <div className="mc-card p-2 text-center">
            <div className="mc-label mb-1">Game cursor</div>
            <div className="font-bold mc-text-primary">{status.gameIdCursor.toString()}</div>
          </div>
          <div className="mc-card p-2 text-center">
            <div className="mc-label mb-1">Backers tracked</div>
            <div className="font-bold mc-text-primary">{status.backerSeenCount.toString()}</div>
          </div>
          <div className="mc-card p-2 text-center">
            <div className="mc-label mb-1">Interval</div>
            <div className="font-bold mc-text-primary">{status.intervalSeconds.toString()}s</div>
          </div>
        </div>
      )}
      <div className="flex gap-2">
        <button className="mc-btn mc-btn-secondary" disabled={!running} onClick={handleStop}>Pause</button>
        <button className="mc-btn mc-btn-primary" disabled={running} onClick={handleResume}>Resume</button>
      </div>
    </div>
  );
}

/* ================================================================
   Trollbox admin controls — pin, mute, post as Reginald.
   ================================================================ */
function ChimeSoundsSubsection() {
  const { data: list = [] } = useListChimeSounds();
  const upload = useAdminUploadChimeSound();
  const del = useAdminDeleteChimeSound();
  const actor = useReadShenaniganActor();

  const [name, setName] = useState('');
  const [file, setFile] = useState<File | null>(null);

  const handleUpload = async () => {
    if (!file) return;
    const finalName = name.trim() || file.name;
    try {
      const buf = await file.arrayBuffer();
      await upload.mutateAsync({ name: finalName, mimeType: file.type || 'audio/mpeg', bytes: new Uint8Array(buf) });
      toast.success(`Uploaded ${finalName}`);
      setName('');
      setFile(null);
    } catch (e) {
      toast.error((e as Error).message);
    }
  };

  const handlePreview = async (n: string) => {
    if (!actor) return;
    try {
      const opt = await actor.getChimeSound(n);
      if (opt.length === 0) return;
      const sound = opt[0];
      const bytes = sound.bytes instanceof Uint8Array
        ? sound.bytes
        : new Uint8Array(sound.bytes as ArrayLike<number>);
      const url = URL.createObjectURL(new Blob([bytes], { type: sound.mimeType }));
      const audio = new Audio(url);
      audio.play().finally(() => setTimeout(() => URL.revokeObjectURL(url), 5000));
    } catch (e) {
      toast.error((e as Error).message);
    }
  };

  return (
    <div>
      <label className="block text-xs text-zinc-400">Chime sounds (pool, random per @-mention)</label>
      <div className="mt-1 space-y-1">
        {list.length === 0 && <div className="text-xs text-zinc-500 italic">No sounds uploaded yet.</div>}
        {list.map((s: any) => (
          <div key={s.name} className="flex items-center gap-2 text-xs">
            <span className="flex-1 truncate text-zinc-200">{s.name}</span>
            <span className="text-zinc-500">{Math.round(Number(s.sizeBytes) / 1024)} KB</span>
            <button onClick={() => handlePreview(s.name)} className="rounded bg-zinc-700 px-2 py-0.5 text-zinc-200">Play</button>
            <button onClick={() => del.mutate(s.name)} className="rounded bg-red-500 px-2 py-0.5 text-white">Delete</button>
          </div>
        ))}
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <input
          type="text"
          placeholder="Name (optional)"
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="flex-1 min-w-32 rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100"
        />
        <input
          type="file"
          accept="audio/*"
          onChange={(e) => setFile(e.target.files?.[0] ?? null)}
          className="text-xs text-zinc-300"
        />
        <button
          onClick={handleUpload}
          disabled={!file || upload.isPending}
          className="rounded bg-amber-500 px-2 py-1 text-xs text-zinc-900 disabled:opacity-40"
        >
          Upload
        </button>
      </div>
    </div>
  );
}

function TrollboxAdminSection() {
  const setPin = useAdminSetPin();
  const muteUser = useAdminMuteUser();
  const unmute = useAdminUnmute();
  const postReg = useAdminPostAsReginald();
  const { data: pin } = useCurrentPin();
  const [pinDraft, setPinDraft] = useState('');
  const [muteText, setMuteText] = useState('');
  const [muteDuration, setMuteDuration] = useState(3600);
  const [regDraft, setRegDraft] = useState('');

  useEffect(() => {
    if (pin && 'pinUpdate' in pin.kind) setPinDraft(pin.kind.pinUpdate.body);
  }, [pin]);

  return (
    <details className="mt-4 rounded border border-zinc-800 p-3">
      <summary className="cursor-pointer text-sm font-medium text-zinc-200">Trollbox</summary>
      <div className="mt-3 space-y-3">
        <div>
          <label className="block text-xs text-zinc-400">Pinned announcement</label>
          <textarea
            value={pinDraft}
            onChange={(e) => setPinDraft(e.target.value.slice(0, 500))}
            rows={2}
            className="mt-1 w-full rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100"
          />
          <div className="mt-1 flex gap-2">
            <button onClick={() => setPin.mutate(pinDraft)} className="rounded bg-amber-500 px-2 py-1 text-xs text-zinc-900">Save pin</button>
            <button onClick={() => { setPinDraft(''); setPin.mutate(''); }} className="rounded bg-zinc-700 px-2 py-1 text-xs text-zinc-200">Clear</button>
          </div>
        </div>

        <div>
          <label className="block text-xs text-zinc-400">Mute user (principal)</label>
          <div className="mt-1 flex gap-2">
            <input
              value={muteText}
              onChange={(e) => setMuteText(e.target.value)}
              placeholder="principal text"
              className="flex-1 rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100"
            />
            <select
              value={muteDuration}
              onChange={(e) => setMuteDuration(Number(e.target.value))}
              className="rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100"
            >
              <option value={3600}>1h</option>
              <option value={86400}>24h</option>
              <option value={604800}>7d</option>
              <option value={315360000}>~forever</option>
            </select>
            <button
              onClick={() => {
                try {
                  const p = Principal.fromText(muteText);
                  muteUser.mutate({ user: p, durationSeconds: BigInt(muteDuration) });
                } catch (e) {
                  toast.error(`Invalid principal: ${(e as Error).message}`);
                }
              }}
              className="rounded bg-red-500 px-2 py-1 text-xs text-white"
            >
              Mute
            </button>
            <button
              onClick={() => {
                try {
                  const p = Principal.fromText(muteText);
                  unmute.mutate(p);
                } catch (e) {
                  toast.error(`Invalid principal: ${(e as Error).message}`);
                }
              }}
              className="rounded bg-zinc-700 px-2 py-1 text-xs text-zinc-200"
            >
              Unmute
            </button>
          </div>
        </div>

        <div>
          <label className="block text-xs text-zinc-400">Post as Reginald</label>
          <textarea
            value={regDraft}
            onChange={(e) => setRegDraft(e.target.value.slice(0, 280))}
            rows={2}
            className="mt-1 w-full rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100"
          />
          <div className="mt-1">
            <button
              onClick={() => { postReg.mutate(regDraft); setRegDraft(''); }}
              disabled={!regDraft.trim()}
              className="rounded bg-amber-500 px-2 py-1 text-xs text-zinc-900 disabled:opacity-40"
            >
              Post
            </button>
          </div>
        </div>

        <ChimeSoundsSubsection />
      </div>
    </details>
  );
}

/* ================================================================
   Flavor pools admin section — editable text pools for Reginald
   lines, rename spell names, and spell-cast quotes.
   ================================================================ */

const FLAVOR_POOLS: Array<{ key: string; label: string; description: string; frontendOnly?: boolean }> = [
  { key: 'renameNamePool', label: 'Rename spell name pool', description: 'Names picked at random when a player gets renamed by the spell.' },
  { key: 'reginald.spellBackfire', label: 'Reginald: spell backfire', description: 'Fires ~25% of the time after a backfired shenanigan.' },
  { key: 'reginald.rankUp', label: 'Reginald: rank-up', description: 'Fires after every promotion to Affiliate or higher.' },
  { key: 'reginald.roundResult', label: 'Reginald: round result', description: 'Fires ~15% of the time after a round resolves.' },
  { key: 'reginald.buzzword', label: 'Reginald: buzzword', description: 'Fires when a chat message contains "guaranteed", "no risk", "100%", or "pump".' },
  { key: 'reginald.karma', label: 'Reginald: karma burn', description: 'Fires after a karma reaction of ≥100 PP.' },
  { key: 'spellFlavor.success', label: 'Spell-cast: success flavor', description: 'Quote shown after a successful spell cast.', frontendOnly: true },
  { key: 'spellFlavor.fail', label: 'Spell-cast: fail flavor', description: 'Quote shown after a fizzled spell cast.', frontendOnly: true },
  { key: 'spellFlavor.backfire', label: 'Spell-cast: backfire flavor', description: 'Quote shown after a backfired spell cast.', frontendOnly: true },
];

function FlavorPoolEditor({ pool }: { pool: typeof FLAVOR_POOLS[number] }) {
  const { data: overrides = [] } = useListFlavorPools();
  const { data: backendDefaults = [] } = useGetFlavorPoolDefaults(pool.frontendOnly ? null : pool.key);
  const setPool = useAdminSetFlavorPool();
  const clearPool = useAdminClearFlavorPool();

  const override = overrides.find(([n]) => n === pool.key);
  const defaults = pool.frontendOnly
    ? (SPELL_FLAVOR_DEFAULTS[pool.key as SpellFlavorKey] ?? [])
    : backendDefaults;
  const effective = override ? override[1] : defaults;
  const isOverridden = !!override;

  const [draft, setDraft] = useState<string>(effective.join('\n'));
  const [editing, setEditing] = useState(false);

  useEffect(() => {
    if (!editing) setDraft(effective.join('\n'));
  }, [effective.join('\n'), editing]);

  const handleSave = async () => {
    const lines = draft.split('\n').map(s => s.trim()).filter(s => s.length > 0);
    try {
      await setPool.mutateAsync({ name: pool.key, lines });
      toast.success(`Saved ${pool.label}`);
      setEditing(false);
    } catch (e) {
      toast.error((e as Error).message);
    }
  };

  const handleReset = async () => {
    try {
      await clearPool.mutateAsync(pool.key);
      toast.success(`Reverted ${pool.label} to defaults`);
      setEditing(false);
    } catch (e) {
      toast.error((e as Error).message);
    }
  };

  return (
    <div className="rounded border border-zinc-800 p-2">
      <div className="flex items-baseline justify-between gap-2">
        <div>
          <span className="text-sm font-medium text-zinc-200">{pool.label}</span>
          <span className={`ml-2 rounded px-1.5 py-0.5 text-[10px] ${isOverridden ? 'bg-amber-500/20 text-amber-300' : 'bg-zinc-700 text-zinc-400'}`}>
            {isOverridden ? 'Customized' : 'Default'}
          </span>
        </div>
        <span className="text-[10px] text-zinc-500">{effective.length} line{effective.length === 1 ? '' : 's'}</span>
      </div>
      <p className="mt-1 text-[11px] text-zinc-500">{pool.description}</p>
      <textarea
        value={draft}
        onChange={(e) => { setEditing(true); setDraft(e.target.value); }}
        rows={Math.max(3, Math.min(10, effective.length + 1))}
        spellCheck={false}
        className="mt-2 w-full resize-y rounded bg-zinc-800 px-2 py-1 font-mono text-xs text-zinc-100"
      />
      <div className="mt-1 flex gap-2">
        <button
          onClick={handleSave}
          disabled={!editing || setPool.isPending}
          className="rounded bg-amber-500 px-2 py-1 text-xs text-zinc-900 disabled:opacity-40"
        >
          Save
        </button>
        <button
          onClick={handleReset}
          disabled={!isOverridden || clearPool.isPending}
          className="rounded bg-zinc-700 px-2 py-1 text-xs text-zinc-200 disabled:opacity-40"
        >
          Reset to defaults
        </button>
      </div>
    </div>
  );
}

function FlavorPoolsSection() {
  return (
    <details className="mt-4 rounded border border-zinc-800 p-3">
      <summary className="cursor-pointer text-sm font-medium text-zinc-200">Flavor pools</summary>
      <div className="mt-3 space-y-3">
        {FLAVOR_POOLS.map((p) => (
          <FlavorPoolEditor key={p.key} pool={p} />
        ))}
      </div>
    </details>
  );
}
