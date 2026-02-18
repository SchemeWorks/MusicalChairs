import React, { useState } from 'react';
import { ChevronDown, ChevronRight, Dices, Rocket, DollarSign, Landmark, Users, Dice5, Wallet, Flame, Shield, Zap, AlertTriangle, HelpCircle, BookOpen, ArrowLeft } from 'lucide-react';

interface DocSection {
  id: string;
  title: string;
  subtitle: string;
  icon: React.ReactNode;
  content: React.ReactNode;
}

function DocAccordion({ section, isOpen, onToggle }: { section: DocSection; isOpen: boolean; onToggle: () => void }) {
  return (
    <div className="mc-card overflow-hidden transition-all">
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
    <table className="w-full text-sm mt-2">
      <thead>
        <tr className="border-b border-white/10">
          {headers.map((h, i) => (
            <th key={i} className={`py-2 mc-text-muted font-normal ${i === headers.length - 1 ? 'text-right' : 'text-left'}`}>{h}</th>
          ))}
        </tr>
      </thead>
      <tbody className="mc-text-dim">
        {rows.map((row, i) => (
          <tr key={i} className="border-b border-white/5">
            {row.map((cell, j) => (
              <td key={j} className={`py-2 ${j === row.length - 1 ? 'text-right' : ''}`}>{cell}</td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/* ── Section data ── */
const docSections: DocSection[] = [
  {
    id: 'overview',
    title: 'What Is Musical Chairs?',
    subtitle: 'The elevator pitch — and the fine print.',
    icon: <Dices className="h-5 w-5 mc-text-green" />,
    content: (
      <>
        <p>Musical Chairs is a transparent Ponzi scheme on the Internet Computer. New deposits fund existing players' returns. When deposits slow and the pot drains, the game resets and anyone still holding a position takes a total loss.</p>
        <p>Every mechanic described in these docs is exactly what happens on-chain. There are no hidden fees, no backdoors, no "trust us" moments. The code is the product, and the product is openly a Ponzi scheme.</p>
        <p className="mc-text-muted">The point is transparency. Most DeFi protocols work this way but obscure it behind jargon. We just say it out loud.</p>
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
            [<span className="mc-text-green font-bold">Simple 21-Day</span>, '11%', '21 days', 'None — withdraw anytime', '1,000 PP / ICP'],
            [<span className="mc-text-purple font-bold">Compounding 15-Day</span>, '12%', '15 days', 'Full lockup', '2,000 PP / ICP'],
            [<span className="mc-text-gold font-bold">Compounding 30-Day</span>, '9%', '30 days', 'Full lockup', '3,000 PP / ICP'],
          ]}
        />
        <div className="mt-4 space-y-2">
          <p><strong className="text-white">Simple mode:</strong> Interest accrues daily on your original deposit. You can withdraw accumulated earnings at any time, subject to exit tolls. Your principal stays in the game until the plan matures or you close the position.</p>
          <p><strong className="text-white">Compounding mode:</strong> Interest compounds daily on your growing balance. Funds are fully locked until maturity. At maturity, you receive your compounded total minus the 13% Jackpot Fee. No early withdrawal.</p>
          <p><strong className="text-white">The risk:</strong> If the pot empties before your plan matures, you lose everything — principal and accrued earnings. Longer plans = higher returns = higher risk of getting caught in a reset.</p>
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
          <p><strong className="text-white">Minimum deposit:</strong> 0.1 ICP for all plan types.</p>
          <p><strong className="text-white">Maximum deposit (Simple only):</strong> The greater of 20% of the current pot balance or 5 ICP. This prevents a single player from draining the pot in one withdrawal.</p>
          <p><strong className="text-white">Maximum deposit (Compounding):</strong> No limit above the 0.1 ICP minimum.</p>
          <p><strong className="text-white">Rate limit:</strong> 3 positions per hour per user.</p>
          <p><strong className="text-white">Entry skim:</strong> 3% of every deposit is skimmed. Half seeds the next round's pot, half repays the House (dealers).</p>
        </div>
        <div className="mc-card p-4 mt-4 mc-accent-gold">
          <p className="text-xs mc-text-muted">Deposits go through the ICRC-2 approve/transfer_from flow. You approve the backend canister to pull funds from your wallet, then the backend executes the transfer. This is a standard ICP token pattern.</p>
        </div>
      </>
    ),
  },
  {
    id: 'fees',
    title: 'Fees & Exit Tolls',
    subtitle: 'Every fee in the system, with exact percentages.',
    icon: <DollarSign className="h-5 w-5 mc-text-gold" />,
    content: (
      <>
        <p className="font-bold text-white mb-2">Entry</p>
        <DocTable
          headers={['Fee', 'Rate', 'Applies To']}
          rows={[
            ['Entry Skim', <span className="mc-text-gold font-bold">3%</span>, 'Every deposit'],
            ['Dealer Maintenance Fee', <span className="mc-text-gold font-bold">3%</span>, 'Every deposit (to dealers)'],
          ]}
        />

        <p className="font-bold text-white mb-2 mt-6">Exit — Simple Positions</p>
        <DocTable
          headers={['Withdrawal Window', 'Toll']}
          rows={[
            ['Day 0–3', <span className="mc-text-danger font-bold">7%</span>],
            ['Day 3–10', <span className="mc-text-gold font-bold">5%</span>],
            ['Day 10+', <span className="mc-text-green font-bold">3%</span>],
          ]}
        />

        <p className="font-bold text-white mb-2 mt-6">Exit — Compounding Positions</p>
        <DocTable
          headers={['Fee', 'Rate', 'When']}
          rows={[
            ['Jackpot Fee', <span className="mc-text-gold font-bold">13%</span>, 'On maturity payout'],
          ]}
        />

        <p className="mt-4 mc-text-muted">All tolls and fees flow back into the pot or to dealers. They fund other players' returns. This is the engine — new money in, old money out.</p>
        <p className="mc-text-muted">Half of entry skims and exit tolls seed the next round's pot. The other half repays the House.</p>
      </>
    ),
  },
  {
    id: 'dealers',
    title: 'The Seed Round (Dealers)',
    subtitle: 'Back the house. Earn a cut. Take on house-level risk.',
    icon: <Landmark className="h-5 w-5 mc-text-gold" />,
    content: (
      <>
        <p>Dealers are the house. They deposit ICP into the Seed Round and earn a 12% entitlement on their stake, repaid over time through player deposits.</p>

        <p className="font-bold text-white mt-4 mb-2">How It Works</p>
        <div className="space-y-2">
          <p><strong className="text-white">Deposit:</strong> Minimum 0.1 ICP. Your deposit goes directly into the pot.</p>
          <p><strong className="text-white">Entitlement:</strong> You're owed 112% of your deposit (original + 12% bonus).</p>
          <p><strong className="text-white">Repayment:</strong> Comes from the 3% dealer maintenance fee on every player deposit.</p>
          <p><strong className="text-white">PP earned:</strong> 4,000 Ponzi Points per ICP deposited.</p>
        </div>

        <p className="font-bold text-white mt-4 mb-2">Fee Distribution</p>
        <DocTable
          headers={['Share', 'Recipient']}
          rows={[
            [<span className="mc-text-gold font-bold">35%</span>, 'Oldest upstream dealer'],
            [<span className="mc-text-purple font-bold">25%</span>, 'Split among other upstream dealers'],
            [<span className="mc-text-cyan font-bold">40%</span>, 'Split among all dealers'],
          ]}
        />

        <p className="font-bold text-white mt-4 mb-2">Dealer Types</p>
        <div className="space-y-2">
          <p><strong className="mc-text-green">Upstream Dealers:</strong> Created by voluntarily depositing into the Seed Round.</p>
          <p><strong className="mc-text-danger">Downstream Dealers:</strong> Created automatically during a Redistribution Event — a random unprofitable player gets converted into a dealer with an entitlement matching their losses plus the 12% bonus.</p>
        </div>
      </>
    ),
  },
  {
    id: 'referrals',
    title: 'The Pyramid (MLM)',
    subtitle: 'Three-level referral system. Yes, we called it a pyramid.',
    icon: <Users className="h-5 w-5 mc-text-cyan" />,
    content: (
      <>
        <p>Share your referral link and earn ICP from your downline's deposits. Three levels deep.</p>
        <DocTable
          headers={['Level', 'Relationship', 'Your Cut']}
          rows={[
            [<span className="mc-text-green font-bold">L1</span>, 'Direct referrals', <span className="mc-text-green font-bold">10%</span>],
            [<span className="mc-text-purple font-bold">L2</span>, 'Their referrals', <span className="mc-text-purple font-bold">5%</span>],
            [<span className="mc-text-gold font-bold">L3</span>, 'Their referrals\' referrals', <span className="mc-text-gold font-bold">3%</span>],
          ]}
        />
        <p className="mt-4">Referral earnings are paid in ICP directly to your internal Musical Chairs wallet. They also contribute to your Ponzi Points balance.</p>
        <p className="mc-text-muted">Yes, it's a pyramid. We're not pretending otherwise.</p>
      </>
    ),
  },
  {
    id: 'redistribution',
    title: 'The Redistribution Event',
    subtitle: 'What happens when the music stops.',
    icon: <Flame className="h-5 w-5 mc-text-danger" />,
    content: (
      <>
        <p>When the pot can't cover a payout, the game resets. This is the core Ponzi mechanic.</p>
        <div className="space-y-2 mt-3">
          <p><strong className="mc-text-danger">All active positions are liquidated.</strong> Total loss for anyone still holding.</p>
          <p><strong className="mc-text-danger">All pending payouts are voided.</strong> Accrued but unwithdrawn earnings disappear.</p>
          <p><strong className="mc-text-gold">A random unprofitable player becomes a Downstream Dealer.</strong> Their entitlement equals their losses plus a 12% dealer bonus.</p>
          <p><strong className="mc-text-green">A new round begins.</strong> Fresh pot, clean slate, same rules.</p>
        </div>
        <div className="mc-card p-4 mt-4 mc-accent-danger">
          <p className="text-xs"><strong className="text-white">The signal:</strong> Watch the pot balance. When deposits slow and the pot starts draining faster than it fills, a reset is coming. The only question is whether you withdraw in time.</p>
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
        <p>Ponzi Points are earned automatically through gameplay. They cannot be bought, sold, or transferred. Their only use is casting Shenanigans.</p>

        <p className="font-bold text-white mt-4 mb-2">Earning Rates</p>
        <DocTable
          headers={['Activity', 'PP Earned']}
          rows={[
            ['Simple 21-Day deposit', '1,000 PP per ICP'],
            ['Compounding 15-Day deposit', '2,000 PP per ICP'],
            ['Compounding 30-Day deposit', '3,000 PP per ICP'],
            ['Seed Round deposit', '4,000 PP per ICP'],
            ['Referral activity', 'Based on referral earnings'],
          ]}
        />
        <p className="mt-4 mc-text-muted">PP do not affect ICP payouts, pot mechanics, or game math in any way. They're purely cosmetic currency for Shenanigans.</p>
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
        <p>11 shenanigans across three categories. All cost PP. All can backfire. Players under 200 PP are protected from negative effects.</p>

        <p className="font-bold mc-text-danger mt-4 mb-2">Offense</p>
        <DocTable
          headers={['Shenanigan', 'Cost', 'Effect', 'Success']}
          rows={[
            ['Money Trickster', '120 PP', 'Steal 2–8% of target\'s PP (max 250)', '60%'],
            ['AOE Skim', '600 PP', 'Siphon 1–3% from every player (max 60/ea)', '40%'],
            ['Mint Tax Siphon', '1,200 PP', 'Skim 5% of target\'s new PP for 7 days (max 1,000)', '70%'],
            ['Downline Heist', '500 PP', 'Steal a referral from someone\'s downline', '30%'],
            ['Purse Cutter', '900 PP', 'Target loses 25–50% PP (max 800)', '20%'],
            ['Whale Rebalance', '800 PP', 'Take 20% from top 3 PP holders (max 300/whale)', '50%'],
          ]}
        />

        <p className="font-bold mc-text-green mt-4 mb-2">Defense & Buffs</p>
        <DocTable
          headers={['Shenanigan', 'Cost', 'Effect', 'Success']}
          rows={[
            ['Magic Mirror', '200 PP', 'Shield: blocks one hostile shenanigan', '100%'],
            ['PP Booster Aura', '300 PP', '+5–15% additional PP for rest of round', '100%'],
            ['Downline Boost', '400 PP', 'Referrals kick up 1.3x PP for rest of round', '100%'],
          ]}
        />

        <p className="font-bold mc-text-purple mt-4 mb-2">Chaos</p>
        <DocTable
          headers={['Shenanigan', 'Cost', 'Effect', 'Success']}
          rows={[
            ['Rename Spell', '200 PP', 'Change someone\'s display name for 7 days', '90%'],
            ['Golden Name', '100 PP', 'Gold name on leaderboard for 24 hours', '100%'],
          ]}
        />
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
        <p>Musical Chairs supports three wallet types. All have full access to every game feature.</p>

        <div className="space-y-3 mt-3">
          <div className="mc-card p-4">
            <p className="font-display text-sm text-white mb-1">Internet Identity</p>
            <p>ICP's native browser-based authentication. No extension needed. Funds are held in an internal wallet managed by the backend canister. Deposit and withdraw through the in-app wallet interface.</p>
          </div>
          <div className="mc-card p-4">
            <p className="font-display text-sm text-white mb-1">Plug Wallet</p>
            <p>Browser extension wallet. Transactions go through Plug's approval flow. Requires the extension to be installed.</p>
          </div>
          <div className="mc-card p-4">
            <p className="font-display text-sm text-white mb-1">OISY Wallet</p>
            <p>Multi-chain wallet built on Internet Identity. No extension needed — authenticates through II under the hood.</p>
          </div>
        </div>

        <p className="mt-4 mc-text-muted">Regardless of wallet type, all ICP deposited into the game is held by the backend canister. When you withdraw, the canister sends ICP back to your connected wallet via ICRC-1 transfer.</p>
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
            ['Position', 'An active game entry. Created when you deposit ICP into a plan. Has a start date, plan type, and accruing earnings.'],
            ['Entry Skim', 'The 3% fee taken from every deposit before it enters the pot.'],
            ['Exit Toll', 'The fee charged when withdrawing from a Simple position. Ranges from 3% to 7% depending on how long you\'ve been in.'],
            ['Jackpot Fee', 'The 13% fee charged on Compounding position payouts at maturity.'],
            ['Dealer', 'A player who has deposited into the Seed Round. Earns a share of the 3% dealer maintenance fee from all deposits.'],
            ['Upstream Dealer', 'A dealer who voluntarily deposited into the Seed Round.'],
            ['Downstream Dealer', 'A dealer created automatically during a Redistribution Event from a random unprofitable player.'],
            ['Entitlement', 'The total amount a dealer is owed: their original deposit plus the 12% dealer bonus.'],
            ['The Redistribution Event', 'When the pot can\'t cover a payout: all positions liquidated, a new round begins, and a random unprofitable player becomes a Downstream Dealer.'],
            ['Ponzi Points (PP)', 'In-game currency earned through deposits, referrals, and dealer stakes. Can only be spent on Shenanigans. Cannot be traded or transferred.'],
            ['Shenanigans', 'Cosmetic game actions cast using PP. Range from stealing other players\' PP to renaming them to boosting your own earnings rate. All are PP-only — they never touch ICP.'],
            ['Downline', 'Players referred by you (L1), or referred by your referrals (L2, L3). You earn ICP from their deposits.'],
            ['Round', 'A full game cycle, from pot creation to redistribution. When the pot empties, the round ends and a new one begins.'],
            ['Musical Chairs Wallet', 'Your in-app ICP balance held by the backend canister. Separate from your external wallet (II/Plug/OISY).'],
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

/* ── Main Component ── */
interface DocsPageProps {
  onBack: () => void;
}

export default function DocsPage({ onBack }: DocsPageProps) {
  const [openSections, setOpenSections] = useState<Set<string>>(new Set());

  const toggleSection = (id: string) => {
    setOpenSections(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const expandAll = () => setOpenSections(new Set(docSections.map(s => s.id)));
  const collapseAll = () => setOpenSections(new Set());

  return (
    <div className="max-w-2xl mx-auto w-full px-4 py-8 md:py-12">
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
