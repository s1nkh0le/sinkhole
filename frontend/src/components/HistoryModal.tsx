import { useState } from 'react';
import { Modal } from './Modal';
import { CopyButton } from './CopyButton';
import { Badge } from './Badge';
import { RollbackConfirmModal } from './RollbackConfirmModal';
import { usePolling } from '../hooks/usePolling';
import { getApi } from '../api/client';
import type { RulesetHistoryEntry } from '../api/types';
import { formatAbsolute } from '../lib/time';
import { EmptyState } from './EmptyState';
import { Clock, AlertTriangle } from 'lucide-react';
import './HistoryModal.css';

interface HistoryModalProps {
  open: boolean;
  onClose: () => void;
  serviceId: string;
  port: number;
  currentVersion: string;
  onRollback: () => void;
}

function HistoryCard({ 
  entry, 
  isCurrentVersion,
  onRollbackClick 
}: { 
  entry: RulesetHistoryEntry; 
  isCurrentVersion: boolean;
  onRollbackClick: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const jsonText = JSON.stringify(entry.ruleset, null, 2);

  return (
    <div className="history-card">
      <div className="history-card__header">
        <div className="history-card__title">
          <span className="history-card__version">v:{entry.version.slice(0, 8)}</span>
          <span className="history-card__time">{formatAbsolute(entry.updated_at_ns)}</span>
          {isCurrentVersion && <Badge variant="info">Current</Badge>}
        </div>
        <div className="history-card__badges">
          <Badge>{entry.ruleset.markers.length} markers</Badge>
          <Badge>{entry.ruleset.filters.length} filters</Badge>
        </div>
      </div>
      
      <div className="history-card__commit">
        {entry.ruleset.commit_message || <span className="text-muted">No commit message</span>}
      </div>

      <div className="history-card__actions">
        <button className="btn btn--ghost btn--sm" onClick={() => setExpanded(!expanded)}>
          {expanded ? 'Hide JSON' : 'View JSON'}
        </button>
        {!isCurrentVersion && (
          <button className="btn btn--secondary btn--sm" onClick={onRollbackClick}>
            Rollback
          </button>
        )}
      </div>

      {expanded && (
        <div style={{ position: 'relative', marginTop: '8px' }}>
          <div 
            className="history-card__json rounded overflow-x-auto text-xs font-mono"
            style={{ backgroundColor: '#000', color: '#d4d4d4', padding: '12px' }}
          >
            <pre style={{ margin: 0 }}>{jsonText}</pre>
          </div>
          <div style={{ position: 'absolute', top: '8px', right: '8px' }}>
            <CopyButton text={jsonText} />
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * Modal listing a port's ruleset version history, newest first, marking the
 * live version and offering rollback to any earlier one (confirmed via
 * RollbackConfirmModal). History is polled while the modal is open.
 */
export function HistoryModal({
  open, onClose, serviceId, port, currentVersion, onRollback
}: HistoryModalProps) {
  const [rollbackEntry, setRollbackEntry] = useState<RulesetHistoryEntry | null>(null);

  const { data, isLoading, error } = usePolling({
    fetcher: async () => {
      const res = await getApi().GET('/services/{service_id}/ports/{port}/ruleset/history', {
        params: { path: { service_id: serviceId, port } }
      });
      if (res.error) throw res.error;
      return res.data!.data;
    },
    enabled: open,
    intervalMs: 10000,
  });

  const entries = data?.entries || [];

  return (
    <>
      <Modal open={open} onClose={onClose} title="Ruleset History" size="xl">
        <div className="history-modal">
          {isLoading && !data && <div className="history-modal__loading">Loading history…</div>}
          
          {!!error && (
            <div className="p-8">
              <EmptyState 
                icon={<AlertTriangle size={32} className="text-error" />}
                title="Failed to load history" 
                description="There was an error fetching the history for this ruleset." 
              />
            </div>
          )}

          {!isLoading && entries.length === 0 && !error && (
            <div className="p-8">
              <EmptyState 
                icon={<Clock size={32} />}
                title="No history available" 
                description="This ruleset has no previous revisions." 
              />
            </div>
          )}

          <div className="history-list">
            {entries.map(entry => (
              <HistoryCard 
                key={entry.version + entry.updated_at_ns} 
                entry={entry} 
                isCurrentVersion={entry.version === currentVersion}
                onRollbackClick={() => setRollbackEntry(entry)} 
              />
            ))}
          </div>
        </div>
      </Modal>

      {rollbackEntry && (
        <RollbackConfirmModal
          open={!!rollbackEntry}
          onClose={() => setRollbackEntry(null)}
          onConfirm={() => {
            setRollbackEntry(null);
            onRollback();
          }}
          serviceId={serviceId}
          port={port}
          currentVersion={currentVersion}
          targetVersion={rollbackEntry.version}
          targetRuleset={rollbackEntry.ruleset}
        />
      )}
    </>
  );
}
