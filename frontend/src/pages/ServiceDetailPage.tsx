import { useState, useCallback } from 'react';
import { useParams, useNavigate, Link } from 'react-router-dom';
import { usePolling } from '../hooks/usePolling';
import { getApi } from '../api/client';
import { useSettings } from '../stores/SettingsContext';
import { useToast } from '../stores/ToastContext';
import { StatusDot } from '../components/StatusDot';
import { TimeSince } from '../components/TimeSince';
import { Badge } from '../components/Badge';
import { LoadingSpinner } from '../components/LoadingSpinner';
import { ErrorDisplay } from '../components/ErrorDisplay';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { Modal } from '../components/Modal';
import { Settings } from 'lucide-react';
import type { ServiceMetadata, Health, EcamEvent } from '../api/types';
import './ServiceDetailPage.css';

function getHealthStatus(health?: Health): 'healthy' | 'degraded' | 'offline' {
  if (!health) return 'offline';
  if ('Healthy' in health) return 'healthy';
  if ('Degraded' in health) return 'degraded';
  return 'offline';
}

function getLastSyncNs(health?: Health): number {
  if (!health) return 0;
  if ('Healthy' in health) return health.Healthy.last_sync_ns;
  if ('Degraded' in health) return health.Degraded.last_sync_ns;
  return 0;
}

/**
 * Detail view for one managed filter (service): its health and recent alerts,
 * the list of guarded ports (each linking into the port detail), and editing of
 * the service metadata or removing it from the fleet. Polling pauses while the
 * metadata edit modal is open.
 */
export function ServiceDetailPage() {
  const { serviceId } = useParams<{ serviceId: string }>();
  const navigate = useNavigate();
  const { settings } = useSettings();
  const { showToast } = useToast();
  const [showDelete, setShowDelete] = useState(false);
  const [editModalOpen, setEditModalOpen] = useState(false);
  const [metaForm, setMetaForm] = useState<ServiceMetadata>({});
  const [saving, setSaving] = useState(false);
  const [expandedAlert, setExpandedAlert] = useState<number | null>(null);

  // Fetch full state for this service — paused during edit
  const { data: stateData, error, isLoading, refresh } = usePolling({
    fetcher: async () => {
      const api = getApi();
      const { data, error } = await api.GET('/state');
      if (error) throw error;
      return data!.data.services.find(s => s.id === serviceId);
    },
    paused: editModalOpen,
    intervalMs: settings.statePollMs,
  });

  // Also fetch ports list — paused during edit
  const { data: portsData } = usePolling({
    fetcher: async () => {
      const api = getApi();
      const { data } = await api.GET('/services/{service_id}/ports', {
        params: { path: { service_id: serviceId! } },
      });
      return data?.data?.ports ?? [];
    },
    enabled: !!serviceId,
    paused: editModalOpen,
    intervalMs: settings.statePollMs,
  });

  // Fetch ECAM — paused during edit
  const { data: ecamData } = usePolling({
    fetcher: async () => {
      const api = getApi();
      const { data } = await api.GET('/services/{service_id}/ecam', {
        params: { path: { service_id: serviceId! } },
      });
      return data?.data?.records ?? [];
    },
    enabled: !!serviceId,
    paused: editModalOpen,
    intervalMs: settings.ecamPollMs,
  });

  const openEditModal = useCallback(() => {
    // Initialize form from current state data when modal opens
    if (stateData?.metadata) {
      setMetaForm({ ...stateData.metadata });
    } else {
      setMetaForm({});
    }
    setEditModalOpen(true);
  }, [stateData]);

  const handleSaveMeta = useCallback(async () => {
    if (!serviceId) return;
    setSaving(true);
    try {
      const api = getApi();
      await api.PUT('/services/{service_id}/meta', {
        params: { path: { service_id: serviceId } },
        body: metaForm,
      });
      setEditModalOpen(false);
      showToast('Service metadata saved');
      refresh();
    } catch { /* ignore */ }
    setSaving(false);
  }, [serviceId, metaForm, refresh, showToast]);

  const handleDelete = useCallback(async () => {
    if (!serviceId) return;
    const api = getApi();
    await api.DELETE('/services/{service_id}/delete', {
      params: { path: { service_id: serviceId } },
    });
    navigate('/services', { replace: true });
  }, [serviceId, navigate]);

  if (isLoading && !stateData) {
    return <div className="page-center"><LoadingSpinner label="Loading service…" /></div>;
  }

  if (error && !stateData) {
    return <div className="page-padding"><ErrorDisplay error={error as Error} onRetry={refresh} /></div>;
  }

  if (!stateData) {
    return <div className="page-padding"><ErrorDisplay error="Service not found" /></div>;
  }

  const ports = (portsData ?? stateData.ports.map(p => p.port)).sort((a, b) => a - b);
  const healthStatus = getHealthStatus(stateData.health);

  return (
    <div className="service-detail">
      <div className="service-detail__header">
        <nav className="breadcrumb">
          <Link to="/services" className="breadcrumb__link">Services</Link>
          <span className="breadcrumb__sep">/</span>
          <span className="breadcrumb__current">{stateData.id}</span>
        </nav>
        <div className="flex items-center gap-3">
          <StatusDot status={healthStatus} size="md" />
          <h1 className="service-detail__title" style={stateData.metadata?.hex_color ? { textDecorationLine: 'underline', textDecorationColor: stateData.metadata.hex_color, textUnderlineOffset: '4px', textDecorationThickness: '3px' } : undefined}>
            {stateData.metadata?.name || stateData.id}
          </h1>
          <button
            className="btn btn--ghost port-detail__config-btn"
            onClick={openEditModal}
            title="Edit service metadata"
            aria-label="Edit service metadata"
          >
            <Settings size={18} />
          </button>
        </div>
        <div className="flex items-center gap-4 mt-2">
          <span className="service-detail__id">ID: {stateData.id}</span>
          <span className="service-detail__url">{stateData.base_url}</span>
          <TimeSince ns={getLastSyncNs(stateData.health)} />
        </div>
      </div>

      {/* Metadata display */}
      <section className="service-detail__section">
        <div className="section-header">
          <h2 className="section-title">Metadata</h2>
        </div>
        <div className="meta-display">
          {stateData.metadata?.description && <p className="meta-display__desc">{stateData.metadata.description}</p>}
          {!stateData.metadata?.name && !stateData.metadata?.description && (
            <p className="meta-display__empty">No metadata configured</p>
          )}
        </div>
      </section>

      {/* Ports section */}
      <section className="service-detail__section">
        <div className="section-header">
          <h2 className="section-title">Ports</h2>
          <Badge variant="default">{ports.length}</Badge>
        </div>
        <div className="ports-list flex flex-col gap-1">
          {ports.map(port => {
            const portNum = typeof port === 'number' ? port : port;
            const snap = stateData.ports.find(p => p.port === portNum)?.snapshot;
            return (
              <Link
                key={portNum}
                to={`/services/${serviceId}/ports/${portNum}`}
                className="port-item flex items-center justify-between"
                style={{ borderLeftColor: snap?.info?.hex_color ?? 'var(--color-border)' }}
              >
                <div className="flex items-center gap-2">
                  <span className="port-item__number">
                    :{portNum}{snap?.info ? <span className="port-item__arrow"> -&gt; :{snap.info.upstream}</span> : null}
                  </span>
                  {snap?.info?.name && <span className="port-item__name">{snap.info.name}</span>}
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  {snap?.info?.channel && (
                    <Badge variant={snap.info.channel === 'tls' ? 'success' : 'default'} size="sm">
                      {snap.info.channel.toUpperCase()}
                    </Badge>
                  )}
                  {snap?.ruleset && (
                    <>
                      <Badge variant="default" size="sm">{snap.ruleset.markers.length} markers</Badge>
                      <Badge variant="default" size="sm">{snap.ruleset.filters.length} filters</Badge>
                      <span className="port-item__version">v:{snap.ver_ruleset.slice(0, 8)}</span>
                    </>
                  )}
                </div>
              </Link>
            );
          })}
        </div>
      </section>

      {/* ECAM mini-feed */}
      <section className="service-detail__section">
        <div className="section-header">
          <h2 className="section-title">Recent Alerts</h2>
        </div>
        {ecamData && ecamData.length > 0 ? (() => {
          // eslint-disable-next-line react-hooks/purity
          const now = Date.now();
          const cutoffNs = (now - 24 * 60 * 60 * 1000) * 1_000_000;
          const filtered = [...ecamData]
            .filter((ev: EcamEvent) => ev.ts_ns >= cutoffNs)
            .sort((a: EcamEvent, b: EcamEvent) => b.ts_ns - a.ts_ns)
            .slice(0, 100);
          return filtered.length > 0 ? (
            <div className="ecam-mini flex flex-col gap-1" style={{ maxHeight: 320, overflowY: 'auto' }}>
              {filtered.map((ev: EcamEvent, i: number) => (
                <div key={i} className="ecam-mini__entry">
                  <div
                    className={`ecam-mini__item flex items-start gap-3 ${expandedAlert === i ? 'ecam-mini__item--expanded' : ''}`}
                    onClick={() => setExpandedAlert(prev => prev === i ? null : i)}
                    role="button"
                    tabIndex={0}
                    onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') setExpandedAlert(prev => prev === i ? null : i); }}
                  >
                    <Badge variant={ev.severity === 'error' ? 'error' : 'warning'} size="sm">{ev.severity}</Badge>
                    <span className="ecam-mini__msg flex-1 min-w-0">{ev.msg}</span>
                    <TimeSince ns={ev.ts_ns} />
                    <span className="ecam-mini__chevron">{expandedAlert === i ? '▾' : '▸'}</span>
                  </div>
                  {expandedAlert === i && ev.data != null && (
                    <pre className="ecam-mini__data">{JSON.stringify(ev.data, null, 2)}</pre>
                  )}
                </div>
              ))}
            </div>
          ) : (
            <p className="meta-display__empty">No alerts in the last 24 hours</p>
          );
        })() : (
          <p className="meta-display__empty">No alerts</p>
        )}
      </section>

      {/* Danger Zone */}
      <section className="service-detail__section">
        <div className="section-header">
          <h2 className="section-title" style={{ color: 'var(--color-error)' }}>Danger Zone</h2>
        </div>
        <div className="flex gap-3 mt-6">
          <button className="btn btn--danger" onClick={() => setShowDelete(true)}>
            Delete Service
          </button>
        </div>
      </section>

      {/* Edit Service Modal */}
      <Modal open={editModalOpen} onClose={() => setEditModalOpen(false)} title="Edit Service" size="md">
        <div className="form-stack">
          <div className="form-field">
            <label className="form-label">Name</label>
            <input value={metaForm.name ?? ''} onChange={e => setMetaForm(m => ({ ...m, name: e.target.value }))} />
          </div>
          <div className="form-field">
            <label className="form-label">Description</label>
            <textarea value={metaForm.description ?? ''} onChange={e => setMetaForm(m => ({ ...m, description: e.target.value }))} rows={2} />
          </div>
          <div className="form-field">
            <label className="form-label">Color</label>
            <div style={{ display: 'flex', gap: 'var(--space-2)', alignItems: 'center' }}>
              <input type="color" value={metaForm.hex_color ?? '#6c8cff'} onChange={e => setMetaForm(m => ({ ...m, hex_color: e.target.value }))} style={{ width: 36, height: 28, padding: 0 }} />
              <input value={metaForm.hex_color ?? ''} onChange={e => setMetaForm(m => ({ ...m, hex_color: e.target.value }))} placeholder="#ff8800" style={{ width: 100 }} />
            </div>
          </div>
          <div className="form-field">
            <label className="form-label">Icon</label>
            <input value={metaForm.icon ?? ''} onChange={e => setMetaForm(m => ({ ...m, icon: e.target.value }))} placeholder="emoji or text" style={{ width: 100 }} />
          </div>
          <div className="form-actions">
            <button className="btn btn--secondary" onClick={() => setEditModalOpen(false)}>Cancel</button>
            <button className="btn btn--primary" onClick={handleSaveMeta} disabled={saving}>
              {saving ? 'Saving…' : 'Save'}
            </button>
          </div>
        </div>
      </Modal>

      <ConfirmDialog
        open={showDelete}
        title="Delete Service"
        message={`Delete "${stateData.metadata?.name || stateData.id}"? The controller will stop managing this filter, but the filter itself continues operating independently.`}
        confirmLabel="Delete"
        onConfirm={handleDelete}
        onCancel={() => setShowDelete(false)}
        variant="danger"
      />
    </div>
  );
}
