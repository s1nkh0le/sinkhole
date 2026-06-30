import { useState, useEffect, useRef, useCallback, useLayoutEffect } from 'react';
import { useSettings } from '../stores/SettingsContext';

interface UseCursorPollingOptions<T> {
  /** Fetch function that takes a cursor (next_since) and returns items + next cursor */
  fetcher: (since: number) => Promise<{ items: T[]; nextSince: number }>;
  /** Override poll interval */
  intervalMs?: number;
  enabled?: boolean;
  /** Maximum number of items to keep in memory */
  maxItems?: number;
}

interface UseCursorPollingResult<T> {
  items: T[];
  error: Error | null;
  isLoading: boolean;
  lastUpdated: Date | null;
  refresh: () => Promise<void>;
  clear: () => void;
}

/**
 * Cursor-based counterpart to {@link usePolling} for append-only streams (ECAM
 * events, traffic records). Each poll asks for items since the last cursor and
 * appends them, keeping at most `maxItems` (oldest dropped). Items are
 * deduplicated so overlapping cursor boundaries and StrictMode double-fetches
 * don't produce duplicates; `refresh` restarts from the beginning, `clear`
 * empties without refetching.
 */
export function useCursorPolling<T>(options: UseCursorPollingOptions<T>): UseCursorPollingResult<T> {
  const { fetcher, intervalMs, enabled = true, maxItems = 5000 } = options;
  const { settings } = useSettings();
  const interval = intervalMs ?? settings.pollIntervalMs;

  const [items, setItems] = useState<T[]>([]);
  const [error, setError] = useState<Error | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const cursorRef = useRef<number>(0);
  const fetcherRef = useRef(fetcher);
  useLayoutEffect(() => { fetcherRef.current = fetcher; }, [fetcher]);
  const mountedRef = useRef(true);
  const isFetchingRef = useRef(false);

  const doFetch = useCallback(async () => {
    if (isFetchingRef.current) return;
    isFetchingRef.current = true;
    try {
      const result = await fetcherRef.current(cursorRef.current);
      if (mountedRef.current && result.items.length > 0) {
        setItems(prev => {
          // Deduplicate by stringified representation to prevent React StrictMode double-append
          // or cursor boundary overlap issues.
          const prevStrs = new Set(prev.map(p => JSON.stringify(p)));
          const newItems = result.items.filter(item => !prevStrs.has(JSON.stringify(item)));
          
          if (newItems.length === 0) return prev;
          
          const combined = [...prev, ...newItems];
          return combined.length > maxItems ? combined.slice(-maxItems) : combined;
        });
        cursorRef.current = result.nextSince;
        setLastUpdated(new Date());
      }
      if (mountedRef.current) {
        setError(null);
      }
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err : new Error(String(err)));
      }
    } finally {
      isFetchingRef.current = false;
      if (mountedRef.current) {
        setIsLoading(false);
      }
    }
  }, [maxItems]);

  useEffect(() => {
    mountedRef.current = true;
    if (enabled) {
      setTimeout(() => { if (mountedRef.current) setIsLoading(true); }, 0);
      doFetch();
    }
    return () => { mountedRef.current = false; };
  }, [enabled, doFetch]);

  useEffect(() => {
    if (!enabled || interval <= 0) return;

    let timerId: ReturnType<typeof setTimeout> | null = null;

    const scheduleNext = () => {
      timerId = setTimeout(async () => {
        if (document.visibilityState === 'visible') {
          await doFetch();
        }
        scheduleNext();
      }, interval);
    };

    scheduleNext();

    const handleVisibility = () => {
      if (document.visibilityState === 'visible') {
        doFetch();
      }
    };
    document.addEventListener('visibilitychange', handleVisibility);

    return () => {
      if (timerId) clearTimeout(timerId);
      document.removeEventListener('visibilitychange', handleVisibility);
    };
  }, [enabled, interval, doFetch]);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    cursorRef.current = 0;
    setItems([]);
    await doFetch();
  }, [doFetch]);

  const clear = useCallback(() => {
    setItems([]);
    cursorRef.current = 0;
  }, []);

  return { items, error, isLoading, lastUpdated, refresh, clear };
}
