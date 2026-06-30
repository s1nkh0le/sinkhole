import { useState, useCallback, useMemo, useEffect } from 'react';
import { getApi } from '../api/client';
import { usePolling } from '../hooks/usePolling';
import { useSettings } from '../stores/SettingsContext';
import { Badge } from '../components/Badge';
import { TimeSince } from '../components/TimeSince';
import { LoadingSpinner } from '../components/LoadingSpinner';
import { EmptyState } from '../components/EmptyState';
import { ErrorDisplay } from '../components/ErrorDisplay';
import { Activity, MousePointerClick, Download } from 'lucide-react';
import { Modal } from '../components/Modal';
import { ExportModal } from '../components/ExportModal';
import { formatByteSize } from '../lib/bytes';
import { TrafficFilterBar } from '../components/TrafficFilterBar';
import { MessageItem } from '../components/traffic/MessageItem';
import { RecordDetail } from '../components/traffic/RecordDetail';
import type { TrafficFilterState } from '../components/TrafficFilterBar';
import type { RecordGroup, EvaluationRecord, CtrlReplyBodyServiceState } from '../api/types';
import './TrafficAnalyzerPage.css';

// Filter State (auto-applies and persists)
const defaultFilter: TrafficFilterState = {
  service_id: '', port: '', decision: '',
  method: '', path_like: '', limit: 100,
  order: 'desc', since: '', until: '', status_min: '', status_max: '',
  time_relative: '', status_eq: '',
  len_min: '', len_max: '', len_eq: '',
  conn_id_min: '', conn_id_max: '', conn_id_eq: '',
  positive_markers: [], negative_markers: [],
  msg_count_min: '', msg_count_max: '', msg_count_eq: '',
  protocol: ''
};

/**
 * Cross-fleet traffic explorer: drives the TrafficFilterBar query, lists the
 * matching connections/records, and shows a selected message's evaluation
 * detail and raw bytes (hex or text), with export to a replay script. The active
 * filter is persisted in sessionStorage so it survives navigation within a session.
 */
export function TrafficAnalyzerPage() {
  const { settings } = useSettings();
  const [selectedGroup, setSelectedGroup] = useState<RecordGroup | null>(null);
  const [selectedRecord, setSelectedRecord] = useState<EvaluationRecord | null>(null);
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [rawView, setRawView] = useState<'hex' | 'text'>('hex');
  const [groupsDataSize, setGroupsDataSize] = useState(0);

  const [filter, setFilterState] = useState<TrafficFilterState>(() => {
    try {
      const stored = sessionStorage.getItem('sinkhole_traffic_filter');
      if (stored) {
        const parsed = JSON.parse(stored);
        return { ...defaultFilter, ...parsed };
      }
    } catch { /* ignore */ }
    return defaultFilter;
  });

  const setFilter = useCallback((patch: Partial<TrafficFilterState>) => {
    setFilterState(prev => {
      const next = { ...prev, ...patch };
      sessionStorage.setItem('sinkhole_traffic_filter', JSON.stringify(next));
      return next;
    });
  }, []);

  const resetFilter = useCallback(() => {
    sessionStorage.removeItem('sinkhole_traffic_filter');
    setFilterState(defaultFilter);
  }, []);

  // ── Fetch /state for service list, marker names, known rule IDs ──
  const { data: stateData } = usePolling({
    fetcher: async () => {
      const api = getApi();
      const { data } = await api.GET('/state');
      return data?.data;
    },
    intervalMs: settings.statePollMs * 3,
  });

  const services: CtrlReplyBodyServiceState[] = useMemo(() => stateData?.services ?? [], [stateData?.services]);
  const serviceNames: Record<string, string> = {};
  const serviceColors: Record<string, string> = {};
  const portColors: Record<number, string> = {};

  services.forEach(s => {
    serviceNames[s.id] = s.metadata?.name || s.id;
    serviceColors[s.id] = s.metadata?.hex_color || 'var(--color-primary)';
    s.ports.forEach(p => {
      if (p.snapshot?.info?.hex_color) {
        portColors[p.port] = p.snapshot.info.hex_color;
      }
    });
  });

  // Build marker registry from /state snapshots
  const { knownRuleIds, allMarkerIds, markerRegistry } = useMemo(() => {
    const mn: Record<string, string> = {};
    const markerIds = new Set<string>();
    const allRuleIds = new Set<string>();
    // One-to-many: each marker ID → all service/port combos it appears in
    const locations: Record<string, { service_id: string, port: number }[]> = {};
    services.forEach(s => {
      s.ports.forEach(p => {
        const rs = p.snapshot?.ruleset;
        if (!rs) return;
        rs.markers.forEach(m => {
          mn[m.id] = m.name || m.id;
          markerIds.add(m.id);
          allRuleIds.add(m.id);
          if (!locations[m.id]) locations[m.id] = [];
          locations[m.id].push({ service_id: s.id, port: p.port });
        });
        rs.filters.forEach(f => {
          allRuleIds.add(f.id);
        });
      });
    });
    return {
      markerNames: mn,
      knownRuleIds: allRuleIds,
      allMarkerIds: Array.from(markerIds),
      markerRegistry: { names: mn, locations, allIds: Array.from(markerIds) },
    };
  }, [services]);

  // ── Fetch records ──
  const buildQuery = useCallback(() => {
    const q: Record<string, unknown> = { limit: filter.limit, order: filter.order };

    // --- Core Routing ---
    if (filter.service_id) q.service_id = filter.service_id;
    if (filter.port) q.port = parseInt(filter.port);

    // Connection ID boundaries (only valid if service and port are selected)
    if (filter.service_id && filter.port) {
      if (filter.conn_id_min) q.conn_id_min = parseInt(filter.conn_id_min);
      if (filter.conn_id_max) q.conn_id_max = parseInt(filter.conn_id_max);
      if (filter.conn_id_eq) q.conn_id = parseInt(filter.conn_id_eq);
    }

    // --- Content Filtering ---
    if (filter.decision) q.decision = filter.decision;
    if (filter.protocol) q.protocol = filter.protocol;
    if (filter.method) q.method = filter.method;
    if (filter.path_like) q.path_like = filter.path_like;

    // HTTP Status
    if (filter.status_min) q.status_min = parseInt(filter.status_min);
    if (filter.status_max) q.status_max = parseInt(filter.status_max);
    if (filter.status_eq) q.status_eq = parseInt(filter.status_eq);

    // Payload Length
    if (filter.len_min) q.len_min = parseInt(filter.len_min);
    if (filter.len_max) q.len_max = parseInt(filter.len_max);
    if (filter.len_eq) q.len = parseInt(filter.len_eq);

    // Message Counts
    if (filter.msg_count_min) q.msg_count_min = parseInt(filter.msg_count_min);
    if (filter.msg_count_max) q.msg_count_max = parseInt(filter.msg_count_max);
    if (filter.msg_count_eq) q.msg_count = parseInt(filter.msg_count_eq);

    // --- Marker Processing ---
    let validPos = filter.positive_markers;
    let validNeg = filter.negative_markers;

    // Only prune stale marker IDs if we have loaded the state at least once
    if (stateData) {
      const validMarkerSet = new Set(allMarkerIds);
      validPos = validPos.filter(id => validMarkerSet.has(id));
      validNeg = validNeg.filter(id => validMarkerSet.has(id));
    }

    if (validPos.length > 0) q.positive_markers = validPos;
    if (validNeg.length > 0) q.negative_markers = validNeg;

    // --- Time Boundaries ---
    if (filter.time_relative) {
      const match = filter.time_relative.match(/^(\d+)([mhd])$/);
      if (match) {
        const amt = parseInt(match[1]);
        const unit = match[2];
        let ms = 0;
        if (unit === 'm') ms = amt * 60 * 1000;
        if (unit === 'h') ms = amt * 60 * 60 * 1000;
        if (unit === 'd') ms = amt * 24 * 60 * 60 * 1000;
        if (ms > 0) {
          q.since = (Date.now() - ms) * 1000000; // API expects nanoseconds
        }
      }
    } else {
      if (filter.since) q.since = new Date(filter.since).getTime() * 1000000;
      if (filter.until) q.until = new Date(filter.until).getTime() * 1000000;
    }

    return q;
  }, [filter, allMarkerIds, stateData]);

  const { data: groupsData, error, isLoading, refresh } = usePolling({
    fetcher: async (opts) => {
      const api = getApi();
      const { data, error } = await api.GET('/records/grouped', {
        params: { query: buildQuery() as Record<string, unknown> },
        signal: opts?.signal,
      });
      if (error) throw error;

      // Notify AppShell's REC indicator that records were successfully fetched
      window.dispatchEvent(new CustomEvent('records-updated', {
        detail: { timestamp: Date.now(), error: null }
      }));
      setGroupsDataSize(new Blob([JSON.stringify(data!.data)]).size);
      return data!.data;
    },
    intervalMs: settings.recordsPollMs,
    enabled: !!stateData, // Wait for state to load before querying records
  });

  // Also dispatch if error changes (to keep REC indicator accurate)
  useEffect(() => {
    if (error) {
      window.dispatchEvent(new CustomEvent('records-updated', {
        detail: { timestamp: Date.now(), error }
      }));
    }
  }, [error]);

  // Keyboard shortcut: Shift+Enter to refresh
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Enter' && e.shiftKey) {
        refresh();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [refresh]);

  // Sort groups: latest first (descending by last_ts_ns)
  const groups = useMemo(() => {
    const gs = [...(groupsData?.groups ?? [])];
    gs.sort((a, b) => b.last_ts_ns - a.last_ts_ns);
    return gs;
  }, [groupsData]);

  const [observedMethods, setObservedMethods] = useState<Set<string>>(new Set(['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'OPTIONS', 'HEAD']));
  const [prevGroupsData, setPrevGroupsData] = useState(groupsData);
  if (groupsData !== prevGroupsData) {
    setPrevGroupsData(groupsData);
    if (groupsData?.groups) {
      let added = false;
      const newMethods = new Set(observedMethods);
      groupsData.groups.forEach(g => {
        g.records.forEach(r => {
          if (r.http?.method && !newMethods.has(r.http.method)) {
            newMethods.add(r.http.method);
            added = true;
          }
        });
      });
      if (added) {
        setObservedMethods(newMethods);
      }
    }
  }

  const derivedMethods = useMemo(() => Array.from(observedMethods).sort(), [observedMethods]);

  // Helper: get union of all markers across records in a group
  const getGroupMarkers = (group: RecordGroup): string[] => {
    const markers = new Set<string>();
    group.records.forEach(r => r.markers.forEach(m => markers.add(m)));
    return Array.from(markers);
  };

  // Calculate selected group total raw payload size without recalculating on every re-render
  const selectedGroupTotalBytes = useMemo(() => {
    if (!selectedGroup) return 0;
    return selectedGroup.records.reduce((acc, rec) => {
      return acc + (rec.raw_b64 ? atob(rec.raw_b64).length : 0);
    }, 0);
  }, [selectedGroup]);



  return (
    <div className="traffic-page flex flex-col">
      <TrafficFilterBar
        filter={filter}
        onChange={setFilter}
        onReset={resetFilter}
        onRefresh={refresh}
        services={services}
        serviceNames={serviceNames}
        serviceColors={serviceColors}
        portColors={portColors}
        markerRegistry={markerRegistry}
        derivedMethods={derivedMethods}
      />

      <div className="traffic-page__body flex flex-1">
        {/* Left panel: connections */}
        <div className="traffic-page__left flex flex-col shrink-0">
          <div className="traffic-page__left-header flex items-center justify-between">
            <span className="traffic-page__left-title">Connections</span>
            <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
              {groupsDataSize > 0 && <Badge variant="default">{formatByteSize(groupsDataSize)}</Badge>}
              <Badge variant="default">{groups.length}</Badge>
            </div>
          </div>
          {isLoading && !groupsData && <div style={{ padding: 'var(--space-4)' }}><LoadingSpinner size="sm" /></div>}
          {!!error && !groupsData && <div style={{ padding: 'var(--space-3)' }}><ErrorDisplay error={error as Error} onRetry={refresh} variant={(error as { code?: number })?.code === 400 ? 'warning' : 'error'} /></div>}
          {!isLoading && groups.length === 0 && <EmptyState icon={<Activity size={32} />} title="No connections" description="No traffic matching your filters." />}
          <div className="connection-list">
            {groups.map(group => {
              const firstRec = group.records[0];
              const hasDrops = group.records.some(r => r.decision === 'drop');
              const isActive = selectedGroup?.conn_id === group.conn_id && selectedGroup?.service_id === group.service_id;
              const groupMarkers = getGroupMarkers(group as RecordGroup);
              const isUnknown = !group.service_id || !serviceNames[group.service_id];
              const svcColor = isUnknown ? '#eab308' : (serviceColors[group.service_id] || 'var(--color-primary)');
              const portColor = isUnknown ? '#ca8a04' : (portColors[group.port] || 'var(--color-primary)');

              const bgStyle = isUnknown
                ? { backgroundImage: 'repeating-linear-gradient(45deg, #3f2305, #3f2305 15px, #241403 15px, #241403 30px)' }
                : (isActive ? { backgroundColor: `${svcColor}15` } : {});

              return (
                <div
                  key={`${group.service_id}_${group.port}_${group.conn_id}`}
                  className={`connection-item ${isActive ? 'connection-item--active' : ''}`}
                  style={{ borderLeftColor: svcColor, ...bgStyle }}
                  onClick={() => { setSelectedGroup(group as RecordGroup); setSelectedRecord(null); }}
                >
                  <div className="connection-item__top flex items-center justify-between">
                    <span className="connection-item__ip">{firstRec?.src_ip}:{firstRec?.src_port}</span>
                    <Badge variant={hasDrops ? 'error' : 'success'} size="sm">{hasDrops ? 'DROP' : 'ALLOW'}</Badge>
                  </div>
                  <div className="connection-item__bottom">
                    <span className="connection-item__service" style={{
                      background: `linear-gradient(to right, ${svcColor} 50%, ${portColor} 50%)`,
                      color: '#fff',
                      padding: '2px 8px',
                      borderRadius: 'var(--radius-full)',
                      fontSize: '0.7rem',
                      fontWeight: 600,
                      display: 'inline-block',
                      textShadow: '0 0 2px rgba(0,0,0,0.5)'
                    }}>
                      {isUnknown ? 'UNKNOWN' : serviceNames[group.service_id]}:{group.port}
                    </span>
                    <span className="connection-item__connid">#{group.conn_id}</span>
                    <span className="connection-item__count">{group.records.length} msg{group.records.length !== 1 ? 's' : ''}</span>
                    <div className="ml-auto text-xs text-muted"><TimeSince ns={group.last_ts_ns} /></div>
                  </div>
                  {groupMarkers.length > 0 && (
                    <div className="connection-item__markers flex flex-wrap">
                      {groupMarkers.map(m => (
                        <span key={m} className="badge badge--sm" style={{ backgroundColor: `${svcColor}30`, color: svcColor, borderColor: `${svcColor}50` }}>{m}</span>
                      ))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>

        {/* Right panel: full request/response viewer */}
        <div className="traffic-page__right flex-1 flex flex-col">
          {!selectedGroup ? (
            <EmptyState icon={<MousePointerClick size={32} />} title="Select a connection" description="Click a connection on the left to view its messages." />
          ) : (
            <div className="messages-panel flex flex-col flex-1">
              <div className="messages-panel__header flex items-center gap-2">
                <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)' }}>
                  <span className="messages-panel__title">
                    Connection #{selectedGroup.conn_id}
                  </span>
                  <Badge variant="default">{selectedGroup.records.length} messages</Badge>
                  <Badge variant="default">{formatByteSize(selectedGroupTotalBytes)}</Badge>
                </div>
                <div style={{ marginLeft: 'auto' }}>
                  <button className="btn btn--secondary btn--sm" onClick={() => setExportModalOpen(true)}>
                    <Download size={14} /> Export
                  </button>
                </div>
              </div>
              <div className="messages-list">
                {selectedGroup.records.map((rec, idx) => (
                  <MessageItem
                    key={`${selectedGroup.conn_id}-${rec.message_seq}-${idx}`}
                    rec={rec}
                    selectedGroup={selectedGroup}
                    serviceColors={serviceColors}
                    setSelectedRecord={setSelectedRecord}
                  />
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Record Detail Modal */}
      <Modal open={!!selectedRecord} onClose={() => setSelectedRecord(null)} title="Record Detail" size="xxl">
        {selectedRecord && (
          <RecordDetail
            record={selectedRecord}
            rawView={rawView}
            onToggleRawView={v => setRawView(v)}
            knownRuleIds={knownRuleIds}
          />
        )}
      </Modal>

      {/* Export Modal */}
      <ExportModal
        group={selectedGroup}
        open={exportModalOpen}
        onClose={() => setExportModalOpen(false)}
      />
    </div>
  );
}
