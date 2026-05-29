/**
 * Desktop sticky sidebar widget for SIWS users — drops `BuySOLFlyout` into
 * the same `mc-shenanigans-sidebar` slot that `BuyPPWidget` uses. `App.tsx`
 * auto-selects between the two by `walletType` (SIWS → BuySOL, else BuyPP).
 */

import BuySOLFlyout from './BuySOLFlyout';

export default function BuySOLWidget() {
  return (
    <div className="mc-buy-pp-widget">
      <BuySOLFlyout variant="widget" />
    </div>
  );
}
