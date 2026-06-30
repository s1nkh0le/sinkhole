/**
 * Nanosecond timestamp utilities.
 * All API timestamps are nanoseconds since Unix epoch (i64).
 */

export function nsToDate(ns: number): Date {
  return new Date(ns / 1_000_000);
}

export function nsToMs(ns: number): number {
  return ns / 1_000_000;
}

export function msToNs(ms: number): number {
  return ms * 1_000_000;
}

export function nowNs(): number {
  return Date.now() * 1_000_000;
}

export function formatAbsolute(ns: number): string {
  if (!ns || ns === 0) return '—';
  const d = nsToDate(ns);
  return d.toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    fractionalSecondDigits: 3,
  });
}

export function formatRelative(ns: number): string {
  if (!ns || ns === 0) return 'never';
  const diffMs = Date.now() - nsToMs(ns);
  if (diffMs < 0) return 'just now';
  
  const seconds = Math.floor(diffMs / 1000);
  if (seconds < 5) return 'just now';
  if (seconds < 60) return `${seconds}s ago`;
  
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

export function formatDuration(seconds: number): string {
  if (seconds < 1) return `${Math.round(seconds * 1000)}ms`;
  if (seconds < 60) return `${seconds.toFixed(1)}s`;
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m}m ${s}s`;
}
