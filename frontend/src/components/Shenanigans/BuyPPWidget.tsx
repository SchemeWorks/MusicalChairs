/**
 * Desktop sticky sidebar widget. Slot into the mc-shenanigans-sidebar column;
 * the sidebar is already position:sticky (top:160px) at lg+ breakpoints, so
 * the widget stays in view as the user scrolls through shenanigan cards.
 */

import BuyPPFlyout from './BuyPPFlyout';

export default function BuyPPWidget() {
  return (
    <div className="mc-buy-pp-widget">
      <BuyPPFlyout variant="widget" />
    </div>
  );
}
