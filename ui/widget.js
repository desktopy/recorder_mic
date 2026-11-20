/* Eco — flyout de la bandeja. Mismos comandos y eventos que la ventana principal (CONTRACT.md). */
(() => {
  'use strict';
  const T = window.__TAURI__;
  if (!T) { document.body.textContent = 'Eco debe ejecutarse dentro de la aplicación.'; return; }
  const invoke = (c, a) => T.core.invoke(c, a);
  const listen = (n, cb) => T.event.listen(n, cb);
  const $ = (s) => document.querySelector(s);
  const pad = (n) => String(n).padStart(2, '0');
  const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
  const errMsg = (e) => (typeof e === 'string' ? e : e && e.message ? e.message : 'Ocurrió un error inesperado.');

  const ICONS = {
    mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21"/>',
    speaker: '<path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5H4z"/><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11"/>',
    pause: '<rect class="fill" x="6.5" y="5" width="3.8" height="14" rx="1.2"/><rect class="fill" x="13.7" y="5" width="3.8" height="14" rx="1.2"/>',
    play: '<path class="fill" d="M8.2 5.6v12.8a.8.8 0 0 0 1.2.7l10.2-6.4a.8.8 0 0 0 0-1.4L9.4 4.9a.8.8 0 0 0-1.2.7z"/>',
    open: '<path d="M7 17L17 7M9 7h8v8"/>',
  };
  const svgOf = (n) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" aria-hidden="true" focusable="false">${ICONS[n] || ''}</svg>`;
  const put = (sel, name) => { const e = $(sel); e.innerHTML = svgOf(name); e.dataset.icon = name; };

  const S = { settings: { captureInput: true, captureOutput: true, theme: 'system' }, engine: { status: 'idle', elapsedMs: 0 }, engineAt: performance.now() };
  const root = $('#w');
  const STATUS_TEXT = { idle: 'Готово к записи', recording: 'Идёт запись', paused: 'Пауза', finalizing: 'Завершение…' };
  const mq = window.matchMedia('(prefers-color-scheme: light)');

  /* ── tema (comparte localStorage con la ventana principal) ── */
  function applyTheme() {
    const t = S.settings.theme || 'system';
    document.documentElement.dataset.theme = (t === 'system' ? mq.matches : t === 'light') ? 'light' : 'dark';
    try { localStorage.setItem('eco-theme', t); } catch (_) { /* sin almacenamiento */ }
  }
  mq.addEventListener('change', () => { if ((S.settings.theme || 'system') === 'system') applyTheme(); });
  window.addEventListener('storage', (e) => {
    if (e.key !== 'eco-theme' || !e.newValue) return;
    S.settings.theme = e.newValue; applyTheme();
  });

  /* ── nota inferior (errores y aviso de bandeja) ── */
  let noteTimer = null;
  function note(msg, kind, ms) {
    const n = $('#note');
    n.textContent = msg || ''; n.title = msg || '';
    n.className = 'note' + (kind ? ' ' + kind : '');
    clearTimeout(noteTimer);
    if (msg && ms) noteTimer = setTimeout(() => note(''), ms);
  }

  /* ── pintado ── */
  const setSwitch = (el, on, locked) => {
    el.setAttribute('aria-checked', on ? 'true' : 'false');
    el.classList.toggle('is-disabled', locked);
    if (locked) el.setAttribute('aria-disabled', 'true'); else el.removeAttribute('aria-disabled');
    el.title = locked ? 'Остановите запись, чтобы изменить источники.' : '';
  };
  function render() {
    const st = S.engine.status;
    root.dataset.status = st;
    const noSrc = !S.settings.captureInput && !S.settings.captureOutput;
    $('#statusText').textContent = st === 'idle' && noSrc ? 'Elige una fuente' : STATUS_TEXT[st] || '';
    const rec = $('#btnRec');
    const recDisabled = st === 'finalizing' || (st === 'idle' && noSrc);
    rec.classList.toggle('is-disabled', recDisabled);
    if (recDisabled) rec.setAttribute('aria-disabled', 'true'); else rec.removeAttribute('aria-disabled');
    const label = st === 'idle' ? 'Начать запись' : st === 'finalizing' ? 'Завершение записи' : 'Остановить запись';
    rec.setAttribute('aria-label', label); rec.title = label;
    const pa = $('#btnPause');
    const canPause = st === 'recording' || st === 'paused';
    pa.classList.toggle('is-disabled', !canPause);
    if (canPause) pa.removeAttribute('aria-disabled'); else pa.setAttribute('aria-disabled', 'true');
    const pl = st === 'paused' ? 'Продолжить' : 'Пауза';
    pa.setAttribute('aria-label', pl); pa.title = canPause ? pl : 'Доступно во время записи.';
    const want = st === 'paused' ? 'play' : 'pause';
    if ($('#pauseIco').dataset.icon !== want) put('#pauseIco', want);
    const locked = st !== 'idle';
    setSwitch($('#swInput'), !!S.settings.captureInput, locked);
    setSwitch($('#swOutput'), !!S.settings.captureOutput, locked);
    $('#src-input').classList.toggle('off', !S.settings.captureInput);
    $('#src-output').classList.toggle('off', !S.settings.captureOutput);
  }
  function setEngine(s) {
    S.engine = { status: s.status, elapsedMs: s.elapsedMs || 0 };
    S.engineAt = performance.now();
    render(); drawTimer();
  }
  function fmt(ms) {
    const s = Math.floor(Math.max(0, ms) / 1000);
    const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60, sec = s % 60;
    return h ? `${pad(h)}:${pad(m)}:${pad(sec)}` : `${pad(m)}:${pad(sec)}`;
  }
  let lastTimer = '';
  function drawTimer() {
    const ms = S.engine.elapsedMs + (S.engine.status === 'recording' ? performance.now() - S.engineAt : 0);
    const t = fmt(ms);
    if (t !== lastTimer) { lastTimer = t; $('#timer').textContent = t; }
  }

  /* ── medidores ── */
  const toLvl = (v) => (v > 0 ? clamp((20 * Math.log10(v) + 60) / 60, 0, 1) : 0);
  const M = { input: { tgt: 0, disp: 0, fill: document.querySelector('#src-input .meter-fill') }, output: { tgt: 0, disp: 0, fill: document.querySelector('#src-output .meter-fill') } };
  let lastFrame = performance.now();
  function frame(now) {
    const dt = Math.min(64, now - lastFrame); lastFrame = now;
    let ring = 0;
    for (const k of ['input', 'output']) {
      const m = M[k];
      const on = !!S.settings[k === 'input' ? 'captureInput' : 'captureOutput'];
      const tgt = on ? m.tgt : 0;
      m.disp = tgt > m.disp ? tgt : Math.max(tgt, m.disp - dt * 0.0016);
      m.fill.style.clipPath = `inset(0 ${((1 - m.disp) * 100).toFixed(1)}% 0 0)`;
      ring = Math.max(ring, m.disp);
    }
    $('#ring').style.setProperty('--lvl', S.engine.status === 'recording' ? ring.toFixed(3) : '0');
    drawTimer();
    requestAnimationFrame(frame);
  }
  function onLevels(p) {
    for (const k of ['input', 'output']) { const s = p && p[k]; if (s) M[k].tgt = toLvl(s.rms); }
  }

  /* ── acciones ── */
  let busy = false;
  async function toggleRecord() {
    if (busy) return;
    const st = S.engine.status;
    if (st === 'finalizing') return;
    if (st === 'idle' && !S.settings.captureInput && !S.settings.captureOutput) { note('Activa al menos una fuente para grabar.', 'err', 4000); return; }
    busy = true; note('');
    try {
      if (st === 'idle') setEngine(await invoke('start_recording'));
      else { setEngine({ ...S.engine, status: 'finalizing' }); await invoke('stop_recording'); setEngine(await invoke('get_state')); }
    } catch (e) {
      note(errMsg(e), 'err', 6000);
      try { setEngine(await invoke('get_state')); } catch (_) { /* sin estado */ }
    } finally { busy = false; }
  }
  async function pauseResume() {
    if (busy) return;
    const st = S.engine.status;
    if (st !== 'recording' && st !== 'paused') return;
    busy = true;
    try { setEngine(await invoke(st === 'recording' ? 'pause_recording' : 'resume_recording')); }
    catch (e) { note(errMsg(e), 'err', 6000); }
    finally { busy = false; }
  }
  async function toggleSource(field) {
    if (S.engine.status !== 'idle') return;
    try {
      const cur = await invoke('get_settings');
      const res = await invoke('set_settings', { settings: { ...cur, [field]: !cur[field] } });
      S.settings = { ...S.settings, ...res }; applyTheme(); render();
    } catch (e) { note(errMsg(e), 'err', 6000); }
  }

  async function refresh() {
    const [set, st] = await Promise.allSettled([invoke('get_settings'), invoke('get_state')]);
    if (set.status === 'fulfilled') { S.settings = { ...S.settings, ...set.value }; applyTheme(); }
    if (st.status === 'fulfilled') setEngine(st.value); else render();
  }
  function replayEnter() {
    root.classList.remove('enter'); void root.offsetWidth; root.classList.add('enter');
  }

  async function init() {
    put('#icoMic', 'mic'); put('#icoSpk', 'speaker'); put('#pauseIco', 'pause'); put('#icoOpen', 'open');
    for (const id of ['btnRec', 'btnPause']) $('#' + id).addEventListener('mousedown', (e) => e.preventDefault());
    $('#btnRec').addEventListener('click', toggleRecord);
    $('#btnPause').addEventListener('click', pauseResume);
    $('#swInput').addEventListener('click', () => toggleSource('captureInput'));
    $('#swOutput').addEventListener('click', () => toggleSource('captureOutput'));
    $('#btnOpen').addEventListener('click', () => invoke('show_main_window').catch((e) => note(errMsg(e), 'err', 5000)));
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { e.preventDefault(); invoke('hide_widget').catch(() => {}); }
    });
    document.addEventListener('contextmenu', (e) => e.preventDefault());

    await Promise.allSettled([
      listen('levels', (ev) => onLevels(ev.payload)),
      listen('state', (ev) => setEngine(ev.payload)),
      listen('engine-error', (ev) => note(ev.payload && ev.payload.message ? ev.payload.message : 'Error en el motor de audio.', 'err', 8000)),
      listen('widget-shown', (ev) => {
        replayEnter();
        const hint = !!(ev.payload && ev.payload.hint);
        note(hint ? 'Eco sigue en la bandeja' : '', hint ? 'hint' : '', hint ? 6000 : 0);
        refresh();
      }),
    ]);
    document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'visible') refresh(); });
    await refresh();
    requestAnimationFrame(frame);
    document.documentElement.dataset.ready = '1';
  }
  init().catch((e) => { console.error(e); note(errMsg(e), 'err'); });
})();
