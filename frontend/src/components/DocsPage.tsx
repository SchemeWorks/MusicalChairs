import React, { useState, useEffect } from 'react';
import { ChevronRight, Dices, Rocket, DollarSign, Landmark, Users, Dice5, Wallet, Flame, Shield, Zap, AlertTriangle, BookOpen, ArrowLeft, X } from 'lucide-react';
import {
  DAILY_RATE_SIMPLE, DAILY_RATE_COMPOUND_15, DAILY_RATE_COMPOUND_30,
  PLAN_DAYS_SIMPLE, PLAN_DAYS_COMPOUND_15, PLAN_DAYS_COMPOUND_30,
  MIN_DEPOSIT_ICP, SIMPLE_MAX_DEPOSIT_POT_FRACTION, SIMPLE_MAX_DEPOSIT_FLOOR_ICP,
  DEPOSIT_RATE_LIMIT,
  COVER_CHARGE_RATE,
  EXIT_TOLL_EARLY, EXIT_TOLL_MID, EXIT_TOLL_LATE,
  EXIT_TOLL_EARLY_DAYS, EXIT_TOLL_MID_DAYS,
  JACKPOT_FEE_RATE_15D, JACKPOT_FEE_RATE_30D,
  FEE_POT_SHARE, FEE_BACKER_SHARE,
  BACKER_OLDEST_UPSTREAM_SHARE, BACKER_OTHER_UPSTREAM_SHARE, BACKER_ALL_SHARE,
  UPSTREAM_BACKER_BONUS, DOWNSTREAM_BACKER_BONUS,
  PP_PER_ICP_SIMPLE, PP_PER_ICP_COMPOUND_15, PP_PER_ICP_COMPOUND_30, PP_PER_ICP_SEED_ROUND,
  REFERRAL_L1_RATE, REFERRAL_L2_RATE, REFERRAL_L3_RATE,
  pct, pctPrecise, fmt,
} from '../lib/gameConstants';
import { useGetMintConfig, useGetShenaniganConfigs } from '../hooks/useQueries';
import type { MintConfig, ShenaniganConfig } from '../declarations/shenanigans/shenanigans.did';

interface DocSection {
  id: string;
  title: string;
  subtitle: string;
  icon: React.ReactNode;
  content: React.ReactNode;
}

function DocAccordion({ section, isOpen, onToggle }: { section: DocSection; isOpen: boolean; onToggle: () => void }) {
  return (
    <div id={`docs-${section.id}`} className="mc-card overflow-hidden transition-all scroll-mt-24">
      <button
        onClick={onToggle}
        className="w-full flex items-center gap-4 p-5 text-left hover:bg-white/[0.03] transition-colors group"
      >
        <span className="shrink-0">{section.icon}</span>
        <div className="flex-1 min-w-0">
          <span className="font-display text-sm text-white block">{section.title}</span>
          <span className="text-xs mc-text-muted block mt-0.5">{section.subtitle}</span>
        </div>
        <ChevronRight className={`h-4 w-4 mc-text-muted transition-transform duration-200 shrink-0 ${isOpen ? 'rotate-90' : ''}`} />
      </button>
      {isOpen && (
        <div className="px-5 pb-5 text-sm mc-text-dim leading-relaxed space-y-4 border-t border-white/5 pt-4">
          {section.content}
        </div>
      )}
    </div>
  );
}

/* ── Inline table helper ── */
function DocTable({ headers, rows }: { headers: string[]; rows: (string | React.ReactNode)[][] }) {
  return (
    <div className="overflow-x-auto -mx-1 mt-2">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-white/10">
            {headers.map((h, i) => (
              <th key={i} className={`py-2 px-1 mc-text-muted font-normal ${i === headers.length - 1 ? 'text-right' : 'text-left'}`}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody className="mc-text-dim">
          {rows.map((row, i) => (
            <tr key={i} className="border-b border-white/5">
              {row.map((cell, j) => (
                <td key={j} className={`py-2 px-1 align-top ${j === row.length - 1 ? 'text-right' : ''}`}>{cell}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/* ── Section data ── */
function buildDocSections(mintConfig: MintConfig | null | undefined, shenaniganConfigs: ShenaniganConfig[] | undefined): DocSection[] {
  // Live PP rates from canister config; fall back to gameConstants while loading.
  const ppSimple = mintConfig ? Number(mintConfig.simple21DayPpPerIcp) : PP_PER_ICP_SIMPLE;
  const ppComp15 = mintConfig ? Number(mintConfig.compounding15DayPpPerIcp) : PP_PER_ICP_COMPOUND_15;
  const ppComp30 = mintConfig ? Number(mintConfig.compounding30DayPpPerIcp) : PP_PER_ICP_COMPOUND_30;
  const ppBacker = mintConfig ? Number(mintConfig.backerPpPerIcp) : PP_PER_ICP_SEED_ROUND;

  // Live shenanigan stats by ID; fall back to spec defaults while loading.
  // The displayed cost is the costSuccess (the upfront commitment); the
  // per-outcome split is shown on the spell card itself in the game UI.
  const sh = (id: number) => shenaniganConfigs?.find(s => Number(s.id) === id);
  const shCost = (id: number, fallback: number) => {
    const c = sh(id)?.costSuccess;
    return typeof c === 'number' ? c : fallback;
  };
  const shOdds = (id: number, fallback: number) => {
    const o = sh(id)?.successOdds;
    return o !== undefined ? Number(o) : fallback;
  };
  const ppCost = (id: number, fallback: number) => `${fmt(Math.round(shCost(id, fallback)))} PP`;
  const ppOdds = (id: number, fallback: number) => `${shOdds(id, fallback)}%`;

  return [
  {
    id: 'overview',
    title: 'What Is Musical Chairs?',
    subtitle: 'The elevator pitch — and the fine print.',
    icon: <Dices className="h-5 w-5 mc-text-green" />,
    content: (
      <>
        <p>Musical Chairs is a transparent Ponzi scheme on the Internet Computer. New deposits fund existing players' returns. When deposits slow and the pot drains, the music stops, the chairs reset, and anyone still holding a position takes a total loss.</p>
        <p>Every mechanic described in these docs is exactly what happens on-chain. There are no hidden fees, no backdoors, no "trust us" moments. The code is the product. The prospectus is the product.</p>
      </>
    ),
  },
  {
    id: 'game-plans',
    title: 'Game Plans',
    subtitle: 'Three ways to play, each with different risk and lockup.',
    icon: <Rocket className="h-5 w-5 mc-text-purple" />,
    content: (
      <>
        <DocTable
          headers={['Plan', 'Daily Rate', 'Duration', 'Lockup', 'PP Earned']}
          rows={[
            [<span className="mc-text-green font-bold">Simple {PLAN_DAYS_SIMPLE}-Day</span>, pct(DAILY_RATE_SIMPLE), `${PLAN_DAYS_SIMPLE} days`, 'Anytime', `${fmt(ppSimple)} / ICP`],
            [<span className="mc-text-purple font-bold">Compounding {PLAN_DAYS_COMPOUND_15}-Day</span>, pct(DAILY_RATE_COMPOUND_15), `${PLAN_DAYS_COMPOUND_15} days`, 'Locked', `${fmt(ppComp15)} / ICP`],
            [<span className="mc-text-gold font-bold">Compounding {PLAN_DAYS_COMPOUND_30}-Day</span>, pct(DAILY_RATE_COMPOUND_30), `${PLAN_DAYS_COMPOUND_30} days`, 'Locked', `${fmt(ppComp30)} / ICP`],
          ]}
        />
        <div className="mt-4 space-y-2">
          <p><strong className="text-white">Simple mode:</strong> Your funds are deployed into the pot; in return you hold a position that accrues interest daily at the Simple rate. Withdraw accrued interest at any time, subject to carried interest. The position closes at the end of its {PLAN_DAYS_SIMPLE}-day term.</p>
          <p><strong className="text-white">Compounding mode:</strong> Your funds are deployed into the pot; in return you hold a position that compounds daily until maturity. At maturity the accumulated interest is paid out, minus carried interest — {pct(JACKPOT_FEE_RATE_15D)} on the {PLAN_DAYS_COMPOUND_15}-day plan, {pct(JACKPOT_FEE_RATE_30D)} on the {PLAN_DAYS_COMPOUND_30}-day plan. No early withdrawal.</p>
          <p><strong className="text-white">The risk:</strong> If the pot empties before your position is paid out, your entitlement is worthless. Longer plans = higher returns = higher risk of getting caught in a reset.</p>
        </div>
      </>
    ),
  },
  {
    id: 'deposits',
    title: 'Deposits & Limits',
    subtitle: 'Minimums, maximums, and what happens to your money.',
    icon: <Wallet className="h-5 w-5 mc-text-cyan" />,
    content: (
      <>
        <div className="space-y-2">
          <p><strong className="text-white">Minimum deposit:</strong> {MIN_DEPOSIT_ICP} ICP for all plan types.</p>
          <p><strong className="text-white">Maximum deposit (Simple only):</strong> The greater of {pct(SIMPLE_MAX_DEPOSIT_POT_FRACTION)} of the current pot balance or {SIMPLE_MAX_DEPOSIT_FLOOR_ICP} ICP. This prevents a single player from draining the pot in one withdrawal.</p>
          <p><strong className="text-white">Maximum deposit (Compounding):</strong> No limit above the {MIN_DEPOSIT_ICP} ICP minimum.</p>
          <p><strong className="text-white">Rate limit:</strong> {DEPOSIT_RATE_LIMIT} positions per hour per user.</p>
          <p><strong className="text-white">Front-End Load:</strong> {pct(COVER_CHARGE_RATE)} of every deposit is skimmed and routed to Management. The other {pct(1 - COVER_CHARGE_RATE)} enters the pot.</p>
        </div>
        <div className="mc-card p-4 mt-4 mc-accent-gold">
          <p className="text-xs mc-text-muted">Deposits go through the ICRC-2 approve/transfer_from flow. You approve the backend canister to pull funds from your wallet, then the backend executes the transfer. This is a standard ICP token pattern.</p>
        </div>
      </>
    ),
  },
  {
    id: 'fees',
    title: 'Fees & Carried Interest',
    subtitle: 'Every fee in the system, with exact percentages.',
    icon: <DollarSign className="h-5 w-5 mc-text-gold" />,
    content: (
      <>
        <p className="font-bold text-white mb-2">Entry</p>
        <DocTable
          headers={['Fee', 'Rate', 'Applies To']}
          rows={[
            ['Front-End Load', <span className="mc-text-gold font-bold">{pct(COVER_CHARGE_RATE)}</span>, 'Every deposit (paid to Management)'],
          ]}
        />

        <p className="font-bold text-white mb-2 mt-6">Exit — Simple Positions</p>
        <DocTable
          headers={['Withdrawal Window', 'Carried Interest']}
          rows={[
            [`Day 0–${EXIT_TOLL_EARLY_DAYS}`, <span className="mc-text-danger font-bold">{pct(EXIT_TOLL_EARLY)}</span>],
            [`Day ${EXIT_TOLL_EARLY_DAYS}–${EXIT_TOLL_MID_DAYS}`, <span className="mc-text-gold font-bold">{pctPrecise(EXIT_TOLL_MID)}</span>],
            [`Day ${EXIT_TOLL_MID_DAYS}+`, <span className="mc-text-green font-bold">{pct(EXIT_TOLL_LATE)}</span>],
          ]}
        />

        <p className="font-bold text-white mb-2 mt-6">Exit — Compounding Positions</p>
        <DocTable
          headers={['Plan', 'Carried Interest', 'When']}
          rows={[
            [`${PLAN_DAYS_COMPOUND_15}-day Compounding`, <span className="mc-text-gold font-bold">{pct(JACKPOT_FEE_RATE_15D)}</span>, 'On maturity payout'],
            [`${PLAN_DAYS_COMPOUND_30}-day Compounding`, <span className="mc-text-gold font-bold">{pct(JACKPOT_FEE_RATE_30D)}</span>, 'On maturity payout'],
          ]}
        />

        <p className="mt-4 mc-text-muted">Carried interest flows back into the pot or to backers. It funds other players' returns. This is the engine — new money in, old money out.</p>
        <p className="mc-text-muted">{pct(FEE_POT_SHARE)} of carried interest seeds the next round's pot. The other {pct(FEE_BACKER_SHARE)} repays the backers. The Front-End Load is separate — it goes to Management and never touches the pot.</p>
      </>
    ),
  },
  {
    id: 'seed-round',
    title: 'The Seed Round (Backers)',
    subtitle: 'Back the fund. Earn a cut. Take on fund-level risk.',
    icon: <Landmark className="h-5 w-5 mc-text-gold" />,
    content: (
      <>
        <p>Backers are the venture capitalists of Musical Chairs. They deposit ICP into the Seed Round and earn an entitlement on their stake, repaid over time through player deposits.</p>

        <p className="font-bold text-white mt-4 mb-2">How It Works</p>
        <div className="space-y-2">
          <p><strong className="text-white">Deposit:</strong> Minimum {MIN_DEPOSIT_ICP} ICP. Your deposit goes directly into the pot.</p>
          <p><strong className="text-white">Entitlement (Series A):</strong> You're owed {pct(1 + UPSTREAM_BACKER_BONUS)} of your deposit (original + {pct(UPSTREAM_BACKER_BONUS)} bonus).</p>
          <p><strong className="text-white">Repayment:</strong> Comes from the backer share ({pct(FEE_BACKER_SHARE)}) of carried interest on every player withdrawal.</p>
          <p><strong className="text-white">PP earned:</strong> {fmt(ppBacker)} Ponzi Points per ICP deposited.</p>
        </div>

        <p className="font-bold text-white mt-4 mb-2">Fee Distribution</p>
        <DocTable
          headers={['Share', 'Recipient']}
          rows={[
            [<span className="mc-text-gold font-bold">{pct(BACKER_OLDEST_UPSTREAM_SHARE)}</span>, 'Oldest Series A backer'],
            [<span className="mc-text-purple font-bold">{pct(BACKER_OTHER_UPSTREAM_SHARE)}</span>, 'Split among other Series A backers'],
            [<span className="mc-text-cyan font-bold">{pct(BACKER_ALL_SHARE)}</span>, 'Split among all backers'],
          ]}
        />

        <p className="font-bold text-white mt-4 mb-2">Backer Types</p>
        <div className="space-y-2">
          <p><strong className="mc-text-green">Series A:</strong> Created by voluntarily depositing into the Seed Round. {pct(UPSTREAM_BACKER_BONUS)} bonus on your stake.</p>
          <p><strong className="mc-text-danger">Series B:</strong> Created automatically during an Emergency Equity Conversion — a random unprofitable player gets converted into a backer with an entitlement matching their losses plus a {pct(DOWNSTREAM_BACKER_BONUS)} bonus.</p>
        </div>
      </>
    ),
  },
  {
    id: 'referrals',
    title: 'The Pyramid (MLM)',
    subtitle: 'Refer someone, take a cut of everything they earn.',
    icon: <Users className="h-5 w-5 mc-text-cyan" />,
    content: (
      <>
        <p>Share your referral link and recruit a downline. <strong className="text-white">You take 10% of every Ponzi Point they earn</strong> — from their deposits, from their own referrals, from any game activity. Doesn't matter where the PP came from. Paid in PP, never ICP.</p>
        <p>And whoever referred <em>you</em> takes 10% of yours, same way. They keep half of what they collect from you; the other half cascades up to whoever referred <em>them</em>. Same split at every tier, all the way up the chain.</p>
      </>
    ),
  },
  {
    id: 'redistribution',
    title: 'Emergency Equity Conversion',
    subtitle: 'What happens when the music stops.',
    icon: <Flame className="h-5 w-5 mc-text-danger" />,
    content: (
      <>
        <p>When the pot can't cover a payout, the music stops and the chairs reset. This is the core Musical Chairs mechanic — a Ponzi just collapses and walks away; we recapitalize and start the next round.</p>
        <div className="space-y-2 mt-3">
          <p><strong className="mc-text-danger">All active positions are liquidated.</strong> Total loss for anyone still holding.</p>
          <p><strong className="mc-text-danger">All pending payouts are voided.</strong> Accrued but unwithdrawn earnings disappear.</p>
          <p><strong className="mc-text-gold">A random unprofitable player becomes a Series B Backer.</strong> Their entitlement equals their losses plus a {pct(DOWNSTREAM_BACKER_BONUS)} bonus.</p>
          <p><strong className="mc-text-green">A new round begins.</strong> Fresh pot, clean slate, same rules.</p>
        </div>
      </>
    ),
  },
  {
    id: 'ponzi-points',
    title: 'Ponzi Points (PP)',
    subtitle: 'The in-game currency. Earned by playing, spent on chaos.',
    icon: <Zap className="h-5 w-5 mc-text-purple" />,
    content: (
      <>
        <p>Ponzi Points are earned automatically through gameplay. They cannot be bought or sold. Their primary use is casting Shenanigans.</p>

        <p className="font-bold text-white mt-4 mb-2">Earning Rates</p>
        <DocTable
          headers={['Activity', 'PP Earned']}
          rows={[
            [`Simple ${PLAN_DAYS_SIMPLE}-Day deposit`, `${fmt(ppSimple)} PP per ICP`],
            [`Compounding ${PLAN_DAYS_COMPOUND_15}-Day deposit`, `${fmt(ppComp15)} PP per ICP`],
            [`Compounding ${PLAN_DAYS_COMPOUND_30}-Day deposit`, `${fmt(ppComp30)} PP per ICP`],
            ['Seed Round deposit', `${fmt(ppBacker)} PP per ICP`],
            ['Referral activity', 'Based on downline PP earnings'],
          ]}
        />
        <p className="mt-4 mc-text-muted">PP do not affect ICP payouts, pot mechanics, or game math in any way. They're purely for Shenanigans and bragging rights.</p>
      </>
    ),
  },
  {
    id: 'shenanigans',
    title: 'Shenanigans',
    subtitle: 'Cosmetic chaos you cast with Ponzi Points.',
    icon: <Dice5 className="h-5 w-5 mc-text-green" />,
    content: (
      <>
        <p>11 shenanigans across three categories. All cost PP. All can backfire. Anyone holding PP is a valid target — no balance floor protection.</p>

        <p className="font-bold mc-text-danger mt-4 mb-2">Offense</p>
        <DocTable
          headers={['Shenanigan', 'Cost', 'Effect', 'Success']}
          rows={[
            ['Money Trickster', ppCost(0, 120), "Steal 2–8% of target's PP (max 250)", ppOdds(0, 60)],
            ['AOE Skim', ppCost(1, 600), 'Siphon 1–3% from every player (max 60/ea)', ppOdds(1, 40)],
            ['Mint Tax Siphon', ppCost(3, 1200), "Skim 5% of target's new PP for 7 days (max 1,000)", ppOdds(3, 70)],
            ['Downline Heist', ppCost(4, 500), "Steal a referral from someone's downline", ppOdds(4, 30)],
            ['Purse Cutter', ppCost(7, 900), 'Target loses 25–50% PP (max 800)', ppOdds(7, 20)],
            ['Whale Rebalance', ppCost(8, 800), 'Take 20% from top 3 PP holders (max 300/whale)', ppOdds(8, 50)],
          ]}
        />

        <p className="font-bold mc-text-green mt-4 mb-2">Defense & Buffs</p>
        <DocTable
          headers={['Shenanigan', 'Cost', 'Effect', 'Success']}
          rows={[
            ['Magic Mirror', ppCost(5, 200), 'Shield: blocks one hostile shenanigan for 24h', ppOdds(5, 100)],
            ['PP Booster Aura', ppCost(6, 300), '+5–15% to all your PP mints for 24h', ppOdds(6, 100)],
            ['Downline Boost', ppCost(9, 400), 'Your referral cascade pays 1.3x for 24h', ppOdds(9, 100)],
          ]}
        />

        <p className="font-bold mc-text-purple mt-4 mb-2">Chaos</p>
        <DocTable
          headers={['Shenanigan', 'Cost', 'Effect', 'Success']}
          rows={[
            ['Rename Spell', ppCost(2, 200), "Slaps a satirical name on someone for 7 days", ppOdds(2, 90)],
            ['Golden Name', ppCost(10, 100), 'Gold name on leaderboard for 24 hours', ppOdds(10, 100)],
          ]}
        />

        <p className="font-bold text-white mt-6 mb-2">How Spells Resolve</p>
        <div className="space-y-3">
          <div className="mc-card p-4">
            <p className="font-display text-xs text-white mb-1">Shields</p>
            <p>Magic Mirror gives the caster <span className="mc-text-gold font-bold">one</span> charge that absorbs the next hostile spell aimed at them within 24 hours. The shielded spell shows as "fail" to the attacker — they still pay the cost, the shield charge is consumed, no damage lands. AOE Skim and Whale Rebalance respect shields per-target (shielded victims skipped, others still hit). Rename and Downline Heist bypass shields by design.</p>
          </div>
        </div>
      </>
    ),
  },
  {
    id: 'wallets',
    title: 'Wallet & Authentication',
    subtitle: 'How you connect and where your funds live.',
    icon: <Shield className="h-5 w-5 mc-text-gold" />,
    content: (
      <>
        <p>Musical Chairs supports Internet Identity and Plug. Both have full access to every game feature. OISY support is coming soon.</p>

        <div className="space-y-3 mt-3">
          <div className="mc-card p-4">
            <p className="font-display text-sm text-white mb-1">Internet Identity</p>
            <p>ICP's native browser-based authentication. No extension needed. Funds stay in your wallet; the backend pulls ICP via ICRC-2 approvals only when you open a position or fund a backer slot.</p>
          </div>
          <div className="mc-card p-4">
            <p className="font-display text-sm text-white mb-1">Plug Wallet</p>
            <p>Browser extension wallet. Transactions go through Plug's approval flow. Requires the extension to be installed.</p>
          </div>
          <div className="mc-card p-4 opacity-50">
            <p className="font-display text-sm text-white mb-1">OISY Wallet <span className="text-xs mc-text-muted font-normal">(coming soon)</span></p>
            <p>Multi-chain wallet built on Internet Identity. No extension needed — authenticates through II under the hood.</p>
          </div>
        </div>

        <p className="mt-4 mc-text-muted">Both wallets use the same flow: you approve an ICRC-2 allowance, the backend pulls the exact amount needed for the action, and payouts are sent back to your wallet via ICRC-1 transfer.</p>
      </>
    ),
  },
  {
    id: 'glossary',
    title: 'Glossary',
    subtitle: 'Every term in the game, defined.',
    icon: <BookOpen className="h-5 w-5 mc-text-muted" />,
    content: (
      <>
        <div className="space-y-3">
          {[
            ['Pot', 'The shared pool of ICP that funds all player returns. Fed by deposits and fees. Drained by withdrawals and payouts.'],
            ['Position', 'An entitlement to future interest payments from the pot. Your principal is part of the pot.'],
            ['Front-End Load', `The ${pct(COVER_CHARGE_RATE)} fee taken from every deposit before it enters the pot. Routed to Management — does not feed the pot or backers.`],
            ['Carried Interest (Simple)', `The fee charged when withdrawing from a Simple position. Ranges from ${pct(EXIT_TOLL_LATE)} to ${pct(EXIT_TOLL_EARLY)} depending on how long you've been in.`],
            ['Carried Interest (Compounding)', `The fee charged on Compounding position payouts at maturity — ${pct(JACKPOT_FEE_RATE_15D)} on the ${PLAN_DAYS_COMPOUND_15}-day plan, ${pct(JACKPOT_FEE_RATE_30D)} on the ${PLAN_DAYS_COMPOUND_30}-day plan.`],
            ['Backer', `A player who has deposited into the Seed Round. Earns a share of the backer repayment pool from carried interest.`],
            ['Series A Backer', `A backer who voluntarily deposited into the Seed Round. Receives a ${pct(UPSTREAM_BACKER_BONUS)} bonus on their stake.`],
            ['Series B Backer', `A backer created automatically during an Emergency Equity Conversion from a random unprofitable player. Receives a ${pct(DOWNSTREAM_BACKER_BONUS)} bonus on their losses.`],
            ['Entitlement', `The total amount a backer is owed: their original deposit plus their bonus (${pct(UPSTREAM_BACKER_BONUS)} for Series A, ${pct(DOWNSTREAM_BACKER_BONUS)} for Series B).`],
            ['Emergency Equity Conversion', `When the pot can't cover a payout: all positions liquidated, a new round begins, and a random unprofitable player becomes a Series B Backer.`],
            ['Ponzi Points (PP)', 'In-game currency earned through deposits, referrals, and backer stakes. Can only be spent on Shenanigans.'],
            ['Shenanigans', "Cosmetic game actions cast using PP. Range from stealing other players' PP to renaming them to boosting your own earnings rate. All are PP-only — they never touch ICP."],
            ['Downline', `Anyone you referred (or anyone they referred, recursively). You collect 10% of every PP they earn, half kept and half cascading further up the chain.`],
            ['Round', 'A full game cycle, from pot creation to reset. When the pot empties, the round ends and a new one begins.'],
            ['Musical Chairs Wallet', 'Your in-app ICP balance held by the backend canister. Separate from your external wallet (II/Plug).'],
          ].map(([term, def]) => (
            <div key={term as string}>
              <span className="font-bold text-white">{term}</span>
              <span className="mc-text-dim"> — {def}</span>
            </div>
          ))}
        </div>
      </>
    ),
  },
  ];
}

/* ── Main Component ── */
interface DocsPageProps {
  onBack: () => void;
}

export default function DocsPage({ onBack }: DocsPageProps) {
  const [openSections, setOpenSections] = useState<Set<string>>(new Set());
  const { data: mintConfig } = useGetMintConfig();
  const { data: shenaniganConfigs } = useGetShenaniganConfigs();
  const docSections = buildDocSections(mintConfig, shenaniganConfigs);

  // Open + scroll to a section when the hash matches #docs-<sectionId>
  useEffect(() => {
    const openFromHash = () => {
      const hash = window.location.hash;
      if (hash.startsWith('#docs-')) {
        const sectionId = hash.replace('#docs-', '');
        const matchingSection = docSections.find(s => s.id === sectionId);
        if (matchingSection) {
          setOpenSections(prev => new Set(prev).add(sectionId));
          requestAnimationFrame(() => {
            const el = document.getElementById(hash.slice(1));
            if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
          });
        }
      }
    };

    openFromHash(); // run on mount
    window.addEventListener('hashchange', openFromHash);
    return () => window.removeEventListener('hashchange', openFromHash);
  }, []);

  // ESC key closes docs
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onBack(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onBack]);

  const toggleSection = (id: string) => {
    setOpenSections(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
        // Clear hash when closing
        if (window.location.hash === `#docs-${id}`) {
          history.replaceState(null, '', '#docs');
        }
      } else {
        next.add(id);
        // Update hash for deep linking
        history.replaceState(null, '', `#docs-${id}`);
      }
      return next;
    });
  };

  const expandAll = () => setOpenSections(new Set(docSections.map(s => s.id)));
  const collapseAll = () => setOpenSections(new Set());

  return (
    <div className="max-w-2xl mx-auto w-full px-4 py-8 md:py-12">
      {/* Floating close — aligned to the docs column's right edge so it sits near the content. */}
      <div className="fixed inset-x-0 top-32 z-50 pointer-events-none">
        <div className="max-w-2xl mx-auto px-4 relative h-0">
          <button
            onClick={onBack}
            aria-label="Close docs"
            className="absolute right-4 top-0 pointer-events-auto mc-bg-elev-2 hover:mc-bg-elev-3 mc-border-subtle border rounded-full p-2 shadow-lg transition-colors"
          >
            <X className="h-4 w-4 mc-text-primary" />
          </button>
        </div>
      </div>
      {/* Back button */}
      <button
        onClick={onBack}
        className="flex items-center gap-2 text-xs mc-text-muted hover:mc-text-primary transition-colors mb-8"
      >
        <ArrowLeft className="h-4 w-4" />
        Back
      </button>

      {/* Header */}
      <div className="mb-8">
        <h1 className="font-display text-2xl md:text-3xl text-white mb-2">Documentation</h1>
        <p className="text-sm mc-text-dim">
          Every mechanic, every fee, every rule — documented. These docs describe what the code does, not what we promise it does.
        </p>
      </div>

      {/* Expand/Collapse controls */}
      <div className="flex gap-3 mb-4">
        <button onClick={expandAll} className="text-xs mc-text-muted hover:mc-text-primary transition-colors">
          Expand all
        </button>
        <span className="mc-text-muted">·</span>
        <button onClick={collapseAll} className="text-xs mc-text-muted hover:mc-text-primary transition-colors">
          Collapse all
        </button>
      </div>

      {/* Sections */}
      <div className="space-y-2">
        {docSections.map(section => (
          <DocAccordion
            key={section.id}
            section={section}
            isOpen={openSections.has(section.id)}
            onToggle={() => toggleSection(section.id)}
          />
        ))}
      </div>

      {/* Gambling warning */}
      <div className="mt-10 mc-warning-box text-center">
        <div className="flex items-center justify-center gap-2 mb-2">
          <AlertTriangle className="h-5 w-5 text-red-400 shrink-0" />
          <span className="text-base font-bold text-red-300 uppercase tracking-wide">This is a gambling game</span>
          <AlertTriangle className="h-5 w-5 text-red-400 shrink-0" />
        </div>
        <p className="text-sm text-red-300/80">All positions carry risk of total loss.</p>
        <p className="text-sm text-red-300/80">Please play responsibly.</p>
      </div>

      {/* Footer */}
      <div className="mt-8 text-center">
        <p className="text-xs mc-text-muted">
          Musical Chairs is deployed on the Internet Computer. Canister source is the documentation.
        </p>
      </div>
    </div>
  );
}
