import { useState, useEffect, useRef } from 'react';
import './SyncProgress.css';

interface SyncBarProps {
  label: string;
  intervalMs: number;
  /** Incremented each time a successful fetch completes — resets the bar */
  tick: number;
  /** True if the last fetch errored */
  hasError: boolean;
  title?: string;
}

function SyncBar({ label, intervalMs, tick, hasError, title }: SyncBarProps) {
  const [progress, setProgress] = useState(0);
  const [initialTime] = useState(() => Date.now());
  const startRef = useRef(initialTime);
  const rafRef = useRef<number>(0);
  const prevTickRef = useRef(tick);

  useEffect(() => {
    // Only reset when tick actually changes (meaning fresh data arrived)
    if (tick !== prevTickRef.current) {
      prevTickRef.current = tick;
      startRef.current = Date.now();
      setProgress(0);
    }
  }, [tick]);

  useEffect(() => {
    const animate = () => {
      const elapsed = Date.now() - startRef.current;
      const pct = Math.min(100, (elapsed / intervalMs) * 100);
      setProgress(pct);
      // Stop animating once we hit 100% — wait for next tick to reset
      if (pct < 100) {
        rafRef.current = requestAnimationFrame(animate);
      }
    };
    rafRef.current = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(rafRef.current);
  }, [intervalMs, tick]);

  return (
    <div className="sync-bar" title={title}>
      <span className="sync-bar__label">{label}</span>
      <div className="sync-bar__track">
        <div
          className={`sync-bar__fill ${hasError ? 'sync-bar__fill--error' : ''}`}
          style={{ width: `${progress}%` }}
        />
      </div>
    </div>
  );
}

export interface SyncChannel {
  label: string;
  intervalMs: number;
  tick: number;
  hasError: boolean;
  title?: string;
}

/**
 * Row of countdown bars, one per polling channel, that fill over each channel's
 * interval and reset when fresh data arrives (its `tick` changes), turning red
 * on error. A live indicator of the background sync cadence.
 */
export function SyncProgress({ channels }: { channels: SyncChannel[] }) {
  return (
    <div className="sync-progress">
      {channels.map(ch => (
        <SyncBar key={ch.label} {...ch} />
      ))}
    </div>
  );
}
