import React, { useState, useMemo } from 'react';
import { Principal } from '@dfinity/principal';
import { Eye, ChevronDown, ChevronRight, History, Activity, Clock, Skull, TrendingUp, Hourglass, AlertTriangle, Users, Search, Copy, ArrowUp, ArrowDown } from 'lucide-react';
import {
  useAdminIsAdmin,
  useAdminGetCurrentRoundId,
  useAdminGetActivePlansSnapshot,
  useAdminGetRoundSummaries,
  useAdminGetEventsByRound,
  useAdminGetEventsForGame,
  useGetAllGames,
  useGetProfileFor,
} from '../hooks/useQueries';
import type { ActivePlanSnapshot, GameRecord, GamePlan, GeneralLedgerEntry, GeneralLedgerEvent, RoundSummary } from '../backend';
import LoadingSpinner from './LoadingSpinner';
import { formatICP } from '../lib/formatICP';

/* ================================================================
   Helpers
   ================================================================ */

function truncPrincipal(p: string): string {
  if (p.length <= 12) return p;
  return `${p.slice(0, 5)}...${p.slice(-3)}`;
}

function principalText(p: Principal): string {
  return p.toString();
}

function planLabel(plan: GamePlan): string {
  if ('simple21Day' in plan) return 'Simple 21d';
  if ('compounding15Day' in plan) return 'Cmpnd 15d';
  if ('compounding30Day' in plan) return 'Cmpnd 30d';
  return 'Unknown';
}

function planAccent(plan: GamePlan): string {
  if ('simple21Day' in plan) return 'mc-text-green';
  if ('compounding15Day' in plan) return 'mc-text-cyan';
  if ('compounding30Day' in plan) return 'mc-text-purple';
  return 'mc-text-dim';
}

// nanos (bigint) → JS Date
function nanosToDate(ns: bigint): Date {
  return new Date(Number(ns / 1_000_000n));
}

function formatTime(ns: bigint): string {
  if (ns === 0n) return '—';
  return nanosToDate(ns).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

/** Resolve a principal to its display name (or a truncated principal fallback). */
function OwnerLabel({ principal, full = false }: { principal: Principal; full?: boolean }) {
  const text = principalText(principal);
  const { data: profile } = useGetProfileFor(text);
  const display = profile?.name ?? truncPrincipal(text);
  return (
    <span className="font-mono text-xs" title={text}>
      <span className={profile?.name ? 'mc-text-primary' : 'mc-text-dim'}>{display}</span>
      {full && <span className="mc-text-muted ml-2 text-[10px]">{truncPrincipal(text)}</span>}
    </span>
  );
}

/* ================================================================
   In-Flight Plans Table
   ================================================================ */

function ActivePlansTable({ snapshots }: { snapshots: ActivePlanSnapshot[] }) {
  const [expandedGameId, setExpandedGameId] = useState<bigint | null>(null);

  if (snapshots.length === 0) {
    return (
      <div className="text-center py-12 mc-text-muted text-sm">
        No in-flight plans. AUM is dry.
      </div>
    );
  }

  // Sort: matured first, then by time elapsed desc
  const sorted = [...snapshots].sort((a, b) => {
    if (a.isMatured !== b.isMatured) return a.isMatured ? -1 : 1;
    return b.daysElapsed - a.daysElapsed;
  });

  return (
    <div className="space-y-1">
      <div className="grid grid-cols-12 gap-2 px-3 py-2 text-[10px] uppercase tracking-wider mc-text-muted border-b border-white/10">
        <div className="col-span-3">Owner</div>
        <div className="col-span-2">Plan</div>
        <div className="col-span-1 text-right">Deposit</div>
        <div className="col-span-2">Progress</div>
        <div className="col-span-1 text-right">Gross</div>
        <div className="col-span-1 text-right">Toll</div>
        <div className="col-span-1 text-right">Net</div>
        <div className="col-span-1 text-right">Status</div>
      </div>

      {sorted.map(snap => (
        <PlanRow
          key={snap.game.id.toString()}
          snapshot={snap}
          expanded={expandedGameId === snap.game.id}
          onToggle={() => setExpandedGameId(expandedGameId === snap.game.id ? null : snap.game.id)}
        />
      ))}
    </div>
  );
}

function PlanRow({ snapshot, expanded, onToggle }: { snapshot: ActivePlanSnapshot; expanded: boolean; onToggle: () => void }) {
  const { game, currentGrossEarnings, currentExitToll, currentNetClaimable, daysElapsed, daysToMaturity, isMatured, wouldBeInsolvent } = snapshot;
  const maturity = daysElapsed + daysToMaturity;
  const pct = maturity > 0 ? Math.min(100, (daysElapsed / maturity) * 100) : 0;

  return (
    <>
      <div
        onClick={onToggle}
        className={`grid grid-cols-12 gap-2 px-3 py-2.5 items-center cursor-pointer rounded-lg transition-colors ${
          expanded ? 'bg-white/[0.06]' : 'hover:bg-white/[0.03]'
        }`}
      >
        <div className="col-span-3 flex items-center gap-1.5">
          {expanded ? <ChevronDown className="h-3 w-3 mc-text-muted" /> : <ChevronRight className="h-3 w-3 mc-text-muted" />}
          <OwnerLabel principal={game.player} />
        </div>
        <div className={`col-span-2 text-xs font-bold ${planAccent(game.plan)}`}>
          {planLabel(game.plan)}
        </div>
        <div className="col-span-1 text-right text-xs mc-text-primary font-mono">
          {formatICP(game.amount)}
        </div>
        <div className="col-span-2">
          <div className="flex items-center gap-2">
            <div className="flex-1 h-1.5 bg-white/10 rounded-full overflow-hidden">
              <div
                className={`h-full ${isMatured ? 'bg-[var(--mc-gold)]' : 'bg-[var(--mc-neon-green)]'} transition-all`}
                style={{ width: `${pct}%` }}
              />
            </div>
            <span className="text-[10px] mc-text-muted font-mono w-12 text-right">
              {daysElapsed.toFixed(1)}/{maturity.toFixed(0)}d
            </span>
          </div>
        </div>
        <div className="col-span-1 text-right text-xs mc-text-gold font-mono">
          {formatICP(currentGrossEarnings)}
        </div>
        <div className="col-span-1 text-right text-xs mc-text-purple font-mono">
          {formatICP(currentExitToll)}
        </div>
        <div className="col-span-1 text-right text-xs mc-text-green font-mono font-bold">
          {formatICP(currentNetClaimable)}
        </div>
        <div className="col-span-1 text-right">
          {wouldBeInsolvent ? (
            <span className="inline-flex items-center gap-1 text-[10px] mc-text-danger" title="AUM can't cover this claim — would trigger reset">
              <AlertTriangle className="h-3 w-3" /> Insolv
            </span>
          ) : isMatured ? (
            <span className="inline-flex items-center gap-1 text-[10px] mc-text-gold" title="Past maturity">
              <Skull className="h-3 w-3" /> Mature
            </span>
          ) : (
            <span className="inline-flex items-center gap-1 text-[10px] mc-text-green" title="Still accruing">
              <Hourglass className="h-3 w-3" /> Live
            </span>
          )}
        </div>
      </div>

      {expanded && <PlanDetail game={game} />}
    </>
  );
}

function PlanDetail({ game }: { game: GameRecord }) {
  const { data: events = [], isLoading } = useAdminGetEventsForGame(game.id);

  return (
    <div className="ml-6 mr-3 mb-2 px-4 py-3 rounded-lg bg-black/40 border border-white/5">
      <div className="grid grid-cols-2 gap-x-6 gap-y-1.5 text-[11px]">
        <div className="flex justify-between">
          <span className="mc-text-muted">Game ID</span>
          <span className="mc-text-primary font-mono">#{game.id.toString()}</span>
        </div>
        <div className="flex justify-between">
          <span className="mc-text-muted">Total withdrawn</span>
          <span className="mc-text-cyan font-mono">{formatICP(game.totalWithdrawn)} ICP</span>
        </div>
        <div className="flex justify-between">
          <span className="mc-text-muted">Started</span>
          <span className="mc-text-dim font-mono">{formatTime(game.startTime)}</span>
        </div>
        <div className="flex justify-between">
          <span className="mc-text-muted">Last update</span>
          <span className="mc-text-dim font-mono">{formatTime(game.lastUpdateTime)}</span>
        </div>
        <div className="col-span-2 mc-text-muted text-[10px] uppercase tracking-wider mt-2 mb-1">
          Event history
        </div>
      </div>

      {isLoading ? (
        <div className="text-center py-3 mc-text-muted text-xs">Loading events...</div>
      ) : events.length === 0 ? (
        <div className="text-center py-3 mc-text-muted text-xs">No events recorded for this game.</div>
      ) : (
        <div className="space-y-1 mt-1">
          {events.map(e => <EventRow key={e.id.toString()} entry={e} compact />)}
        </div>
      )}
    </div>
  );
}

/* ================================================================
   Event log row
   ================================================================ */

function describeEvent(ev: GeneralLedgerEvent): { kind: string; detail: React.ReactNode; accent: string; sign: 'in' | 'out' | 'neutral' } {
  if ('deposit' in ev) {
    const d = ev.deposit;
    return {
      kind: 'deposit',
      accent: 'mc-text-green',
      sign: 'in',
      detail: <>player <OwnerLabel principal={d.player} /> · <span className="mc-text-primary font-mono">{formatICP(d.gross)} ICP</span> gross · {formatICP(d.coverCharge)} load · game #{d.gameId.toString()}</>,
    };
  }
  if ('backerDeposit' in ev) {
    const b = ev.backerDeposit;
    return {
      kind: 'backerDeposit',
      accent: 'mc-text-cyan',
      sign: 'in',
      detail: <>backer <OwnerLabel principal={b.backer} /> · <span className="mc-text-primary font-mono">{formatICP(b.amount)} ICP</span> · entitlement {formatICP(b.entitlement)}</>,
    };
  }
  if ('withdrawal' in ev) {
    const w = ev.withdrawal;
    return {
      kind: 'withdrawal',
      accent: 'mc-text-gold',
      sign: 'out',
      detail: <><OwnerLabel principal={w.player} /> withdrew <span className="mc-text-primary font-mono">{formatICP(w.netToPlayer)} ICP</span> (toll {formatICP(w.toll)}) · game #{w.gameId.toString()}{w.isInsolvent ? <span className="mc-text-danger ml-1">[insolv]</span> : null}</>,
    };
  }
  if ('settlement' in ev) {
    const s = ev.settlement;
    return {
      kind: 'settlement',
      accent: 'mc-text-gold',
      sign: 'out',
      detail: <><OwnerLabel principal={s.player} /> settled <span className="mc-text-primary font-mono">{formatICP(s.netToPlayer)} ICP</span> (toll {formatICP(s.toll)}) · game #{s.gameId.toString()}{s.isInsolvent ? <span className="mc-text-danger ml-1">[insolv]</span> : null}</>,
    };
  }
  if ('tollDistribution' in ev) {
    const t = ev.tollDistribution;
    return {
      kind: 'tollDistribution',
      accent: 'mc-text-purple',
      sign: 'neutral',
      detail: <>toll {formatICP(t.tollAmount)} · seed {formatICP(t.toSeedReserve)} · oldest {formatICP(t.toOldestSeriesA)} · others {formatICP(t.toOtherSeriesA)} · all {formatICP(t.toAllBackers)}</>,
    };
  }
  if ('backerRepaymentClaim' in ev) {
    const c = ev.backerRepaymentClaim;
    return {
      kind: 'backerRepaymentClaim',
      accent: 'mc-text-cyan',
      sign: 'out',
      detail: <><OwnerLabel principal={c.backer} /> claimed <span className="mc-text-primary font-mono">{formatICP(c.amount)} ICP</span> repayment</>,
    };
  }
  if ('coverChargeAccrued' in ev) {
    const c = ev.coverChargeAccrued;
    return {
      kind: 'coverChargeAccrued',
      accent: 'mc-text-dim',
      sign: 'neutral',
      detail: <>load {(Number(c.amountE8s) / 1e8).toFixed(4)} ICP from <OwnerLabel principal={c.player} /> · game #{c.gameId.toString()}</>,
    };
  }
  if ('coverChargeSwept' in ev) {
    const s = ev.coverChargeSwept;
    return {
      kind: 'coverChargeSwept',
      accent: 'mc-text-purple',
      sign: 'out',
      detail: <>swept {(Number(s.amountE8s) / 1e8).toFixed(4)} ICP to <OwnerLabel principal={s.toBackend} /> · block #{s.blockIndex.toString()}</>,
    };
  }
  if ('gameReset' in ev) {
    const r = ev.gameReset;
    return {
      kind: 'gameReset',
      accent: 'mc-text-danger',
      sign: 'neutral',
      detail: <><span className="font-bold">ROUND RESET</span> · seed carried {formatICP(r.seedReserveCarried)} ICP · {r.reason}</>,
    };
  }
  if ('backdatedGameCreated' in ev) {
    const b = ev.backdatedGameCreated;
    return {
      kind: 'backdatedGameCreated',
      accent: 'mc-text-pink',
      sign: 'in',
      detail: <>admin backdated #{b.gameId.toString()} · {formatICP(b.amount)} ICP · player <OwnerLabel principal={b.player} /></>,
    };
  }
  if ('seriesBPromotion' in ev) {
    const p = ev.seriesBPromotion;
    return {
      kind: 'seriesBPromotion',
      accent: 'mc-text-cyan',
      sign: 'neutral',
      detail: <><OwnerLabel principal={p.owner} /> promoted to Series B · underwater {formatICP(p.underwater)} ICP → entitlement {formatICP(p.entitlement)}</>,
    };
  }
  return { kind: 'unknown', detail: 'unknown event', accent: 'mc-text-muted', sign: 'neutral' };
}

function EventRow({ entry, compact = false }: { entry: GeneralLedgerEntry; compact?: boolean }) {
  const { kind, detail, accent, sign } = describeEvent(entry.event);
  const arrow = sign === 'in' ? '↓' : sign === 'out' ? '↑' : '·';
  return (
    <div className={`flex items-baseline gap-2 ${compact ? 'py-1 px-2 text-[11px]' : 'py-1.5 px-3 text-xs'} rounded hover:bg-white/[0.03]`}>
      <span className="mc-text-muted font-mono text-[10px] w-20 shrink-0">{formatTime(entry.timestamp)}</span>
      <span className={`${accent} font-bold uppercase tracking-wider text-[10px] w-24 shrink-0`}>{arrow} {kind}</span>
      <span className="mc-text-dim flex-1 min-w-0">{detail}</span>
      <span className="mc-text-muted font-mono text-[10px] shrink-0">#{entry.id.toString()}</span>
    </div>
  );
}

/* ================================================================
   Round Browser
   ================================================================ */

function RoundBrowser({ summaries, currentRoundId }: { summaries: RoundSummary[]; currentRoundId: bigint }) {
  const [selectedRound, setSelectedRound] = useState<bigint>(currentRoundId);
  const { data: events = [], isLoading } = useAdminGetEventsByRound(selectedRound);

  const selected = summaries.find(s => s.roundId === selectedRound);

  if (summaries.length === 0) {
    return <div className="text-center py-8 mc-text-muted text-sm">No rounds yet.</div>;
  }

  return (
    <div className="space-y-4">
      {/* Round selector */}
      <div className="flex flex-wrap gap-2 items-center">
        <span className="text-[10px] uppercase tracking-wider mc-text-muted shrink-0">Round</span>
        <div className="flex flex-wrap gap-1.5">
          {[...summaries].reverse().map(s => {
            const isCurrent = s.roundId === currentRoundId;
            const isSelected = s.roundId === selectedRound;
            return (
              <button
                key={s.roundId.toString()}
                onClick={() => setSelectedRound(s.roundId)}
                className={`px-2.5 py-1 rounded text-xs font-bold transition-all border ${
                  isSelected
                    ? 'bg-[var(--mc-purple)]/30 mc-text-primary border-[var(--mc-purple)]/50'
                    : isCurrent
                    ? 'bg-[var(--mc-neon-green)]/10 mc-text-green border-[var(--mc-neon-green)]/30 hover:bg-[var(--mc-neon-green)]/20'
                    : 'mc-text-dim border-white/10 hover:bg-white/5'
                }`}
              >
                #{s.roundId.toString()}{isCurrent && <span className="ml-1 text-[9px]">●</span>}
              </button>
            );
          })}
        </div>
      </div>

      {/* Selected round summary */}
      {selected && (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs">
          <div>
            <div className="mc-text-muted text-[10px] uppercase tracking-wider">Status</div>
            <div className={`font-bold ${selected.endTime.length > 0 ? 'mc-text-danger' : 'mc-text-green'}`}>
              {selected.endTime.length > 0 ? 'Closed' : 'In flight'}
            </div>
          </div>
          <div>
            <div className="mc-text-muted text-[10px] uppercase tracking-wider">Events</div>
            <div className="font-bold mc-text-primary font-mono">{selected.eventCount.toString()}</div>
          </div>
          <div>
            <div className="mc-text-muted text-[10px] uppercase tracking-wider">Started</div>
            <div className="font-mono mc-text-dim text-[11px]">{formatTime(selected.startTime)}</div>
          </div>
          <div>
            <div className="mc-text-muted text-[10px] uppercase tracking-wider">Ended</div>
            <div className="font-mono mc-text-dim text-[11px]">
              {selected.endTime[0] !== undefined ? formatTime(selected.endTime[0]) : '—'}
            </div>
          </div>
          {selected.endReason[0] !== undefined && (
            <div className="col-span-2 md:col-span-4">
              <div className="mc-text-muted text-[10px] uppercase tracking-wider">Close reason</div>
              <div className="mc-text-danger text-xs">{selected.endReason[0]}</div>
            </div>
          )}
        </div>
      )}

      {/* Events */}
      <div className="border-t border-white/10 pt-3">
        {isLoading ? (
          <div className="text-center py-8"><LoadingSpinner /></div>
        ) : events.length === 0 ? (
          <div className="text-center py-8 mc-text-muted text-sm">No events in this round.</div>
        ) : (
          <div className="space-y-0.5 max-h-[600px] overflow-y-auto pr-1">
            {/* Sort descending — latest first */}
            {[...events].sort((a, b) => Number(b.timestamp - a.timestamp)).map(e => (
              <EventRow key={e.id.toString()} entry={e} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/* ================================================================
   Main component
   ================================================================ */

export default function CharlesGodView() {
  const { data: isAdmin, isLoading: adminLoading } = useAdminIsAdmin();
  const [activeTab, setActiveTab] = useState<'live' | 'rounds' | 'roster'>('live');

  const { data: snapshots = [], isLoading: snapsLoading, error: snapsErr } = useAdminGetActivePlansSnapshot(isAdmin === true);
  const { data: currentRoundId } = useAdminGetCurrentRoundId(isAdmin === true);
  const { data: summaries = [] } = useAdminGetRoundSummaries(isAdmin === true);
  const { data: allGames = [], isLoading: rosterLoading, error: rosterErr } = useGetAllGames(isAdmin === true);

  if (adminLoading) {
    return <div className="flex justify-center py-12"><LoadingSpinner /></div>;
  }

  if (isAdmin !== true) {
    return (
      <div className="text-center py-16">
        <Skull className="h-12 w-12 mc-text-danger mb-4 mx-auto" />
        <h2 className="font-display text-xl text-white mb-3">Restricted</h2>
        <p className="mc-text-dim text-sm">Charles only. The backend verified your principal.</p>
      </div>
    );
  }

  const matured = snapshots.filter(s => s.isMatured).length;
  const insolvent = snapshots.filter(s => s.wouldBeInsolvent).length;
  const totalDeposited = snapshots.reduce((acc, s) => acc + s.game.amount, 0);
  const totalClaimable = snapshots.reduce((acc, s) => acc + s.currentNetClaimable, 0);

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <Eye className="h-5 w-5 mc-text-gold" />
        <h2 className="font-display text-lg mc-text-primary">God View</h2>
        <span className="mc-text-muted text-xs">
          everything the suckers can't see
        </span>
      </div>

      {/* Top stats */}
      <div className="grid grid-cols-2 md:grid-cols-5 gap-3">
        <StatCard label="Round" value={currentRoundId !== undefined ? `#${currentRoundId.toString()}` : '—'} accent="mc-text-purple" />
        <StatCard label="In-flight" value={snapshots.length.toString()} accent="mc-text-green" />
        <StatCard label="Matured" value={matured.toString()} accent="mc-text-gold" />
        <StatCard label="Insolvent" value={insolvent.toString()} accent={insolvent > 0 ? 'mc-text-danger' : 'mc-text-dim'} />
        <StatCard label="Total claimable" value={`${formatICP(totalClaimable)} / ${formatICP(totalDeposited)}`} accent="mc-text-cyan" />
      </div>

      {/* Tab bar */}
      <div className="inline-flex rounded-full bg-white/5 border border-white/10 p-1 gap-1">
        {(['live', 'rounds', 'roster'] as const).map(t => (
          <button
            key={t}
            onClick={() => setActiveTab(t)}
            className={`px-5 py-2 rounded-full text-xs font-bold transition-all ${
              activeTab === t
                ? 'bg-[var(--mc-purple)]/30 mc-text-primary border border-[var(--mc-purple)]/40'
                : 'mc-text-muted hover:mc-text-dim hover:bg-white/5'
            }`}
          >
            {t === 'live' ? (
              <><Activity className="h-3.5 w-3.5 inline mr-1.5" /> In-Flight ({snapshots.length})</>
            ) : t === 'rounds' ? (
              <><History className="h-3.5 w-3.5 inline mr-1.5" /> Round Log</>
            ) : (
              <><Users className="h-3.5 w-3.5 inline mr-1.5" /> Roster</>
            )}
          </button>
        ))}
      </div>

      {/* Panels */}
      {activeTab === 'live' ? (
        snapsLoading ? (
          <div className="flex justify-center py-12"><LoadingSpinner /></div>
        ) : snapsErr ? (
          <div className="text-center py-8 mc-text-danger text-sm">Failed to load: {String(snapsErr)}</div>
        ) : (
          <ActivePlansTable snapshots={snapshots} />
        )
      ) : activeTab === 'rounds' ? (
        currentRoundId !== undefined && (
          <RoundBrowser summaries={summaries} currentRoundId={currentRoundId} />
        )
      ) : (
        rosterLoading ? (
          <div className="flex justify-center py-12"><LoadingSpinner /></div>
        ) : rosterErr ? (
          <div className="text-center py-8 mc-text-danger text-sm">Failed to load: {String(rosterErr)}</div>
        ) : (
          <RosterTable games={allGames} />
        )
      )}
    </div>
  );
}

/* ================================================================
   Roster — one row per unique player, sortable + filterable
   ================================================================ */

type RosterRow = {
  principal: Principal;
  principalText: string;
  gamesCount: number;
  activeGamesCount: number;
  totalDeposited: number;
  totalWithdrawn: number;
  net: number; // totalWithdrawn - totalDeposited (negative = still in the hole)
  firstSeenNs: bigint;
  lastActivityNs: bigint;
};

type SortKey =
  | 'principal'
  | 'games'
  | 'active'
  | 'deposited'
  | 'withdrawn'
  | 'net'
  | 'firstSeen'
  | 'lastActivity';
type SortDir = 'asc' | 'desc';

// Natural direction for a fresh column click: desc for "more is interesting",
// asc for text. Matches what a spreadsheet user would expect.
const NATURAL_DIR: Record<SortKey, SortDir> = {
  principal: 'asc',
  games: 'desc',
  active: 'desc',
  deposited: 'desc',
  withdrawn: 'desc',
  net: 'desc',
  firstSeen: 'desc', // newest first
  lastActivity: 'desc', // most recent first
};

function aggregateRoster(games: GameRecord[]): RosterRow[] {
  const byPrincipal = new Map<string, RosterRow>();
  for (const g of games) {
    const key = g.player.toString();
    let row = byPrincipal.get(key);
    if (!row) {
      row = {
        principal: g.player,
        principalText: key,
        gamesCount: 0,
        activeGamesCount: 0,
        totalDeposited: 0,
        totalWithdrawn: 0,
        net: 0,
        firstSeenNs: g.startTime,
        lastActivityNs: g.lastUpdateTime,
      };
      byPrincipal.set(key, row);
    }
    row.gamesCount += 1;
    if (g.isActive) row.activeGamesCount += 1;
    row.totalDeposited += g.amount;
    row.totalWithdrawn += g.totalWithdrawn;
    if (g.startTime < row.firstSeenNs) row.firstSeenNs = g.startTime;
    if (g.lastUpdateTime > row.lastActivityNs) row.lastActivityNs = g.lastUpdateTime;
  }
  // Finalize net after all sums settled
  for (const row of byPrincipal.values()) {
    row.net = row.totalWithdrawn - row.totalDeposited;
  }
  return Array.from(byPrincipal.values());
}

function compareRows(a: RosterRow, b: RosterRow, key: SortKey, dir: SortDir): number {
  const sign = dir === 'asc' ? 1 : -1;
  switch (key) {
    case 'principal':
      return sign * a.principalText.localeCompare(b.principalText);
    case 'games':
      return sign * (a.gamesCount - b.gamesCount);
    case 'active':
      return sign * (a.activeGamesCount - b.activeGamesCount);
    case 'deposited':
      return sign * (a.totalDeposited - b.totalDeposited);
    case 'withdrawn':
      return sign * (a.totalWithdrawn - b.totalWithdrawn);
    case 'net':
      return sign * (a.net - b.net);
    case 'firstSeen':
      return sign * Number(a.firstSeenNs - b.firstSeenNs);
    case 'lastActivity':
      return sign * Number(a.lastActivityNs - b.lastActivityNs);
  }
}

function RosterTable({ games }: { games: GameRecord[] }) {
  const [sortKey, setSortKey] = useState<SortKey>('lastActivity');
  const [sortDir, setSortDir] = useState<SortDir>('desc');
  const [filter, setFilter] = useState('');
  const [copied, setCopied] = useState<string | null>(null);

  const rows = useMemo(() => aggregateRoster(games), [games]);

  const filteredSorted = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const filtered = q ? rows.filter(r => r.principalText.toLowerCase().includes(q)) : rows;
    return [...filtered].sort((a, b) => compareRows(a, b, sortKey, sortDir));
  }, [rows, filter, sortKey, sortDir]);

  const onHeaderClick = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir(d => (d === 'asc' ? 'desc' : 'asc'));
    } else {
      setSortKey(key);
      setSortDir(NATURAL_DIR[key]);
    }
  };

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(text);
      setTimeout(() => setCopied(c => (c === text ? null : c)), 1200);
    } catch {
      // clipboard may be unavailable (insecure context) — no-op
    }
  };

  if (rows.length === 0) {
    return (
      <div className="text-center py-12 mc-text-muted text-sm">
        No players yet. The fund awaits its first sucker.
      </div>
    );
  }

  // Aggregate caveat: anyone who created a profile but never deposited
  // doesn't appear in getAllGames(). Surface that so Charles knows.
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <div className="mc-text-muted text-xs">
          {filteredSorted.length} of {rows.length} player{rows.length === 1 ? '' : 's'} who&apos;ve placed at least one game.
        </div>
        <div className="relative">
          <Search className="h-3.5 w-3.5 mc-text-muted absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none" />
          <input
            type="text"
            value={filter}
            onChange={e => setFilter(e.target.value)}
            placeholder="Filter by principal…"
            className="bg-white/5 border border-white/10 rounded-lg pl-8 pr-3 py-1.5 text-xs mc-text-primary placeholder:mc-text-muted focus:outline-none focus:border-white/20 w-64"
          />
        </div>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <thead>
            <tr className="text-[10px] uppercase tracking-wider mc-text-muted border-b border-white/10">
              <th className="text-left py-2 px-2 font-normal">Player</th>
              <SortableHeader label="Principal" k="principal" sortKey={sortKey} sortDir={sortDir} onClick={onHeaderClick} align="left" />
              <SortableHeader label="Games" k="games" sortKey={sortKey} sortDir={sortDir} onClick={onHeaderClick} align="right" />
              <SortableHeader label="Active" k="active" sortKey={sortKey} sortDir={sortDir} onClick={onHeaderClick} align="right" />
              <SortableHeader label="Deposited" k="deposited" sortKey={sortKey} sortDir={sortDir} onClick={onHeaderClick} align="right" />
              <SortableHeader label="Withdrawn" k="withdrawn" sortKey={sortKey} sortDir={sortDir} onClick={onHeaderClick} align="right" />
              <SortableHeader label="Net" k="net" sortKey={sortKey} sortDir={sortDir} onClick={onHeaderClick} align="right" />
              <SortableHeader label="First seen" k="firstSeen" sortKey={sortKey} sortDir={sortDir} onClick={onHeaderClick} align="right" />
              <SortableHeader label="Last activity" k="lastActivity" sortKey={sortKey} sortDir={sortDir} onClick={onHeaderClick} align="right" />
            </tr>
          </thead>
          <tbody>
            {filteredSorted.map(row => (
              <tr key={row.principalText} className="border-b border-white/5 hover:bg-white/[0.03] transition-colors">
                <td className="py-2 px-2">
                  <OwnerLabel principal={row.principal} />
                </td>
                <td className="py-2 px-2">
                  <button
                    onClick={() => copy(row.principalText)}
                    className="font-mono text-[11px] mc-text-dim hover:mc-text-primary inline-flex items-center gap-1.5 group"
                    title="Click to copy full principal"
                  >
                    <span>{truncPrincipal(row.principalText)}</span>
                    {copied === row.principalText ? (
                      <span className="mc-text-green text-[10px]">copied</span>
                    ) : (
                      <Copy className="h-3 w-3 opacity-40 group-hover:opacity-100" />
                    )}
                  </button>
                </td>
                <td className="py-2 px-2 text-right font-mono mc-text-primary">{row.gamesCount}</td>
                <td className="py-2 px-2 text-right font-mono">
                  <span className={row.activeGamesCount > 0 ? 'mc-text-green' : 'mc-text-muted'}>
                    {row.activeGamesCount}
                  </span>
                </td>
                <td className="py-2 px-2 text-right font-mono mc-text-primary">{formatICP(row.totalDeposited)}</td>
                <td className="py-2 px-2 text-right font-mono mc-text-gold">{formatICP(row.totalWithdrawn)}</td>
                <td className={`py-2 px-2 text-right font-mono ${row.net >= 0 ? 'mc-text-green' : 'mc-text-danger'}`}>
                  {row.net >= 0 ? '+' : ''}{formatICP(row.net)}
                </td>
                <td className="py-2 px-2 text-right mc-text-dim">{formatTime(row.firstSeenNs)}</td>
                <td className="py-2 px-2 text-right mc-text-dim">{formatTime(row.lastActivityNs)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function SortableHeader({
  label, k, sortKey, sortDir, onClick, align,
}: {
  label: string;
  k: SortKey;
  sortKey: SortKey;
  sortDir: SortDir;
  onClick: (k: SortKey) => void;
  align: 'left' | 'right';
}) {
  const active = sortKey === k;
  return (
    <th className={`py-2 px-2 font-normal ${align === 'right' ? 'text-right' : 'text-left'}`}>
      <button
        onClick={() => onClick(k)}
        className={`inline-flex items-center gap-1 hover:mc-text-primary transition-colors ${active ? 'mc-text-primary' : ''}`}
      >
        <span>{label}</span>
        {active && (sortDir === 'asc' ? <ArrowUp className="h-3 w-3" /> : <ArrowDown className="h-3 w-3" />)}
      </button>
    </th>
  );
}

function StatCard({ label, value, accent }: { label: string; value: string; accent: string }) {
  return (
    <div className="mc-card p-3">
      <div className="mc-text-muted mb-1 uppercase tracking-wider text-[10px]">{label}</div>
      <div className={`font-bold ${accent} font-mono text-sm`}>{value}</div>
    </div>
  );
}
