/* ================================================================
   util.js – Helpers: toasts, time formatting, DOM shortcuts
   ================================================================ */

/* ---------- DOM helpers ---------- */
export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];
export function el(tag, attrs = {}, children = []) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === 'class') e.className = v;
    else if (k === 'html') e.innerHTML = v;
    else if (k === 'text') e.textContent = v;
    else if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
    else if (k === 'style' && typeof v === 'object') Object.assign(e.style, v);
    else e.setAttribute(k, v);
  }
  for (const c of children) {
    if (typeof c === 'string') e.appendChild(document.createTextNode(c));
    else if (c) e.appendChild(c);
  }
  return e;
}
export function html(strings, ...vals) {
  const t = document.createElement('template');
  t.innerHTML = String.raw(strings, ...vals).trim();
  return t.content.firstChild;
}

/* ---------- Time ---------- */
export function nsToDate(ns) {
  return new Date(Number(BigInt(ns) / 1000000n));
}
export function fmtTime(ns) {
  try {
    const d = nsToDate(ns);
    return d.toLocaleTimeString('en-GB', { hour12: false }) + '.' +
           String(d.getMilliseconds()).padStart(3, '0');
  } catch { return '—'; }
}
export function fmtDateTime(ns) {
  try {
    const d = nsToDate(ns);
    return d.toLocaleDateString('en-GB') + ' ' + fmtTime(ns);
  } catch { return '—'; }
}
export function fmtAgo(ns) {
  const diff = Date.now() - Number(BigInt(ns) / 1000000n);
  if (diff < 1000) return 'just now';
  if (diff < 60000) return `${Math.floor(diff/1000)}s ago`;
  if (diff < 3600000) return `${Math.floor(diff/60000)}m ago`;
  return `${Math.floor(diff/3600000)}h ago`;
}

/* ---------- Toast ---------- */
let toastContainer;
export function toast(msg, type = 'ok', timeout = 4000) {
  if (!toastContainer) {
    toastContainer = el('div', { class: 'toast-container' });
    document.body.appendChild(toastContainer);
  }
  const t = el('div', { class: `toast ${type}`, text: msg });
  toastContainer.appendChild(t);
  setTimeout(() => { t.remove(); }, timeout);
}

/* ---------- Misc ---------- */
export function debounce(fn, ms = 300) {
  let timer;
  return (...args) => { clearTimeout(timer); timer = setTimeout(() => fn(...args), ms); };
}
export function escHtml(s) {
  const d = document.createElement('div');
  d.textContent = s;
  return d.innerHTML;
}
export function modeLabel(m) {
  switch (m) {
    case 'blacklist': return 'Blacklist';
    case 'whitelist_all': return 'Whitelist (ALL)';
    case 'whitelist_any': return 'Whitelist (ANY)';
    default: return m;
  }
}
export function dirLabel(d) {
  switch (d) {
    case 'inbound': return '→ IN';
    case 'outbound': return '← OUT';
    case 'both': return '↔ BOTH';
    default: return d;
  }
}
export function scopeTag(s) {
  return `<span class="scope-tag">${escHtml(s)}</span>`;
}
