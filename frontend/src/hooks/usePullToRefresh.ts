import { useRef, useCallback, useState, useEffect } from 'react';

interface UsePullToRefreshOptions {
  onRefresh: () => Promise<void>;
  threshold?: number;   // px to pull before triggering (default: 80)
  disabled?: boolean;
}

/**
 * Pull-to-refresh hook for mobile. Attaches to a scrollable container.
 * Returns a ref callback + a spinner indicator element's state.
 *
 * Only activates on touch devices when the container is scrolled to the top.
 */
export function usePullToRefresh({ onRefresh, threshold = 80, disabled = false }: UsePullToRefreshOptions) {
  const [pulling, setPulling] = useState(false);
  const [pullDistance, setPullDistance] = useState(0);
  const [refreshing, setRefreshing] = useState(false);
  const startY = useRef(0);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const isTracking = useRef(false);

  const handleTouchStart = useCallback((e: TouchEvent) => {
    if (disabled || refreshing) return;
    const container = containerRef.current;
    // Only engage if scrolled to top
    if (container && container.scrollTop <= 0) {
      startY.current = e.touches[0].clientY;
      isTracking.current = true;
    }
  }, [disabled, refreshing]);

  const handleTouchMove = useCallback((e: TouchEvent) => {
    if (!isTracking.current || disabled || refreshing) return;
    const deltaY = e.touches[0].clientY - startY.current;
    if (deltaY > 0) {
      setPulling(true);
      // Dampen the pull (50% resistance)
      setPullDistance(Math.min(deltaY * 0.5, threshold * 1.5));
    } else {
      setPulling(false);
      setPullDistance(0);
    }
  }, [disabled, refreshing, threshold]);

  const handleTouchEnd = useCallback(async () => {
    if (!isTracking.current) return;
    isTracking.current = false;

    if (pullDistance >= threshold && !refreshing) {
      setRefreshing(true);
      setPullDistance(threshold * 0.5); // Hold at a partial position while refreshing
      try {
        await onRefresh();
      } finally {
        setRefreshing(false);
        setPulling(false);
        setPullDistance(0);
      }
    } else {
      setPulling(false);
      setPullDistance(0);
    }
  }, [pullDistance, threshold, refreshing, onRefresh]);

  // Attach listeners to the container
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    container.addEventListener('touchstart', handleTouchStart, { passive: true });
    container.addEventListener('touchmove', handleTouchMove, { passive: true });
    container.addEventListener('touchend', handleTouchEnd);

    return () => {
      container.removeEventListener('touchstart', handleTouchStart);
      container.removeEventListener('touchmove', handleTouchMove);
      container.removeEventListener('touchend', handleTouchEnd);
    };
  }, [handleTouchStart, handleTouchMove, handleTouchEnd]);

  return {
    containerRef,
    pulling: pulling || refreshing,
    pullDistance,
    refreshing,
    isTriggered: pullDistance >= threshold,
  };
}
