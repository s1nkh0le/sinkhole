import { useState, useEffect, useCallback } from 'react';
import { useParams, Link, useBlocker } from 'react-router-dom';
import { usePolling } from '../hooks/usePolling';
import { useSettings } from '../stores/SettingsContext';
import { useToast } from '../stores/ToastContext';
import { getApi } from '../api/client';
import { StatusDot } from '../components/StatusDot';
import { Badge } from '../components/Badge';
import { ScopeSelector } from '../components/ScopeSelector';
import { LoadingSpinner } from '../components/LoadingSpinner';
import { ErrorDisplay } from '../components/ErrorDisplay';
import { Modal } from '../components/Modal';
import { RulesetEditor } from './RulesetEditor';
import { Settings } from 'lucide-react';
import type { Scope, Health, CtrlReplyBodyServiceState } from '../api/types';
import './PortDetailPage.css';

function getHealthStatus(health?: Health): 'healthy' | 'degraded' | 'offline' {
  if (!health) return 'offline';
  if ('Healthy' in health) return 'healthy';
  if ('Degraded' in health) return 'degraded';
  return 'offline';
}

/**
 * Detail view for one guarded port: edits its config (name, comment, color,
 * enabled scopes) and hosts the RulesetEditor with markers/filters tabs. Guards
 * against losing unsaved ruleset edits on both in-app navigation and browser
 * close/reload while the editor is dirty.
 */
export function PortDetailPage() {
  const { serviceId, port } = useParams<{ serviceId: string; port: string }>();
  const portNum = parseInt(port!, 10);
  const { settings } = useSettings();
  const { showToast } = useToast();
  const [activeTab, setActiveTab] = useState<'markers' | 'filters'>('filters');
  const [markerCount, setMarkerCount] = useState(0);
  const [filterCount, setFilterCount] = useState(0);
  const [configModalOpen, setConfigModalOpen] = useState(false);
  const [configForm, setConfigForm] = useState<{ name: string; comment: string; hex_color: string; enabled_scopes: Scope[] }>({
    name: '', comment: '', hex_color: '', enabled_scopes: [],
  });
  const [saving, setSaving] = useState(false);
  const [rulesetDirty, setRulesetDirty] = useState(false);

  // Navigation guard: in-app navigation
  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      rulesetDirty && currentLocation.pathname !== nextLocation.pathname
  );

  // Navigation guard: browser close/reload
  useEffect(() => {
    if (!rulesetDirty) return;
    const handler = (e: BeforeUnloadEvent) => { e.preventDefault(); };
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  }, [rulesetDirty]);

  const { data: portInfo, error, isLoading, refresh } = usePolling({
    fetcher: async () => {
      const api = getApi();
      const { data, error } = await api.GET('/services/{service_id}/ports/{port}/config', {
        params: { path: { service_id: serviceId!, port: portNum } },
      });
      if (error) throw error;
      return data!.data.info;
    },
    enabled: !!serviceId,
    paused: configModalOpen,
    intervalMs: settings.statePollMs,
  });

  // Fetch service health to detect degraded state
  const { data: serviceHealth, isLoading: healthLoading } = usePolling({
    fetcher: async () => {
      const api = getApi();
      const { data } = await api.GET('/state');
      const svc = data?.data?.services?.find((s: CtrlReplyBodyServiceState) => s.id === serviceId);
      return svc?.health;
    },
    enabled: !!serviceId,
    intervalMs: settings.healthPollMs,
  });

  const svcHealthStatus = getHealthStatus(serviceHealth as Health);
  const isDegraded = svcHealthStatus === 'degraded' || svcHealthStatus === 'offline';



  const openConfigModal = () => {
    if (portInfo) {
      setConfigForm({
        name: portInfo.name ?? '',
        comment: portInfo.comment ?? '',
        hex_color: portInfo.hex_color ?? '',
        enabled_scopes: portInfo.enabled_scopes,
      });
    }
    setConfigModalOpen(true);
  };

  const handleSaveConfig = useCallback(async () => {
    if (!serviceId) return;
    setSaving(true);
    try {
      const api = getApi();
      const res = await api.PUT('/services/{service_id}/ports/{port}/config', {
        params: { path: { service_id: serviceId, port: portNum } },
        body: {
          name: configForm.name || null,
          comment: configForm.comment || null,
          hex_color: configForm.hex_color || null,
          enabled_scopes: configForm.enabled_scopes,
        },
      });
      if (res.error) {
        const errData = res.error as { title?: string; detail?: string };
        const errMsg = errData?.title ?? errData?.detail ?? 'Failed to save port configuration';
        showToast(errMsg, 'error');
      } else {
        setConfigModalOpen(false);
        showToast('Port configuration saved');
        refresh();
      }
    } catch (e) {
      showToast(`Port config error: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
    setSaving(false);
  }, [serviceId, portNum, configForm, refresh, showToast]);

  if ((isLoading && !portInfo) || (healthLoading && !serviceHealth)) {
    return <div className="page-center"><LoadingSpinner label="Loading port…" /></div>;
  }

  if (error && !portInfo) {
    return <div className="page-padding"><ErrorDisplay error={error as Error} onRetry={refresh} /></div>;
  }

  if (!portInfo) return null;

  return (
    <div className="port-detail">
      <div className="port-detail__header">
        <nav className="breadcrumb">
          <Link to="/services" className="breadcrumb__link">Services</Link>
          <span className="breadcrumb__sep">/</span>
          <Link to={`/services/${serviceId}`} className="breadcrumb__link">{serviceId}</Link>
          <span className="breadcrumb__sep">/</span>
          <span className="breadcrumb__current">:{portInfo.public}</span>
        </nav>
        <div className="flex items-center gap-3">
          <StatusDot status={svcHealthStatus} size="md" />
          <h1 className="port-detail__title" style={portInfo.hex_color ? { textDecorationLine: 'underline', textDecorationColor: portInfo.hex_color, textUnderlineOffset: '4px', textDecorationThickness: '3px' } : undefined}>
            Port :{portInfo.public} -&gt; :{portInfo.upstream}
            {portInfo.name && <span className="port-detail__name"> — {portInfo.name}</span>}
          </h1>
          <button
            className="btn btn--ghost port-detail__config-btn"
            onClick={openConfigModal}
            title="Port configuration"
            aria-label="Port configuration"
          >
            <Settings size={18} />
          </button>
        </div>
        <div className="flex gap-2 mt-2">
          {portInfo.channel && (
            <Badge variant={portInfo.channel === 'tls' ? 'success' : 'default'}>
              {portInfo.channel.toUpperCase()}
            </Badge>
          )}
          <Badge variant="info">{portInfo.enabled_scopes.length} scopes</Badge>
          {isDegraded && <Badge variant="warning">Service {svcHealthStatus}</Badge>}
        </div>
      </div>

      <div className="port-detail__tabs flex gap-1">
        <button
          className={`port-detail__tab ${activeTab === 'markers' ? 'port-detail__tab--active' : ''}`}
          onClick={() => setActiveTab('markers')}
        >
          Markers <Badge>{markerCount}</Badge>
        </button>
        <button
          className={`port-detail__tab ${activeTab === 'filters' ? 'port-detail__tab--active' : ''}`}
          onClick={() => setActiveTab('filters')}
        >
          Filters <Badge>{filterCount}</Badge>
        </button>
      </div>

      <RulesetEditor
        serviceId={serviceId!}
        port={portNum}
        enabledScopes={portInfo.enabled_scopes}
        view={activeTab}
        onDirtyChange={setRulesetDirty}
        onCountsChange={(m, f) => { setMarkerCount(m); setFilterCount(f); }}
        isDegraded={isDegraded}
      />

      {/* Config Modal */}
      <Modal open={configModalOpen} onClose={() => setConfigModalOpen(false)} title="Port Configuration" size="md">
        <div className="flex flex-col gap-4">
          <div className="form-field">
            <label className="form-label">Name</label>
            <input
              value={configForm.name}
              onChange={e => setConfigForm(f => ({ ...f, name: e.target.value }))}
              placeholder="Port name"
            />
          </div>
          <div className="form-field">
            <label className="form-label">Comment</label>
            <input
              value={configForm.comment}
              onChange={e => setConfigForm(f => ({ ...f, comment: e.target.value }))}
              placeholder="Comment"
            />
          </div>
          <div className="form-field">
            <label className="form-label">Color</label>
            <div style={{ display: 'flex', gap: 'var(--space-2)', alignItems: 'center' }}>
              <input
                type="color"
                value={configForm.hex_color || '#6c8cff'}
                onChange={e => setConfigForm(f => ({ ...f, hex_color: e.target.value }))}
                style={{ width: 36, height: 28, padding: 0 }}
              />
              <input
                value={configForm.hex_color}
                onChange={e => setConfigForm(f => ({ ...f, hex_color: e.target.value }))}
                placeholder="#ff8800"
                style={{ width: 100 }}
              />
            </div>
          </div>
          <div className="form-field">
            <label className="form-label">Enabled Scopes</label>
            <ScopeSelector
              value={configForm.enabled_scopes}
              onChange={scopes => setConfigForm(f => ({ ...f, enabled_scopes: scopes }))}
              disabled={false}
            />
          </div>
          <div className="form-actions">
            <button className="btn btn--ghost" onClick={() => setConfigModalOpen(false)}>Cancel</button>
            <button className="btn btn--primary" onClick={handleSaveConfig} disabled={saving}>
              {saving ? 'Saving…' : 'Save'}
            </button>
          </div>
        </div>
      </Modal>

      {/* Navigation guard modal */}
      {blocker.state === 'blocked' && (
        <Modal open={true} onClose={() => blocker.reset?.()} title="Unsaved Changes" size="sm">
          <p className="meta-display__desc" style={{ marginBottom: 'var(--space-4)' }}>
            You have unsaved ruleset changes. Discard them and leave?
          </p>
          <div className="form-actions">
            <button className="btn btn--secondary" onClick={() => blocker.reset?.()}>Stay</button>
            <button className="btn btn--warning" onClick={() => blocker.proceed?.()}>Discard & Leave</button>
          </div>
        </Modal>
      )}
    </div>
  );
}
