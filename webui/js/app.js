/* ================================================================
   app.js – Application bootstrap
   ================================================================ */

import { setBase } from './api.js';
import * as api from './api.js';
import { $, $$, el, toast, escHtml } from './util.js';
import { route, setContentEl, resolve, navigate } from './router.js';
import { render as renderDashboard } from './pages/dashboard.js';
import { render as renderFilter } from './pages/filter.js';

/* ---------- Theme ---------- */
function initTheme() {
  const saved = localStorage.getItem('sinkhole-theme') || 'dark';
  document.documentElement.setAttribute('data-theme', saved);
  updateThemeButton(saved);
}

function toggleTheme() {
  const current = document.documentElement.getAttribute('data-theme') || 'dark';
  const next = current === 'dark' ? 'light' : 'dark';
  document.documentElement.setAttribute('data-theme', next);
  localStorage.setItem('sinkhole-theme', next);
  updateThemeButton(next);
}

function updateThemeButton(theme) {
  const btn = $('#theme-btn');
  if (btn) btn.textContent = theme === 'dark' ? '☀ Light' : '☾ Dark';
}

/* ---------- Connection ---------- */
function initConnection() {
  const saved = localStorage.getItem('sinkhole-controller-url') || 'http://localhost:7000';
  setBase(saved);
  const input = $('#ctrl-url');
  if (input) input.value = saved;
}

function saveConnection() {
  const url = $('#ctrl-url')?.value?.trim();
  if (!url) return;
  localStorage.setItem('sinkhole-controller-url', url);
  setBase(url);
  toast('Controller URL saved', 'ok');
  refreshSidebar();
  resolve();
}

/* ---------- Sidebar ---------- */
async function refreshSidebar() {
  const list = $('#sidebar-filters');
  if (!list) return;
  list.innerHTML = '<div class="text-muted text-sm" style="padding:8px 16px">Loading...</div>';

  try {
    const res = await api.listServices();
    const services = res?.data || [];
    list.innerHTML = '';

    for (const svc of services) {
      const item = el('div', {
        class: 'sidebar-item',
        'data-svc': svc.id,
        onclick: () => navigate(`#/filter/${svc.id}`),
      });
      item.innerHTML = `
        <span class="dot ${svc.healthy ? 'healthy' : 'unhealthy'}"></span>
        <span class="truncate">${escHtml(svc.id)}</span>
      `;
      list.appendChild(item);
    }

    if (!services.length) {
      list.innerHTML = '<div class="text-muted text-sm" style="padding:8px 16px">No filters found</div>';
    }

    highlightActiveFilter();
  } catch (e) {
    list.innerHTML = `<div class="text-red text-sm" style="padding:8px 16px">Connection failed</div>`;
  }
}

function highlightActiveFilter() {
  const hash = window.location.hash || '';
  const match = hash.match(/^#\/filter\/([^/]+)/);
  const activeId = match ? match[1] : null;
  $$('.sidebar-item').forEach(item => {
    item.classList.toggle('active', item.dataset.svc === activeId);
  });
}

/* ---------- Routes ---------- */
route('/', renderDashboard);
route('/filter/:id', renderFilter);

/* ---------- Init ---------- */
document.addEventListener('DOMContentLoaded', () => {
  initTheme();
  initConnection();

  // Theme toggle
  const themeBtn = $('#theme-btn');
  if (themeBtn) themeBtn.onclick = toggleTheme;

  // Connection save
  const saveBtn = $('#ctrl-save');
  if (saveBtn) saveBtn.onclick = saveConnection;
  const urlInput = $('#ctrl-url');
  if (urlInput) urlInput.addEventListener('keydown', e => { if (e.key === 'Enter') saveConnection(); });

  // Dashboard link
  const dashLink = $('#nav-dashboard');
  if (dashLink) dashLink.onclick = () => navigate('#/');

  // Router
  setContentEl($('#main-content'));
  refreshSidebar();
  resolve();

  // Update sidebar highlight on nav
  window.addEventListener('hashchange', () => {
    highlightActiveFilter();
  });

  // Periodic sidebar refresh
  setInterval(refreshSidebar, 15000);
});
