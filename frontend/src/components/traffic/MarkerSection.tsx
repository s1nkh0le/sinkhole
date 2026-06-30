import { Activity } from 'lucide-react';
import type { TrafficFilterState, MarkerRegistry } from './trafficTypes';

// A single marker chip with +/- buttons to add it as a positive or negative
// filter, plus an optional tooltip listing the service:port locations it is
// shared across.
function MarkerPill({
  id, name, isPos, isNeg, toggle, sharedLocs, serviceNames, serviceColors, portColors
}: {
  id: string, name: string, isPos: boolean, isNeg: boolean,
  toggle: (id: string, list: 'positive' | 'negative') => void,
  sharedLocs?: Array<{service_id: string, port: number}>,
  serviceNames?: Record<string, string>,
  serviceColors?: Record<string, string>,
  portColors?: Record<number, string>
}) {
  return (
    <div className={`filter-marker-pill ${sharedLocs ? 'filter-marker-pill--has-tooltip' : ''}`}>
      <span className="filter-marker-pill__name">{name}</span>
      {sharedLocs && serviceNames && serviceColors && portColors && (
        <div className="filter-marker-pill__tooltip">
          <div className="filter-marker-pill__tooltip-title">Shared across:</div>
          {sharedLocs.map(loc => (
            <div key={`${loc.service_id}:${loc.port}`} className="filter-marker-pill__tooltip-row">
               <span className="filter-color-dot" style={{ background: serviceColors[loc.service_id] || 'var(--color-primary)' }} />
               {serviceNames[loc.service_id] || loc.service_id}
               <span style={{opacity: 0.5, margin: '0 4px'}}>:</span>
               <span className="filter-color-dot" style={{ background: portColors[loc.port] || 'var(--color-border)' }} />
               {loc.port}
            </div>
          ))}
        </div>
      )}
      <div className="filter-marker-pill__actions">
        <button className={`filter-marker-btn ${isPos ? 'filter-marker-btn--pos' : ''}`} onClick={() => toggle(id, 'positive')}>+</button>
        <button className={`filter-marker-btn ${isNeg ? 'filter-marker-btn--neg' : ''}`} onClick={() => toggle(id, 'negative')}>-</button>
      </div>
    </div>
  );
}

/**
 * The markers area of the traffic filter bar. In single-port mode it shows a
 * flat strip of marker chips; in broad mode it partitions markers into a
 * "Shared" group (markers seen in 2+ locations) plus a service -> port hierarchy
 * for the rest, so a large fleet stays navigable.
 */
export function MarkerSection({ filter, isSinglePort, markerRegistry, serviceNames, serviceColors, portColors, toggleMarker }: {
  filter: TrafficFilterState;
  isSinglePort: boolean;
  markerRegistry: MarkerRegistry;
  serviceNames: Record<string, string>;
  serviceColors: Record<string, string>;
  portColors: Record<number, string>;
  toggleMarker: (id: string, list: 'positive' | 'negative') => void;
}) {
  const { locations } = markerRegistry;

  // Scope markers to current filter
  const scopedIds = markerRegistry.allIds.filter(id => {
    const locs = locations[id] || [];
    return locs.some(loc => {
      if (filter.service_id && loc.service_id !== filter.service_id) return false;
      if (filter.port && loc.port !== parseInt(filter.port)) return false;
      return true;
    });
  });

  if (scopedIds.length === 0) {
    return (
      <div className="filter-group">
        <label className="filter-label"><Activity size={14}/> Markers</label>
        <span className="text-muted text-sm">No markers available</span>
      </div>
    );
  }

  // ── Mode 1: Single port → flat strip ──
  if (isSinglePort) {
    return (
      <div className="filter-group">
        <label className="filter-label"><Activity size={14}/> Markers</label>
        <div className="filter-marker-tags">
          {scopedIds.map(id => (
            <MarkerPill key={id} id={id} name={id}
              isPos={filter.positive_markers.includes(id)}
              isNeg={filter.negative_markers.includes(id)}
              toggle={toggleMarker} />
          ))}
        </div>
      </div>
    );
  }

  // ── Mode 2: Broad selection → hierarchical ──
  /**
   * Complex Marker Partitioning Logic:
   * When filtering globally (no specific port selected), we may have hundreds of markers
   * across different services.
   *
   * To prevent a messy flat list, we partition markers into two groups:
   * 1. Multi-markers ("Shared Markers"): Markers that appear in >= 2 locations.
   *    These are hoisted to the top because they likely represent global tracking (e.g. "auth_token_found").
   * 2. Single-markers: Markers specific to a single service:port combination.
   *    These are grouped visually by their specific Service -> Port hierarchy.
   */
  // Partition: multi-markers (same ID in ≥2 scoped locations) vs single-markers
  const multiIds: string[] = [];
  const multiIdsLocs = new Map<string, Array<{service_id: string, port: number}>>();
  const singleIds: string[] = [];
  for (const id of scopedIds) {
    const locs = (locations[id] || []).filter(loc => {
      if (filter.service_id && loc.service_id !== filter.service_id) return false;
      return true;
    });
    // Deduplicate by service_id+port
    const uniqueLocs = new Set<string>();
    const deduped: Array<{service_id: string, port: number}> = [];
    for (const l of locs) {
      const key = `${l.service_id}:${l.port}`;
      if (!uniqueLocs.has(key)) {
        uniqueLocs.add(key);
        deduped.push(l);
      }
    }

    if (deduped.length > 1) {
      multiIds.push(id);
      multiIdsLocs.set(id, deduped);
    } else {
      singleIds.push(id);
    }
  }

  // Group single-markers by service → port
  type PortGroup = { port: number; ids: string[] };
  type ServiceGroup = { service_id: string; ports: PortGroup[] };
  const serviceMap = new Map<string, Map<number, string[]>>();
  for (const id of singleIds) {
    const locs = (locations[id] || []).filter(loc => {
      if (filter.service_id && loc.service_id !== filter.service_id) return false;
      return true;
    });
    if (locs.length === 0) continue;
    const loc = locs[0]; // single location
    if (!serviceMap.has(loc.service_id)) serviceMap.set(loc.service_id, new Map());
    const portMap = serviceMap.get(loc.service_id)!;
    if (!portMap.has(loc.port)) portMap.set(loc.port, []);
    portMap.get(loc.port)!.push(id);
  }
  const serviceGroups: ServiceGroup[] = Array.from(serviceMap.entries()).map(([sid, portMap]) => ({
    service_id: sid,
    ports: Array.from(portMap.entries()).map(([port, ids]) => ({ port, ids })).sort((a, b) => a.port - b.port),
  }));

  return (
    <div className="filter-group">
      <label className="filter-label"><Activity size={14}/> Markers</label>
      <div className="filter-markers-hierarchy">
        {multiIds.length > 0 && (
          <div className="filter-markers-multi">
            <div className="filter-markers-multi__header">Shared Markers</div>
            <div className="filter-marker-tags">
              {multiIds.map(id => (
                <MarkerPill key={id} id={id} name={id}
                  isPos={filter.positive_markers.includes(id)}
                  isNeg={filter.negative_markers.includes(id)}
                  toggle={toggleMarker}
                  sharedLocs={multiIdsLocs.get(id)}
                  serviceNames={serviceNames}
                  serviceColors={serviceColors}
                  portColors={portColors}
                />
              ))}
            </div>
          </div>
        )}
        {serviceGroups.map(sg => (
          <div key={sg.service_id} className="filter-markers-service">
            <div className="filter-markers-service__header">
              <span className="filter-color-dot" style={{ background: serviceColors[sg.service_id] || 'var(--color-primary)' }} />
              {serviceNames[sg.service_id] || sg.service_id}
            </div>
            {sg.ports.map(pg => (
              <div key={pg.port} className="filter-markers-port">
                <div className="filter-markers-port__header">
                  <span className="filter-color-dot" style={{ background: portColors[pg.port] || 'var(--color-border)' }} />
                  :{pg.port}
                </div>
                <div className="filter-marker-tags">
                  {pg.ids.map(id => (
                    <MarkerPill key={id} id={id} name={id}
                      isPos={filter.positive_markers.includes(id)}
                      isNeg={filter.negative_markers.includes(id)}
                      toggle={toggleMarker} />
                  ))}
                </div>
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
