/** Founder's Allocation — Charles's-office admin panel (MVP): inventory, tiers, cash out. */
import { useState } from 'react';
import { Principal } from '@dfinity/principal';
import { toast } from 'sonner';
import { useWallet } from '../hooks/useWallet';
import { isCharles } from '../lib/charles';
import { useDeskTiers, useDeskStats, useDeskAddTier, useDeskRemoveTier, useDeskDepositInventory, useDeskWithdrawInventory, useWithdrawDeskProceeds } from '../hooks/useDeskAdmin';
import { tierRateToUnits, unitsToTierRate, formatPpUnits } from '../lib/ppDesk';
import { wholePpToUnits } from '../hooks/usePpLedger';
import { formatSOL } from '../solana/lamports';
import LoadingSpinner from './LoadingSpinner';

function Field({ label, value, onChange, placeholder }: { label: string; value: string; onChange: (v: string) => void; placeholder?: string }) {
  return (
    <label className="block">
      <span className="block text-xs font-bold uppercase tracking-wider text-white/50 mb-1.5">{label}</span>
      <input className="w-full bg-white/5 border border-white/10 rounded-lg px-3 py-2.5 text-sm text-white placeholder-white/30 focus:outline-none focus:border-purple-500/50 font-body"
        value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} inputMode="decimal" />
    </label>
  );
}

export default function PpDeskPanel() {
  const { principal, solanaPubkey } = useWallet();
  const { data: tiers } = useDeskTiers();
  const { data: stats } = useDeskStats();
  const addTier = useDeskAddTier();
  const removeTier = useDeskRemoveTier();
  const deposit = useDeskDepositInventory();
  const withdrawPp = useDeskWithdrawInventory();
  const withdrawProceeds = useWithdrawDeskProceeds();

  const [depositPp, setDepositPp] = useState('');
  const [withdrawPpAmt, setWithdrawPpAmt] = useState('');
  const [newRate, setNewRate] = useState('');
  const [newQty, setNewQty] = useState('');

  if (!principal || !isCharles(principal)) {
    return <div className="text-center py-12 mc-text-muted text-sm">Charles only.</div>;
  }

  const num = (s: string) => { const n = Number(s); return Number.isFinite(n) && n > 0 ? n : 0; };

  return (
    <div className="space-y-6">
      {/* Inventory & stats */}
      <div className="mc-card-elevated p-5">
        <h3 className="font-display text-lg mc-text-gold mb-3">Inventory & Stats</h3>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-center mb-4">
          <div><div className="mc-label">Available</div><div className="font-bold mc-text-primary">{stats ? formatPpUnits(stats.availableUnits) : '…'} PP</div></div>
          <div><div className="mc-label">Reserved</div><div className="font-bold mc-text-primary">{stats ? formatPpUnits(stats.reservedUnits) : '…'} PP</div></div>
          <div><div className="mc-label">Sold</div><div className="font-bold mc-text-primary">{stats ? formatPpUnits(stats.totalSoldUnits) : '…'} PP</div></div>
          <div><div className="mc-label">Open buys</div><div className="font-bold mc-text-primary">{stats ? stats.openBuyIntents.toString() : '…'}</div></div>
        </div>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div className="flex items-end gap-2">
            <div className="flex-1"><Field label="Deposit PP (whole)" value={depositPp} onChange={setDepositPp} placeholder="500000" /></div>
            <button className="mc-btn-primary" disabled={deposit.isPending || num(depositPp) === 0} onClick={() => deposit.mutate({ units: wholePpToUnits(Math.trunc(num(depositPp))) }, { onSuccess: () => setDepositPp('') })}>{deposit.isPending ? <LoadingSpinner /> : 'Deposit'}</button>
          </div>
          <div className="flex items-end gap-2">
            <div className="flex-1"><Field label="Withdraw PP (whole)" value={withdrawPpAmt} onChange={setWithdrawPpAmt} placeholder="100000" /></div>
            <button className="mc-btn-secondary" disabled={withdrawPp.isPending || num(withdrawPpAmt) === 0} onClick={() => withdrawPp.mutate({ units: wholePpToUnits(Math.trunc(num(withdrawPpAmt))), to: Principal.fromText(principal) }, { onSuccess: () => setWithdrawPpAmt('') })}>{withdrawPp.isPending ? <LoadingSpinner /> : 'Withdraw'}</button>
          </div>
        </div>
      </div>

      {/* Tiers */}
      <div className="mc-card-elevated p-5">
        <h3 className="font-display text-lg mc-text-gold mb-1">Price Ladder</h3>
        <p className="text-[11px] mc-text-muted mb-3">Top = best deal. Rate is whole PP per 0.1 SOL; quantity is whole PP. Early buyers get the top tier.</p>
        <div className="space-y-2 mb-4">
          {(tiers ?? []).map((t, i) => (
            <div key={i} className="mc-card p-3 flex flex-wrap items-center gap-3 text-sm">
              <span className="mc-text-muted">#{i}</span>
              <span className="mc-text-primary font-bold">{unitsToTierRate(t.ratePpUnitsPer0_1Sol).toLocaleString()} PP / 0.1 SOL</span>
              <span className="mc-text-muted">{formatPpUnits(t.ppUnitsSold)} / {formatPpUnits(t.ppUnitsTotal)} sold · {formatPpUnits(t.ppUnitsReserved)} reserved</span>
              <button className="mc-btn-secondary text-xs ml-auto" disabled={removeTier.isPending} onClick={() => removeTier.mutate({ index: BigInt(i) })}>Remove</button>
            </div>
          ))}
          {(tiers ?? []).length === 0 && <div className="mc-text-muted text-sm">No tiers yet — add one below to open the desk.</div>}
        </div>
        <div className="flex flex-wrap items-end gap-2">
          <div className="flex-1 min-w-[140px]"><Field label="Rate (PP / 0.1 SOL)" value={newRate} onChange={setNewRate} placeholder="250000" /></div>
          <div className="flex-1 min-w-[140px]"><Field label="Quantity (PP)" value={newQty} onChange={setNewQty} placeholder="1000000" /></div>
          <button className="mc-btn-primary" disabled={addTier.isPending || num(newRate) === 0 || num(newQty) === 0}
            onClick={() => { addTier.mutate({ rateUnits: tierRateToUnits(num(newRate)), qtyUnits: wholePpToUnits(Math.trunc(num(newQty))) }); setNewRate(''); setNewQty(''); }}>
            {addTier.isPending ? <LoadingSpinner /> : 'Add tier'}
          </button>
        </div>
      </div>

      {/* Cash out */}
      <div className="mc-card-elevated p-5">
        <h3 className="font-display text-lg mc-text-gold mb-3">Cash Out</h3>
        <div className="flex items-center justify-between gap-3">
          <div><div className="mc-label">Accrued proceeds</div><div className="font-bold mc-text-gold">{stats ? formatSOL(stats.proceedsLamports) : '…'} SOL</div></div>
          <button className="mc-btn-primary" disabled={withdrawProceeds.isPending || !solanaPubkey || (stats?.proceedsLamports ?? 0n) === 0n}
            onClick={() => { if (!solanaPubkey) { toast.error('No Phantom address on this session'); return; } withdrawProceeds.mutate({ toAddress: solanaPubkey }); }}>
            {withdrawProceeds.isPending ? <LoadingSpinner /> : 'Withdraw SOL to Phantom'}
          </button>
        </div>
      </div>
    </div>
  );
}
