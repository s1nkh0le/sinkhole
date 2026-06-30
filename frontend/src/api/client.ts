import createClient, { type Middleware } from 'openapi-fetch';
import type { paths } from './schema';

// Module-level auth state. AuthContext mirrors the bearer token here so every
// request can read it without threading it through React, and it survives a page
// reload by seeding from sessionStorage.
let authToken: string | null = sessionStorage.getItem('sinkhole_auth_token');
let onUnauthorized: (() => void) | null = null;

/**
 * Base URL of the controller API. Uses the controller URL configured in
 * settings, falling back to the page origin when none is set.
 */
export function getBaseUrl(): string {
  try {
    const stored = localStorage.getItem('sinkhole_settings');
    if (stored) {
      const parsed = JSON.parse(stored);
      if (parsed.controllerUrl) return parsed.controllerUrl;
    }
  } catch { /* malformed settings: fall back to the origin */ }
  return window.location.origin;
}

/** Set (or clear) the bearer token sent with every subsequent request. */
export function setAuthToken(token: string | null) {
  authToken = token;
}

/** Register the callback invoked when a request is rejected with 401 or 403. */
export function setOnUnauthorized(cb: () => void) {
  onUnauthorized = cb;
}

/**
 * Serialize query params the way the controller expects: arrays collapse to a
 * single comma-joined value (e.g. `?markers=a,b`), and nullish or empty values
 * are dropped entirely.
 */
function querySerializer(q: Record<string, unknown>): string {
  const parts: string[] = [];
  for (const [k, v] of Object.entries(q)) {
    if (v === undefined || v === null) continue;
    if (Array.isArray(v)) {
      if (v.length > 0) {
        parts.push(`${encodeURIComponent(k)}=${v.map(x => encodeURIComponent(String(x))).join(',')}`);
      }
    } else {
      parts.push(`${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`);
    }
  }
  return parts.join('&');
}

// Injects the current bearer token and reports auth failures. Reads the
// module-level state per request, so it always uses the latest token, and
// covers every verb (no need to wrap each method by hand).
const authMiddleware: Middleware = {
  onRequest({ request }) {
    if (authToken) request.headers.set('Authorization', `Bearer ${authToken}`);
    return request;
  },
  onResponse({ response }) {
    if (response.status === 401 || response.status === 403) onUnauthorized?.();
    return response;
  },
};

/**
 * Build an API client bound to the current controller URL. Clients are cheap,
 * so a fresh one per use keeps the base URL in sync with settings; auth and
 * 401/403 handling come from shared middleware.
 */
export function getApi() {
  const client = createClient<paths>({ baseUrl: getBaseUrl(), querySerializer });
  client.use(authMiddleware);
  return client;
}

export type ApiClient = ReturnType<typeof getApi>;
