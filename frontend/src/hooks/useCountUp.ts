import { useState, useEffect, useRef, useCallback } from 'react';

/**
 * Animates a number from 0 to `target` over `duration` ms.
 * Returns the current animated value.
 * Re-animates whenever `target` changes significantly (>1% diff).
 *
 * Optional `resetToken`: when this value changes, the animation restarts from 0.
 * Useful for tab-switch re-animation or visibility-triggered replays.
 *
 * Optional `options.autoResetOnVisible`: when true, uses IntersectionObserver
 * to detect when the element scrolls into view and re-triggers the animation.
 * Returns a `ref` callback to attach to the animated element.
 */
export function useCountUp(
  target: number,
  duration?: number,
  resetToken?: number,
  options?: { autoResetOnVisible?: boolean }
): number;
export function useCountUp(
  target: number,
  duration: number,
  resetToken: number | undefined,
  options: { autoResetOnVisible: true }
): { value: number; ref: (node: HTMLElement | null) => void };
export function useCountUp(
  target: number,
  duration: number = 1000,
  resetToken?: number,
  options?: { autoResetOnVisible?: boolean }
): number | { value: number; ref: (node: HTMLElement | null) => void } {
  const [value, setValue] = useState(0);
  const prevTarget = useRef(0);
  const frameRef = useRef<number>(0);
  const [visibilityToken, setVisibilityToken] = useState(0);
  const observerRef = useRef<IntersectionObserver | null>(null);
  const nodeRef = useRef<HTMLElement | null>(null);

  // Combined token: changes when resetToken or visibilityToken changes
  const effectiveToken = (resetToken ?? 0) + visibilityToken;

  // Reset from 0 when token changes
  useEffect(() => {
    if (effectiveToken > 0) {
      prevTarget.current = 0;
      setValue(0);
    }
  }, [effectiveToken]);

  useEffect(() => {
    // Skip animation for trivial changes (< 1% or target is 0)
    if (target === 0) {
      setValue(0);
      prevTarget.current = 0;
      return;
    }

    // Respect prefers-reduced-motion: skip animation entirely
    const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (prefersReducedMotion) {
      setValue(target);
      prevTarget.current = target;
      return;
    }

    const diff = Math.abs(target - prevTarget.current);
    if (diff / Math.max(Math.abs(target), 1) < 0.01) {
      setValue(target);
      prevTarget.current = target;
      return;
    }

    const startValue = prevTarget.current;
    const startTime = performance.now();

    const animate = (now: number) => {
      const elapsed = now - startTime;
      const progress = Math.min(elapsed / duration, 1);
      // Ease-out cubic
      const eased = 1 - Math.pow(1 - progress, 3);
      const current = startValue + (target - startValue) * eased;
      setValue(current);

      if (progress < 1) {
        frameRef.current = requestAnimationFrame(animate);
      } else {
        setValue(target);
        prevTarget.current = target;
      }
    };

    frameRef.current = requestAnimationFrame(animate);

    return () => {
      if (frameRef.current) cancelAnimationFrame(frameRef.current);
    };
  }, [target, duration, effectiveToken]);

  // IntersectionObserver-based auto-reset
  const refCallback = useCallback((node: HTMLElement | null) => {
    // Cleanup previous observer
    if (observerRef.current) {
      observerRef.current.disconnect();
      observerRef.current = null;
    }

    if (node && options?.autoResetOnVisible) {
      nodeRef.current = node;
      observerRef.current = new IntersectionObserver(
        (entries) => {
          entries.forEach((entry) => {
            if (entry.isIntersecting) {
              setVisibilityToken((t) => t + 1);
            }
          });
        },
        { threshold: 0.3 }
      );
      observerRef.current.observe(node);
    }
  }, [options?.autoResetOnVisible]);

  // Cleanup observer on unmount
  useEffect(() => {
    return () => {
      if (observerRef.current) {
        observerRef.current.disconnect();
      }
    };
  }, []);

  if (options?.autoResetOnVisible) {
    return { value, ref: refCallback };
  }

  return value;
}
