/* ================================================================
   api.js – Thin wrapper around the Controller REST API
   All calls go through the controller, which proxies to filters.
   ================================================================ */

const state = {
  base: '',          // controller URL, e.g. http://localhost:7000
};

export function setBase(url) {
  state.base = url.replace(/\/+$/, '');
}
export function getBase() { return state.base; }

async function req(method, path, body) {
  const url = `${state.base}${path}`;
  const opts = {
    method,
    headers: { 'Content-Type': 'application/json' },
  };
  if (body !== undefined) opts.body = JSON.stringify(body);

  const res = await fetch(url, opts);
  if (!res.ok) {
    let msg = res.statusText;
    try { const j = await res.json(); msg = j.errors?.join('; ') || j.msg || msg; } catch {}
    throw new Error(`${res.status} ${msg}`);
  }
  if (res.status === 204) return null;
  return res.json();
}

/* ---- Health ---- */
export const health        = ()          => req('GET', '/health');
export const listServices  = ()          => req('GET', '/services');
export const getState      = ()          => req('GET', '/state');

/* ---- Global aggregates ---- */
export const getEcamGlobal  = (since = 0)       => req('GET', `/ecam?since=${since}`);
export const getRecordsGlobal = (params = {}) => {
  const q = new URLSearchParams();
  for (const [k,v] of Object.entries(params)) if (v !== undefined && v !== '') q.set(k, v);
  return req('GET', `/records?${q}`);
};

/* ---- Per-service ---- */
export const listPorts      = (svc)              => req('GET', `/services/${svc}/ports`);
export const getPortInfo    = (svc, port)        => req('GET', `/services/${svc}/ports/${port}/info`);
export const getPortConfig  = (svc, port)        => req('GET', `/services/${svc}/ports/${port}/config`);
export const putPortConfig  = (svc, port, body)  => req('PUT', `/services/${svc}/ports/${port}/config`, body);
export const getRuleset     = (svc, port)        => req('GET', `/services/${svc}/ports/${port}/ruleset`);
export const putRuleset     = (svc, port, body)  => req('PUT', `/services/${svc}/ports/${port}/ruleset`, body);
export const getCachedConfig  = (svc, port)      => req('GET', `/services/${svc}/ports/${port}/config/cached`);
export const getCachedRuleset = (svc, port)      => req('GET', `/services/${svc}/ports/${port}/ruleset/cached`);
export const getEcam          = (svc, since = 0) => req('GET', `/services/${svc}/ecam?since=${since}`);
export const getLogs          = (svc, port, params = {}) => {
  const q = new URLSearchParams();
  for (const [k,v] of Object.entries(params)) if (v !== undefined && v !== '') q.set(k, v);
  return req('GET', `/services/${svc}/ports/${port}/logs?${q}`);
};
