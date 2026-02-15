import React, { useState, useEffect } from 'react';
import { toast } from 'sonner';
import { Save, RotateCcw, AlertTriangle, CheckCircle, Info, ChevronRight, Coins, Waves, Pencil, Building2, Target, FlipHorizontal2, ArrowUp, Scissors, Fish, TrendingUp, Sparkles, SlidersHorizontal } from 'lucide-react';
import { CharlesIcon } from '../lib/charles';
import { useGetShenaniganConfigs, useUpdateShenaniganConfig, useSaveAllShenaniganConfigs, useResetShenaniganConfig } from '../hooks/useQueries';
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
        ? 'bg-green-500/10 text-green-400 border-green-500/30'
        : 'bg-red-500/10 text-red-400 border-red-500/30 animate-pulse'
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
        if (updated) setSelectedShenanigan(updated);
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
        <p className="mc-text-muted text-sm mt-1 font-accent italic">Pull the strings. Tweak the odds. The house edge is whatever you say it is.</p>
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

      {/* Minter reminder */}
      <div className="mc-card mc-accent-danger p-4">
        <div className="flex items-start gap-3">
          <AlertTriangle className="h-5 w-5 mc-text-danger flex-shrink-0 mt-0.5" />
          <div className="text-sm mc-text-dim space-y-1">
            <p className="font-bold mc-text-danger">Post-Deploy Ritual</p>
            <p>After each new deployment, re-authorize the backend to mint PP. Forget this and shenanigans stop working:</p>
            <code className="block bg-white/5 px-3 py-2 rounded text-xs mc-text-gold mt-1 font-mono break-all">
              dfx canister call 5xv2o-iiaaa-aaaac-qeclq-cai set_minter '(principal "YOUR_BACKEND_ID")'
            </code>
          </div>
        </div>
      </div>

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
                    onClick={() => setSelectedShenanigan(shen)}
                    className={`w-full text-left p-3 rounded-lg border transition-all group ${
                      isActive
                        ? 'border-purple-500/50 bg-purple-500/10 shadow-[0_0_15px_rgba(168,85,247,0.15)]'
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
                    <div className="bg-green-500 transition-all" style={{ width: `${selectedShenanigan.successOdds}%` }} />
                    <div className="bg-yellow-500 transition-all" style={{ width: `${selectedShenanigan.failureOdds}%` }} />
                    <div className="bg-red-500 transition-all" style={{ width: `${selectedShenanigan.backfireOdds}%` }} />
                  </div>
                  <div className="flex justify-between text-[10px] mt-1 mc-text-muted">
                    <span className="text-green-400">Success</span>
                    <span className="text-yellow-400">Fail</span>
                    <span className="text-red-400">Backfire</span>
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
                  <AdminInput label="Effect Values" value={selectedShenanigan.effectValues.join(', ')}
                    onChange={v => {
                      const values = v.split(',').map(s => parseFloat(s.trim())).filter(n => !isNaN(n));
                      updateField('effectValues', values);
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
                      bg-green-500/20 text-green-400 border border-green-500/30 hover:bg-green-500/30 hover:shadow-[0_0_20px_rgba(34,197,94,0.15)]
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
    </div>
  );
}
