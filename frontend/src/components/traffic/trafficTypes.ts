// Shared types for the Traffic Analyzer query bar and its consumers.

/** The full set of traffic-query inputs, all stored as strings for form binding. */
export interface TrafficFilterState {
  service_id: string;
  port: string;
  decision: '' | 'allow' | 'drop';
  method: string;
  path_like: string;
  limit: number;
  order: 'desc' | 'asc';
  since: string;
  until: string;
  time_relative: string;
  status_min: string;
  status_max: string;
  status_eq: string;
  len_min: string;
  len_max: string;
  len_eq: string;
  conn_id_min: string;
  conn_id_max: string;
  conn_id_eq: string;
  positive_markers: string[];
  negative_markers: string[];
  msg_count_min: string;
  msg_count_max: string;
  msg_count_eq: string;
  protocol: string;
}

/** A single place (service + public port) where a marker is configured. */
export interface MarkerLocation { service_id: string; port: number; }

/**
 * Registry of all markers known across the fleet, derived from the live state.
 */
export interface MarkerRegistry {
  /** Map of marker IDs to their human-readable names */
  names: Record<string, string>;
  /** Map of marker IDs to the specific services and ports where they are configured */
  locations: Record<string, MarkerLocation[]>;
  /** A flat array of all known marker IDs */
  allIds: string[];
}
