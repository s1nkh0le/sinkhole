import { useState } from 'react';
import { Badge } from '../Badge';
import { TimeSince } from '../TimeSince';
import { CopyButton } from '../CopyButton';
import { decodeBase64, bytesToHexDump, bytesToText, formatByteSize } from '../../lib/bytes';
import { statusClass, formatProtocol, HTTP_STATUS_CODES } from './trafficHttp';
import type { EvaluationRecord, RecordGroup } from '../../api/types';

/**
 * Renders an individual message within a connection group.
 * Maintains local state for viewing the message payload as text vs hexadecimal.
 *
 * @param props.rec The EvaluationRecord to display
 * @param props.selectedGroup The parent group to which this record belongs
 * @param props.serviceColors A map of service IDs to their hex colors for marker rendering
 * @param props.setSelectedRecord Callback to open the full RecordDetail modal for this record
 */
export function MessageItem({
  rec,
  selectedGroup,
  serviceColors,
  setSelectedRecord
}: {
  rec: EvaluationRecord;
  selectedGroup: RecordGroup;
  serviceColors: Record<string, string>;
  setSelectedRecord: (r: EvaluationRecord) => void;
}) {
  const [viewMode, setViewMode] = useState<'text' | 'hex'>(rec.protocol === 'raw-binary' ? 'hex' : 'text');

  return (
    <div className={`message-viewer ${rec.inbound ? 'message-viewer--in' : 'message-viewer--out'} ${rec.decision === 'drop' ? 'message-viewer--dropped' : ''}`}>
      <div className="message-viewer__header flex items-center gap-2 flex-wrap" onClick={() => setSelectedRecord(rec)}>
        <span className="message-viewer__seq">#{rec.message_seq}</span>
        <span className="message-viewer__dir">
          {rec.inbound ? '↓ IN' : '↑ OUT'}
        </span>
        <Badge variant={rec.decision === 'drop' ? 'error' : 'success'} size="sm">{rec.decision}</Badge>
        {rec.protocol && (
          <span className="mono font-bold" style={{ fontSize: 'var(--text-xs)', color: 'var(--color-text-secondary)', textDecoration: 'underline' }}>
            {formatProtocol(rec.protocol)}
          </span>
        )}
        {rec.http?.method && <span className="mono font-bold" style={{ fontSize: 'var(--text-xs)', color: 'var(--color-text-secondary)' }}>{rec.http.method}</span>}
        {rec.http?.path && <span className="message-viewer__path" title={rec.http.path}>{rec.http.path}</span>}
        {rec.http?.status != null && (
          <span
            className={`message-viewer__status message-viewer__status--${statusClass(rec.http.status)}`}
            title={HTTP_STATUS_CODES[rec.http.status] || 'Unknown status code'}
            style={{ textDecoration: 'underline', textDecorationColor: 'currentColor', textUnderlineOffset: '2px', cursor: 'help' }}
          >
            {rec.http.status}
          </span>
        )}
        {rec.markers.length > 0 && (
          <span className="message-viewer__markers">
            {rec.markers.map((m: string) => (
              <span key={m} className="badge badge--sm" style={{ backgroundColor: `${serviceColors[selectedGroup.service_id]}30`, color: serviceColors[selectedGroup.service_id], borderColor: `${serviceColors[selectedGroup.service_id]}50` }}>
                {m}
              </span>
            ))}
          </span>
        )}
        <div className="ml-auto flex items-center gap-3">
          <span className="message-viewer__size">{formatByteSize(rec.raw_b64 ? atob(rec.raw_b64).length : 0)}</span>
          <div className="text-xs text-muted"><TimeSince ns={rec.timestamp_ns} /></div>
        </div>
      </div>
      {rec.raw_b64 && (
        <div className="message-viewer__raw-wrapper">
          <div style={{ position: 'absolute', top: 'var(--space-2)', right: '36px', zIndex: 10 }}>
            <button
              className="btn btn--ghost btn--xs"
              style={{ padding: '2px 6px', fontSize: '10px', textTransform: 'uppercase', letterSpacing: '0.05em' }}
              onClick={() => setViewMode(viewMode === 'text' ? 'hex' : 'text')}
            >
              {viewMode === 'text' ? 'Hex' : 'Text'}
            </button>
          </div>
          <CopyButton
            text={viewMode === 'text' ? bytesToText(decodeBase64(rec.raw_b64)) : bytesToHexDump(decodeBase64(rec.raw_b64))}
            className="message-viewer__raw-copy"
          />
          <pre className="message-viewer__raw">
            {viewMode === 'text' ? bytesToText(decodeBase64(rec.raw_b64)) : bytesToHexDump(decodeBase64(rec.raw_b64))}
          </pre>
        </div>
      )}
    </div>
  );
}
