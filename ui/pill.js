/* Eco — píldora flotante: punto, temporizador y botón detener. */
(() => {
  'use strict';
  const T = window.__TAURI__;
  if (!T) return;
  const invoke = (c, a) => T.core.invoke(c, a);
  const listen = (n, cb) => T.event.listen(n, cb);
  const $ = (s) => document.querySelector(s);
  const pad = (n) => String(n).padStart(2, '0');
  const E = { status: 'idle', elapsedMs: 0, at: performance.now() };

  function fmt(ms) {
    const s = Math.floor(Math.max(0, ms) / 1000);
    const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60, sec = s % 60;
    return h ? `${pad(h)}:${pad(m)}:${pad(sec)}` : `${pad(m)}:${pad(sec)}`;
  }
  let last = '';
  function draw() {
    const ms = E.elapsedMs + (E.status === 'recording' ? performance.now() - E.at : 0);
    const t = fmt(ms);
    if (t !== last) { last = t; $('#timer').textContent = t; }
    $('#pill').dataset.status = E.status === 'paused' ? 'paused' : 'recording';
  }
  const set = (s) => { E.status = s.status; E.elapsedMs = s.elapsedMs || 0; E.at = performance.now(); draw(); };
  function applyTheme(t) {
    const light = t === 'light' || ((t || 'system') === 'system' && matchMedia('(prefers-color-scheme: light)').matches);
    document.documentElement.dataset.theme = light ? 'light' : 'dark';
  }

  async function stop() {
    const b = $('#btnStop');
    b.classList.add('busy');
    try { await invoke('stop_recording'); } catch (_) { /* el estado llega por el evento */ }
    b.classList.remove('busy');
  }

  async function init() {
    $('#btnStop').addEventListener('click', stop);
    document.addEventListener('contextmenu', (e) => e.preventDefault());
    await Promise.allSettled([listen('state', (ev) => set(ev.payload))]);
    const [st, se] = await Promise.allSettled([invoke('get_state'), invoke('get_settings')]);
    if (se.status === 'fulfilled') applyTheme(se.value.theme);
    if (st.status === 'fulfilled') set(st.value);
    document.addEventListener('visibilitychange', async () => {
      if (document.visibilityState !== 'visible') return;
      try { set(await invoke('get_state')); applyTheme((await invoke('get_settings')).theme); } catch (_) { /* sin estado */ }
    });
    setInterval(draw, 200);
    document.documentElement.dataset.ready = '1';
  }
  init();
})();
