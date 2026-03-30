/* ================================================================
   router.js – Minimal hash-based SPA router
   ================================================================ */

const routes = [];
let contentEl;

export function setContentEl(el) { contentEl = el; }

export function route(pattern, handler) {
  routes.push({ pattern, handler });
}

export function navigate(hash) {
  window.location.hash = hash;
}

function matchRoute(hash) {
  for (const r of routes) {
    if (typeof r.pattern === 'string') {
      // exact or parameterised "/svc/:id/port/:port"
      const re = new RegExp('^' + r.pattern.replace(/:([^/]+)/g, '(?<$1>[^/]+)') + '$');
      const m = hash.match(re);
      if (m) return { handler: r.handler, params: m.groups || {} };
    } else {
      const m = hash.match(r.pattern);
      if (m) return { handler: r.handler, params: m.groups || {} };
    }
  }
  return null;
}

export function resolve() {
  const hash = (window.location.hash || '#/').replace(/^#/, '');
  const m = matchRoute(hash);
  if (m && contentEl) {
    contentEl.innerHTML = '';
    m.handler(contentEl, m.params);
  }
}

window.addEventListener('hashchange', resolve);
