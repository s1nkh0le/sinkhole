import type { MarkerDef, FilterDef } from '../api/types';

export interface LibraryEntry {
  id: string;
  name: string;
  description: string;
  category: string;
  markers: MarkerDef[];
  filters: FilterDef[];
}

/**
 * Built-in starting points the operator can drop into a port's ruleset and then
 * tweak. They are intentionally conservative defaults: a few are aggressive
 * (command-injection, large-payload) and may need tuning for a given service.
 * Patterns use partial (substring) matching, so they need no anchors.
 */
export const RULE_LIBRARY: LibraryEntry[] = [
  // ───────────────────────── Recon & Automation ─────────────────────────
  {
    id: 'block-cli-clients',
    name: 'Block CLI & Scripting Clients',
    description:
      'Drops requests whose User-Agent identifies a command-line or scripting HTTP client (python-requests/urllib, curl/wget, and common HTTP libraries). Stops quick scripted probing while leaving real browsers alone. Split into separate filters so a record shows which family was blocked.',
    category: 'Recon & Automation',
    markers: [],
    filters: [
      {
        id: 'block_ua_python', name: 'Block Python HTTP clients', enabled: true,
        scope: 'http.header', direction: 'inbound',
        matcher: { type: 'header_regex', header_name: 'User-Agent', pattern: 'python-requests|python-urllib|urllib|aiohttp|httpx|python/', case_sensitive: false },
        mode: 'blacklist',
      },
      {
        id: 'block_ua_curl_wget', name: 'Block curl / wget / httpie', enabled: true,
        scope: 'http.header', direction: 'inbound',
        matcher: { type: 'header_regex', header_name: 'User-Agent', pattern: 'curl|wget|httpie', case_sensitive: false },
        mode: 'blacklist',
      },
      {
        id: 'block_ua_libraries', name: 'Block HTTP libraries', enabled: true,
        scope: 'http.header', direction: 'inbound',
        matcher: { type: 'header_regex', header_name: 'User-Agent', pattern: 'go-http-client|libwww-perl|okhttp|node-fetch|axios|java/|ruby|powershell|guzzle', case_sensitive: false },
        mode: 'blacklist',
      },
    ],
  },
  {
    id: 'block-attack-tools',
    name: 'Block Known Attack Tools',
    description:
      'Drops requests whose User-Agent matches well-known scanners and exploitation tools (sqlmap, nikto, nmap, nuclei, gobuster, ...). A cheap first line against automated attacks; a determined attacker can spoof the User-Agent, so pair it with content rules.',
    category: 'Recon & Automation',
    markers: [],
    filters: [
      {
        id: 'block_ua_scanners', name: 'Block scanner User-Agents', enabled: true,
        scope: 'http.header', direction: 'inbound',
        matcher: { type: 'header_regex', header_name: 'User-Agent', pattern: 'sqlmap|nikto|nmap|masscan|nuclei|gobuster|dirbuster|feroxbuster|ffuf|wpscan|acunetix|nessus|zgrab|fuzz', case_sensitive: false },
        mode: 'blacklist',
      },
    ],
  },

  // ───────────────────────── Flag Protection ─────────────────────────
  {
    id: 'block-flag-exfil',
    name: 'Block Flag Exfiltration',
    description:
      "Drops any response whose contents match the flag format, so a working exploit cannot read the flag back out. Covers HTTP response bodies and raw (non-HTTP) streams. Edit the pattern to your event's flag format before enabling.",
    category: 'Flag Protection',
    markers: [],
    filters: [
      {
        id: 'block_flag_http_body', name: 'Block flag in HTTP response', enabled: true,
        scope: 'http.body', direction: 'outbound',
        matcher: { type: 'body_regex', pattern: 'flag\\{[^}]*\\}', case_sensitive: false },
        mode: 'blacklist',
      },
      {
        id: 'block_flag_stream', name: 'Block flag in raw stream', enabled: true,
        scope: 'stream', direction: 'outbound',
        matcher: { type: 'stream_regex', pattern: 'flag\\{[^}]*\\}', case_sensitive: false },
        mode: 'blacklist',
      },
    ],
  },

  // ───────────────────────── Exploit Mitigation ─────────────────────────
  {
    id: 'block-path-traversal',
    name: 'Block Path Traversal',
    description: 'Detects and blocks common path-traversal sequences (../, encoded variants) in the request path.',
    category: 'Exploit Mitigation',
    markers: [],
    filters: [
      {
        id: 'block_path_traversal', name: 'Block Path Traversal', enabled: true,
        scope: 'http.path', direction: 'inbound',
        matcher: { type: 'path_regex', pattern: '\\.\\.[\\/]|%2e%2e|\\.\\.%2f', case_sensitive: false },
        mode: 'blacklist',
      },
    ],
  },
  {
    id: 'block-sensitive-files',
    name: 'Block Sensitive File Access',
    description:
      'Drops requests that reference sensitive local paths or PHP/file wrappers commonly used in LFI/RFI (/etc/passwd, /proc/self, php://, file://, ...). Matches on both the request path and body.',
    category: 'Exploit Mitigation',
    markers: [],
    filters: [
      {
        id: 'block_lfi_path', name: 'Block sensitive paths', enabled: true,
        scope: 'http.path', direction: 'inbound',
        matcher: { type: 'path_regex', pattern: '/etc/passwd|/etc/shadow|/proc/self|php://|file://|expect://|data://', case_sensitive: false },
        mode: 'blacklist',
      },
      {
        id: 'block_lfi_body', name: 'Block wrappers in body', enabled: true,
        scope: 'http.body', direction: 'inbound',
        matcher: { type: 'body_regex', pattern: 'php://|file://|expect://|/etc/passwd', case_sensitive: false },
        mode: 'blacklist',
      },
    ],
  },
  {
    id: 'block-command-injection',
    name: 'Block Command Injection Patterns',
    description:
      'Drops requests whose path or body contain common shell fragments (command substitution, /bin/sh, reverse-shell snippets, inline downloads). Aggressive: tune or disable individual filters if your service legitimately accepts such input.',
    category: 'Exploit Mitigation',
    markers: [],
    filters: [
      {
        id: 'block_cmdi_body', name: 'Block shell payloads in body', enabled: true,
        scope: 'http.body', direction: 'inbound',
        matcher: { type: 'body_regex', pattern: '\\$\\(|`|/bin/sh|/bin/bash|nc -e|bash -i|wget |curl ', case_sensitive: false },
        mode: 'blacklist',
      },
      {
        id: 'block_cmdi_path', name: 'Block shell payloads in path', enabled: true,
        scope: 'http.path', direction: 'inbound',
        matcher: { type: 'path_regex', pattern: '\\$\\(|`|/bin/sh|/bin/bash|%3b|%7c', case_sensitive: false },
        mode: 'blacklist',
      },
    ],
  },

  // ───────────────────────── Hardening ─────────────────────────
  {
    id: 'allow-get-only',
    name: 'Allow GET/HEAD Only',
    description: 'Whitelists only the GET and HEAD methods; every other method (POST, PUT, DELETE, ...) is dropped. Use for read-only services.',
    category: 'Hardening',
    markers: [],
    filters: [
      {
        id: 'allow_safe_methods', name: 'Allow Safe Methods', enabled: true,
        scope: 'http.method', direction: 'inbound',
        matcher: { type: 'method_regex', pattern: '^(GET|HEAD)$', case_sensitive: true },
        mode: 'whitelistall',
      },
    ],
  },
  {
    id: 'block-large-payloads',
    name: 'Block Large Payloads',
    description: 'Drops requests whose HTTP body is larger than 1 MB. Useful against oversized uploads and bulk data exfiltration attempts.',
    category: 'Hardening',
    markers: [
      {
        id: 'large_body_marker', name: 'Large Body Detected', enabled: true,
        scope: 'http.body', direction: 'inbound',
        matcher: { type: 'size', op: '>', value: 1048576 },
      },
    ],
    filters: [
      {
        id: 'block_large_body', name: 'Block Large Body', enabled: true,
        scope: 'http.body', direction: 'inbound',
        matcher: { type: 'always', value: true },
        mode: 'blacklist',
        needed_markers_all: ['large_body_marker'],
      },
    ],
  },
];

/**
 * Returns a deep clone of a library entry's markers and filters
 * with unique IDs to avoid collisions with existing rules.
 */
export function instantiateLibraryEntry(
  entry: LibraryEntry,
  existingIds: Set<string>
): { markers: MarkerDef[]; filters: FilterDef[] } {
  const makeUniqueId = (baseId: string): string => {
    let id = baseId;
    let counter = 1;
    while (existingIds.has(id)) {
      id = `${baseId}_${counter}`;
      counter++;
    }
    existingIds.add(id);
    return id;
  };

  // Build ID remapping for markers (needed for filter references)
  const idMap = new Map<string, string>();

  const markers = entry.markers.map(m => {
    const newId = makeUniqueId(m.id);
    idMap.set(m.id, newId);
    return { ...JSON.parse(JSON.stringify(m)), id: newId };
  });

  const filters = entry.filters.map(f => {
    const newId = makeUniqueId(f.id);
    const clone = JSON.parse(JSON.stringify(f)) as FilterDef;
    clone.id = newId;
    // Remap marker references
    if (clone.needed_markers_all) {
      clone.needed_markers_all = clone.needed_markers_all.map(id => idMap.get(id) ?? id);
    }
    if (clone.needed_markers_any) {
      clone.needed_markers_any = clone.needed_markers_any.map(id => idMap.get(id) ?? id);
    }
    return clone;
  });

  return { markers, filters };
}
