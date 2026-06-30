import { useState } from 'react';
import { Modal } from './Modal';
import { useToast } from '../stores/ToastContext';
import { getApi } from '../api/client';
import type { RuleSetRaw } from '../api/types';
import './RollbackConfirmModal.css';



interface RollbackConfirmModalProps {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  serviceId: string;
  port: number;
  currentVersion: string;
  targetVersion: string;
  targetRuleset: RuleSetRaw;
}

/**
 * Confirmation step for rolling a port's ruleset back to an earlier version.
 * Shows the target version, takes a commit message, and on confirm re-applies
 * the historical ruleset as a new version (rollback is forward, not destructive).
 */
export function RollbackConfirmModal({
  open, onClose, onConfirm, serviceId, port, currentVersion, targetVersion, targetRuleset
}: RollbackConfirmModalProps) {
  const { showToast } = useToast();
  const [commitMessage, setCommitMessage] = useState(
    `Rollback from version ${currentVersion.slice(0, 8)} to version ${targetVersion.slice(0, 8)}`
  );
  const [submitting, setSubmitting] = useState(false);

  const handleConfirm = async () => {
    setSubmitting(true);
    try {
      const api = getApi();
      const rulesetToDeploy: RuleSetRaw = {
        ...targetRuleset,
        commit_message: commitMessage,
      };

      const res = await api.PUT('/services/{service_id}/ports/{port}/ruleset', {
        params: { path: { service_id: serviceId, port } },
        body: rulesetToDeploy,
      });

      if (res.error) throw res.error;
      showToast('Rollback successful', 'success');
      onConfirm();
    } catch (e: unknown) {
      const err = e as { detail?: string; title?: string };
      const msg = err.detail ?? err.title ?? String(e);
      showToast(`Rollback failed: ${msg}`, 'error');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal open={open} onClose={onClose} title="Confirm Rollback" size="md">
      <div className="rollback-confirm">
        <p className="rollback-confirm__text">
          Are you sure you want to rollback to version <strong>{targetVersion.slice(0, 8)}</strong>? 
          This will immediately deploy the historical ruleset to the filter.
        </p>
        
        <div className="form-field" style={{ marginTop: 'var(--space-4)' }}>
          <label className="form-label">Commit Message</label>
          <textarea 
            value={commitMessage}
            onChange={(e) => setCommitMessage(e.target.value)}
            disabled={submitting}
            placeholder="Rollback commit message"
            rows={3}
            style={{ resize: 'vertical' }}
          />
        </div>

        <div className="rollback-confirm__actions" style={{ marginTop: 'var(--space-6)', display: 'flex', justifyContent: 'flex-end', gap: 'var(--space-2)' }}>
          <button className="btn btn--ghost" onClick={onClose} disabled={submitting}>Cancel</button>
          <button className="btn btn--danger" onClick={handleConfirm} disabled={submitting}>
            {submitting ? 'Rolling back…' : 'Confirm Rollback'}
          </button>
        </div>
      </div>
    </Modal>
  );
}
