import { useState, useCallback, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { usePolling } from '../hooks/usePolling';
import { getApi } from '../api/client';
import { useSettings } from '../stores/SettingsContext';
import { useToast } from '../stores/ToastContext';
import { StatusDot } from '../components/StatusDot';
import { ColorBar } from '../components/ColorDot';
import { TimeSince } from '../components/TimeSince';
import { Badge } from '../components/Badge';
import { LoadingSpinner } from '../components/LoadingSpinner';
import { EmptyState } from '../components/EmptyState';
import { ErrorDisplay } from '../components/ErrorDisplay';
import { Modal } from '../components/Modal';
import { Server } from 'lucide-react';
import type { Health } from '../api/types';
import './ServicesPage.css';

interface ErrorEnvelope {
  status: string;
  code: number;
  title: string;
  detail?: string | null;
  error?: string | null;
  instance?: string | null;
}

function getHealthStatus(health: Health): 'healthy' | 'degraded' | 'offline' {
  if ('Healthy' in health) return 'healthy';
  if ('Degraded' in health) return 'degraded';
  return 'offline';
}

function getLastSyncNs(health: Health): number {
  if ('Healthy' in health) return health.Healthy.last_sync_ns;
  if ('Degraded' in health) return health.Degraded.last_sync_ns;
  return 0;
}

/* ---------- URL validation helpers ---------- */
const LOCALHOST_ALIASES = new Set(['localhost', '127.0.0.1', '0.0.0.0']);

/** Normalize hostname so localhost/127.0.0.1/0.0.0.0 are treated as equivalent */
function normalizeHost(host: string): string {
  const lower = host.toLowerCase();
  return LOCALHOST_ALIASES.has(lower) ? '127.0.0.1' : lower;
}

function isValidHost(host: string): boolean {
  if (!host) return false;
  // Block 0.0.0.0 — browsers can't CORS to it
  if (host === '0.0.0.0') return false;
  // IPv4
  if (/^\d{1,3}(\.\d{1,3}){3}$/.test(host)) {
    return host.split('.').every(p => { const n = Number(p); return n >= 0 && n <= 255; });
  }
  // IPv6 bare
  if (/^[\da-fA-F:]+$/.test(host) && host.includes(':')) return true;
  // Domain or internal hostname (e.g., example.com, localhost, filter-weather)
  return /^([a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?\.)*[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?$/.test(host);
}

function isValidPort(port: string): boolean {
  const n = Number(port);
  return Number.isInteger(n) && n >= 1 && n <= 65535;
}

function isValidRawUrl(url: string): boolean {
  try {
    const u = new URL(url);
    if (u.hostname === '0.0.0.0') return false; // Block 0.0.0.0
    return (u.protocol === 'http:' || u.protocol === 'https:') && !!u.hostname;
  } catch {
    return false;
  }
}

/** Extract normalized hostname + port from a URL string */
function urlIdentity(urlStr: string): string | null {
  try {
    const u = new URL(urlStr);
    const host = normalizeHost(u.hostname);
    const port = u.port || (u.protocol === 'https:' ? '443' : '80');
    return `${host}:${port}`;
  } catch {
    return null;
  }
}

/**
 * Checks if the filter URL points to the controller API itself.
 * Treats localhost/127.0.0.1/0.0.0.0 as equivalent.
 */
function isSelfUrl(filterBaseUrl: string, controllerUrl: string): boolean {
  const a = urlIdentity(filterBaseUrl);
  const b = urlIdentity(controllerUrl);
  return a !== null && b !== null && a === b;
}

/**
 * Checks if the filter URL is already registered by an existing service.
 * Returns the matching service ID, or null.
 */
function findDuplicateServiceUrl(
  filterBaseUrl: string,
  services: { id: string; base_url: string }[],
): string | null {
  const newId = urlIdentity(filterBaseUrl);
  if (!newId) return null;
  for (const svc of services) {
    if (urlIdentity(svc.base_url) === newId) return svc.id;
  }
  return null;
}

/**
 * Fleet overview and the landing page: lists every managed filter (service)
 * with health and a link into its detail, and hosts the "add service" flow
 * (guided or raw URL entry, with validation and a health preflight before the
 * controller starts managing it).
 */
export function ServicesPage() {
  const navigate = useNavigate();
  const { settings } = useSettings();
  const { showToast } = useToast();
  const [showAdd, setShowAdd] = useState(false);
  const [addForm, setAddForm] = useState({ id: '', token: '', name: '', check_health: true });
  const [addError, setAddError] = useState<ErrorEnvelope | null>(null);
  const [addLoading, setAddLoading] = useState(false);
  const [addPhase, setAddPhase] = useState<string>('');

  // Dual-mode URL state
  const [urlMode, setUrlMode] = useState<'guided' | 'raw'>('guided');
  const [guidedUrl, setGuidedUrl] = useState({ protocol: 'http', host: '', port: '' });
  const [rawUrl, setRawUrl] = useState('');

  // Derived base_url & validation
  const baseUrl = useMemo(() => {
    if (urlMode === 'raw') return rawUrl.trim();
    const { protocol, host, port } = guidedUrl;
    if (!host.trim() || !port.trim()) return '';
    return `${protocol}://${host.trim()}:${port.trim()}`;
  }, [urlMode, guidedUrl, rawUrl]);

  const urlValid = useMemo(() => {
    if (urlMode === 'raw') return isValidRawUrl(rawUrl.trim());
    return isValidHost(guidedUrl.host.trim()) && isValidPort(guidedUrl.port);
  }, [urlMode, guidedUrl, rawUrl]);

  // Self-URL check: detect if the user is trying to register the controller itself
  const controllerApiUrl = useMemo(() => {
    return settings.controllerUrl || window.location.origin;
  }, [settings.controllerUrl]);

  const selfUrlWarning = useMemo(() => {
    if (!baseUrl || !urlValid) return false;
    return isSelfUrl(baseUrl, controllerApiUrl);
  }, [baseUrl, urlValid, controllerApiUrl]);

  // 0.0.0.0 hint
  const zeroAddrHint = useMemo(() => {
    if (urlMode === 'guided') return guidedUrl.host.trim() === '0.0.0.0';
    try { return new URL(rawUrl.trim()).hostname === '0.0.0.0'; } catch { return false; }
  }, [urlMode, guidedUrl, rawUrl]);

  const { data, error, isLoading, refresh } = usePolling({
    fetcher: async () => {
      const api = getApi();
      const { data, error } = await api.GET('/state');
      if (error) throw error;
      return data!.data;
    },
    intervalMs: settings.statePollMs,
  });

  const services = useMemo(() => data?.services ?? [], [data?.services]);

  // Duplicate URL detection: is this URL already registered by another service?
  const duplicateUrlService = useMemo(() => {
    if (!baseUrl || !urlValid) return null;
    return findDuplicateServiceUrl(baseUrl, services);
  }, [baseUrl, urlValid, services]);

  const handleAddService = useCallback(async () => {
    if (!addForm.id.trim() || !baseUrl || !addForm.token.trim() || !urlValid) return;
    if (selfUrlWarning) return; // blocked by self-URL check
    setAddLoading(true);
    setAddError(null);

    try {
      // Add the service via the controller API
      setAddPhase('Adding service…');
      const api = getApi();
      const { error } = await api.PUT('/services/new', {
        body: {
          id: addForm.id.trim(),
          base_url: baseUrl,
          token: addForm.token.trim(),
          name: addForm.name.trim() || null,
          check_health_before_adding: addForm.check_health,
        },
      });
      if (error) {
        const env = error as unknown as ErrorEnvelope;
        setAddError({
          status: env.status ?? 'error',
          code: env.code ?? 0,
          title: env.title ?? 'Failed to add service',
          detail: env.detail,
          error: env.error,
          instance: env.instance,
        });
        return;
      }
      setShowAdd(false);
      setAddForm({ id: '', token: '', name: '', check_health: true });
      setGuidedUrl({ protocol: 'http', host: '', port: '' });
      setRawUrl('');
      showToast('Service added successfully');
      refresh();
    } catch (e) {
      setAddError({ status: 'error', code: 0, title: 'Network error', error: String(e) });
    } finally {
      setAddLoading(false);
      setAddPhase('');
    }
  }, [addForm, baseUrl, urlValid, selfUrlWarning, refresh, showToast]);

  if (isLoading && !data) {
    return <div className="page-center"><LoadingSpinner label="Loading services…" /></div>;
  }

  if (error && !data) {
    return <div className="page-padding"><ErrorDisplay error={error as Error} onRetry={refresh} /></div>;
  }



  return (
    <div className="services-page">
      <div className="flex items-start justify-between mb-6">
        <div>
          <h1 className="services-page__title">Services</h1>
          <p className="services-page__subtitle">{services.length} filter{services.length !== 1 ? 's' : ''} registered</p>
        </div>
        <button className="btn btn--primary" onClick={() => setShowAdd(true)}>
          + Add Service
        </button>
      </div>

      {services.length === 0 ? (
        <EmptyState
          icon={<Server size={32} />}
          title="No services found"
          description="It looks like there are no services registered yet."
          action={<button className="btn btn--primary" onClick={() => setShowAdd(true)}>Add Service</button>}
        />
      ) : (
        <div className="services-page__grid">
          {services.map(svc => (
            <div
              key={svc.id}
              className="service-card"
              onClick={() => navigate(`/services/${svc.id}`)}
              style={{ borderTopColor: svc.metadata?.hex_color ?? 'var(--color-border)' }}
            >
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2 min-w-0">
                  <ColorBar color={svc.metadata?.hex_color} />
                  <span className="service-card__name">{svc.metadata?.name || svc.id}</span>
                </div>
                <StatusDot status={getHealthStatus(svc.health)} />
              </div>
              <div className="service-card__url">{svc.base_url}</div>
              <div className="flex items-center gap-3">
                <Badge variant="default">{svc.ports.length} port{svc.ports.length !== 1 ? 's' : ''}</Badge>
                <TimeSince ns={getLastSyncNs(svc.health)} />
              </div>
              {svc.ports.length > 0 && (
                <div className="flex flex-wrap gap-1 mt-3">
                  {[...svc.ports].sort((a, b) => a.port - b.port).map(p => (
                    <span
                      key={p.port}
                      className="service-card__port-badge"
                      style={{ borderColor: p.snapshot?.info?.hex_color ?? 'var(--color-border)' }}
                      onClick={e => { e.stopPropagation(); navigate(`/services/${svc.id}/ports/${p.port}`); }}
                      role="link"
                      tabIndex={0}
                      onKeyDown={e => { if (e.key === 'Enter') { e.stopPropagation(); navigate(`/services/${svc.id}/ports/${p.port}`); } }}
                    >
                      :{p.port}{p.snapshot?.info ? ` -> :${p.snapshot.info.upstream}` : ''}
                    </span>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Add Service Modal */}
      <Modal open={showAdd} onClose={() => setShowAdd(false)} title="Add Service" size="md">
        <div className="form-stack">
          <div className="form-field">
            <label className="form-label">Service ID *</label>
            <input value={addForm.id} onChange={e => setAddForm(f => ({ ...f, id: e.target.value }))} placeholder="e.g. web-01" />
            <span className="form-hint">Unique, stable identifier. Cannot be changed later.</span>
          </div>

          {/* Dual-mode URL input */}
          <div className="form-field">
            <div className="form-label-row">
              <label className="form-label">Filter URL *</label>
              <button
                type="button"
                className="form-mode-toggle"
                onClick={() => setUrlMode(m => m === 'guided' ? 'raw' : 'guided')}
              >
                {urlMode === 'guided' ? 'Switch to raw URL' : 'Switch to guided mode'}
              </button>
            </div>

            {urlMode === 'guided' ? (
              <div className="url-guided">
                <select
                  className="url-guided__protocol"
                  value={guidedUrl.protocol}
                  onChange={e => setGuidedUrl(u => ({ ...u, protocol: e.target.value }))}
                >
                  <option value="http">http://</option>
                  <option value="https">https://</option>
                </select>
                <input
                  className="url-guided__host"
                  value={guidedUrl.host}
                  onChange={e => setGuidedUrl(u => ({ ...u, host: e.target.value }))}
                  placeholder="127.0.0.1 or hostname"
                />
                <span className="url-guided__sep">:</span>
                <input
                  className="url-guided__port"
                  type="number"
                  min={1}
                  max={65535}
                  value={guidedUrl.port}
                  onChange={e => setGuidedUrl(u => ({ ...u, port: e.target.value }))}
                  placeholder="7001"
                />
              </div>
            ) : (
              <input
                value={rawUrl}
                onChange={e => setRawUrl(e.target.value)}
                placeholder="http://127.0.0.1:7001"
              />
            )}

            {zeroAddrHint && (
              <span className="form-hint form-hint--error">🛑 0.0.0.0 is not a valid target — browsers block CORS to it. Use 127.0.0.1 or localhost instead.</span>
            )}
            {baseUrl && !urlValid && !zeroAddrHint && (
              <span className="form-hint form-hint--warn">⚠ Invalid URL — check host and port (1–65535)</span>
            )}
            {baseUrl && urlValid && !selfUrlWarning && !duplicateUrlService && (
              <span className="form-hint">-&gt; {baseUrl}</span>
            )}
            {selfUrlWarning && (
              <span className="form-hint form-hint--error">
                🛑 This URL matches the controller's own API address ({controllerApiUrl}).
                You are probably trying to register the controller itself as a filter — this is not allowed.
              </span>
            )}
            {duplicateUrlService && !selfUrlWarning && (
              <span className="form-hint form-hint--warn">
                ⚠ This URL is already registered by service "{duplicateUrlService}". Adding a duplicate may cause conflicts.
              </span>
            )}
          </div>

          <div className="form-field">
            <label className="form-label">Filter Token *</label>
            <input type="password" value={addForm.token} onChange={e => setAddForm(f => ({ ...f, token: e.target.value }))} placeholder="Filter's bearer token" />
            <span className="form-hint form-hint--warn">⚠ Write-once sensitive value. You cannot retrieve it later.</span>
          </div>
          <div className="form-field">
            <label className="form-label">Name (optional)</label>
            <input value={addForm.name} onChange={e => setAddForm(f => ({ ...f, name: e.target.value }))} placeholder="Human-friendly name" />
          </div>
          <label className="form-checkbox">
            <input type="checkbox" checked={addForm.check_health} onChange={e => setAddForm(f => ({ ...f, check_health: e.target.checked }))} />
            Verify filter health before adding
          </label>

          {addError && (
            <div className="add-error-detail">
              <div className="add-error-detail__title">{addError.title}</div>
              {addError.detail && <div className="add-error-detail__text">{addError.detail}</div>}
              {addError.error && <div className="add-error-detail__text add-error-detail__text--mono">{addError.error}</div>}
            </div>
          )}

          <div className="form-actions">
            <button className="btn btn--secondary" onClick={() => setShowAdd(false)}>Cancel</button>
            <button
              className="btn btn--primary"
              onClick={handleAddService}
              disabled={addLoading || !addForm.id.trim() || !baseUrl || !addForm.token.trim() || !urlValid || selfUrlWarning || zeroAddrHint}
            >
              {addLoading ? addPhase || 'Adding…' : 'Add Service'}
            </button>
          </div>
        </div>
      </Modal>
    </div>
  );
}
