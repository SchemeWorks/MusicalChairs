import { useState, useEffect, useRef } from 'react';

/**
 * Animates a number from 0 to `target` over `duration` ms.
 * Returns the current animated value.
 * Re-animates whenever `target` changes significantly (>1% diff).
 */
export function useCountUp(target: number, duration: number = 1000): number {
  const [value, setValue] = useState(0);
  const prevTarget = useRef(0);
  const frameRef = useRef<number>(0);

  useEffect(() => {
    // Skip animation for trivial changes (< 1% or target is 0)
    if (target === 0) {
      setValue(0);
      prevTarget.current = 0;
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
  }, [target, duration]);

  return value;
}
