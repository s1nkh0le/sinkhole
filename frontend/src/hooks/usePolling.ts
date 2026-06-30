import { useState, useEffect, useRef, useCallback, useLayoutEffect } from 'react';
import { useSettings } from '../stores/SettingsContext';

interface UsePollingOptions<T> {
  fetcher: (opts?: { signal?: AbortSignal }) => Promise<T>;
  intervalMs?: number; // overrides settings if provided
  enabled?: boolean;
  paused?: boolean; // skip scheduled fetches (e.g. while editing forms)
}

interface UsePollingResult<T> {
  data: T | null;
  error: unknown | null;
  isLoading: boolean;
  lastUpdated: Date | null;
  refresh: () => Promise<void>;
}

/**
 * Repeatedly fetches a single snapshot on an interval and exposes the latest
 * value. Each fetch aborts the previous one, state updates are skipped after
 * unmount, polling pauses while the tab is hidden, and `paused` suspends
 * scheduled fetches (e.g. while a form is open) without tearing the hook down.
 * The interval defaults to the user's poll setting unless `intervalMs` is given;
 * `intervalMs <= 0` disables polling but still does the initial fetch.
 */
export function usePolling<T>(options: UsePollingOptions<T>): UsePollingResult<T> {
  const { fetcher, intervalMs, enabled = true, paused = false } = options;
  const { settings } = useSettings();
  const interval = intervalMs ?? settings.pollIntervalMs;

  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<unknown | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const fetcherRef = useRef(fetcher);
  useLayoutEffect(() => { fetcherRef.current = fetcher; }, [fetcher]);
  const mountedRef = useRef(true);
  const pausedRef = useRef(paused);
  useLayoutEffect(() => { pausedRef.current = paused; }, [paused]);

  const abortControllerRef = useRef<AbortController | null>(null);

  const doFetch = useCallback(async () => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    const controller = new AbortController();
    abortControllerRef.current = controller;

    try {
      const result = await fetcherRef.current({ signal: controller.signal });
      if (mountedRef.current) {
        setData(result);
        setError(null);
        setLastUpdated(new Date());
      }
    } catch (err: unknown) {
      if ((err as Error)?.name === 'AbortError') return;
      if (mountedRef.current) {
        setError(err);
      }
    } finally {
      if (mountedRef.current && abortControllerRef.current === controller) {
        setIsLoading(false);
      }
    }
  }, []);

  // Initial fetch
  useEffect(() => {
    mountedRef.current = true;
    if (enabled) {
      setTimeout(() => { if (mountedRef.current) setIsLoading(true); }, 0);
      doFetch();
    }
    return () => { mountedRef.current = false; };
  }, [enabled, doFetch]);

  // Polling with visibility awareness
  useEffect(() => {
    if (!enabled || interval <= 0) return;

    let timerId: ReturnType<typeof setTimeout> | null = null;

    const scheduleNext = () => {
      timerId = setTimeout(async () => {
        if (document.visibilityState === 'visible' && !pausedRef.current) {
          await doFetch();
        }
        scheduleNext();
      }, interval);
    };

    scheduleNext();

    // Also fetch when tab becomes visible again
    const handleVisibility = () => {
      if (document.visibilityState === 'visible' && !pausedRef.current) {
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
    await doFetch();
  }, [doFetch]);

  return { data, error, isLoading, lastUpdated, refresh };
}
