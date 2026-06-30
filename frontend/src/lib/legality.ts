import type { Scope, MatcherType, RuleDirection, Mode } from '../api/types';

/**
 * Scope × Matcher legality matrix.
 * Source of truth: filter/src/rules/legality.rs
 */
const SCOPE_MATCHER_LEGALITY: Record<Scope, Set<MatcherType>> = {
  'stream':      new Set(['stream_regex', 'stream_bytes', 'bytes_hex', 'size', 'always']),
  'http.path':   new Set(['path_regex', 'size', 'always']),
  'http.body':   new Set(['body_regex', 'body_bytes', 'bytes_hex', 'size', 'always']),
  'http.header': new Set(['header_regex', 'size', 'always']),
  'http.method': new Set(['method_regex', 'always']),
  'http.status': new Set(['status_regex', 'always']),
  'timing':      new Set(['session.lifetime', 'session.idle', 'always']),
};

/**
 * Returns the set of legal matcher types for a given scope.
 */
export function legalMatchersForScope(scope: Scope): MatcherType[] {
  return Array.from(SCOPE_MATCHER_LEGALITY[scope] ?? []);
}

/**
 * Checks if a (scope, matcherType) combination is legal.
 */
export function isMatcherLegalForScope(scope: Scope, matcherType: MatcherType): boolean {
  return SCOPE_MATCHER_LEGALITY[scope]?.has(matcherType) ?? false;
}

/**
 * Returns legal directions for a given scope.
 * - http.status: 'outbound' or 'both' only (never 'inbound')
 * - timing: 'both' only
 * - everything else: all three
 */
export function legalDirectionsForScope(scope: Scope): RuleDirection[] {
  switch (scope) {
    case 'http.status': return ['outbound', 'both'];
    case 'timing': return ['both'];
    default: return ['inbound', 'outbound', 'both'];
  }
}

/**
 * Checks if a (scope, direction) combination is legal.
 */
export function isDirectionLegalForScope(scope: Scope, direction: RuleDirection): boolean {
  return legalDirectionsForScope(scope).includes(direction);
}

/** All possible scopes */
export const ALL_SCOPES: Scope[] = [
  'stream', 'http.path', 'http.body', 'http.header',
  'http.method', 'http.status', 'timing',
];

/** All possible matcher types */
export const ALL_MATCHER_TYPES: MatcherType[] = [
  'stream_regex', 'path_regex', 'body_regex', 'header_regex',
  'method_regex', 'status_regex', 'size',
  'session.lifetime', 'session.idle',
  'stream_bytes', 'body_bytes', 'bytes_hex', 'always',
];

/** Human-readable labels for matcher types */
export const MATCHER_TYPE_LABELS: Record<MatcherType, string> = {
  'stream_regex': 'Stream Regex',
  'path_regex': 'Path Regex',
  'body_regex': 'Body Regex',
  'header_regex': 'Header Regex',
  'method_regex': 'Method Regex',
  'status_regex': 'Status Regex',
  'size': 'Size',
  'session.lifetime': 'Session Lifetime',
  'session.idle': 'Session Idle',
  'stream_bytes': 'Stream Bytes',
  'body_bytes': 'Body Bytes',
  'bytes_hex': 'Hex Signature',
  'always': 'Always',
};

/** Human-readable labels for scopes */
export const SCOPE_LABELS: Record<Scope, string> = {
  'stream': 'Stream',
  'http.path': 'HTTP Path',
  'http.body': 'HTTP Body',
  'http.header': 'HTTP Header',
  'http.method': 'HTTP Method',
  'http.status': 'HTTP Status',
  'timing': 'Timing',
};

/** Human-readable labels for directions */
export const DIRECTION_LABELS: Record<RuleDirection, string> = {
  'inbound': 'Inbound',
  'outbound': 'Outbound',
  'both': 'Both',
};

/** Human-readable labels for modes */
export const MODE_LABELS: Record<Mode, string> = {
  'blacklist': 'Blacklist',
  'whitelistall': 'Whitelist All',
  'whitelistany': 'Whitelist Any',
};
