import React from 'react';
import confetti from 'canvas-confetti';

interface WhitelistedFanfareProps {
  /** When true, the overlay is mounted and visible. */
  open: boolean;
  /** Called when the overlay dismisses (auto after 5s or user click/key). */
  onClose: () => void;
}

const DISMISS_MS = 5000;

const GOLD_COLORS = ['#FFD700', '#FFF4B0', '#E8C547', '#FFFFFF'];

/**
 * Full-viewport cast-moment overlay for a successful Whitelisted cast.
 * Fires a gold confetti burst from screen center on mount, displays a gold-
 * bordered card with the spell name + 72-hour duration, and auto-dismisses
 * after 5s (or any click / keypress).
 */
export default function WhitelistedFanfare({ open, onClose }: WhitelistedFanfareProps) {
  // Fire confetti exactly once per open transition.
  React.useEffect(() => {
    if (!open) return;
    confetti({
      particleCount: 150,
      spread: 90,
      origin: { x: 0.5, y: 0.5 },
      colors: GOLD_COLORS,
      gravity: 1,
      ticks: 200,
      zIndex: 9999,
    });
  }, [open]);

  // Stable ref to the latest onClose so the auto-dismiss effect doesn't
  // restart every time the parent re-renders. The parent (Shenanigans)
  // polls on a 10s interval and creates a fresh onClose arrow per render,
  // so depending on `onClose` directly would perpetually reset the 5s timer
  // and the auto-dismiss would never fire.
  const onCloseRef = React.useRef(onClose);
  React.useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  // Auto-dismiss timer + key listener. Depends only on `open`.
  React.useEffect(() => {
    if (!open) return;
    const dismiss = () => onCloseRef.current();
    const timer = window.setTimeout(dismiss, DISMISS_MS);
    window.addEventListener('keydown', dismiss, { once: true });
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener('keydown', dismiss);
    };
  }, [open]);

  if (!open) return null;

  return (
    <div
      onClick={onClose}
      className="fixed inset-0 z-[9998] flex items-center justify-center cursor-pointer"
      style={{
        background:
          'radial-gradient(circle at center, rgba(255,215,0,0.15) 0%, rgba(0,0,0,0.7) 60%)',
        animation: 'mc-fanfare-fade-in 200ms ease-out',
      }}
      role="dialog"
      aria-label="Whitelisted spell cast successfully"
    >
      <div
        className="mc-card-elevated border-2 border-[var(--mc-gold)] rounded-2xl px-12 py-10 text-center max-w-md mx-4"
        style={{ boxShadow: '0 0 60px rgba(255, 215, 0, 0.4)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mc-name-vip font-display text-5xl mb-3 tracking-wide">
          WHITELISTED
        </div>
        <div className="mc-text-gold font-display text-xl mb-4 tracking-widest">
          72 HOURS
        </div>
        <div className="italic text-sm mc-text-muted">
          You're on the list now.
        </div>
      </div>
    </div>
  );
}
