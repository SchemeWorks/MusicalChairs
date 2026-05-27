/**
 * Mobile bottom-LEFT FAB — mirrors the trollbox FAB on the right. Tap opens
 * the same BuyPPFlyout in a bottom sheet. Hidden on lg+ (desktop uses the
 * sidebar widget instead).
 *
 * Position: bottom-20 to clear the mc-bottom-tabs nav strip (≈64px tall), with
 * env(safe-area-inset-bottom) absorbed by the tab strip's own padding.
 */

import { useState } from 'react';
import { Zap } from 'lucide-react';
import BuyPPFlyout from './BuyPPFlyout';

export default function BuyPPFab() {
  const [open, setOpen] = useState(false);

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        aria-label="Buy PP"
        className="mc-buy-pp-fab lg:hidden"
      >
        <img src="/pp-coin.png" alt="" className="w-7 h-7" draggable={false} />
        <span className="mc-buy-pp-fab-zap">
          <Zap className="h-3 w-3" />
        </span>
      </button>

      {open && (
        <>
          <div
            className="mc-buy-pp-sheet-backdrop lg:hidden"
            onClick={() => setOpen(false)}
          />
          <div className="mc-buy-pp-sheet lg:hidden" role="dialog" aria-label="Buy PP">
            <BuyPPFlyout variant="sheet" onClose={() => setOpen(false)} />
          </div>
        </>
      )}
    </>
  );
}
