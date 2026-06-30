import { formatRelative, formatAbsolute } from '../lib/time';

interface TimeSinceProps {
  ns: number;
  showAbsolute?: boolean;
}

/**
 * Renders a nanosecond epoch timestamp as a relative "Xm ago" string (or the
 * absolute time when `showAbsolute`), with the full timestamp in the tooltip.
 */
export function TimeSince({ ns, showAbsolute = false }: TimeSinceProps) {
  if (!ns) return <span className="time-since">—</span>;
  const relative = formatRelative(ns);
  const absolute = formatAbsolute(ns);

  return (
    <time
      className="time-since"
      dateTime={new Date(ns / 1_000_000).toISOString()}
      title={absolute}
      style={{ color: 'var(--color-text-secondary)', fontSize: 'var(--text-sm)' }}
    >
      {showAbsolute ? absolute : relative}
    </time>
  );
}
