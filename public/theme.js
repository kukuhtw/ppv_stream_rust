// Shared theme + platform config — included in every page

(function () {
  // ── Theme (light / dark) ──
  const THEME_KEY = 'ppv_theme';
  const saved = localStorage.getItem(THEME_KEY) || 'dark';
  document.documentElement.setAttribute('data-bs-theme', saved);

  // After DOM ready, set toggle icon
  document.addEventListener('DOMContentLoaded', function () {
    const btn = document.getElementById('themeToggle');
    if (btn) btn.textContent = saved === 'dark' ? '☀️' : '🌙';
    applyPlatformConfig();
  });
})();

function toggleTheme() {
  const html = document.documentElement;
  const next = html.getAttribute('data-bs-theme') === 'dark' ? 'light' : 'dark';
  html.setAttribute('data-bs-theme', next);
  localStorage.setItem('ppv_theme', next);
  const btn = document.getElementById('themeToggle');
  if (btn) btn.textContent = next === 'dark' ? '☀️' : '🌙';
}

// ── Platform config (name / tagline) ──
function getPlatformConfig() {
  try { return JSON.parse(localStorage.getItem('ppv_platform_config') || '{}'); }
  catch { return {}; }
}

function savePlatformConfig(cfg) {
  localStorage.setItem('ppv_platform_config', JSON.stringify(cfg));
  applyPlatformConfig();
}

function applyPlatformConfig() {
  const cfg = getPlatformConfig();
  const name = cfg.name || 'PPV Stream';
  const tagline = cfg.tagline || 'Pay-Per-View Streaming Platform';
  document.querySelectorAll('[data-platform="name"]').forEach(el => el.textContent = name);
  document.querySelectorAll('[data-platform="tagline"]').forEach(el => el.textContent = tagline);
  document.querySelectorAll('[data-platform="brand"]').forEach(el => {
    el.textContent = '🎬 ' + name;
  });
  document.querySelectorAll('[data-platform="title"]').forEach(el => {
    document.title = el.getAttribute('data-title-tpl')
      ? el.getAttribute('data-title-tpl').replace('{name}', name)
      : name;
  });
}

// ── Shared escape helper ──
function esc(s) {
  return String(s ?? '')
    .replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;').replaceAll("'", '&#039;');
}
