import { useState } from 'react';
import { Trash2, FileX } from 'lucide-react';
import { Badge } from '../Badge';
import { CopyButton } from '../CopyButton';
import { RuleDetailCard } from '../RuleDetailCard';
import { EmptyState } from '../EmptyState';
import { formatAbsolute } from '../../lib/time';
import { decodeBase64, bytesToHexDump, bytesToText, formatByteSize } from '../../lib/bytes';
import { statusClass, HTTP_STATUS_CODES } from './trafficHttp';
import type { EvaluationRecord } from '../../api/types';

/**
 * Displays the full metadata and diagnostic details for a specific EvaluationRecord.
 * Shows matched rules, HTTP breakdown, and raw payload data in a tabbed interface.
 *
 * @param props.record The specific traffic record to inspect
 * @param props.rawView Global state tracking whether raw views should default to 'hex' or 'text'
 * @param props.onToggleRawView Callback to toggle the global raw view state
 * @param props.knownRuleIds Set of currently active rule IDs (used to detect if a rule that triggered this record was since deleted)
 */
export function RecordDetail({
  record,
  rawView,
  onToggleRawView,
  knownRuleIds,
}: {
  record: EvaluationRecord;
  rawView: 'hex' | 'text';
  onToggleRawView: (v: 'hex' | 'text') => void;
  knownRuleIds: Set<string>;
}) {
  const [tab, setTab] = useState<'overview' | 'http' | 'rules' | 'raw'>('overview');

  const isRemoved = (id: string) => knownRuleIds.size > 0 && !knownRuleIds.has(id);

  return (
    <div className="record-detail">
      <div className="record-detail__tabs flex gap-1">
        {(['overview', 'http', 'rules', 'raw'] as const).map(t => (
          <button key={t} className={`record-detail__tab ${tab === t ? 'record-detail__tab--active' : ''}`} onClick={() => setTab(t)}>
            {t === 'overview' ? 'Overview' : t === 'http' ? 'HTTP' : t === 'rules' ? 'Rule Details' : 'Raw Data'}
          </button>
        ))}
      </div>

      {tab === 'overview' && (
        <div className="record-detail__grid">
          <div className="detail-field flex flex-col gap-1"><span className="detail-label">Connection</span><span className="detail-value mono" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>#{record.conn_id} <CopyButton text={record.conn_id.toString()} /></span></div>
          <div className="detail-field flex flex-col gap-1"><span className="detail-label">Message Seq</span><span className="detail-value" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>#{record.message_seq} <CopyButton text={record.message_seq.toString()} /></span></div>
          <div className="detail-field flex flex-col gap-1"><span className="detail-label">Timestamp</span><span className="detail-value" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>{formatAbsolute(record.timestamp_ns)} <CopyButton text={record.timestamp_ns.toString()} /></span></div>
          <div className="detail-field flex flex-col gap-1"><span className="detail-label">Direction</span><Badge variant="info">{record.inbound ? 'Inbound' : 'Outbound'}</Badge></div>
          <div className="detail-field flex flex-col gap-1"><span className="detail-label">Decision</span><Badge variant={record.decision === 'drop' ? 'error' : 'success'}>{record.decision}</Badge></div>
          <div className="detail-field flex flex-col gap-1"><span className="detail-label">Source</span><span className="detail-value mono" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>{record.src_ip}:{record.src_port} <CopyButton text={`${record.src_ip}:${record.src_port}`} /></span></div>
          <div className="detail-field flex flex-col gap-1"><span className="detail-label">Destination</span><span className="detail-value mono" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>{record.dst_ip}:{record.dst_port} <CopyButton text={`${record.dst_ip}:${record.dst_port}`} /></span></div>
          <div className="detail-field flex flex-col gap-1"><span className="detail-label">Public Port</span><span className="detail-value mono" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>{record.public_port} <CopyButton text={record.public_port.toString()} /></span></div>
          <div className="detail-field flex flex-col gap-1"><span className="detail-label">Upstream Port</span><span className="detail-value mono" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>{record.upstream_port} <CopyButton text={record.upstream_port.toString()} /></span></div>
          {record.markers.length > 0 && (
            <div className="detail-field detail-field--wide flex flex-col gap-1">
              <span className="detail-label">Markers</span>
              <div className="detail-badges flex flex-wrap gap-1">
                {record.markers.map(m => (
                  <span key={m} className="detail-marker-badge inline-flex items-center gap-1">
                    <Badge variant="accent">{m}</Badge>
                    {isRemoved(m) && <span className="removed-indicator" title="Rule has been removed" style={{ display: 'inline-flex', alignItems: 'center', gap: 'var(--space-1)', marginLeft: 'var(--space-1)', color: 'var(--color-text-muted)' }}><Trash2 size={12} /> (removed)</span>}
                  </span>
                ))}
              </div>
            </div>
          )}
          {record.drop_reasons.length > 0 && (
            <div className="detail-field detail-field--wide flex flex-col gap-1">
              <span className="detail-label">Drop Reasons</span>
              <div className="drop-reasons flex flex-col gap-2">
                {record.drop_reasons.map((dr, i) => (
                  <div key={i} className="drop-reason flex items-center gap-2">
                    <Badge variant="error">{dr.kind}</Badge>
                    {'filter_ids' in dr && <span className="detail-value mono">{dr.filter_ids.join(', ')}</span>}
                    {'missing_ids' in dr && <span className="detail-value mono">missing: {dr.missing_ids.join(', ')} ({dr.hit}/{dr.required})</span>}
                    {'hit_ids' in dr && <span className="detail-value mono">hits: {dr.hit_ids.join(', ')} (/{dr.group_size})</span>}
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {tab === 'http' && (
        <div>
          {record.http ? (
            <div className="record-detail__grid">
              {record.http.method && <div className="detail-field flex flex-col gap-1"><span className="detail-label">Method</span><span className="mono font-bold" style={{ fontSize: 'var(--text-sm)', color: 'var(--color-text-secondary)' }}>{record.http.method}</span></div>}
              {record.http.path && <div className="detail-field flex flex-col gap-1"><span className="detail-label">Path</span><span className="detail-value mono" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>{record.http.path} <CopyButton text={record.http.path} /></span></div>}
              {record.http.status != null && (
                <div className="detail-field flex flex-col gap-1">
                  <span className="detail-label">Status</span>
                  <span
                    className={`message-viewer__status message-viewer__status--${statusClass(record.http.status)}`}
                    title={HTTP_STATUS_CODES[record.http.status] || 'Unknown status code'}
                    style={{ textDecoration: 'underline', textDecorationColor: 'currentColor', textUnderlineOffset: '2px', cursor: 'help' }}
                  >
                    {record.http.status} {record.http.reason ?? ''}
                  </span>
                </div>
              )}
              <div className="detail-field flex flex-col gap-1"><span className="detail-label">Body Length</span><span className="detail-value">{formatByteSize(record.http.body_len)}</span></div>
              {record.http.truncated && <div className="detail-field flex flex-col gap-1"><Badge variant="warning">Truncated</Badge></div>}
              {record.http.headers.length > 0 && (
                <div className="detail-field detail-field--wide flex flex-col gap-1">
                  <span className="detail-label">Headers</span>
                  <table className="headers-table">
                    <tbody>
                      {record.http.headers.map(([k, v], i) => (
                        <tr key={i}><td className="headers-table__key">{k}</td><td className="headers-table__val">{v}</td></tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
              {/* HTTP body content */}
              <div className="detail-field detail-field--wide flex flex-col gap-1">
                <span className="detail-label">Body</span>
                {record.raw_b64 ? (
                  <pre className="raw-viewer">{bytesToText(decodeBase64(record.raw_b64))}</pre>
                ) : (
                  <span className="detail-value" style={{ color: 'var(--color-text-tertiary)' }}>No body data</span>
                )}
              </div>
            </div>
          ) : (
            <EmptyState icon={<FileX size={32} />} title="No HTTP data" description="This message was not parsed as HTTP." />
          )}
        </div>
      )}

      {tab === 'rules' && (() => {
        if (record.details.length === 0) {
          return <EmptyState icon={<FileX size={32} />} title="No rule details" description="Rule evaluation details are only available when the port's debug flag is enabled." />;
        }

        const markers = record.details.filter(d => d.kind === 'marker');
        const filters = record.details.filter(d => d.kind === 'filter');

        const matchedMarkers = markers.filter(m => m.match_output.status === 'satisfied');
        const unmatchedMarkers = markers.filter(m => m.match_output.status !== 'satisfied');
        const matchedFilters = filters.filter(f => f.match_output.status === 'satisfied');
        const skippedFilters = filters.filter(f => f.match_output.status !== 'satisfied');

        const renderGroup = (title: string, list: typeof record.details) => {
          if (list.length === 0) return null;
          return (
            <div className="details-group" style={{ marginBottom: 'var(--space-6)' }}>
              <h4 style={{ margin: '0 0 var(--space-3) 0', color: 'var(--color-text-secondary)', fontSize: 'var(--text-xs)', textTransform: 'uppercase', letterSpacing: '0.05em' }}>
                {title} <span style={{ opacity: 0.5, marginLeft: 'var(--space-2)' }}>({list.length})</span>
              </h4>
              <div className="details-list flex flex-col gap-2">
                {list.map((detail, i) => (
                  <RuleDetailCard key={`${detail.id}-${i}`} detail={detail} isRemoved={isRemoved(detail.id)} />
                ))}
              </div>
            </div>
          );
        };

        return (
          <div>
            {renderGroup('Matched Markers', matchedMarkers)}
            {renderGroup('Matched Filters', matchedFilters)}
            {renderGroup('Unmatched/Skipped Filters', skippedFilters)}
            {renderGroup('Unmatched Markers', unmatchedMarkers)}
          </div>
        );
      })()}

      {tab === 'raw' && (
        <div>
          <div className="raw-controls flex gap-1">
            <button className={`btn btn--sm ${rawView === 'hex' ? 'btn--primary' : 'btn--ghost'}`} onClick={() => onToggleRawView('hex')}>Hex</button>
            <button className={`btn btn--sm ${rawView === 'text' ? 'btn--primary' : 'btn--ghost'}`} onClick={() => onToggleRawView('text')}>Text</button>
          </div>
          {record.raw_b64 ? (
            <pre className="raw-viewer">
              {rawView === 'hex'
                ? bytesToHexDump(decodeBase64(record.raw_b64))
                : bytesToText(decodeBase64(record.raw_b64))
              }
            </pre>
          ) : (
            <EmptyState icon={<FileX size={32} />} title="No raw data" />
          )}
        </div>
      )}
    </div>
  );
}
