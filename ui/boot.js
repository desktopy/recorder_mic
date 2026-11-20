/* Aplica el tema antes del primer pintado (evita parpadeo). */
(function () {
  try {
    var t = localStorage.getItem('eco-theme') || 'system';
    var m = /[?&]theme=(light|dark|system)/.exec(location.search);
    if (m && /[?&]mock\b/.test(location.search)) t = m[1];
    var light = t === 'light' || (t === 'system' && window.matchMedia('(prefers-color-scheme: light)').matches);
    document.documentElement.dataset.theme = light ? 'light' : 'dark';
  } catch (e) { document.documentElement.dataset.theme = 'dark'; }
})();
