/* ================================================================
   pages/dashboard.js – Overview / home page
   Shows all filters (services) with health, port count, ECAM highlights.
   ================================================================ */

import * as api from '../api.js';
import { $, el, toast, fmtAgo, escHtml } from '../util.js';
import { navigate } from '../router.js';

export async function render(root) {
  root.innerHTML = `
    <div class="page-title">⬡ Dashboard</div>
    <div class="page-subtitle">Active sinkhole filters overview</div>
    <div id="dash-stats" class="grid-4 mb-16"></div>
    <div class="card-header" style="padding:0 0 12px">
      <span class="card-title">Filters</span>
    </div>
    <div id="dash-filters"></div>
    <div class="card mt-16">
      <div class="card-header"><span class="card-title">Recent ECAM Events</span></div>
      <div id="dash-ecam" style="max-height:300px;overflow-y:auto"></div>
    </div>
  `;

  try {
    const [svcRes, stateRes] = await Promise.all([
      api.listServices(),
      api.getState().catch(() => null),
    ]);

    const services = svcRes?.data || [];
    renderStats(services);
    renderFilters(services);
    loadEcam();
  } catch (e) {
    toast(e.message, 'error');
  }
}

function renderStats(services) {
  const c = $('#dash-stats');
  const healthy = services.filter(s => s.healthy).length;
  const totalPorts = services.reduce((a, s) => a + (s.ports?.length || 0), 0);
  c.innerHTML = `
    <div class="card stat-box"><div class="stat-value">${services.length}</div><div class="stat-label">Filters</div></div>
    <div class="card stat-box"><div class="stat-value">${healthy}</div><div class="stat-label">Healthy</div></div>
    <div class="card stat-box"><div class="stat-value">${services.length - healthy}</div><div class="stat-label">Unhealthy</div></div>
    <div class="card stat-box"><div class="stat-value">${totalPorts}</div><div class="stat-label">Ports</div></div>
  `;
}

function renderFilters(services) {
  const c = $('#dash-filters');
  if (!services.length) {
    c.innerHTML = '<div class="text-muted text-sm" style="padding:16px">No filters connected. Configure SERVICE_* env vars on the controller.</div>';
    return;
  }
  c.innerHTML = '';
  for (const svc of services) {
    const card = el('div', { class: 'card', style: 'cursor:pointer' });
    card.innerHTML = `
      <div class="card-header">
        <span class="card-title">
          <span class="dot ${svc.healthy ? 'healthy' : 'unhealthy'}"></span>
          ${escHtml(svc.id)}
        </span>
        <span class="card-badge ${svc.healthy ? 'badge-green' : 'badge-red'}">${svc.healthy ? 'ONLINE' : 'OFFLINE'}</span>
      </div>
      <div class="text-sm text-muted">${escHtml(svc.url || '—')}</div>
      <div class="flex gap-8 mt-8 flex-wrap">
        ${(svc.ports || []).map(p => `<span class="scope-tag">:${p}</span>`).join('')}
      </div>
      ${svc.last_sync_ns ? `<div class="text-sm text-muted mt-8">Last sync: ${fmtAgo(svc.last_sync_ns)}</div>` : ''}
    `;
    card.onclick = () => navigate(`#/filter/${svc.id}`);
    c.appendChild(card);
  }
}

async function loadEcam() {
  const c = $('#dash-ecam');
  try {
    const res = await api.getEcamGlobal(0);
    const events = (res?.data || []).slice(-50).reverse();
    if (!events.length) { c.innerHTML = '<div class="text-muted text-sm" style="padding:10px">No events</div>'; return; }
    c.innerHTML = events.map(e => `
      <div class="log-line">
        <span class="log-ts">${fmtTimeInline(e.ts_ns)}</span>
        <span class="log-sev ${e.severity}">${e.severity}</span>
        <span class="log-origin">${originStr(e.origin)}</span>
        <span class="log-msg">${escHtml(e.msg)}</span>
      </div>
    `).join('');
  } catch {
    c.innerHTML = '<div class="text-muted text-sm" style="padding:10px">Failed to load ECAM</div>';
  }
}

function fmtTimeInline(ns) {
  try {
    const d = new Date(Number(BigInt(ns) / 1000000n));
    return d.toLocaleTimeString('en-GB', { hour12: false }) + '.' + String(d.getMilliseconds()).padStart(3, '0');
  } catch { return '—'; }
}

function originStr(o) {
  if (typeof o === 'string') return o;
  if (typeof o === 'object') {
    const key = Object.keys(o)[0];
    return `${key}(${JSON.stringify(o[key])})`;
  }
  return String(o);
}
