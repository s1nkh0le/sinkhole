import { useState } from 'react';
import { useCursorPolling } from '../hooks/useCursorPolling';
import { getApi } from '../api/client';
import { Badge } from '../components/Badge';
import { TimeSince } from '../components/TimeSince';
import { LoadingSpinner } from '../components/LoadingSpinner';
import { EmptyState } from '../components/EmptyState';
import { ErrorDisplay } from '../components/ErrorDisplay';
import { CheckCircle } from 'lucide-react';
import { formatAbsolute } from '../lib/time';
import type { EcamEvent } from '../api/types';
import './EcamPage.css';

function formatOrigin(origin: EcamEvent['origin']): string {
  if (typeof origin === 'string') return origin;
  if ('Network' in origin) {
    const net = origin.Network;
    if (typeof net === 'string') return `Network:${net}`;
    if ('ClientToFilter' in net) return `Client->Filter:${net.ClientToFilter}`;
    if ('ServerToFilter' in net) return `Server->Filter:${net.ServerToFilter}`;
  }
  if ('Port' in origin) return `Port:${origin.Port}`;
  if ('Matcher' in origin) return `Matcher:${origin.Matcher.port}/${origin.Matcher.filter_id}`;
  return JSON.stringify(origin);
}

/**
 * Live view of the centralized ECAM event log (the system's operational event
 * stream) across the fleet. Cursor-polls `/ecam` for new events, with a
 * severity filter and expandable rows for the structured detail.
 */
export function EcamPage() {
  const [severityFilter, setSeverityFilter] = useState<'' | 'warn' | 'error'>('');
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null);

  const { items, error, isLoading, lastUpdated, refresh } = useCursorPolling<EcamEvent>({
    fetcher: async (since) => {
      const api = getApi();
      const { data, error } = await api.GET('/ecam', {
        params: { query: { since } as Record<string, unknown> },
      });
      if (error) throw error;
      return {
        items: data!.data.records,
        nextSince: data!.data.next_since,
      };
    },
  });

  const filtered = severityFilter
    ? items.filter(e => e.severity === severityFilter)
    : items;

  // Show newest first
  const sorted = [...filtered].sort((a, b) => b.ts_ns - a.ts_ns);

  return (
    <div className="ecam-page">
      <div className="ecam-page__header">
        <div>
          <h1 className="ecam-page__title">ECAM</h1>
          <p className="ecam-page__subtitle">System alerts and warnings</p>
        </div>
        <div className="ecam-page__controls">
          <select value={severityFilter} onChange={e => setSeverityFilter(e.target.value as '' | 'warn' | 'error')}>
            <option value="">All severity</option>
            <option value="warn">Warnings</option>
            <option value="error">Errors</option>
          </select>
          <button className="btn btn--ghost btn--sm" onClick={refresh}>⟳</button>
          {lastUpdated && (
            <span className="ecam-page__updated">Updated {lastUpdated.toLocaleTimeString()}</span>
          )}
        </div>
      </div>

      {isLoading && items.length === 0 && <div className="page-center"><LoadingSpinner label="Loading events…" /></div>}
      {error && items.length === 0 && <ErrorDisplay error={error} onRetry={refresh} />}

      {!isLoading && sorted.length === 0 && (
        <EmptyState icon={<CheckCircle size={32} />} title="No alerts" description="No ECAM events to display." />
      )}

      <div className="ecam-feed">
        {sorted.map((ev, idx) => (
          <div key={idx} className={`ecam-event ecam-event--${ev.severity}`}>
            <div className="ecam-event__header">
              <Badge variant={ev.severity === 'error' ? 'error' : 'warning'}>{ev.severity}</Badge>
              <span className="ecam-event__origin">{formatOrigin(ev.origin)}</span>
              <span className="ecam-event__time" title={formatAbsolute(ev.ts_ns)}>
                <TimeSince ns={ev.ts_ns} />
              </span>
            </div>
            <p className="ecam-event__msg">{ev.msg}</p>
            {!!ev.data && (
              <button
                className="ecam-event__btn"
                onClick={() => setExpandedIdx(expandedIdx === idx ? null : idx)}
              >
                {expandedIdx === idx ? 'Hide details' : 'Show details'}
              </button>
            )}
            {expandedIdx === idx && !!ev.data && (
              <pre className="ecam-event__data">{JSON.stringify(ev.data, null, 2)}</pre>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
