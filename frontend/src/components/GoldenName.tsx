import React from 'react';
import { Principal } from '@dfinity/principal';
import { useDisplayName, useIsGolden } from './trollbox/useDisplayName';

interface GoldenNameProps {
  /** Resolved display name (caller decides how to get it). */
  name: string;
  /** Whether this player has an active Whitelisted (goldenName) spell. */
  isGolden: boolean;
  /**
   * Optional className for the wrapping `<span>`. Use to set size/weight
   * (e.g. `text-xs font-bold`). Color and decoration are handled internally:
   * non-golden falls back to `mc-text-primary`; golden uses `.mc-name-vip`.
   */
  className?: string;
}

/**
 * Renders a player display name with the Whitelisted VIP treatment when
 * `isGolden` is true: a ◆ prefix glyph plus the animated gold gradient sweep
 * + pulsing glow (see `.mc-name-vip` in index.css). Falls back to plain
 * `mc-text-primary` when not golden.
 *
 * The component is pure presentation — callers resolve `name` and `isGolden`
 * themselves. Use `<GoldenNameByPrincipal>` for the common case where you
 * already have a principal and want both lookups in one call.
 */
export default function GoldenName({ name, isGolden, className }: GoldenNameProps) {
  if (isGolden) {
    // The ◆ prefix lives in its own span with an explicit gold color, because
    // `.mc-name-vip` uses `color: transparent` + background-clip:text on the
    // name span — inheriting that would render the glyph invisible.
    return (
      <span className={`inline-flex items-baseline gap-1 ${className ?? ''}`.trim()}>
        <span aria-hidden="true" className="mc-text-gold">◆</span>
        <span className="mc-name-vip">{name}</span>
      </span>
    );
  }
  return (
    <span className={`mc-text-primary ${className ?? ''}`.trim()}>{name}</span>
  );
}

/**
 * Convenience wrapper that resolves both display name and golden status from
 * a principal. Use this in lists where each row has a principal and you want
 * the standard VIP treatment.
 */
export function GoldenNameByPrincipal({
  principal,
  className,
}: {
  principal: Principal | null;
  className?: string;
}) {
  const name = useDisplayName(principal);
  const isGolden = useIsGolden(principal);
  return <GoldenName name={name || '…'} isGolden={isGolden} className={className} />;
}
