import { useState, useMemo } from 'react';
import { useSettings } from '../stores/SettingsContext';
import { useAuth } from '../stores/AuthContext';
import { useToast } from '../stores/ToastContext';
import { Modal } from '../components/Modal';
import { ConnectionConfigForm } from '../components/ConnectionConfigForm';
import './SettingsPage.css';

/**
 * Settings screen: edits poll intervals and other preferences in a draft that
 * is only committed on save, switches the active controller (URL + token, via a
 * verified modal), and offers logout. The draft resyncs if settings change
 * externally (e.g. a reset).
 */
export function SettingsPage() {
  const { settings, updateSettings, resetSettings } = useSettings();
  const { token, login, logout } = useAuth();
  const { showToast } = useToast();

  // Draft state — edits go here, only applied on Save
  const [draft, setDraft] = useState({ ...settings });
  const [prevSettings, setPrevSettings] = useState(settings);

  // Change-URL modal state
  const [urlModalOpen, setUrlModalOpen] = useState(false);

  // Reset draft when settings change externally (e.g. after reset)
  if (settings !== prevSettings) {
    setPrevSettings(settings);
    setDraft({ ...settings });
  }

  const isDirty = useMemo(() => {
    return JSON.stringify(draft) !== JSON.stringify(settings);
  }, [draft, settings]);

  const handleSave = () => {
    updateSettings(draft);
    showToast('Settings saved', 'success');
  };

  const handleDiscard = () => {
    setDraft({ ...settings });
  };

  const handleReset = () => {
    resetSettings();
    showToast('Settings reset to defaults', 'info');
  };

  // --- URL modal handlers ---
  const openUrlModal = () => {
    setUrlModalOpen(true);
  };

  const closeUrlModal = () => {
    setUrlModalOpen(false);
  };

  const handleApplyConfig = async (newUrl: string, newToken: string) => {
    updateSettings({ ...settings, controllerUrl: newUrl });
    
    // Switch auth context
    if (newToken) {
      await login(newToken);
    } else {
      logout();
    }

    showToast('Controller configuration updated', 'success');
    closeUrlModal();
  };

  // --- Token copy handler ---
  const copyToken = async () => {
    if (!token) return;
    try {
      await navigator.clipboard.writeText(token);
      showToast('Token copied to clipboard', 'success');
    } catch {
      showToast('Failed to copy token', 'error');
    }
  };

  return (
    <div className="settings-page">
      <h1 className="settings-page__title">Settings</h1>
      <p className="settings-page__subtitle">Configuration is stored in your browser's localStorage.</p>

      <section className="settings-section">
        <h2 className="settings-section__title">Connection</h2>
        <div className="form-stack">
          <div className="form-field">
            <label className="form-label">Controller URL</label>
            <div className="settings-display">
              <span className="settings-display__value">{settings.controllerUrl}</span>
              <button className="btn btn--ghost btn--sm" onClick={openUrlModal}>Change</button>
            </div>
            <span className="form-hint">Base URL for the sinkhole controller API.</span>
          </div>
          <div className="form-field">
            <label className="form-label">API Token</label>
            <div className="settings-display">
              <span className="settings-display__value settings-display__value--masked">
                {token ? '••••••••••••' : 'Not set'}
              </span>
              {token && (
                <button className="btn btn--ghost btn--sm" onClick={copyToken} aria-label="Copy token">
                  ⎘
                </button>
              )}
            </div>
            <span className="form-hint">Token is stored in sessionStorage and set during login.</span>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h2 className="settings-section__title">Polling Intervals</h2>
        <div className="form-stack">
          <div className="form-field">
            <label className="form-label">State / Config (ms)</label>
            <input
              type="number"
              value={draft.statePollMs}
              onChange={e => setDraft(d => ({ ...d, statePollMs: parseInt(e.target.value) || 0 }))}
              min={1000}
              step={1000}
            />
            <span className="form-hint">Services, ports, rulesets. Default: 5000ms. Min: 1000ms.</span>
          </div>
          <div className="form-field">
            <label className="form-label">Health (ms)</label>
            <input
              type="number"
              value={draft.healthPollMs}
              onChange={e => setDraft(d => ({ ...d, healthPollMs: parseInt(e.target.value) || 0 }))}
              min={2000}
              step={1000}
            />
            <span className="form-hint">Controller health check. Default: 10000ms. Min: 2000ms.</span>
          </div>
          <div className="form-field">
            <label className="form-label">ECAM Events (ms)</label>
            <input
              type="number"
              value={draft.ecamPollMs}
              onChange={e => setDraft(d => ({ ...d, ecamPollMs: parseInt(e.target.value) || 0 }))}
              min={2000}
              step={1000}
            />
            <span className="form-hint">ECAM alert feed. Default: 8000ms. Min: 2000ms.</span>
          </div>
          <div className="form-field">
            <label className="form-label">Traffic Records (ms)</label>
            <input
              type="number"
              value={draft.recordsPollMs}
              onChange={e => setDraft(d => ({ ...d, recordsPollMs: parseInt(e.target.value) || 0 }))}
              min={3000}
              step={1000}
            />
            <span className="form-hint">Record queries in Traffic Analyzer. Default: 15000ms. Min: 3000ms.</span>
          </div>
        </div>
      </section>

      {isDirty && (
        <div className="settings-page__save-bar">
          <span className="settings-page__save-label">You have unsaved changes</span>
          <button className="btn btn--ghost btn--sm" onClick={handleDiscard}>Discard</button>
          <button className="btn btn--primary btn--sm" onClick={handleSave}>Save Settings</button>
        </div>
      )}

      <section className="settings-section">
        <h2 className="settings-section__title">Danger Zone</h2>
        <button className="btn btn--danger" onClick={handleReset}>
          Reset All Settings
        </button>
        <span className="form-hint" style={{ marginTop: 'var(--space-2)', display: 'block' }}>
          Resets all settings to their default values.
        </span>
      </section>

      {/* Change Controller URL modal */}
      <Modal open={urlModalOpen} onClose={closeUrlModal} title="Change Controller Configuration" size="lg">
        <div className="alert alert--warning" style={{ marginBottom: 'var(--space-4)', padding: 'var(--space-3)', backgroundColor: 'var(--color-warning-transparent, rgba(245, 158, 11, 0.1))', borderLeft: '4px solid var(--color-warning, #f59e0b)', borderRadius: 'var(--radius-md)' }}>
          <strong>Warning:</strong> Changing the controller URL or token will effectively log you out of the current session and switch contexts.
        </div>
        
        <ConnectionConfigForm
          initialUrl={settings.controllerUrl}
          initialToken=""
          onApply={handleApplyConfig}
          onCancel={closeUrlModal}
          applyLabel="Apply Changes"
          preventSameUrl={true}
        />
      </Modal>
    </div>
  );
}
