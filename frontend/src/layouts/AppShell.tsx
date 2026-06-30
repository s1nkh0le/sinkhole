import { useEffect, useRef, useState } from 'react';
import { Outlet, NavLink, useNavigate } from 'react-router-dom';
import { useAuth } from '../stores/AuthContext';
import { useSettings } from '../stores/SettingsContext';
import { useToast } from '../stores/ToastContext';
import { usePolling } from '../hooks/usePolling';
import { getApi } from '../api/client';
import { StatusDot } from '../components/StatusDot';
import { Badge } from '../components/Badge';
import { SyncProgress } from '../components/SyncProgress';
import type { SyncChannel } from '../components/SyncProgress';
import './AppShell.css';

type Health = { healthy: boolean; services_ok?: number; services_total?: number };

interface ServiceSnapshot {
  id: string;
  health: Record<string, unknown>;
  metadata?: { name?: string };
}

function getServiceHealthKey(health: Record<string, unknown>): 'healthy' | 'degraded' | 'offline' {
  if ('Healthy' in health) return 'healthy';
  if ('Degraded' in health) return 'degraded';
  return 'offline';
}

/**
 * Chrome for the authenticated app: sidebar navigation, the routed page
 * `<Outlet>`, and the global sync/health indicators. Polls `/health` and
 * `/state` for the overall fleet status shown in the sidebar, plus lightweight
 * liveness polls for the other resources feeding the SyncProgress bars.
 */
export function AppShell() {
  const { logout } = useAuth();
  const { settings } = useSettings();
  const { showToast } = useToast();
  const navigate = useNavigate();

  const { data: healthData, error: healthError, lastUpdated: healthUpdated } = usePolling({
    fetcher: async () => {
      const api = getApi();
      const { data } = await api.GET('/health');
      return data?.data as Health | undefined;
    },
    intervalMs: settings.healthPollMs,
  });

  const { data: stateData, error: stateError, lastUpdated: stateUpdated, refresh: refreshState } = usePolling({
    fetcher: async () => {
      const api = getApi();
      const { data } = await api.GET('/state');
      return data?.data;
    },
    intervalMs: settings.statePollMs,
  });

  // Lightweight polls for ECAM, Records, Config — just to track liveness
  const { error: ecamError, lastUpdated: ecamUpdated, refresh: refreshEcam } = usePolling({
    fetcher: async () => {
      const api = getApi();
      const { data } = await api.GET('/ecam');
      return data;
    },
    intervalMs: settings.ecamPollMs,
  });

  const [recordsUpdated, setRecordsUpdated] = useState<Date | null>(null);
  const [recordsError, setRecordsError] = useState<Error | null>(null);

  useEffect(() => {
    const handleRecordsUpdated = (e: CustomEvent) => {
      setRecordsUpdated(new Date(e.detail.timestamp));
      setRecordsError(e.detail.error);
    };
    window.addEventListener('records-updated', handleRecordsUpdated as EventListener);
    return () => window.removeEventListener('records-updated', handleRecordsUpdated as EventListener);
  }, []);

  const { error: configError, lastUpdated: configUpdated, refresh: refreshConfig } = usePolling({
    fetcher: async () => {
      const api = getApi();
      const { data } = await api.GET('/state');
      return data;
    },
    intervalMs: settings.statePollMs * 3,
  });

  const isDebug = stateData?.controller?.debug === true;
  // Controller is offline if health fetch errored OR the response says unhealthy
  const isOffline = !!healthError || (healthData != null && !healthData.healthy);

  // Track controller online/offline transitions — reset sync on recovery
  const prevOfflineRef = useRef(isOffline);
  useEffect(() => {
    const wasOffline = prevOfflineRef.current;
    prevOfflineRef.current = isOffline;

    if (wasOffline && !isOffline) {
      // Just came back online — refresh everything immediately
      showToast('Controller is back online', 'success');
      refreshState();
      refreshEcam();
      refreshConfig();
    } else if (!wasOffline && isOffline) {
      showToast('Controller is offline', 'error');
    }
  }, [isOffline, showToast, refreshState, refreshEcam, refreshConfig]);

  // Track individual service health transitions
  const prevServiceHealthRef = useRef<Record<string, string>>({});

  useEffect(() => {
    if (!stateData?.services) return;
    
    const newMap: Record<string, string> = {};
    for (const svc of stateData.services as ServiceSnapshot[]) {
      const status = getServiceHealthKey(svc.health);
      newMap[svc.id] = status;

      const prev = prevServiceHealthRef.current[svc.id];
      if (prev && prev !== status) {
        if (status === 'degraded' || status === 'offline') {
          showToast(`Service ${svc.id} is now ${status}`, 'warning');
        } else if (status === 'healthy') {
          showToast(`Service ${svc.id} is now healthy`, 'success');
        }
      }
    }
    prevServiceHealthRef.current = newMap;
  }, [stateData, showToast]);

  const syncChannels: SyncChannel[] = [
    { label: 'STA', intervalMs: settings.statePollMs, tick: stateUpdated?.getTime() ?? 0, hasError: isOffline || !!stateError, title: 'State polling' },
    { label: 'HLT', intervalMs: settings.healthPollMs, tick: healthUpdated?.getTime() ?? 0, hasError: isOffline || !!healthError, title: 'Health polling' },
    { label: 'ECM', intervalMs: settings.ecamPollMs, tick: ecamUpdated?.getTime() ?? 0, hasError: isOffline || !!ecamError, title: 'ECAM alerts polling' },
    { label: 'REC', intervalMs: settings.recordsPollMs, tick: recordsUpdated?.getTime() ?? 0, hasError: isOffline || !!recordsError, title: 'Traffic records polling' },
    { label: 'CFG', intervalMs: settings.statePollMs * 3, tick: configUpdated?.getTime() ?? 0, hasError: isOffline || !!configError, title: 'Configuration polling' },
  ];

  const handleLogout = () => {
    logout();
    navigate('/login');
  };

  return (
    <div className="app-shell">
      <nav className="app-shell__sidebar">
        <div className="app-shell__logo">
          <img
            // public/ asset: prefix with the Vite base (`/ui/` when embedded in the
            // controller, `/` when dev-served) so it resolves under either mount.
            // Vite can't rewrite this string literal the way it does index.html links.
            src={`${import.meta.env.BASE_URL}${isDebug ? 'icon-192-debug.png' : 'icon-192.png'}`}
            alt="Sinkhole"
            className="app-shell__logo-icon"
          />
          <span className="app-shell__logo-text">sinkhole</span>
          {isDebug && <Badge variant="warning">DEBUG</Badge>}
        </div>

        <div className="app-shell__nav">
          <NavLink to="/services" className={({ isActive }) => `app-shell__nav-item ${isActive ? 'app-shell__nav-item--active' : ''}`}>
            <svg className="app-shell__nav-icon" viewBox="0 0 16 16" fill="none"><rect x="1" y="1" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.5"/><rect x="9" y="1" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.5"/><rect x="1" y="9" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.5"/><rect x="9" y="9" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.5"/></svg>
            Services
          </NavLink>
          <NavLink to="/traffic" className={({ isActive }) => `app-shell__nav-item ${isActive ? 'app-shell__nav-item--active' : ''}`}>
            <svg className="app-shell__nav-icon" viewBox="0 0 16 16" fill="none"><path d="M1 12L4 5L7 8L10 3L15 10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/><line x1="1" y1="14" x2="15" y2="14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
            Traffic
          </NavLink>
          <NavLink to="/ecam" className={({ isActive }) => `app-shell__nav-item ${isActive ? 'app-shell__nav-item--active' : ''}`}>
            <svg className="app-shell__nav-icon" viewBox="0 0 16 16" fill="none"><path d="M8 1L14 13H2L8 1Z" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round"/><line x1="8" y1="6" x2="8" y2="9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/><circle cx="8" cy="11" r="0.5" fill="currentColor"/></svg>
            ECAM
          </NavLink>
          <NavLink to="/settings" className={({ isActive }) => `app-shell__nav-item ${isActive ? 'app-shell__nav-item--active' : ''}`}>
            <svg className="app-shell__nav-icon" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="3" stroke="currentColor" strokeWidth="1.5"/><path d="M8 1v2M8 13v2M1 8h2M13 8h2M3.05 3.05l1.41 1.41M11.54 11.54l1.41 1.41M3.05 12.95l1.41-1.41M11.54 4.46l1.41-1.41" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
            Settings
          </NavLink>
        </div>

        <div className="app-shell__sidebar-footer">
          {stateData?.services && (stateData.services as ServiceSnapshot[]).length > 0 && (
            <div className="app-shell__services-list" style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
              {(stateData.services as ServiceSnapshot[]).map(svc => (
                <div key={svc.id} className="app-shell__health" style={{ padding: 0, gap: 'var(--space-3)' }}>
                  <StatusDot status={getServiceHealthKey(svc.health)} size="sm" />
                  <span className="app-shell__health-label" style={{ fontSize: 'var(--text-xs)' }}>
                    {svc.metadata?.name || svc.id}
                  </span>
                </div>
              ))}
              <div style={{ height: '1px', background: 'var(--color-border-subtle)', margin: 'var(--space-2) calc(-1 * var(--space-4))' }} />
            </div>
          )}
          <div className="app-shell__health">
            <StatusDot status={isOffline ? 'offline' : 'healthy'} size="sm" />
            <span className="app-shell__health-label">Controller</span>
          </div>
          <SyncProgress channels={syncChannels} />
          <button className="app-shell__logout" onClick={handleLogout}>
            Sign Out
          </button>
        </div>
      </nav>

      <main className="app-shell__content">
        <Outlet />
      </main>
    </div>
  );
}
