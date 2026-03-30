/* ================================================================
   pages/filter.js – Single filter view with sub-tabs:
     Overview | Rules | ECAM | Traffic
   ================================================================ */

import * as api from '../api.js';
import { $, $$, el, toast, escHtml, fmtTime, fmtDateTime, fmtAgo, modeLabel, dirLabel, scopeTag } from '../util.js';
import { navigate } from '../router.js';

let currentSvc = null;
let currentPorts = [];
let currentTab = 'overview';

export async function render(root, params) {
  currentSvc = params.id;
  root.innerHTML = `
    <div class="flex items-center gap-8 mb-8">
      <a href="#/" style="font-size:.8rem;color:var(--text-2)">← Filters</a>
    </div>
    <div class="page-title" id="filter-title">⬡ ${escHtml(currentSvc)}</div>
    <div class="page-subtitle" id="filter-subtitle">Loading...</div>
    <div class="tabs" id="filter-tabs">
      <div class="tab active" data-tab="overview">Overview</div>
      <div class="tab" data-tab="rules">Rules</div>
      <div class="tab" data-tab="ecam">ECAM</div>
      <div class="tab" data-tab="traffic">Traffic</div>
    </div>
    <div id="filter-content" class="fade-in"></div>
  `;

  $$('#filter-tabs .tab').forEach(t => {
    t.onclick = () => {
      $$('#filter-tabs .tab').forEach(x => x.classList.remove('active'));
      t.classList.add('active');
      currentTab = t.dataset.tab;
      renderTab($('#filter-content'));
    };
  });

  try {
    const portsRes = await api.listPorts(currentSvc);
    currentPorts = (portsRes?.data || []).map(p => p.port ?? p.public ?? p);
    $('#filter-subtitle').textContent = `${currentPorts.length} port(s): ${currentPorts.map(p => ':' + p).join(', ')}`;

    renderTab($('#filter-content'));
  } catch (e) {
    toast(e.message, 'error');
    $('#filter-subtitle').textContent = 'Error loading filter';
  }
}

async function renderTab(container) {
  container.innerHTML = '<div class="text-muted text-sm">Loading...</div>';
  try {
    switch (currentTab) {
      case 'overview': await renderOverview(container); break;
      case 'rules':    await renderRules(container); break;
      case 'ecam':     await renderEcam(container); break;
      case 'traffic':  await renderTraffic(container); break;
    }
  } catch (e) {
    container.innerHTML = `<div class="text-red text-sm">Error: ${escHtml(e.message)}</div>`;
  }
}

/* ================================================================
   OVERVIEW TAB
   ================================================================ */
async function renderOverview(c) {
  const portCards = [];
  for (const port of currentPorts) {
    try {
      const cfgRes = await api.getPortConfig(currentSvc, port);
      const info = cfgRes?.data || {};
      portCards.push({ port, info, error: null });
    } catch (e) {
      portCards.push({ port, info: {}, error: e.message });
    }
  }

  c.innerHTML = `
    <div class="grid-2" id="port-cards"></div>
  `;

  const grid = $('#port-cards', c);
  for (const { port, info, error } of portCards) {
    const card = el('div', { class: 'card' });
    const color = info.hex_color || '#666';
    card.innerHTML = `
      <div class="card-header">
        <span class="card-title">
          <span style="width:10px;height:10px;border-radius:50%;background:${escHtml(color)};display:inline-block"></span>
          Port :${port}
        </span>
        <button class="btn btn-sm" data-edit-port="${port}">Edit</button>
      </div>
      ${error ? `<div class="text-red text-sm">${escHtml(error)}</div>` : `
        <div class="text-sm"><b>Name:</b> ${escHtml(info.name || '—')}</div>
        <div class="text-sm"><b>Upstream:</b> :${info.upstream || '?'}</div>
        <div class="text-sm"><b>Comment:</b> ${escHtml(info.comment || '—')}</div>
        <div class="text-sm mt-8"><b>Scopes:</b> ${(info.enabled_scopes || []).map(s => scopeTag(s)).join(' ')}</div>
        <div class="text-sm text-muted mt-8">Version: ${escHtml((info.version || '').slice(0, 12))}…</div>
      `}
    `;
    const editBtn = card.querySelector('[data-edit-port]');
    if (editBtn) editBtn.onclick = (e) => { e.stopPropagation(); openPortConfigModal(port, info); };
    grid.appendChild(card);
  }
}

/* ================================================================
   PORT CONFIG MODAL
   ================================================================ */
const ALL_SCOPES = ['meta','stream','http.header','http.path','http.body','http.method','http.status','timing'];

function openPortConfigModal(port, info) {
  const overlay = el('div', { class: 'modal-overlay' });
  const modal = el('div', { class: 'modal' });
  modal.innerHTML = `
    <div class="modal-title">Edit Port :${port}</div>
    <div class="input-group">
      <label class="input-label">Name</label>
      <input class="input" id="mc-name" value="${escHtml(info.name || '')}">
    </div>
    <div class="input-group">
      <label class="input-label">Comment</label>
      <textarea class="textarea" id="mc-comment">${escHtml(info.comment || '')}</textarea>
    </div>
    <div class="input-group">
      <label class="input-label">Color</label>
      <input class="input" id="mc-color" value="${escHtml(info.hex_color || '#FF8800')}" placeholder="#RRGGBB">
    </div>
    <div class="input-group">
      <label class="input-label">Enabled Scopes</label>
      <div class="flex flex-wrap gap-8" id="mc-scopes">
        ${ALL_SCOPES.map(s => `
          <label class="toggle-wrap" style="font-size:.75rem">
            <span class="toggle"><input type="checkbox" value="${s}" ${(info.enabled_scopes || []).includes(s) ? 'checked' : ''}><span class="slider"></span></span>
            ${s}
          </label>
        `).join('')}
      </div>
    </div>
    <div class="input-group">
      <label class="input-label">TLS Settings</label>
      <label class="toggle-wrap mb-8" style="font-size:.75rem">
        <span class="toggle"><input type="checkbox" id="mc-tls-enabled"><span class="slider"></span></span>
        TLS Enabled
      </label>
      <div id="mc-tls-fields" class="hidden">
        <div class="input-group">
          <label class="input-label">Certificate (PEM)</label>
          <div class="upload-zone" id="mc-cert-zone">
            Drop .pem certificate here or click to upload
            <input type="file" accept=".pem,.crt,.cert" id="mc-cert-file">
          </div>
          <div id="mc-cert-name" class="text-sm text-accent mt-8"></div>
        </div>
        <div class="input-group">
          <label class="input-label">Private Key (PEM)</label>
          <div class="upload-zone" id="mc-key-zone">
            Drop .pem key here or click to upload
            <input type="file" accept=".pem,.key" id="mc-key-file">
          </div>
          <div id="mc-key-name" class="text-sm text-accent mt-8"></div>
        </div>
        <label class="toggle-wrap mb-8" style="font-size:.75rem">
          <span class="toggle"><input type="checkbox" id="mc-tls-upstream"><span class="slider"></span></span>
          Re-encrypt to upstream (TLS)
        </label>
        <label class="toggle-wrap" style="font-size:.75rem">
          <span class="toggle"><input type="checkbox" id="mc-tls-skip" checked><span class="slider"></span></span>
          Skip upstream cert verification
        </label>
      </div>
    </div>
    <div class="modal-actions">
      <button class="btn" id="mc-cancel">Cancel</button>
      <button class="btn btn-accent" id="mc-save">Save Config</button>
    </div>
  `;
  overlay.appendChild(modal);
  document.body.appendChild(overlay);

  // TLS toggle
  const tlsCheck = modal.querySelector('#mc-tls-enabled');
  const tlsFields = modal.querySelector('#mc-tls-fields');
  tlsCheck.onchange = () => tlsFields.classList.toggle('hidden', !tlsCheck.checked);

  // File upload zones
  setupUploadZone(modal.querySelector('#mc-cert-zone'), modal.querySelector('#mc-cert-file'), modal.querySelector('#mc-cert-name'));
  setupUploadZone(modal.querySelector('#mc-key-zone'), modal.querySelector('#mc-key-file'), modal.querySelector('#mc-key-name'));

  overlay.onclick = (e) => { if (e.target === overlay) overlay.remove(); };
  modal.querySelector('#mc-cancel').onclick = () => overlay.remove();
  modal.querySelector('#mc-save').onclick = async () => {
    const scopes = [...modal.querySelectorAll('#mc-scopes input:checked')].map(i => i.value);
    if (!scopes.length) { toast('At least 1 scope required', 'error'); return; }
    const body = {
      name: modal.querySelector('#mc-name').value || null,
      comment: modal.querySelector('#mc-comment').value || null,
      hex_color: modal.querySelector('#mc-color').value || null,
      enabled_scopes: scopes,
    };
    try {
      await api.putPortConfig(currentSvc, port, body);
      toast('Config saved', 'ok');
      overlay.remove();
      renderTab($('#filter-content'));
    } catch (e) {
      toast(e.message, 'error');
    }
  };
}

function setupUploadZone(zone, input, nameEl) {
  zone.onclick = () => input.click();
  zone.ondragover = (e) => { e.preventDefault(); zone.classList.add('dragover'); };
  zone.ondragleave = () => zone.classList.remove('dragover');
  zone.ondrop = (e) => {
    e.preventDefault();
    zone.classList.remove('dragover');
    if (e.dataTransfer.files.length) {
      input.files = e.dataTransfer.files;
      nameEl.textContent = `✓ ${e.dataTransfer.files[0].name}`;
    }
  };
  input.onchange = () => {
    if (input.files.length) nameEl.textContent = `✓ ${input.files[0].name}`;
  };
}

/* ================================================================
   RULES TAB
   ================================================================ */
async function renderRules(c) {
  if (!currentPorts.length) { c.innerHTML = '<div class="text-muted">No ports</div>'; return; }

  c.innerHTML = `
    <div class="input-group" style="max-width:200px">
      <label class="input-label">Port</label>
      <select class="select" id="rules-port-select">
        ${currentPorts.map(p => `<option value="${p}">:${p}</option>`).join('')}
      </select>
    </div>
    <div id="rules-body"></div>
  `;

  const sel = $('#rules-port-select', c);
  sel.onchange = () => loadRulesForPort(sel.value);
  loadRulesForPort(sel.value);
}

async function loadRulesForPort(port) {
  const body = $('#rules-body');
  body.innerHTML = '<div class="text-muted text-sm">Loading ruleset...</div>';

  try {
    const res = await api.getRuleset(currentSvc, port);
    const rs = res?.data?.ruleset || res?.data || {};
    const markers = rs.markers || [];
    const filters = rs.filters || [];

    body.innerHTML = `
      <div class="flex items-center justify-between mb-8">
        <span class="text-sm text-muted">Version: ${escHtml((res?.data?.version || '').slice(0, 16))}…</span>
        <div class="flex gap-8">
          <button class="btn btn-sm" id="rules-add-marker">+ Marker</button>
          <button class="btn btn-sm" id="rules-add-filter">+ Filter</button>
          <button class="btn btn-accent btn-sm" id="rules-save">Save Ruleset</button>
        </div>
      </div>
      ${rs.comment ? `<div class="text-sm text-muted mb-8">Comment: ${escHtml(rs.comment)}</div>` : ''}

      <h3 style="font-size:.85rem;color:var(--accent);margin:16px 0 8px">Markers (${markers.length})</h3>
      <div id="rules-markers">
        ${markers.length ? markers.map(m => renderMarkerCard(m)).join('') : '<div class="text-muted text-sm">No markers defined</div>'}
      </div>

      <h3 style="font-size:.85rem;color:var(--accent);margin:16px 0 8px">Filters – Blacklist</h3>
      <div id="rules-blacklist">
        ${renderFilterGroup(filters.filter(f => f.mode === 'blacklist'))}
      </div>

      <h3 style="font-size:.85rem;color:var(--accent);margin:16px 0 8px">Filters – Whitelist ALL</h3>
      <div id="rules-wl-all">
        ${renderFilterGroup(filters.filter(f => f.mode === 'whitelist_all'))}
      </div>

      <h3 style="font-size:.85rem;color:var(--accent);margin:16px 0 8px">Filters – Whitelist ANY</h3>
      <div id="rules-wl-any">
        ${renderFilterGroup(filters.filter(f => f.mode === 'whitelist_any'))}
      </div>
    `;

    // Wire up add/save buttons
    body.querySelector('#rules-add-marker').onclick = () => openRuleModal('marker', port, rs);
    body.querySelector('#rules-add-filter').onclick = () => openRuleModal('filter', port, rs);
    body.querySelector('#rules-save').onclick = () => saveRuleset(port, rs);

    // Wire up edit/delete buttons
    body.querySelectorAll('[data-edit-rule]').forEach(btn => {
      btn.onclick = () => {
        const id = btn.dataset.editRule;
        const kind = btn.dataset.kind;
        const list = kind === 'marker' ? rs.markers : rs.filters;
        const rule = list.find(r => r.id === id);
        if (rule) openRuleModal(kind, port, rs, rule);
      };
    });
    body.querySelectorAll('[data-del-rule]').forEach(btn => {
      btn.onclick = () => {
        const id = btn.dataset.delRule;
        const kind = btn.dataset.kind;
        if (kind === 'marker') rs.markers = rs.markers.filter(m => m.id !== id);
        else rs.filters = rs.filters.filter(f => f.id !== id);
        loadRulesForPort(port);
        toast(`Deleted ${id} (unsaved)`, 'warn');
      };
    });
  } catch (e) {
    body.innerHTML = `<div class="text-red text-sm">Error: ${escHtml(e.message)}</div>`;
  }
}

function renderMarkerCard(m) {
  return `
    <div class="rule-card">
      <div class="rule-header">
        <span class="rule-id">${escHtml(m.id)} ${m.enabled === false ? '<span class="text-muted">(disabled)</span>' : ''}</span>
        <div class="flex gap-8">
          <button class="btn btn-sm" data-edit-rule="${escHtml(m.id)}" data-kind="marker">Edit</button>
          <button class="btn btn-sm btn-danger" data-del-rule="${escHtml(m.id)}" data-kind="marker">✕</button>
        </div>
      </div>
      <div class="rule-meta">
        <span>${scopeTag(m.scope)}</span>
        <span>${dirLabel(m.direction || 'both')}</span>
        <span>${matcherSummary(m.matcher)}</span>
      </div>
      ${m.name ? `<div class="text-sm text-muted mt-8">${escHtml(m.name)}</div>` : ''}
    </div>
  `;
}

function renderFilterGroup(filters) {
  if (!filters.length) return '<div class="text-muted text-sm">None</div>';
  return filters.map(f => `
    <div class="rule-card">
      <div class="rule-header">
        <span class="rule-id">
          ${escHtml(f.id)} ${f.enabled === false ? '<span class="text-muted">(disabled)</span>' : ''}
          <span class="card-badge ${f.mode === 'blacklist' ? 'badge-red' : 'badge-blue'}">${modeLabel(f.mode)}</span>
        </span>
        <div class="flex gap-8">
          <button class="btn btn-sm" data-edit-rule="${escHtml(f.id)}" data-kind="filter">Edit</button>
          <button class="btn btn-sm btn-danger" data-del-rule="${escHtml(f.id)}" data-kind="filter">✕</button>
        </div>
      </div>
      <div class="rule-meta">
        <span>${scopeTag(f.scope)}</span>
        <span>${dirLabel(f.direction || 'both')}</span>
        <span>${matcherSummary(f.matcher)}</span>
      </div>
      ${f.needed_markers_all?.length ? `<div class="text-sm mt-8">Requires ALL: ${f.needed_markers_all.map(m => `<code>${escHtml(m)}</code>`).join(', ')}</div>` : ''}
      ${f.needed_markers_any?.length ? `<div class="text-sm mt-8">Requires ANY: ${f.needed_markers_any.map(m => `<code>${escHtml(m)}</code>`).join(', ')}</div>` : ''}
    </div>
  `).join('');
}

function matcherSummary(m) {
  if (!m) return '—';
  const t = m.type || Object.keys(m)[0] || '?';
  switch (t) {
    case 'stream_regex': case 'path_regex': case 'body_regex':
    case 'method_regex': case 'status_regex':
      return `<code style="color:var(--purple)">${escHtml(t)}</code> /${escHtml(m.pattern || '')}/`;
    case 'header_regex':
      return `<code style="color:var(--purple)">header</code> ${escHtml(m.header_name || '')} ~ /${escHtml(m.pattern || '')}/`;
    case 'size':
      return `<code style="color:var(--purple)">size</code> ${m.op} ${m.value}${m.field_name ? ` (${m.field_name})` : ''}`;
    case 'session.lifetime': case 'session.idle':
      return `<code style="color:var(--purple)">${t}</code> ${m.op} ${m.value}s`;
    case 'always':
      return `<code style="color:var(--purple)">always</code> ${m.value}`;
    default:
      return `<code>${escHtml(t)}</code>`;
  }
}

/* ================================================================
   RULE ADD/EDIT MODAL
   ================================================================ */
const MATCHER_TYPES = [
  'stream_regex','path_regex','body_regex','header_regex',
  'method_regex','status_regex','size','session.lifetime','session.idle','always'
];

function openRuleModal(kind, port, ruleset, existing = null) {
  const isEdit = !!existing;
  const overlay = el('div', { class: 'modal-overlay' });
  const modal = el('div', { class: 'modal' });
  const e = existing || {};
  const matcherType = e.matcher?.type || 'stream_regex';

  modal.innerHTML = `
    <div class="modal-title">${isEdit ? 'Edit' : 'Add'} ${kind === 'marker' ? 'Marker' : 'Filter'}</div>

    <div class="input-group">
      <label class="input-label">ID</label>
      <input class="input" id="rm-id" value="${escHtml(e.id || '')}" ${isEdit ? 'readonly style="opacity:.6"' : ''} placeholder="my_rule_id">
    </div>
    <div class="input-group">
      <label class="input-label">Name (optional)</label>
      <input class="input" id="rm-name" value="${escHtml(e.name || '')}">
    </div>
    <div class="grid-2">
      <div class="input-group">
        <label class="input-label">Direction</label>
        <select class="select" id="rm-dir">
          <option value="both" ${(e.direction||'both')==='both'?'selected':''}>Both</option>
          <option value="inbound" ${e.direction==='inbound'?'selected':''}>Inbound</option>
          <option value="outbound" ${e.direction==='outbound'?'selected':''}>Outbound</option>
        </select>
      </div>
      <div class="input-group">
        <label class="input-label">Scope</label>
        <select class="select" id="rm-scope">
          ${ALL_SCOPES.map(s => `<option value="${s}" ${e.scope===s?'selected':''}>${s}</option>`).join('')}
        </select>
      </div>
    </div>
    <label class="toggle-wrap mb-8" style="font-size:.75rem">
      <span class="toggle"><input type="checkbox" id="rm-enabled" ${e.enabled !== false ? 'checked' : ''}><span class="slider"></span></span>
      Enabled
    </label>

    ${kind === 'filter' ? `
      <div class="input-group">
        <label class="input-label">Mode</label>
        <select class="select" id="rm-mode">
          <option value="blacklist" ${e.mode==='blacklist'?'selected':''}>Blacklist</option>
          <option value="whitelist_all" ${e.mode==='whitelist_all'?'selected':''}>Whitelist ALL</option>
          <option value="whitelist_any" ${e.mode==='whitelist_any'?'selected':''}>Whitelist ANY</option>
        </select>
      </div>
      <div class="input-group">
        <label class="input-label">Required Markers (ALL) – comma separated IDs</label>
        <input class="input" id="rm-markers-all" value="${escHtml((e.needed_markers_all||[]).join(', '))}">
      </div>
      <div class="input-group">
        <label class="input-label">Required Markers (ANY) – comma separated IDs</label>
        <input class="input" id="rm-markers-any" value="${escHtml((e.needed_markers_any||[]).join(', '))}">
      </div>
    ` : ''}

    <hr style="border-color:var(--border);margin:12px 0">
    <div class="input-group">
      <label class="input-label">Matcher Type</label>
      <select class="select" id="rm-matcher-type">
        ${MATCHER_TYPES.map(t => `<option value="${t}" ${matcherType===t?'selected':''}>${t}</option>`).join('')}
      </select>
    </div>
    <div id="rm-matcher-fields"></div>

    <div class="modal-actions">
      <button class="btn" id="rm-cancel">Cancel</button>
      <button class="btn btn-accent" id="rm-apply">${isEdit ? 'Update' : 'Add'}</button>
    </div>
  `;
  overlay.appendChild(modal);
  document.body.appendChild(overlay);

  const matcherFieldsContainer = modal.querySelector('#rm-matcher-fields');
  const renderMatcherFields = (type) => {
    const m = e.matcher?.type === type ? e.matcher : {};
    let h = '';
    if (type.endsWith('_regex') || type === 'stream_regex') {
      h = `
        <div class="input-group"><label class="input-label">Pattern</label><input class="input" id="rm-m-pattern" value="${escHtml(m.pattern || '')}"></div>
        <label class="toggle-wrap" style="font-size:.75rem"><span class="toggle"><input type="checkbox" id="rm-m-case" ${m.case_sensitive !== false ? 'checked' : ''}><span class="slider"></span></span>Case Sensitive</label>
      `;
      if (type === 'header_regex') {
        h = `<div class="input-group"><label class="input-label">Header Name</label><input class="input" id="rm-m-header" value="${escHtml(m.header_name || '')}"></div>` + h;
      }
    } else if (type === 'size') {
      h = `
        <div class="grid-2">
          <div class="input-group"><label class="input-label">Operator</label><select class="select" id="rm-m-op">
            ${['>','>=','<','<=','==','!='].map(o => `<option ${m.op===o?'selected':''}>${o}</option>`).join('')}
          </select></div>
          <div class="input-group"><label class="input-label">Value</label><input class="input" id="rm-m-value" type="number" value="${m.value||0}"></div>
        </div>
        <div class="input-group"><label class="input-label">Field Name (header scope only)</label><input class="input" id="rm-m-field" value="${escHtml(m.field_name || '')}"></div>
      `;
    } else if (type === 'session.lifetime' || type === 'session.idle') {
      h = `
        <div class="grid-2">
          <div class="input-group"><label class="input-label">Operator</label><select class="select" id="rm-m-op">
            ${['>','>=','<','<=','==','!='].map(o => `<option ${m.op===o?'selected':''}>${o}</option>`).join('')}
          </select></div>
          <div class="input-group"><label class="input-label">Value (seconds)</label><input class="input" id="rm-m-value" type="number" step="0.1" value="${m.value||0}"></div>
        </div>
      `;
    } else if (type === 'always') {
      h = `
        <label class="toggle-wrap" style="font-size:.75rem"><span class="toggle"><input type="checkbox" id="rm-m-always-val" ${m.value !== false ? 'checked' : ''}><span class="slider"></span></span>Always match</label>
      `;
    }
    matcherFieldsContainer.innerHTML = h;
  };

  const typeSelect = modal.querySelector('#rm-matcher-type');
  typeSelect.onchange = () => renderMatcherFields(typeSelect.value);
  renderMatcherFields(matcherType);

  overlay.onclick = (ev) => { if (ev.target === overlay) overlay.remove(); };
  modal.querySelector('#rm-cancel').onclick = () => overlay.remove();
  modal.querySelector('#rm-apply').onclick = () => {
    const id = modal.querySelector('#rm-id').value.trim();
    if (!id) { toast('ID is required', 'error'); return; }

    const mType = typeSelect.value;
    const matcher = { type: mType };
    if (mType.endsWith('_regex') || mType === 'stream_regex') {
      matcher.pattern = modal.querySelector('#rm-m-pattern')?.value || '';
      matcher.case_sensitive = modal.querySelector('#rm-m-case')?.checked ?? true;
      if (mType === 'header_regex') matcher.header_name = modal.querySelector('#rm-m-header')?.value || '';
    } else if (mType === 'size') {
      matcher.op = modal.querySelector('#rm-m-op')?.value || '>';
      matcher.value = parseInt(modal.querySelector('#rm-m-value')?.value || '0', 10);
      const fn = modal.querySelector('#rm-m-field')?.value?.trim();
      matcher.field_name = fn || null;
    } else if (mType === 'session.lifetime' || mType === 'session.idle') {
      matcher.op = modal.querySelector('#rm-m-op')?.value || '>';
      matcher.value = parseFloat(modal.querySelector('#rm-m-value')?.value || '0');
    } else if (mType === 'always') {
      matcher.value = modal.querySelector('#rm-m-always-val')?.checked ?? true;
    }

    const rule = {
      id,
      name: modal.querySelector('#rm-name')?.value || null,
      direction: modal.querySelector('#rm-dir')?.value || 'both',
      scope: modal.querySelector('#rm-scope')?.value || 'stream',
      matcher,
      enabled: modal.querySelector('#rm-enabled')?.checked ?? true,
    };

    if (kind === 'filter') {
      rule.mode = modal.querySelector('#rm-mode')?.value || 'blacklist';
      rule.needed_markers_all = parseCSV(modal.querySelector('#rm-markers-all')?.value);
      rule.needed_markers_any = parseCSV(modal.querySelector('#rm-markers-any')?.value);
    }

    // Update ruleset in memory
    const list = kind === 'marker' ? ruleset.markers : ruleset.filters;
    const idx = list.findIndex(r => r.id === id);
    if (idx >= 0) list[idx] = rule;
    else list.push(rule);

    overlay.remove();
    loadRulesForPort(port);
    toast(`${isEdit ? 'Updated' : 'Added'} ${id} (unsaved)`, 'warn');
  };
}

function parseCSV(s) {
  return (s || '').split(',').map(x => x.trim()).filter(Boolean);
}

async function saveRuleset(port, rs) {
  try {
    const body = { comment: rs.comment || null, markers: rs.markers || [], filters: rs.filters || [] };
    const res = await api.putRuleset(currentSvc, port, body);
    toast(`Ruleset saved! version=${(res?.data?.version || '').slice(0, 12)}…`, 'ok');
    loadRulesForPort(port);
  } catch (e) {
    toast(e.message, 'error');
  }
}

/* ================================================================
   ECAM TAB
   ================================================================ */
let ecamTimer = null;
let ecamSince = 0;

async function renderEcam(c) {
  if (ecamTimer) { clearInterval(ecamTimer); ecamTimer = null; }
  ecamSince = 0;

  c.innerHTML = `
    <div class="flex items-center justify-between mb-8">
      <span class="text-sm text-muted">Live event stream – auto-refreshes every 3s</span>
      <button class="btn btn-sm" id="ecam-clear">Clear</button>
    </div>
    <div id="ecam-stream" style="max-height:60vh;overflow-y:auto"></div>
  `;

  c.querySelector('#ecam-clear').onclick = () => {
    ecamSince = 0;
    c.querySelector('#ecam-stream').innerHTML = '';
  };

  const poll = async () => {
    try {
      const res = await api.getEcam(currentSvc, ecamSince);
      const events = res?.data || [];
      if (events.length) {
        ecamSince = res.next_since || (events[events.length - 1].ts_ns + 1);
        const stream = c.querySelector('#ecam-stream');
        for (const e of events) {
          const line = el('div', { class: 'log-line' });
          line.innerHTML = `
            <span class="log-ts">${fmtTime(e.ts_ns)}</span>
            <span class="log-sev ${e.severity}">${e.severity}</span>
            <span class="log-origin">${escHtml(originStr(e.origin))}</span>
            <span class="log-msg">${escHtml(e.msg)}</span>
          `;
          stream.prepend(line);
        }
      }
    } catch {}
  };

  await poll();
  ecamTimer = setInterval(poll, 3000);
}

function originStr(o) {
  if (typeof o === 'string') return o;
  if (typeof o === 'object' && o !== null) {
    const key = Object.keys(o)[0];
    const v = o[key];
    if (typeof v === 'object') return `${key}(${JSON.stringify(v)})`;
    return `${key}(${v})`;
  }
  return String(o);
}

/* ================================================================
   TRAFFIC TAB – Packet logs & charts
   ================================================================ */
async function renderTraffic(c) {
  if (ecamTimer) { clearInterval(ecamTimer); ecamTimer = null; }

  c.innerHTML = `
    <div class="grid-2 mb-16">
      <div class="input-group">
        <label class="input-label">Port</label>
        <select class="select" id="traffic-port">
          ${currentPorts.map(p => `<option value="${p}">:${p}</option>`).join('')}
        </select>
      </div>
      <div class="input-group">
        <label class="input-label">Decision</label>
        <select class="select" id="traffic-decision">
          <option value="">All</option>
          <option value="allow">Allow</option>
          <option value="drop">Drop</option>
        </select>
      </div>
    </div>

    <div class="grid-2 mb-16">
      <div class="card">
        <div class="card-title mb-8">Decisions Over Time</div>
        <div class="chart-container"><canvas id="chart-decisions"></canvas></div>
      </div>
      <div class="card">
        <div class="card-title mb-8">Traffic by Port</div>
        <div class="chart-container"><canvas id="chart-ports"></canvas></div>
      </div>
    </div>

    <div class="card">
      <div class="card-header">
        <span class="card-title">Recent Packets</span>
        <button class="btn btn-sm" id="traffic-refresh">Refresh</button>
      </div>
      <div class="tbl-wrap">
        <table>
          <thead><tr>
            <th>Time</th><th>Conn</th><th>Dir</th><th>Decision</th>
            <th>Src</th><th>Port</th><th>Method</th><th>Path</th><th>Status</th><th>Size</th>
          </tr></thead>
          <tbody id="traffic-rows"></tbody>
        </table>
      </div>
      <div id="traffic-empty" class="text-muted text-sm hidden" style="padding:12px">No records found</div>
    </div>
  `;

  const loadTraffic = async () => {
    const port = c.querySelector('#traffic-port').value;
    const decision = c.querySelector('#traffic-decision').value;
    const params = { limit: 200, order: 'desc' };
    if (decision) params.decision = decision;

    try {
      const res = await api.getLogs(currentSvc, port, params);
      const records = res?.data?.records || [];
      renderTrafficTable(c, records);
      renderCharts(c, records, port);
    } catch (e) {
      toast(e.message, 'error');
    }
  };

  c.querySelector('#traffic-port').onchange = loadTraffic;
  c.querySelector('#traffic-decision').onchange = loadTraffic;
  c.querySelector('#traffic-refresh').onclick = loadTraffic;
  await loadTraffic();
}

function renderTrafficTable(c, records) {
  const tbody = c.querySelector('#traffic-rows');
  const empty = c.querySelector('#traffic-empty');
  if (!records.length) { tbody.innerHTML = ''; empty.classList.remove('hidden'); return; }
  empty.classList.add('hidden');

  tbody.innerHTML = records.map(r => {
    const http = r.http || {};
    return `<tr>
      <td class="text-sm">${fmtTime(r.timestamp_ns)}</td>
      <td>${r.conn_id || '—'}</td>
      <td>${r.inbound ? '→ IN' : '← OUT'}</td>
      <td><span class="decision ${r.decision?.toLowerCase()}">${r.decision || '?'}</span></td>
      <td class="text-sm">${escHtml(r.src_ip || '')}:${r.src_port || ''}</td>
      <td>:${r.public_port || '?'}</td>
      <td>${escHtml(http.method || '')}</td>
      <td class="truncate" style="max-width:200px">${escHtml(http.path || '')}</td>
      <td>${http.status || ''}</td>
      <td class="text-sm">${http.body_len != null ? http.body_len + 'B' : '—'}</td>
    </tr>`;
  }).join('');
}

function renderCharts(c, records, selectedPort) {
  if (typeof Chart === 'undefined') return;

  // Decision over time chart
  const decCanvas = c.querySelector('#chart-decisions');
  if (decCanvas._chart) decCanvas._chart.destroy();

  const buckets = {};
  for (const r of records) {
    const t = new Date(Number(BigInt(r.timestamp_ns) / 1000000n));
    const key = `${t.getHours()}:${String(t.getMinutes()).padStart(2,'0')}`;
    if (!buckets[key]) buckets[key] = { allow: 0, drop: 0 };
    const d = (r.decision || '').toLowerCase();
    if (d === 'allow') buckets[key].allow++;
    else if (d === 'drop') buckets[key].drop++;
  }
  const labels = Object.keys(buckets).reverse();

  decCanvas._chart = new Chart(decCanvas, {
    type: 'bar',
    data: {
      labels,
      datasets: [
        { label: 'Allow', data: labels.map(l => buckets[l].allow), backgroundColor: getComputedStyle(document.documentElement).getPropertyValue('--accent').trim() + '80' },
        { label: 'Drop',  data: labels.map(l => buckets[l].drop),  backgroundColor: getComputedStyle(document.documentElement).getPropertyValue('--red').trim() + '80' },
      ],
    },
    options: chartOpts('Decisions'),
  });

  // Port distribution chart
  const portCanvas = c.querySelector('#chart-ports');
  if (portCanvas._chart) portCanvas._chart.destroy();

  const portBuckets = {};
  for (const r of records) {
    const p = r.public_port || '?';
    portBuckets[p] = (portBuckets[p] || 0) + 1;
  }
  const portLabels = Object.keys(portBuckets);

  portCanvas._chart = new Chart(portCanvas, {
    type: 'doughnut',
    data: {
      labels: portLabels.map(p => ':' + p),
      datasets: [{
        data: portLabels.map(p => portBuckets[p]),
        backgroundColor: ['#00ff41','#4dabf7','#ff8c00','#b48eff','#ff3c3c','#ffd600','#00d2ff'],
      }],
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: { position: 'right', labels: { color: getComputedStyle(document.documentElement).getPropertyValue('--text-1').trim(), font: { family: 'JetBrains Mono', size: 11 } } },
      },
    },
  });
}

function chartOpts(title) {
  const style = getComputedStyle(document.documentElement);
  const textColor = style.getPropertyValue('--text-1').trim();
  const gridColor = style.getPropertyValue('--chart-grid').trim();
  return {
    responsive: true,
    maintainAspectRatio: false,
    plugins: {
      legend: { labels: { color: textColor, font: { family: 'JetBrains Mono', size: 11 } } },
    },
    scales: {
      x: { ticks: { color: textColor, font: { size: 10 } }, grid: { color: gridColor } },
      y: { ticks: { color: textColor, font: { size: 10 } }, grid: { color: gridColor }, beginAtZero: true },
    },
  };
}
