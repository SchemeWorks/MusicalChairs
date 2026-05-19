import { useRef, useState } from 'react';
import { Dialog, DialogContent, DialogTitle, DialogDescription } from '@/components/ui/dialog';
import { KeyRound, ShieldAlert, ExternalLink } from 'lucide-react';

const CANISTERS: { name: string; id: string; note: string }[] = [
  { name: 'backend',      id: '5zxxg-tyaaa-aaaac-qeckq-cai', note: 'The pot, the rules, the math.' },
  { name: 'shenanigans',  id: 'j56tm-oaaaa-aaaac-qf34q-cai', note: 'Referrals, Ponzi Points spend.' },
  { name: 'pp_ledger',    id: '5xv2o-iiaaa-aaaac-qeclq-cai', note: 'Ponzi Points ICRC ledger.' },
  { name: 'ponzi_math',   id: 'guy42-yqaaa-aaaaj-qr5pq-cai', note: 'Math canister. To be black-holed.' },
  { name: 'frontend',     id: '5qu42-fqaaa-aaaac-qecla-cai', note: 'This site, served on-chain.' },
];

export default function BetaBadge() {
  const [open, setOpen] = useState(false);
  const [hintOpen, setHintOpen] = useState(false);
  const closeTimerRef = useRef<number | undefined>(undefined);

  const showHint = () => {
    if (closeTimerRef.current) {
      window.clearTimeout(closeTimerRef.current);
      closeTimerRef.current = undefined;
    }
    setHintOpen(true);
  };
  const hideHintSoon = () => {
    if (closeTimerRef.current) window.clearTimeout(closeTimerRef.current);
    closeTimerRef.current = window.setTimeout(() => setHintOpen(false), 120);
  };
  const openModal = () => {
    if (closeTimerRef.current) window.clearTimeout(closeTimerRef.current);
    setHintOpen(false);
    setOpen(true);
  };

  return (
    <>
      <div
        className="relative inline-block"
        onMouseEnter={showHint}
        onMouseLeave={hideHintSoon}
        onFocus={showHint}
        onBlur={hideHintSoon}
      >
        <button
          type="button"
          onClick={openModal}
          className="mc-beta-chip"
          aria-label="Beta disclosure — read the risk factors"
          aria-haspopup="dialog"
        >
          BETA
        </button>

        {hintOpen && (
          <button
            type="button"
            onClick={openModal}
            onMouseEnter={showHint}
            onMouseLeave={hideHintSoon}
            className="mc-beta-hint"
            aria-label="Open beta disclosure"
          >
            <div className="font-display text-[11px] mc-text-gold tracking-wider">
              We could rug you.
            </div>
            <div className="text-[11px] mc-text-dim mt-0.5">
              (We won't.) <span className="mc-text-gold">Read more &rarr;</span>
            </div>
          </button>
        )}
      </div>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="mc-dialog mc-beta-dialog max-w-xl max-h-[90vh] overflow-y-auto">
          <div className="space-y-5">
            <div>
              <div className="flex items-center gap-2 mb-2">
                <ShieldAlert className="h-4 w-4 mc-text-gold shrink-0" />
                <span className="text-[10px] tracking-[0.2em] uppercase mc-text-muted">
                  Risk Factors &sect; 1
                </span>
              </div>
              <DialogTitle className="font-display text-xl text-white leading-tight">
                Beta Software, Founder Keys
              </DialogTitle>
              <DialogDescription className="mc-text-dim text-sm mt-2 leading-relaxed">
                We're not immutable yet. Here's exactly what that means before you make a deposit.
              </DialogDescription>
            </div>

            <section className="space-y-2">
              <h3 className="font-display text-[11px] mc-text-gold uppercase tracking-[0.15em]">
                The site is in beta
              </h3>
              <p className="text-sm mc-text-dim leading-relaxed">
                Doors just opened. The economy is still being tuned, edges still being filed. Expect
                occasional downtime while we ship upgrades, numbers that may shift if we discover
                something broken, and general jankiness.
              </p>
            </section>

            <section className="space-y-2">
              <div className="flex items-center gap-2">
                <KeyRound className="h-3.5 w-3.5 mc-text-gold shrink-0" />
                <h3 className="font-display text-[11px] mc-text-gold uppercase tracking-[0.15em]">
                  The founders retain keys
                </h3>
              </div>
              <p className="text-sm mc-text-dim leading-relaxed">
                The canisters powering this thing are <span className="mc-text-primary font-semibold">upgradeable by the dev</span>.
                Right now, that's one person. Admin functions exist that could, in theory:
              </p>
              <ul className="text-sm mc-text-dim space-y-1 mt-2 ml-4 list-disc marker:mc-text-purple">
                <li>Drain the pot</li>
                <li>Change rules mid-round</li>
                <li>Tune fees to whatever number we feel like</li>
                <li>Pause withdrawals</li>
                <li>Upgrade the contracts to do something completely different</li>
              </ul>
              <p className="text-sm leading-relaxed mt-3 mc-text-primary">
                It would be trivial to rug you. <span className="mc-text-gold font-semibold">We are not going to do that.</span>{' '}
                <span className="mc-text-dim">But you have to take our word for it. For now.</span>
              </p>
            </section>

            <section className="space-y-2">
              <h3 className="font-display text-[11px] mc-text-gold uppercase tracking-[0.15em]">
                The plan
              </h3>
              <p className="text-sm mc-text-dim leading-relaxed">
                Two paths.
              </p>
              <p className="text-sm mc-text-dim leading-relaxed">
                <span className="mc-text-primary font-semibold">ponzi_math</span> gets
                {' '}<span className="mc-text-gold font-semibold">black-holed</span> once we're confident in
                the numbers — permanently immutable, no upgrade path, no admin keys. The math becomes
                trustless.
              </p>
              <p className="text-sm mc-text-dim leading-relaxed">
                The game canisters stay <span className="mc-text-primary font-semibold">dev-controlled</span> —
                we've got more mechanics, balance tweaks, and shenanigans to ship, and freezing them would
                freeze the product.
              </p>
              <p className="text-sm mc-text-dim leading-relaxed pt-1">
                Size your deposits accordingly.
              </p>
            </section>

            <section className="space-y-2 pt-1">
              <h3 className="font-display text-[11px] mc-text-gold uppercase tracking-[0.15em]">
                Verify it yourself
              </h3>
              <p className="text-xs mc-text-muted leading-relaxed">
                Every canister below is public. Read the code, watch the calls.
              </p>
              <div className="mc-beta-canisters">
                {CANISTERS.map(c => (
                  <a
                    key={c.id}
                    href={`https://dashboard.internetcomputer.org/canister/${c.id}`}
                    target="_blank"
                    rel="noreferrer"
                    className="mc-beta-canister-row"
                  >
                    <div className="flex-1 min-w-0">
                      <div className="font-display text-[10px] mc-text-gold uppercase tracking-wider">
                        {c.name}
                      </div>
                      <div className="font-mono text-[11px] mc-text-dim truncate">{c.id}</div>
                      <div className="text-[10px] mc-text-muted mt-0.5">{c.note}</div>
                    </div>
                    <ExternalLink className="h-3.5 w-3.5 mc-text-muted shrink-0" />
                  </a>
                ))}
              </div>
            </section>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
