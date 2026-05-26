import React from 'react';
import { Info, Shield, Zap, AlertTriangle } from 'lucide-react';

export default function GuardrailsTooltip() {
  const [open, setOpen] = React.useState(false);
  const wrapRef = React.useRef<HTMLDivElement | null>(null);

  React.useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  return (
    <div ref={wrapRef} className="relative">
      <button
        type="button"
        aria-label="Guardrails"
        aria-expanded={open}
        onClick={() => setOpen(v => !v)}
        className="h-7 w-7 rounded-full bg-white/5 hover:bg-white/10 flex items-center justify-center mc-text-muted hover:mc-text-cyan"
        title="Guardrails"
      >
        <Info className="h-4 w-4" />
      </button>
      {open && (
        <div
          role="dialog"
          aria-label="Shenanigans guardrails"
          className="absolute right-0 top-full mt-2 z-50 w-72 rounded-lg border border-white/10 bg-zinc-900 shadow-xl p-3"
          style={{ boxShadow: '0 12px 40px rgba(0,0,0,0.5)' }}
        >
          <h4 className="font-display text-sm mc-text-primary mb-2 flex items-center gap-1.5">
            <Shield className="h-3.5 w-3.5 mc-text-cyan" /> Guardrails
          </h4>
          <div className="space-y-2 text-xs mc-text-dim">
            <div className="flex items-start gap-2">
              <Info className="h-3 w-3 mc-text-cyan mt-0.5 flex-shrink-0" />
              <span><strong className="mc-text-primary">PP &amp; Cosmetics Only</strong> — Never affects ICP, AUM, backer selection, or payout math.</span>
            </div>
            <div className="flex items-start gap-2">
              <Zap className="h-3 w-3 mc-text-purple mt-0.5 flex-shrink-0" />
              <span><strong className="mc-text-primary">Cooldowns</strong> — A successful cast locks that spell for hours. Failures and backfires? Try again immediately.</span>
            </div>
            <div className="flex items-start gap-2">
              <AlertTriangle className="h-3 w-3 mc-text-gold mt-0.5 flex-shrink-0" />
              <span><strong className="mc-text-primary">No Refunds</strong> — Every cast burns PP, win or lose.</span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
