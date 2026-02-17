import React, { useState } from 'react';
import { ChevronDown, X, Dices, Rocket, DollarSign, Landmark, Users, Dice5, Wallet, Flame, Shield, Zap, AlertTriangle } from 'lucide-react';

interface AccordionSection {
  id: string;
  title: string;
  icon: React.ReactNode;
  content: React.ReactNode;
}

function AccordionItem({ section, isOpen, onToggle }: { section: AccordionSection; isOpen: boolean; onToggle: () => void }) {
  return (
    <div className="mc-card overflow-hidden">
      <button
        onClick={onToggle}
        className="w-full flex items-center gap-3 p-4 text-left hover:bg-white/[0.02] transition-colors"
      >
        <span className="shrink-0">{section.icon}</span>
        <span className="font-display text-sm text-white flex-1">{section.title}</span>
        <ChevronDown className={`h-4 w-4 mc-text-muted transition-transform duration-200 ${isOpen ? 'rotate-180' : ''}`} />
      </button>
      {isOpen && (
        <div className="px-4 pb-4 text-xs mc-text-dim leading-relaxed space-y-3 border-t border-white/5 pt-3">
          {section.content}
        </div>
      )}
    </div>
  );
}

const sections: AccordionSection[] = [
  {
    id: 'how-it-works',
    title: 'How It Works',
    icon: <Dices className="h-5 w-5 mc-text-green" />,
    content: (
      <>
        <p>Musical Chairs is a transparent Ponzi scheme on the Internet Computer. Everyone knows the rules upfront.</p>
        <ol className="list-decimal list-inside space-y-1.5 ml-1">
          <li><strong className="text-white">Deposit ICP</strong> — Choose a game plan and deposit ICP into the pot.</li>
          <li><strong className="text-white">Earn daily</strong> — Your position accrues interest from the pot every day.</li>
          <li><strong className="text-white">Withdraw</strong> — Cash out anytime. Earlier exits pay a higher toll.</li>
          <li><strong className="text-white">The catch</strong> — When the pot empties, the game resets. Anyone still in takes a total loss. That's the Ponzi part.</li>
        </ol>
        <p>You also earn <strong className="mc-text-purple">Ponzi Points</strong> for participating, which you can spend on Shenanigans (cosmetic chaos).</p>
      </>
    ),
  },
  {
    id: 'game-plans',
    title: 'Game Plans',
    icon: <Rocket className="h-5 w-5 mc-text-purple" />,
    content: (
      <>
        <p>Three plans with different risk/reward profiles:</p>
        <div className="space-y-3 mt-2">
          <div className="mc-card p-3 mc-accent-green">
            <div className="flex items-center gap-2 mb-1">
              <span className="font-display text-xs mc-text-green">Simple 21-Day</span>
            </div>
            <p>11% daily return for 21 days. Withdraw anytime — you get your earnings minus the exit toll. No lock-up. Lowest risk, steady returns.</p>
          </div>
          <div className="mc-card p-3 mc-accent-purple">
            <div className="flex items-center gap-2 mb-1">
              <span className="font-display text-xs mc-text-purple">Compounding 15-Day</span>
            </div>
            <p>12% daily, compounded. Funds are locked for the full 15 days. Higher returns, but no early withdrawal. Moderate risk.</p>
          </div>
          <div className="mc-card p-3 mc-accent-gold">
            <div className="flex items-center gap-2 mb-1">
              <span className="font-display text-xs mc-text-gold">Compounding 30-Day</span>
            </div>
            <p>9% daily, compounded over 30 days. Longest lock, biggest potential. Highest risk — a lot can happen in 30 days.</p>
          </div>
        </div>
      </>
    ),
  },
  {
    id: 'exit-tolls',
    title: 'Exit Tolls',
    icon: <DollarSign className="h-5 w-5 mc-text-gold" />,
    content: (
      <>
        <p>Simple plan withdrawals incur a toll based on how long you've been in:</p>
        <table className="w-full mt-2 text-xs">
          <thead>
            <tr className="border-b border-white/10">
              <th className="text-left py-1.5 mc-text-muted font-normal">Withdrawal Day</th>
              <th className="text-right py-1.5 mc-text-muted font-normal">Toll</th>
            </tr>
          </thead>
          <tbody className="mc-text-dim">
            <tr className="border-b border-white/5"><td className="py-1.5">Day 0-3</td><td className="text-right mc-text-danger">7%</td></tr>
            <tr className="border-b border-white/5"><td className="py-1.5">Day 3-10</td><td className="text-right mc-text-gold">5%</td></tr>
            <tr><td className="py-1.5">Day 10+</td><td className="text-right mc-text-green">3%</td></tr>
          </tbody>
        </table>
        <p className="mt-2">Compounding plans charge a flat <strong className="mc-text-gold">13% Jackpot Fee</strong> on maturity. No early withdrawal available.</p>
        <p className="mt-1 mc-text-muted">Tolls go back into the pot — they fund other players' returns.</p>
      </>
    ),
  },
  {
    id: 'dealers',
    title: 'Dealers & Seed Round',
    icon: <Landmark className="h-5 w-5 mc-text-gold" />,
    content: (
      <>
        <p>Dealers are the "VCs" of Musical Chairs. Back the house and earn a cut of the action.</p>
        <ul className="list-disc list-inside space-y-1.5 ml-1 mt-2">
          <li><strong className="text-white">12% entitlement</strong> — Dealers collectively earn 12% of all deposits.</li>
          <li><strong className="text-white">3% maintenance fee</strong> — A small fee on dealer earnings goes back to the house.</li>
          <li><strong className="text-white">Pro-rata distribution</strong> — Your share scales with the size of your dealer stake.</li>
          <li><strong className="text-white">Withdrawal</strong> — You can pull your dealer stake anytime.</li>
        </ul>
        <p className="mt-2 mc-text-muted">Think of it as owning a tiny piece of the casino. The risk: if the game resets, your stake is part of what gets redistributed.</p>
      </>
    ),
  },
  {
    id: 'shenanigans',
    title: 'Shenanigans',
    icon: <Dice5 className="h-5 w-5 mc-text-green" />,
    content: (
      <>
        <p>Spend Ponzi Points on cosmetic chaos. 11 shenanigans across three categories:</p>

        <div className="mt-3 space-y-2">
          <p className="font-display text-xs mc-text-danger">Offense</p>
          <ul className="space-y-1 ml-1">
            <li><strong className="text-white">Money Trickster</strong> (120 PP) — Steal 2-8% of a target's PP. 60% success.</li>
            <li><strong className="text-white">AOE Skim</strong> (600 PP) — Siphon 1-3% from every player. 40% success.</li>
            <li><strong className="text-white">Mint Tax Siphon</strong> (1200 PP) — Skim 5% of a target's new PP for 7 days. 70% success.</li>
            <li><strong className="text-white">Downline Heist</strong> (500 PP) — Steal a referral from someone's downline. 30% success.</li>
            <li><strong className="text-white">Purse Cutter</strong> (900 PP) — Target loses 25-50% of their PP. 20% success.</li>
            <li><strong className="text-white">Whale Rebalance</strong> (800 PP) — Take 20% from the top 3 PP holders. 50% success.</li>
          </ul>
        </div>

        <div className="mt-3 space-y-2">
          <p className="font-display text-xs mc-text-green">Defense & Buffs</p>
          <ul className="space-y-1 ml-1">
            <li><strong className="text-white">Magic Mirror</strong> (200 PP) — Shield that blocks one hostile shenanigan. 100% success.</li>
            <li><strong className="text-white">PP Booster Aura</strong> (300 PP) — Earn +5-15% additional PP for the rest of the round. 100% success.</li>
            <li><strong className="text-white">Downline Boost</strong> (400 PP) — Your referrals kick up 1.3x PP for the rest of the round. 100% success.</li>
          </ul>
        </div>

        <div className="mt-3 space-y-2">
          <p className="font-display text-xs mc-text-purple">Chaos</p>
          <ul className="space-y-1 ml-1">
            <li><strong className="text-white">Rename Spell</strong> (200 PP) — Change someone's display name for 7 days. 90% success.</li>
            <li><strong className="text-white">Golden Name</strong> (100 PP) — Gold name on the leaderboard for 24 hours. 100% success.</li>
          </ul>
        </div>

        <p className="mt-3 mc-text-muted">All shenanigans can backfire. Players under 200 PP are protected from negative effects.</p>
      </>
    ),
  },
  {
    id: 'ponzi-points',
    title: 'Ponzi Points',
    icon: <Zap className="h-5 w-5 mc-text-purple" />,
    content: (
      <>
        <p>Ponzi Points (PP) are earned by playing and can only be spent on Shenanigans. You cannot buy or trade them.</p>
        <div className="mt-2 space-y-1">
          <p><strong className="text-white">Earning rates per plan:</strong></p>
          <ul className="list-disc list-inside ml-1 space-y-1">
            <li>Simple 21-Day: PP earned on deposit + daily accrual</li>
            <li>Compounding 15-Day: Higher PP multiplier (locked funds = more commitment)</li>
            <li>Compounding 30-Day: Highest PP multiplier</li>
          </ul>
        </div>
        <p className="mt-2">PP are also earned through referrals (your downline's activity generates PP for you) and special events.</p>
      </>
    ),
  },
  {
    id: 'pyramid',
    title: 'The Pyramid (MLM)',
    icon: <Users className="h-5 w-5 mc-text-cyan" />,
    content: (
      <>
        <p>Three-level referral pyramid. Share your referral link and earn from your downline's deposits:</p>
        <table className="w-full mt-2 text-xs">
          <thead>
            <tr className="border-b border-white/10">
              <th className="text-left py-1.5 mc-text-muted font-normal">Level</th>
              <th className="text-left py-1.5 mc-text-muted font-normal">Relationship</th>
              <th className="text-right py-1.5 mc-text-muted font-normal">Your Cut</th>
            </tr>
          </thead>
          <tbody className="mc-text-dim">
            <tr className="border-b border-white/5"><td className="py-1.5 mc-text-green">L1</td><td>Direct referrals</td><td className="text-right font-bold mc-text-green">10%</td></tr>
            <tr className="border-b border-white/5"><td className="py-1.5 mc-text-purple">L2</td><td>Their referrals</td><td className="text-right font-bold mc-text-purple">5%</td></tr>
            <tr><td className="py-1.5 mc-text-gold">L3</td><td>Their referrals' referrals</td><td className="text-right font-bold mc-text-gold">3%</td></tr>
          </tbody>
        </table>
        <p className="mt-2 mc-text-muted">Referral earnings are paid in ICP directly to your internal wallet.</p>
      </>
    ),
  },
  {
    id: 'redistribution',
    title: 'Redistribution Events',
    icon: <Flame className="h-5 w-5 mc-text-danger" />,
    content: (
      <>
        <p>When the pot balance drops critically low, a redistribution event triggers:</p>
        <ul className="list-disc list-inside space-y-1.5 ml-1 mt-2">
          <li><strong className="text-white">All active positions are liquidated</strong> — total loss for anyone still in.</li>
          <li><strong className="text-white">The game resets</strong> — a new round begins with a fresh pot.</li>
          <li><strong className="text-white">Dealer stakes are affected</strong> — seed round participants also take a hit.</li>
        </ul>
        <p className="mt-2">This is the core Ponzi mechanic. The game explicitly tells you this will happen. The only question is <em>when</em>.</p>
        <p className="mt-1 mc-text-muted">Watch the pot balance in the status bar. When it starts dropping fast, that's your signal.</p>
      </>
    ),
  },
  {
    id: 'wallet',
    title: 'Wallet System',
    icon: <Wallet className="h-5 w-5 mc-text-gold" />,
    content: (
      <>
        <p>Musical Chairs supports two wallet modes:</p>
        <div className="mt-2 space-y-2">
          <div className="mc-card p-3">
            <p className="font-display text-xs text-white mb-1">Internet Identity (II)</p>
            <p>Browser-based authentication. Your ICP is held in an internal wallet managed by the canister. Deposit and withdraw through the in-app wallet interface.</p>
          </div>
          <div className="mc-card p-3">
            <p className="font-display text-xs text-white mb-1">Plug / OISY Wallet</p>
            <p>External wallet connection. Transactions go through your wallet's approval flow. More control, familiar UX if you use these wallets already.</p>
          </div>
        </div>
        <p className="mt-2 mc-text-muted">Both wallet types have full access to all game features. The difference is custody — II keeps funds in the canister, external wallets keep funds in your wallet.</p>
      </>
    ),
  },
];

interface GameDocsProps {
  onClose: () => void;
}

export default function GameDocs({ onClose }: GameDocsProps) {
  const [openSections, setOpenSections] = useState<Set<string>>(new Set(['how-it-works']));

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

  return (
    <div className="fixed inset-0 z-50 flex flex-col mc-bg overflow-y-auto">
      {/* Header */}
      <div className="sticky top-0 z-10 mc-header px-4 py-3">
        <div className="max-w-2xl mx-auto flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Dices className="h-5 w-5 mc-text-gold" />
            <h1 className="font-display text-base text-white">How To Play</h1>
          </div>
          <button
            onClick={onClose}
            className="p-2 mc-text-muted hover:mc-text-primary transition-colors"
          >
            <X className="h-5 w-5" />
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="max-w-2xl mx-auto w-full px-4 py-6 space-y-2">
        <p className="text-xs mc-text-muted mb-4 text-center font-accent italic">
          "Knowledge is power. Power is money. Money is what you're about to lose." — Charles
        </p>

        {sections.map(section => (
          <AccordionItem
            key={section.id}
            section={section}
            isOpen={openSections.has(section.id)}
            onToggle={() => toggleSection(section.id)}
          />
        ))}

        {/* Disclaimer */}
        <div className="mt-6 pt-4 border-t border-white/5 text-center">
          <p className="text-xs mc-text-muted opacity-60">
            <AlertTriangle className="h-3 w-3 inline-block mr-1 align-text-top" />
            This is a gambling game. Please play responsibly.
          </p>
        </div>
      </div>
    </div>
  );
}
