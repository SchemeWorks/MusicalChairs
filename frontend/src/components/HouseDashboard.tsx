import React, { useState } from 'react';
import { toast } from 'sonner';
import { useWallet } from '../hooks/useWallet';
import { useGetGeneralLedger, useGetGeneralLedgerStats, useGetBackerPositions, useGetGameStats, useGetAllBackerRepayments, useClaimDealerRepayment, useGetUserNames, useGetMintConfig, useGetSolBackerPositions, useGetSolAllBackerRepayments, useGetSolGeneralLedger, useGetSolGeneralLedgerStats, useClaimSolBackerRepayment } from '../hooks/useQueries';
import type { GeneralLedgerEntry, BackerPosition, BackerKey } from '../backend';
import LoadingSpinner from './LoadingSpinner';
import AddBackerMoney from './AddBackerMoney';
import ClaimRepaymentToast from './ClaimRepaymentToast';
import { triggerConfetti } from './ConfettiCanvas';
import { formatICP } from '../lib/formatICP';
import { formatSolFloat } from '../solana/lamports';
import { Progress } from '@/components/ui/progress';
import { Info, DollarSign, TrendingUp, Shield, Zap, Landmark, BarChart3, Flame, Coins, Banknote, Gem, Dice5, ArrowDownLeft, ArrowUpRight } from 'lucide-react';

/* ================================================================
   Tab Control
   ================================================================ */
function TabControl({ activeTab, onTabChange, backerCount, ledgerCount }: {
  activeTab: 'backers' | 'ledger';
  onTabChange: (t: 'backers' | 'ledger') => void;
  backerCount?: number;
  ledgerCount?: number;
}) {
  return (
    <div className="flex justify-center mb-6">
      <div className="inline-flex rounded-full bg-white/5 border border-white/10 p-1 gap-1">
        {(['backers', 'ledger'] as const).map(tab => (
          <button
            key={tab}
            onClick={() => onTabChange(tab)}
            className={`px-6 py-2.5 rounded-full text-sm font-bold transition-all ${
              activeTab === tab
                ? 'bg-[var(--mc-purple)]/30 mc-text-primary border border-[var(--mc-purple)]/40 shadow-[0_0_12px_rgba(168,85,247,0.2)]'
                : 'mc-text-muted hover:mc-text-dim hover:bg-white/5'
            }`}
          >
            {tab === 'backers' ? (
              <><Landmark className="h-4 w-4 inline mr-1" /> Backers{backerCount !== undefined && <span className="mc-text-muted ml-1">({backerCount})</span>}</>
            ) : (
              <><BarChart3 className="h-4 w-4 inline mr-1" /> Ledger{ledgerCount !== undefined && <span className="mc-text-muted ml-1">({ledgerCount})</span>}</>
            )}
          </button>
        ))}
      </div>
    </div>
  );
}

/* ================================================================
   Backer Info (How Backer Positions Work)
   ================================================================ */
function BackerInfoCard() {
  const { walletType } = useWallet();
  const isSol = walletType === 'siws';
  const unit = isSol ? 'SOL' : 'ICP';
  const { data: mintConfig } = useGetMintConfig();
  const ppPerIcp = mintConfig ? Number(isSol ? mintConfig.backerPpPerSol : mintConfig.backerPpPerIcp) : 0;
  const sections = [
    {
      icon: <Info className="h-5 w-5 mc-text-cyan" />,
      title: 'What Are Backer Positions?',
      accent: 'mc-accent-cyan',
      content: (
        <div className="text-xs mc-text-dim space-y-1">
          <p><strong className="mc-text-primary">Series A Backers:</strong> Visionary early investors who fund the project voluntarily</p>
          <p><strong className="mc-text-primary">Series B Backers:</strong> Investors promoted via Emergency Equity Conversion</p>
          <p className="mt-2">Series A backers earn a <strong className="mc-text-green">24% return</strong>. Series B earns <strong className="mc-text-green">16%</strong>. Plus direct fee payments.</p>
        </div>
      ),
    },
    {
      icon: <DollarSign className="h-5 w-5 mc-text-green" />,
      title: 'How Repayment Works',
      accent: 'mc-accent-green',
      content: (
        <div className="text-xs mc-text-dim space-y-1">
          <p>Of the 50% of fees earmarked for backer repayment:</p>
          <p><strong className="mc-text-primary">35%</strong> goes to the earliest Series A Backer</p>
          <p><strong className="mc-text-primary">25%</strong> split evenly among other Series A Backers</p>
          <p><strong className="mc-text-primary">40%</strong> split evenly among all backers</p>
        </div>
      ),
    },
    {
      icon: <TrendingUp className="h-5 w-5 mc-text-green" />,
      title: 'Guaranteed* Returns',
      accent: 'mc-accent-green',
      content: (
        <p className="text-xs mc-text-dim">
          Series A: <strong className="mc-text-green">1.24 {unit} back per {unit} (24% bonus)</strong>.
          Series B: <strong className="mc-text-green">1.16 {unit} back per {unit} (16% bonus)</strong>.
          Repaid automatically through platform fees.
        </p>
      ),
    },
    {
      icon: <Shield className="h-5 w-5 mc-text-gold" />,
      title: 'Risk & Rewards',
      accent: 'mc-accent-gold',
      content: (
        <div className="text-xs mc-text-dim space-y-1">
          <p>Repayment depends on platform activity. More investors = faster repayment.
          You also earn <strong className="mc-text-purple">{ppPerIcp.toLocaleString()} Ponzi Points per {unit}</strong> deposited.</p>
          <p className="font-accent italic mc-text-muted">Management reserves the right to a modest operational fee on every deposit.</p>
        </div>
      ),
    },
  ];

  return (
    <div className="space-y-4">
      <h3 className="font-display text-base mc-text-primary text-center">How Backer Positions Work</h3>
      <p className="text-center text-sm mc-text-dim italic font-accent">
        Become a VC — put your money in someone else's project and call it strategy.
      </p>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        {sections.map(s => (
          <div key={s.title} className={`mc-card ${s.accent} p-4`}>
            <div className="flex items-center gap-2 mb-2">
              {s.icon}
              <span className="font-bold text-sm mc-text-primary">{s.title}</span>
            </div>
            <div>{s.content}</div>
          </div>
        ))}
      </div>

      {/* Redistribution Event callout — always visible, dramatic treatment */}
      <div className="mc-card mc-accent-danger mc-redistribution-pulse p-5">
        <div className="flex items-start gap-3">
          <Flame className="h-6 w-6 mc-text-danger flex-shrink-0" />
          <div>
            <h4 className="font-display text-sm mc-text-danger mb-2 flex items-center gap-2">
              Emergency Equity Conversion <Zap className="h-4 w-4" />
            </h4>
            <div className="text-xs mc-text-dim space-y-1 leading-relaxed">
              <p><strong className="mc-text-primary">When AUM empties:</strong> A random unprofitable depositor is promoted to Series B Investor.</p>
              <p><strong className="mc-text-primary">Entitlement:</strong> Whatever they were underwater, plus the Series B return.</p>
              <p><strong className="mc-text-primary">Multiple backers</strong> can coexist, sharing fee payments via the distribution system.</p>
              <p className="mc-text-muted italic mt-2">When the fund runs dry, management restructures. Standard operating procedure.</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

/* ================================================================
   House Ledger Records
   ================================================================ */
// Derive a human-readable event type and summary from the tagged GeneralLedgerEvent variant.
function describeEvent(entry: GeneralLedgerEntry): { label: string; isInflow: boolean | null } {
  const eventKey = Object.keys(entry.event)[0];
  const eventData: any = (entry.event as any)[eventKey];
  switch (eventKey) {
    case 'deposit': return { label: `Deposit — ${Object.keys(eventData.plan)[0]}`, isInflow: true };
    case 'backerDeposit': return { label: 'Backer Deposit', isInflow: true };
    case 'withdrawal': return { label: `Withdrawal${eventData.isInsolvent ? ' (Insolvent)' : ''}`, isInflow: false };
    case 'settlement': return { label: `Settlement${eventData.isInsolvent ? ' (Insolvent)' : ''}`, isInflow: false };
    case 'tollDistribution': return { label: 'Toll Distribution', isInflow: null };
    case 'backerRepaymentClaim': return { label: 'Backer Repayment Claim', isInflow: false };
    case 'coverChargeAccrued': return { label: 'Front-End Load Accrued', isInflow: null };
    case 'coverChargeSwept': return { label: 'Front-End Load Swept', isInflow: null };
    case 'gameReset': return { label: `Game Reset — ${eventData.reason}`, isInflow: null };
    case 'backdatedGameCreated': return { label: 'Backdated Game Created', isInflow: null };
    default: return { label: eventKey, isInflow: null };
  }
}

function HouseLedgerRecords() {
  const { walletType } = useWallet();
  const isSol = walletType === 'siws';
  const fmt = (n: number) => (isSol ? formatSolFloat(n) : formatICP(n));
  const unit = isSol ? 'SOL' : 'ICP';

  const icpLedger = useGetGeneralLedger();
  const solLedger = useGetSolGeneralLedger();
  const { data: ledgerRecordsRaw = [], isLoading, error, refetch } = isSol ? solLedger : icpLedger;
  const ledgerRecords = ledgerRecordsRaw as GeneralLedgerEntry[];

  const icpStats = useGetGeneralLedgerStats();
  const solStats = useGetSolGeneralLedgerStats();
  const { data: ledgerStats, isLoading: statsLoading, error: statsError, refetch: refetchStats } = isSol ? solStats : icpStats;

  if (error || statsError) {
    return (
      <div className="mc-status-red p-4 text-center text-sm">
        <p className="mb-2">Failed to load ledger data.</p>
        <button onClick={() => { refetch(); refetchStats(); }} className="mc-btn-secondary px-4 py-2 text-xs rounded-lg">
          Retry
        </button>
      </div>
    );
  }

  if (isLoading || statsLoading) return <LoadingSpinner />;

  const fmtDate = (ts: bigint) => {
    try {
      return new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(new Date(Number(ts) / 1000000));
    } catch { return 'Invalid Date'; }
  };

  const stats = ledgerStats || { totalInflows: 0, totalOutflows: 0, netFlow: 0, entryCount: BigInt(0) };
  const records = Array.isArray(ledgerRecords) ? ledgerRecords : [];

  return (
    <div className="space-y-6">
      {/* Stats row */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        {[
          { label: 'Total Inflows', value: `${fmt(stats.totalInflows)} ${unit}`, color: 'mc-text-green' },
          { label: 'Total Outflows', value: `${fmt(stats.totalOutflows)} ${unit}`, color: 'mc-text-pink' },
          { label: 'Net Flow', value: `${fmt(stats.netFlow)} ${unit}`, color: 'mc-text-cyan' },
          { label: 'Total Records', value: `${Number(stats.entryCount)}`, color: 'mc-text-purple' },
        ].map(s => (
          <div key={s.label} className="mc-card p-3 text-center">
            <div className="mc-label mb-1">{s.label}</div>
            <div className={`text-lg font-bold ${s.color}`}>{s.value}</div>
          </div>
        ))}
      </div>

      {/* Transaction Timeline */}
      {records.length === 0 ? (
        <div className="text-center py-8">
          <BarChart3 className="h-10 w-10 mc-text-muted mb-3 mx-auto opacity-40" />
          <p className="mc-text-dim text-sm">No ledger records yet.</p>
        </div>
      ) : (
        <div className="max-h-96 overflow-y-auto">
          {records
            .sort((a, b) => Number(b.timestamp) - Number(a.timestamp))
            .map((record, i) => {
              const { label, isInflow } = describeEvent(record);
              const dotClass = isInflow === true ? 'mc-bg-green' : isInflow === false ? 'mc-bg-danger' : 'bg-white/30';
              const arrowEl = isInflow === true
                ? <ArrowDownLeft className="h-3.5 w-3.5 mc-text-green flex-shrink-0" />
                : isInflow === false
                ? <ArrowUpRight className="h-3.5 w-3.5 mc-text-danger flex-shrink-0" />
                : null;
              return (
                <div key={Number(record.id)} className="flex gap-3 py-3 border-b border-white/5 last:border-0">
                  {/* Timeline dot + line */}
                  <div className="flex flex-col items-center">
                    <div className={`w-2.5 h-2.5 rounded-full mt-1 flex-shrink-0 ${dotClass}`} />
                    {i < records.length - 1 && (
                      <div className="w-px flex-1 bg-white/10 mt-1" />
                    )}
                  </div>
                  {/* Content */}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      {arrowEl}
                      <span className="text-sm font-bold truncate mc-text-primary">{label}</span>
                    </div>
                    <div className="flex justify-between mt-1 text-xs mc-text-muted">
                      <span>{fmtDate(record.timestamp)}</span>
                    </div>
                  </div>
                </div>
              );
            })}
        </div>
      )}
    </div>
  );
}

/* ================================================================
   Backer Positions
   ================================================================ */
function BackerPositions() {
  const { principal, walletType } = useWallet();
  const isSol = walletType === 'siws';
  const fmt = (n: number) => (isSol ? formatSolFloat(n) : formatICP(n));
  const unit = isSol ? 'SOL' : 'ICP';

  const icpBackers = useGetBackerPositions();
  const solBackers = useGetSolBackerPositions();
  const { data: backerPositions = [], isLoading, error, refetch } = isSol ? solBackers : icpBackers;

  const icpRepay = useGetAllBackerRepayments();
  const solRepay = useGetSolAllBackerRepayments();
  const { data: repaymentEntries = [] } = isSol ? solRepay : icpRepay;

  const claimIcp = useClaimDealerRepayment();
  const claimSol = useClaimSolBackerRepayment();
  const claimRepayment = isSol ? claimSol : claimIcp;

  const rawBackers = (Array.isArray(backerPositions) ? backerPositions : []) as BackerPosition[];
  const { data: nameByPrincipal } = useGetUserNames(rawBackers.map(b => b.owner.toString()));

  const [claimToast, setClaimToast] = useState<{ amount: number } | null>(null);

  const handleClaim = async (amount: number) => {
    try {
      const paid = await claimRepayment.mutateAsync();
      triggerConfetti();
      setClaimToast({ amount: Number(paid) });
    } catch (err: any) {
      toast.error(err?.message || `Claim failed (had ${fmt(amount)} ${unit} claimable)`);
    }
  };

  const backerKeyId = (principal: string, type: { seriesA?: null; seriesB?: null }) =>
    `${principal}-${'seriesA' in type ? 'A' : 'B'}`;

  const repaidByKey = new Map<string, number>(
    (repaymentEntries as Array<[BackerKey, number]>).map(([[p, t], v]) => [backerKeyId(p.toString(), t), v])
  );

  if (error) {
    return (
      <div className="mc-status-red p-4 text-center text-sm">
        <p className="mb-2">Failed to load backer data.</p>
        <button onClick={() => refetch()} className="mc-btn-secondary px-4 py-2 text-xs rounded-lg">Retry</button>
      </div>
    );
  }

  if (isLoading) return <LoadingSpinner />;

  const backers = [...rawBackers].sort((a, b) => {
    const aIsA = 'seriesA' in a.backerType;
    const bIsA = 'seriesA' in b.backerType;
    if (aIsA !== bIsA) return aIsA ? -1 : 1;
    return Number(a.startTime - b.startTime);
  });

  const fmtDate = (ts: bigint) => {
    try {
      return new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(new Date(Number(ts) / 1000000));
    } catch { return 'Invalid Date'; }
  };

  const totalHouseMoney = backers.reduce((s, d) => s + (d.amount || 0), 0);
  const totalEntitlement = backers.reduce((s, d) => s + (d.entitlement || 0), 0);
  const totalRepaid = backers.reduce(
    (s, d) => s + (repaidByKey.get(backerKeyId(d.owner.toString(), d.backerType)) || 0),
    0,
  );
  const outstandingDebt = Math.max(0, totalEntitlement - totalRepaid);

  return (
    <div className="space-y-6">
      {/* Fund the Project — deposit card with warning inline + stats side by side */}
      <div className="grid grid-cols-1 lg:grid-cols-[1fr_auto] gap-4">
        <div className="mc-card-elevated p-6">
          <h3 className="font-display text-lg mc-text-gold mb-3">Fund the Project</h3>
          <AddBackerMoney />
        </div>

        {/* Stats stacked to the right — two cards matching Fund the Project height */}
        <div className="flex flex-col gap-3 lg:w-56">
          <div className="mc-card mc-accent-danger p-5 text-center flex-1 flex flex-col justify-center">
            <div className="mc-label mb-1">Outstanding Backer Debt</div>
            <div className="text-xl font-bold mc-text-danger">{fmt(outstandingDebt)} {unit}</div>
          </div>
          <div className="mc-card mc-accent-cyan p-5 text-center flex-1 flex flex-col justify-center">
            <div className="mc-label mb-1">Total Funds Raised</div>
            <div className="text-xl font-bold mc-text-cyan">{fmt(totalHouseMoney)} {unit}</div>
          </div>
        </div>
      </div>

      {/* Backer list */}
      {backers.length > 0 ? (
        <div className="space-y-3">
          <h3 className="font-display text-base mc-text-primary text-center">Current Backers</h3>
          {backers.map(backer => {
            const repaid = repaidByKey.get(backerKeyId(backer.owner.toString(), backer.backerType)) || 0;
            const repayPct = backer.entitlement > 0
              ? (repaid / backer.entitlement) * 100
              : 0;
            const remaining = Math.max(0, backer.entitlement - repaid);
            const isSeriesA = 'seriesA' in backer.backerType;

            return (
              <div
                key={backerKeyId(backer.owner.toString(), backer.backerType)}
                className={`mc-card p-5 ${isSeriesA ? 'mc-accent-green' : 'mc-accent-gold'}`}
              >
                <div className="grid grid-cols-1 lg:grid-cols-3 gap-4 items-center">
                  {/* Info */}
                  <div className="flex items-center gap-3">
                    <div className={`w-10 h-10 rounded-lg flex items-center justify-center text-xl ${
                      isSeriesA ? 'bg-[var(--mc-neon-green)]/20' : 'bg-[var(--mc-gold)]/20'
                    }`}>
                      {isSeriesA ? <Gem className="h-5 w-5 mc-text-green" /> : <Dice5 className="h-5 w-5 mc-text-gold" />}
                    </div>
                    <div>
                      <div className="font-bold mc-text-primary">
                        {(() => {
                          const owner = backer.owner.toString();
                          if (owner === principal) return 'My Equity';
                          const name = nameByPrincipal?.get(owner);
                          return name && name.length > 0 ? name : 'Anonymous Backer';
                        })()}
                      </div>
                      <span className={`text-xs px-2 py-0.5 rounded-full font-bold ${
                        isSeriesA ? 'bg-[var(--mc-neon-green)]/20 mc-text-green' : 'bg-[var(--mc-gold)]/20 mc-text-gold'
                      }`}>
                        {isSeriesA ? 'Series A' : 'Series B'}
                      </span>
                      <div className="text-xs mc-text-muted mt-1">
                        Joined: {fmtDate(backer.startTime)}
                      </div>
                    </div>
                  </div>

                  {/* Entitlement */}
                  <div className="md:text-center">
                    <div className="mc-label">Entitlement</div>
                    <div className={`text-xl font-bold ${isSeriesA ? 'mc-text-green' : 'mc-text-gold'}`}>
                      {fmt(backer.entitlement)} {unit}
                    </div>
                  </div>

                  {/* Repayment */}
                  <div>
                    <div className="mc-label mb-1">Repayment</div>
                    <div className="text-sm font-bold mc-text-primary mb-1">
                      {fmt(repaid)} / {fmt(backer.entitlement)} {unit}
                    </div>
                    <Progress value={Math.max(0, Math.min(100, repayPct))} className="mb-1 h-2" />
                    <div className="flex justify-between text-xs mc-text-muted">
                      <span>{Math.max(0, repayPct).toFixed(1)}% repaid</span>
                      <span className="mc-text-danger">Remaining: {fmt(remaining)} {unit}</span>
                    </div>
                  </div>
                </div>
                {backer.owner.toString() === principal && repaid > 0 && (
                  <div className="mt-3 pt-3 border-t border-white/5 flex items-center justify-between gap-3">
                    <span className="text-xs mc-text-dim">
                      <span className="mc-text-green font-bold">{fmt(repaid)} {unit}</span> ready to claim
                    </span>
                    <button
                      onClick={() => handleClaim(repaid)}
                      disabled={claimRepayment.isPending}
                      className="mc-btn-primary px-4 py-2 text-xs font-bold rounded-lg disabled:opacity-50"
                    >
                      {claimRepayment.isPending ? 'Claiming…' : 'Claim Repayment'}
                    </button>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      ) : (
        <div className="text-center py-8">
          <Landmark className="h-10 w-10 mc-text-gold mb-3 mx-auto" />
          <p className="font-display text-sm mc-text-primary mb-1">No backers yet</p>
          <p className="text-sm mc-text-dim">Fund the project above to become the first Series A backer.</p>
        </div>
      )}

      <div className="mc-border-subtle border-t my-6" />
      {/* Info card */}
      <BackerInfoCard />

      {claimToast && (
        <ClaimRepaymentToast
          amount={claimToast.amount}
          onClose={() => setClaimToast(null)}
        />
      )}
    </div>
  );
}

/* ================================================================
   Main Export
   ================================================================ */
export default function HouseDashboard() {
  const [activeTab, setActiveTab] = useState<'backers' | 'ledger'>('backers');
  const { walletType } = useWallet();
  const isSol = walletType === 'siws';
  const icpBackers = useGetBackerPositions();
  const solBackers = useGetSolBackerPositions();
  const icpLedger = useGetGeneralLedger();
  const solLedger = useGetSolGeneralLedger();
  const backerPositions = (isSol ? solBackers : icpBackers).data;
  const ledgerRecords = (isSol ? solLedger : icpLedger).data;

  const backerCount = Array.isArray(backerPositions) ? backerPositions.length : 0;
  const ledgerCount = Array.isArray(ledgerRecords) ? ledgerRecords.length : 0;

  return (
    <div className="space-y-6">
      <TabControl activeTab={activeTab} onTabChange={setActiveTab} backerCount={backerCount} ledgerCount={ledgerCount} />
      <div className="mc-enter">
        {activeTab === 'backers' ? <BackerPositions /> : <HouseLedgerRecords />}
      </div>
    </div>
  );
}
