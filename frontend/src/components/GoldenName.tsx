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
    // Split the caller's className so truncation classes (`truncate`,
    // `max-w-*`, `overflow-*`, etc.) reach the inner text-bearing span,
    // where `text-overflow: ellipsis` actually applies. Layout/sizing
    // classes stay on the wrapper. Without this, callers passing
    // `truncate max-w-[80px]` would see long names hard-clip or push the
    // layout, because flex containers don't apply text-overflow to
    // their children.
    const { wrapperClass, innerTextClass } = splitTruncationClasses(className);

    // The ◆ prefix lives in its own span with an explicit gold color,
    // because `.mc-name-vip` uses `color: transparent` + background-clip:text
    // on the name span — inheriting that would render the glyph invisible.
    return (
      <span className={`inline-flex items-baseline gap-1 min-w-0 ${wrapperClass}`.trim()}>
        <span aria-hidden="true" className="mc-text-gold flex-shrink-0">◆</span>
        <span className={`mc-name-vip ${innerTextClass}`.trim()}>{name}</span>
      </span>
    );
  }
  return (
    <span className={`mc-text-primary ${className ?? ''}`.trim()}>{name}</span>
  );
}

// Tailwind classes that need to land on the text-bearing span (where
// text-overflow can act) rather than the flex wrapper. Matches by exact
// token or by prefix.
const TEXT_TRUNCATION_TOKENS = ['truncate', 'text-ellipsis', 'text-clip'];
const TEXT_TRUNCATION_PREFIXES = ['overflow-', 'max-w-', 'whitespace-'];

function splitTruncationClasses(className: string | undefined): {
  wrapperClass: string;
  innerTextClass: string;
} {
  if (!className) return { wrapperClass: '', innerTextClass: '' };
  const tokens = className.split(/\s+/).filter(Boolean);
  const inner: string[] = [];
  const wrapper: string[] = [];
  for (const t of tokens) {
    const matches =
      TEXT_TRUNCATION_TOKENS.includes(t) ||
      TEXT_TRUNCATION_PREFIXES.some(p => t.startsWith(p));
    (matches ? inner : wrapper).push(t);
  }
  return { wrapperClass: wrapper.join(' '), innerTextClass: inner.join(' ') };
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
