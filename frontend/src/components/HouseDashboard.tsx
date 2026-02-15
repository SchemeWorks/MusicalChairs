import React, { useState } from 'react';
import { useGetHouseLedger, useGetHouseLedgerStats, useGetBackerPositions, useGetGameStats } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';
import AddHouseMoney from './AddHouseMoney';
import { formatICP } from '../lib/formatICP';
import { Progress } from '@/components/ui/progress';
import { Info, DollarSign, TrendingUp, Shield, Zap, Landmark, BarChart3, Flame, Coins, Banknote, Gem, Dice5, AlertTriangle } from 'lucide-react';

/* ================================================================
   Tab Control
   ================================================================ */
function TabControl({ activeTab, onTabChange }: { activeTab: 'backers' | 'ledger'; onTabChange: (t: 'backers' | 'ledger') => void }) {
  return (
    <div className="flex justify-center mb-6">
      <div className="inline-flex rounded-full bg-white/5 border border-white/10 p-1 gap-1">
        {(['backers', 'ledger'] as const).map(tab => (
          <button
            key={tab}
            onClick={() => onTabChange(tab)}
            className={`px-6 py-2.5 rounded-full text-sm font-bold transition-all ${
              activeTab === tab
                ? 'bg-purple-500/30 mc-text-primary border border-purple-500/40 shadow-[0_0_12px_rgba(168,85,247,0.2)]'
                : 'mc-text-muted hover:mc-text-dim hover:bg-white/5'
            }`}
          >
            {tab === 'backers' ? <><Landmark className="h-4 w-4 inline mr-1" /> Backers</> : <><BarChart3 className="h-4 w-4 inline mr-1" /> Ledger</>}
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
  const sections = [
    {
      icon: <Info className="h-5 w-5 mc-text-cyan" />,
      title: 'What Are Backer Positions?',
      accent: 'mc-accent-cyan',
      content: (
        <div className="text-xs mc-text-dim space-y-1">
          <p><strong className="mc-text-primary">Series A Backers:</strong> Users who voluntarily deposit house money</p>
          <p><strong className="mc-text-primary">Series B Backers:</strong> Users selected by The Redistribution Event</p>
          <p className="mt-2">All backers earn a <strong className="mc-text-green">12% return</strong> on investment plus direct fee payments.</p>
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
          <p><strong className="mc-text-primary">35%</strong> goes to the oldest Series A Backer</p>
          <p><strong className="mc-text-primary">25%</strong> split evenly among other Series A Backers</p>
          <p><strong className="mc-text-primary">40%</strong> split evenly among all backers</p>
        </div>
      ),
    },
    {
      icon: <TrendingUp className="h-5 w-5 mc-text-green" />,
      title: 'Guaranteed Returns',
      accent: 'mc-accent-green',
      content: (
        <p className="text-xs mc-text-dim">
          Every ICP deposited as house money entitles you to <strong className="mc-text-green">1.12 ICP back (12% bonus)</strong>.
          This debt is automatically repaid through platform fees.
        </p>
      ),
    },
    {
      icon: <Shield className="h-5 w-5 mc-text-gold" />,
      title: 'Risk & Rewards',
      accent: 'mc-accent-gold',
      content: (
        <p className="text-xs mc-text-dim">
          Repayment depends on platform activity. More players = faster repayment.
          You also earn <strong className="mc-text-purple">4,000 Ponzi Points per ICP</strong> deposited.
        </p>
      ),
    },
  ];

  const [openSection, setOpenSection] = useState<string | null>(null);

  return (
    <div className="space-y-4">
      <h3 className="font-display text-base mc-text-primary text-center">How Backer Positions Work</h3>
      <p className="text-center text-sm mc-text-dim italic font-accent">
        Become a VC — put your money in someone else's scheme and call it strategy.
      </p>
      <div className="space-y-2">
        {sections.map(s => {
          const isOpen = openSection === s.title;
          return (
            <div key={s.title} className={`mc-card ${s.accent} overflow-hidden`}>
              <button
                onClick={() => setOpenSection(isOpen ? null : s.title)}
                className="w-full flex items-center justify-between p-4 text-left"
              >
                <div className="flex items-center gap-2">
                  {s.icon}
                  <span className="font-bold text-sm mc-text-primary">{s.title}</span>
                </div>
                <span className={`text-xs mc-text-muted transition-transform ${isOpen ? 'rotate-180' : ''}`}>▼</span>
              </button>
              {isOpen && <div className="px-4 pb-4">{s.content}</div>}
            </div>
          );
        })}
      </div>

      {/* Redistribution Event callout — always visible */}
      <div className="mc-card mc-accent-danger p-5">
        <div className="flex items-start gap-3">
          <Flame className="h-6 w-6 mc-text-danger flex-shrink-0" />
          <div>
            <h4 className="font-display text-sm mc-text-danger mb-2 flex items-center gap-2">
              The Redistribution Event <Zap className="h-4 w-4" />
            </h4>
            <div className="text-xs mc-text-dim space-y-1 leading-relaxed">
              <p><strong className="mc-text-primary">When the pot empties:</strong> A random unprofitable depositor becomes a Series B Backer.</p>
              <p><strong className="mc-text-primary">Entitlement:</strong> Whatever they were underwater, plus a 12% backer bonus.</p>
              <p><strong className="mc-text-primary">Multiple backers</strong> can coexist, sharing fee payments via the distribution system.</p>
              <p className="mc-text-muted italic mt-2">This ensures the casino always has backing, even when players drain the pot.</p>
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
function HouseLedgerRecords() {
  const { data: ledgerRecords = [], isLoading, error, refetch } = useGetHouseLedger();
  const { data: ledgerStats, isLoading: statsLoading, error: statsError, refetch: refetchStats } = useGetHouseLedgerStats();

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

  const stats = ledgerStats || { totalDeposits: 0, totalWithdrawals: 0, netBalance: 0, recordCount: BigInt(0) };
  const records = Array.isArray(ledgerRecords) ? ledgerRecords : [];

  return (
    <div className="space-y-6">
      {/* Stats row */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        {[
          { label: 'Total Deposits', value: `${formatICP(stats.totalDeposits)} ICP`, color: 'mc-text-green' },
          { label: 'Total Withdrawals', value: `${formatICP(stats.totalWithdrawals)} ICP`, color: 'mc-text-pink' },
          { label: 'Net Balance', value: `${formatICP(stats.netBalance)} ICP`, color: 'mc-text-cyan' },
          { label: 'Total Records', value: `${Number(stats.recordCount)}`, color: 'mc-text-purple' },
        ].map(s => (
          <div key={s.label} className="mc-card p-3 text-center">
            <div className="mc-label mb-1">{s.label}</div>
            <div className={`text-lg font-bold ${s.color}`}>{s.value}</div>
          </div>
        ))}
      </div>

      {/* Records */}
      {records.length === 0 ? (
        <div className="text-center py-8">
          <BarChart3 className="h-10 w-10 mc-text-muted mb-3 mx-auto opacity-40" />
          <p className="mc-text-dim text-sm">No house ledger records yet.</p>
        </div>
      ) : (
        <div className="space-y-2 max-h-96 overflow-y-auto">
          {records
            .sort((a, b) => Number(b.timestamp) - Number(a.timestamp))
            .map(record => (
              <div
                key={Number(record.id)}
                className={`mc-card p-3 flex items-center justify-between ${
                  record.amount > 0 ? 'mc-accent-green' : 'mc-accent-danger'
                }`}
              >
                <div className="flex items-center gap-3">
                  {record.amount > 0 ? <Coins className="h-5 w-5 mc-text-green" /> : <Banknote className="h-5 w-5 mc-text-danger" />}
                  <div>
                    <div className="font-bold text-sm mc-text-primary">{record.description || 'House Money Transaction'}</div>
                    <div className="text-xs mc-text-muted">{fmtDate(record.timestamp)}</div>
                  </div>
                </div>
                <div className="text-right">
                  <div className={`text-lg font-bold ${record.amount > 0 ? 'mc-text-green' : 'mc-text-danger'}`}>
                    {record.amount > 0 ? '+' : ''}{formatICP(record.amount)} ICP
                  </div>
                  <div className="text-xs mc-text-muted">#{Number(record.id)}</div>
                </div>
              </div>
            ))}
        </div>
      )}
    </div>
  );
}

/* ================================================================
   Backer Positions
   ================================================================ */
function BackerPositions() {
  const { data: backerPositions = [], isLoading, error, refetch } = useGetBackerPositions();

  if (error) {
    return (
      <div className="mc-status-red p-4 text-center text-sm">
        <p className="mb-2">Failed to load backer data.</p>
        <button onClick={() => refetch()} className="mc-btn-secondary px-4 py-2 text-xs rounded-lg">Retry</button>
      </div>
    );
  }

  if (isLoading) return <LoadingSpinner />;

  const backers = Array.isArray(backerPositions) ? backerPositions : [];

  const fmtDate = (ts: bigint) => {
    try {
      return new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(new Date(Number(ts) / 1000000));
    } catch { return 'Invalid Date'; }
  };

  const totalHouseMoney = backers.reduce((s, d) => s + (d.amount || 0), 0);
  const totalDebt = backers.reduce((s, d) => s + (d.entitlement || 0), 0);

  return (
    <div className="space-y-6">
      {/* Add house money + stats */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="space-y-4">
          <div className="mc-card p-5">
            <AddHouseMoney />
          </div>
          <div className="mc-status-red p-3 text-center text-sm font-bold">
            <AlertTriangle className="h-4 w-4 inline mr-1" /> THIS IS A GAMBLING GAME<br />
            <span className="font-normal text-xs opacity-80">Only play with money you can afford to lose</span>
          </div>
        </div>

        <div className="space-y-4">
          <div className="mc-card mc-accent-danger p-5 text-center">
            <div className="mc-label mb-1">Outstanding Backer Debt</div>
            <div className="text-2xl font-bold mc-text-danger">{formatICP(totalDebt)} ICP</div>
          </div>
          <div className="mc-card mc-accent-cyan p-5 text-center">
            <div className="mc-label mb-1">Total House Money Added</div>
            <div className="text-2xl font-bold mc-text-cyan">{formatICP(totalHouseMoney)} ICP</div>
          </div>
        </div>
      </div>

      {/* Backer list */}
      {backers.length > 0 ? (
        <div className="space-y-3">
          <h3 className="font-display text-base mc-text-primary text-center">Current Backers</h3>
          {backers.map(backer => {
            const repayPct = backer.entitlement > 0 ? ((backer.entitlement - backer.amount) / backer.entitlement) * 100 : 0;
            const isSeriesA = 'upstream' in backer.dealerType;

            return (
              <div
                key={backer.owner.toString()}
                className={`mc-card p-5 ${isSeriesA ? 'mc-accent-green' : 'mc-accent-gold'}`}
              >
                <div className="grid grid-cols-1 lg:grid-cols-3 gap-4 items-center">
                  {/* Info */}
                  <div className="flex items-center gap-3">
                    <div className={`w-10 h-10 rounded-lg flex items-center justify-center text-xl ${
                      isSeriesA ? 'bg-green-500/20' : 'bg-yellow-500/20'
                    }`}>
                      {isSeriesA ? <Gem className="h-5 w-5 mc-text-green" /> : <Dice5 className="h-5 w-5 mc-text-gold" />}
                    </div>
                    <div>
                      <div className="font-bold mc-text-primary">{backer.name}</div>
                      <span className={`text-xs px-2 py-0.5 rounded-full font-bold ${
                        isSeriesA ? 'bg-green-500/20 mc-text-green' : 'bg-yellow-500/20 mc-text-gold'
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
                      {formatICP(backer.entitlement)} ICP
                    </div>
                  </div>

                  {/* Repayment */}
                  <div>
                    <div className="mc-label mb-1">Repayment</div>
                    <div className="text-sm font-bold mc-text-primary mb-1">
                      {formatICP(backer.entitlement - backer.amount)} / {formatICP(backer.entitlement)} ICP
                    </div>
                    <Progress value={Math.max(0, Math.min(100, repayPct))} className="mb-1 h-2" />
                    <div className="flex justify-between text-xs mc-text-muted">
                      <span>{Math.max(0, repayPct).toFixed(1)}% repaid</span>
                      <span className="mc-text-danger">Remaining: {formatICP(backer.amount)} ICP</span>
                    </div>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      ) : (
        <div className="text-center py-8">
          <Landmark className="h-10 w-10 mc-text-gold mb-3 mx-auto" />
          <p className="font-bold mc-text-primary mb-1">No backers yet</p>
          <p className="text-sm mc-text-dim">Deposit house money above to become the first Series A backer.</p>
        </div>
      )}

      {/* Info card */}
      <BackerInfoCard />
    </div>
  );
}

/* ================================================================
   Main Export
   ================================================================ */
export default function HouseDashboard() {
  const [activeTab, setActiveTab] = useState<'backers' | 'ledger'>('backers');

  return (
    <div className="space-y-6">
      <TabControl activeTab={activeTab} onTabChange={setActiveTab} />
      <div className="mc-enter">
        {activeTab === 'backers' ? <BackerPositions /> : <HouseLedgerRecords />}
      </div>
    </div>
  );
}
