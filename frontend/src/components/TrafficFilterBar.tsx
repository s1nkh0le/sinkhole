import { useState } from 'react';
import { Filter, Server, Activity, Clock, ShieldAlert, Hash, Search } from 'lucide-react';
import { Badge } from './Badge';
import { FilterPopover } from './traffic/FilterPopover';
import { MarkerSection } from './traffic/MarkerSection';
import type { TrafficFilterState, MarkerRegistry } from './traffic/trafficTypes';
import type { CtrlReplyBodyServiceState } from '../api/types';
import './TrafficFilterBar.css';

// Re-exported so existing consumers can keep importing these from this module.
export type { TrafficFilterState, MarkerRegistry, MarkerLocation } from './traffic/trafficTypes';

/**
 * Props for the main TrafficFilterBar component which handles all
 * user input for traffic query generation.
 */
export interface TrafficFilterBarProps {
  filter: TrafficFilterState;
  onChange: (f: TrafficFilterState) => void;
  onRefresh: () => void;
  onReset: () => void;

  services: CtrlReplyBodyServiceState[];
  serviceNames: Record<string, string>;
  serviceColors: Record<string, string>;
  portColors: Record<number, string>;
  markerRegistry: MarkerRegistry;
  derivedMethods: string[];
}

/**
 * Query builder for the Traffic Analyzer. Edits the full TrafficFilter (service
 * and port selection, protocol, direction, decision, status/size/time ranges,
 * and positive/negative marker filters) and reports changes upward via
 * `onChange`. Purely a controlled editor: it holds only UI mode toggles
 * (exact vs range, relative vs explicit) and owns no fetched data itself.
 */
export function TrafficFilterBar({ filter, onChange, onReset, onRefresh, services, serviceNames, serviceColors, portColors, markerRegistry, derivedMethods }: TrafficFilterBarProps) {
  const [expanded, setExpanded] = useState(false);
  const [timeMode, setTimeMode] = useState<'relative' | 'explicit'>(filter.time_relative ? 'relative' : 'explicit');
  const [statusMode, setStatusMode] = useState<'exact' | 'range'>(filter.status_eq ? 'exact' : 'range');
  const [sizeMode, setSizeMode] = useState<'exact' | 'range'>(filter.len_eq ? 'exact' : 'range');

  const setFilter = (patch: Partial<TrafficFilterState>) => {
    onChange({ ...filter, ...patch });
  };

  // Compute available ports for the selected service
  const availablePorts = filter.service_id
    ? (services.find(s => s.id === filter.service_id)?.ports.map(p => p.port) || [])
    : Array.from(new Set(services.flatMap(s => s.ports.map(p => p.port)))).sort((a,b) => a-b);

  // Port auto-selects service: find the owning service for a given port
  const findServiceForPort = (port: number): string => {
    for (const s of services) {
      if (s.ports.some(p => p.port === port)) return s.id;
    }
    return '';
  };

  // Determine if we're in single-port mode (flat) or broad mode (hierarchical)
  const isSinglePort = !!(filter.service_id && filter.port);

  const activeCount = [
    filter.service_id, filter.port, filter.decision, filter.method, filter.path_like,
    ...filter.positive_markers, ...filter.negative_markers, filter.since, filter.until, filter.time_relative,
    filter.status_min, filter.status_max, filter.status_eq, filter.len_min, filter.len_max, filter.len_eq,
    filter.conn_id_min, filter.conn_id_max, filter.conn_id_eq
  ].filter(v => !!v).length;

  const toggleMarker = (id: string, list: 'positive' | 'negative') => {
    const otherList = list === 'positive' ? 'negative_markers' : 'positive_markers';
    const targetList = list === 'positive' ? 'positive_markers' : 'negative_markers';
    const current = filter[targetList];
    const other = filter[otherList];

    if (current.includes(id)) {
      setFilter({ [targetList]: current.filter(x => x !== id) });
    } else {
      setFilter({
        [targetList]: [...current, id],
        [otherList]: other.filter(x => x !== id)
      });
    }
  };

  const [connMode, setConnMode] = useState<'exact' | 'range'>('exact');
  const [msgCountMode, setMsgCountMode] = useState<'exact' | 'range'>(
    (filter.msg_count_min || filter.msg_count_max) ? 'range' : 'exact'
  );

  return (
    <div className="traffic-filter-bar">
      <div className="flex items-center gap-4 py-3 px-4">
        <button className="btn btn--ghost btn--sm" onClick={() => setExpanded(!expanded)}>
          <Filter size={16} />
          Filters {activeCount > 0 && <Badge variant="info" size="sm">{activeCount}</Badge>}
        </button>

        <div className="flex items-center gap-2">
          <FilterPopover
            label={filter.service_id ? (serviceNames[filter.service_id] || filter.service_id) : 'All Services'}
            icon={<Server size={16} />}
            isActive={!!filter.service_id}
          >
            <div className="filter-list">
              <button
                className={`filter-list__item ${!filter.service_id ? 'active' : ''}`}
                onClick={() => setFilter({ service_id: '', port: '' })}
              >
                All Services
              </button>
              {services.map(s => {
                const color = s.metadata?.hex_color || 'var(--color-primary)';
                return (
                  <button
                    key={s.id}
                    className={`filter-list__item ${filter.service_id === s.id ? 'active' : ''}`}
                    onClick={() => setFilter({ service_id: s.id, port: '' })}
                  >
                    <span className="filter-color-dot" style={{ background: color }} />
                    {s.metadata?.name || s.id}
                  </button>
                );
              })}
            </div>
          </FilterPopover>

          <FilterPopover
            label={filter.port ? `Port: ${filter.port}` : 'All Ports'}
            icon={<Hash size={16} />}
            isActive={!!filter.port}
          >
            <div className="flex flex-col filter-list--grid" style={{ gap: 2 }}>
              <button
                className={`filter-list__item ${!filter.port ? 'active' : ''}`}
                onClick={() => setFilter({ port: '' })}
              >
                Any
              </button>
              {availablePorts.map(p => (
                <button
                  key={p}
                  className={`filter-list__item ${filter.port === String(p) ? 'active' : ''}`}
                  onClick={() => {
                    const svc = findServiceForPort(p);
                    setFilter({ port: String(p), ...(svc ? { service_id: svc } : {}) });
                  }}
                >
                  <span className="filter-color-dot" style={{ background: portColors[p] || 'var(--color-border)' }} />
                  :{p}
                </button>
              ))}
            </div>
          </FilterPopover>

          <FilterPopover
            label={filter.decision ? filter.decision.toUpperCase() : 'All Decisions'}
            icon={<ShieldAlert size={16} />}
            isActive={!!filter.decision}
          >
            <div className="flex flex-col" style={{ gap: 2 }}>
              <button className={`filter-list__item ${!filter.decision ? 'active-neutral' : ''}`} onClick={() => setFilter({ decision: '' })}>Any</button>
              <button className={`filter-list__item ${filter.decision === 'allow' ? 'active' : ''}`} onClick={() => setFilter({ decision: 'allow' })}>
                <Badge variant="success" size="sm">ALLOW</Badge>
              </button>
              <button className={`filter-list__item ${filter.decision === 'drop' ? 'active' : ''}`} onClick={() => setFilter({ decision: 'drop' })}>
                <Badge variant="error" size="sm">DROP</Badge>
              </button>
            </div>
          </FilterPopover>
        </div>

        <div className="flex-1" />

        <div className="flex items-center gap-2">
          <button className="btn btn--secondary btn--sm" onClick={onRefresh}>Refresh Traffic</button>
          <button className="btn btn--ghost btn--sm text-danger" onClick={onReset}>Clear Filters</button>
        </div>
      </div>

      {expanded && (
        <div className="traffic-filter-bar__expanded flex flex-col gap-4 px-4 pb-4">
          <MarkerSection
            filter={filter}
            isSinglePort={isSinglePort}
            markerRegistry={markerRegistry}
            serviceNames={serviceNames}
            serviceColors={serviceColors}
            portColors={portColors}
            toggleMarker={toggleMarker}
          />

          <div className="flex flex-col gap-4">
            <div className="filter-section">
              <div className="filter-section__header">Core Traffic Filters</div>
              <div className="filter-group-grid">
                <div className="filter-group">
                  <label className="filter-label">
                    <Clock size={14}/> Time Range
                    <div className="btn-group filter-label-toggle">
                      <button className={`btn btn--sm ${timeMode === 'explicit' ? 'btn--primary' : 'btn--ghost'}`} onClick={() => {
                        setTimeMode('explicit');
                        setFilter({ time_relative: '' });
                      }}>Explicit</button>
                      <button className={`btn btn--sm ${timeMode === 'relative' ? 'btn--primary' : 'btn--ghost'}`} onClick={() => {
                        setTimeMode('relative');
                        setFilter({ since: '', until: '', time_relative: '15m' });
                      }}>Relative</button>
                    </div>
                  </label>
                  <div className="filter-inputs">
                    {timeMode === 'relative' ? (() => {
                      const match = filter.time_relative.match(/^(\d+)([mhd])$/);
                      const amt = match ? match[1] : '15';
                      const unit = match ? match[2] : 'm';
                      return (
                        <>
                          <input
                            type="text" inputMode="numeric" pattern="[0-9]*"
                            className="form-input form-input--sm w-20"
                            value={amt}
                            onChange={e => setFilter({ time_relative: `${e.target.value}${unit}` })}
                          />
                          <select
                            className="form-input form-input--sm w-24"
                            value={unit}
                            onChange={e => setFilter({ time_relative: `${amt}${e.target.value}` })}
                          >
                            <option value="m">minutes</option>
                            <option value="h">hours</option>
                            <option value="d">days</option>
                          </select>
                        </>
                      );
                    })() : (
                      <>
                        <input type="datetime-local" className="form-input form-input--sm" value={filter.since} onChange={e => setFilter({ since: e.target.value })} />
                        <span className="text-muted">to</span>
                        <input type="datetime-local" className="form-input form-input--sm" value={filter.until} onChange={e => setFilter({ until: e.target.value })} />
                      </>
                    )}
                  </div>
                </div>

                <div className="filter-group" title={!isSinglePort ? "Select a service and port first" : undefined}>
                  <label className="filter-label" style={{ opacity: !isSinglePort ? 0.5 : 1 }}>
                    <span>Connection ID</span>
                    <div className="btn-group filter-label-toggle">
                      <button disabled={!isSinglePort} className={`btn btn--sm ${connMode === 'exact' ? 'btn--primary' : 'btn--ghost'}`} onClick={() => {
                        setConnMode('exact');
                        setFilter({ conn_id_min: '', conn_id_max: '' });
                      }}>Exact</button>
                      <button disabled={!isSinglePort} className={`btn btn--sm ${connMode === 'range' ? 'btn--primary' : 'btn--ghost'}`} onClick={() => {
                        setConnMode('range');
                        setFilter({ conn_id_eq: '' });
                      }}>Range</button>
                    </div>
                  </label>
                  <div className="filter-inputs" style={{ opacity: !isSinglePort ? 0.5 : 1 }}>
                    {connMode === 'exact' ? (
                      <input disabled={!isSinglePort} type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-20" placeholder="Exact ID" value={filter.conn_id_eq} onChange={e => setFilter({ conn_id_eq: e.target.value })} />
                    ) : (
                      <>
                        <input disabled={!isSinglePort} type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-20" placeholder="Min ID" value={filter.conn_id_min} onChange={e => setFilter({ conn_id_min: e.target.value })} />
                        <span className="text-muted">-</span>
                        <input disabled={!isSinglePort} type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-20" placeholder="Max ID" value={filter.conn_id_max} onChange={e => setFilter({ conn_id_max: e.target.value })} />
                      </>
                    )}
                  </div>
                </div>

                <div className="filter-group">
                  <label className="filter-label">
                    <span>Message Count</span>
                    <div className="btn-group filter-label-toggle">
                      <button className={`btn btn--sm ${msgCountMode === 'exact' ? 'btn--primary' : 'btn--ghost'}`} onClick={() => {
                        setMsgCountMode('exact');
                        setFilter({ msg_count_min: '', msg_count_max: '' });
                      }}>Exact</button>
                      <button className={`btn btn--sm ${msgCountMode === 'range' ? 'btn--primary' : 'btn--ghost'}`} onClick={() => {
                        setMsgCountMode('range');
                        setFilter({ msg_count_eq: '' });
                      }}>Range</button>
                    </div>
                  </label>
                  <div className="filter-inputs">
                    {msgCountMode === 'exact' ? (
                      <input type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-20" placeholder="Exact" value={filter.msg_count_eq} onChange={e => setFilter({ msg_count_eq: e.target.value })} />
                    ) : (
                      <>
                        <input type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-20" placeholder="Min" value={filter.msg_count_min} onChange={e => setFilter({ msg_count_min: e.target.value })} />
                        <span className="text-muted">-</span>
                        <input type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-20" placeholder="Max" value={filter.msg_count_max} onChange={e => setFilter({ msg_count_max: e.target.value })} />
                      </>
                    )}
                  </div>
                </div>

              </div>
            </div>

            <div className="filter-section">
              <div className="filter-section__header">Message Content &amp; Limits</div>
              <div className="filter-group-grid">

                <div className="filter-group">
                  <label className="filter-label"><Search size={14}/> HTTP Pattern</label>
                  <div className="filter-inputs">
                    <select className="form-input form-input--sm w-32" value={filter.method} onChange={e => setFilter({ method: e.target.value })}>
                      <option value="">Any Method</option>
                      {derivedMethods.map(m => <option key={m} value={m}>{m}</option>)}
                    </select>
                    <input type="text" className="form-input form-input--sm" placeholder="Path contains..." value={filter.path_like} onChange={e => setFilter({ path_like: e.target.value })} />
                  </div>
                </div>

                <div className="filter-group">
                  <label className="filter-label"><Activity size={14}/> Protocol</label>
                  <div className="filter-inputs">
                    <select className="form-input form-input--sm" value={filter.protocol} onChange={e => setFilter({ protocol: e.target.value })}>
                      <option value="">Any Protocol</option>
                      <option value="http/1.0">HTTP/1.0</option>
                      <option value="http/1.1">HTTP/1.1</option>
                      <option value="http/2">HTTP/2</option>
                      <option value="raw-ascii">Raw ASCII</option>
                      <option value="raw-binary">Raw Binary</option>
                    </select>
                  </div>
                </div>

            <div className="filter-group">
              <label className="filter-label">
                <span>Status Code</span>
                <div className="btn-group filter-label-toggle">
                  <button className={`btn btn--sm ${statusMode === 'exact' ? 'btn--primary' : 'btn--ghost'}`} onClick={() => {
                    setStatusMode('exact');
                    setFilter({ status_min: '', status_max: '' });
                  }}>Exact</button>
                  <button className={`btn btn--sm ${statusMode === 'range' ? 'btn--primary' : 'btn--ghost'}`} onClick={() => {
                    setStatusMode('range');
                    setFilter({ status_eq: '' });
                  }}>Range</button>
                </div>
              </label>
              <div className="filter-inputs">
                {statusMode === 'exact' ? (
                  <input type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-20" placeholder="Exact" value={filter.status_eq} onChange={e => setFilter({ status_eq: e.target.value })} />
                ) : (
                  <>
                    <input type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-20" placeholder="Min" value={filter.status_min} onChange={e => setFilter({ status_min: e.target.value })} />
                    <span className="text-muted">-</span>
                    <input type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-20" placeholder="Max" value={filter.status_max} onChange={e => setFilter({ status_max: e.target.value })} />
                  </>
                )}
              </div>
            </div>

            <div className="filter-group">
              <label className="filter-label">
                <span>Response Size</span>
                <div className="btn-group filter-label-toggle">
                  <button className={`btn btn--sm ${sizeMode === 'exact' ? 'btn--primary' : 'btn--ghost'}`} onClick={() => {
                    setSizeMode('exact');
                    setFilter({ len_min: '', len_max: '' });
                  }}>Exact</button>
                  <button className={`btn btn--sm ${sizeMode === 'range' ? 'btn--primary' : 'btn--ghost'}`} onClick={() => {
                    setSizeMode('range');
                    setFilter({ len_eq: '' });
                  }}>Range</button>
                </div>
              </label>
              <div className="filter-inputs">
                {sizeMode === 'exact' ? (
                  <input type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-24" placeholder="Exact bytes" value={filter.len_eq} onChange={e => setFilter({ len_eq: e.target.value })} />
                ) : (
                  <>
                    <input type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-24" placeholder="Min bytes" value={filter.len_min} onChange={e => setFilter({ len_min: e.target.value })} />
                    <span className="text-muted">-</span>
                    <input type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-24" placeholder="Max bytes" value={filter.len_max} onChange={e => setFilter({ len_max: e.target.value })} />
                  </>
                )}
              </div>
            </div>

            <div className="filter-group">
              <label className="filter-label">Limits</label>
              <div className="filter-inputs">
                <select className="form-input form-input--sm" value={filter.order} onChange={e => setFilter({ order: e.target.value as 'desc' | 'asc' })}>
                  <option value="desc">Newest First</option>
                  <option value="asc">Oldest First</option>
                </select>
                <input type="text" inputMode="numeric" pattern="[0-9]*" className="form-input form-input--sm w-20" value={filter.limit} onChange={e => setFilter({ limit: parseInt(e.target.value) || 100 })} />
              </div>
            </div>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
