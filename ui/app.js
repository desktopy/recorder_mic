/* Eco — interfaz. Sin dependencias. Habla con el backend vía window.__TAURI__ (contrato en CONTRACT.md). */
(() => {
  'use strict';

  const T = window.__TAURI__;
  if (!T) {
    document.body.textContent = 'Eco debe ejecutarse dentro de la aplicación.';
    return;
  }
  const invoke = (cmd, args) => T.core.invoke(cmd, args);
  const listen = (name, cb) => T.event.listen(name, cb);
  const convertFileSrc = (p) => T.core.convertFileSrc(p);
  const MOCK = window.__ECO_MOCK__ || {};

  /* ───────────── utilidades ───────────── */
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const pad = (n, l = 2) => String(n).padStart(l, '0');
  const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
  const errMsg = (e) => (typeof e === 'string' ? e : e && e.message ? e.message : 'Ocurrió un error inesperado.');

  function h(tag, attrs, ...kids) {
    const n = document.createElement(tag);
    if (attrs) {
      for (const [k, v] of Object.entries(attrs)) {
        if (v == null || v === false) continue;
        if (k === 'class') n.className = v;
        else if (k === 'html') n.innerHTML = v;
        else if (k.startsWith('on')) n.addEventListener(k.slice(2).toLowerCase(), v);
        else n.setAttribute(k, v === true ? '' : v);
      }
    }
    for (const c of kids.flat()) if (c != null && c !== false) n.append(c.nodeType ? c : document.createTextNode(c));
    return n;
  }

  const ICONS = {
    logo: '<circle cx="12" cy="12" r="9.2" style="opacity:.45"/><circle cx="12" cy="12" r="3.6" style="fill:var(--accent);stroke:none"/>',
    min: '<path d="M5 12h14" style="stroke-width:1.5"/>',
    close: '<path d="M6 6l12 12M18 6L6 18" style="stroke-width:1.5"/>',
    gear: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 0 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 0 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 0 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 0 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>',
    mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21"/>',
    speaker: '<path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5H4z"/><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11"/>',
    pause: '<rect class="fill" x="6.5" y="5" width="3.8" height="14" rx="1.2"/><rect class="fill" x="13.7" y="5" width="3.8" height="14" rx="1.2"/>',
    play: '<path class="fill" d="M8.2 5.6v12.8a.8.8 0 0 0 1.2.7l10.2-6.4a.8.8 0 0 0 0-1.4L9.4 4.9a.8.8 0 0 0-1.2.7z"/>',
    pencil: '<path d="M4 20l1-4L16.5 4.5a2 2 0 0 1 3 3L8 19l-4 1z"/><path d="M14.5 6.5l3 3"/>',
    chevron: '<path d="M6 9l6 6 6-6"/>',
    folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2.5h8a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z"/>',
    trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3"/>',
    check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
    empty: '<circle cx="12" cy="12" r="10.5" style="opacity:.35"/><circle cx="12" cy="12" r="7.5" style="opacity:.6"/><rect x="10" y="7.5" width="4" height="7" rx="2"/><path d="M8.2 12.4a3.8 3.8 0 0 0 7.6 0M12 16.2v1.6"/>',
  };
  const svgOf = (name) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" aria-hidden="true" focusable="false">${ICONS[name] || ''}</svg>`;
  const icon = (name) => h('span', { class: 'ico', html: svgOf(name) });

  const MONTHS = ['ene', 'feb', 'mar', 'abr', 'may', 'jun', 'jul', 'ago', 'sep', 'oct', 'nov', 'dic'];
  function fmtWhen(iso) {
    const d = new Date(iso);
    if (isNaN(d)) return '';
    const now = new Date();
    const day = (x) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
    const diff = Math.round((day(now) - day(d)) / 864e5);
    const hm = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
    if (diff === 0) return `сегодня ${hm}`;
    if (diff === 1) return `вчера ${hm}`;
    const base = `${d.getDate()} ${MONTHS[d.getMonth()]}`;
    return d.getFullYear() === now.getFullYear() ? `${base}, ${hm}` : `${base} ${d.getFullYear()}`;
  }
  function fmtClock(sec) {
    sec = Math.max(0, Math.round(sec || 0));
    const hr = Math.floor(sec / 3600), m = Math.floor(sec / 60) % 60, s = sec % 60;
    return hr ? `${hr}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
  }
  function fmtTimer(ms) {
    const t = Math.floor(Math.max(0, ms) / 100);
    const dec = t % 10, s = Math.floor(t / 10);
    const sec = s % 60, min = Math.floor(s / 60) % 60, hr = Math.floor(s / 3600);
    return { main: hr ? `${pad(hr)}:${pad(min)}:${pad(sec)}` : `${pad(min)}:${pad(sec)}`, dec };
  }
  function fmtBytes(b) {
    if (b < 1024 * 1024) return `${Math.max(1, Math.round(b / 1024))} KB`;
    if (b < 1024 ** 3) return `${(b / 1024 ** 2).toFixed(b < 10 * 1024 ** 2 ? 1 : 0).replace('.', ',')} MB`;
    return `${(b / 1024 ** 3).toFixed(2).replace('.', ',')} GB`;
  }
  const fmtRate = (hz) => `${(hz / 1000).toString().replace('.', ',')} kHz`;
  const fmtCh = (n) => (n === 1 ? 'mono' : n === 2 ? 'стерео' : `${n} canales`);
  function midTruncate(s, max) {
    if (s.length <= max) return s;
    const a = Math.ceil((max - 1) * 0.4), b = max - 1 - a;
    return s.slice(0, a) + '…' + s.slice(s.length - b);
  }

  /* ───────────── estado ───────────── */
  const DEFAULTS = {
    inputId: 'default', outputId: 'default', captureInput: true, captureOutput: false, mode: 'separate', format: 'mp3',
    outputDir: '', inputGainDb: 0, outputGainDb: 0, inputAgc: true, mixVoicePriority: true, monitorWhenIdle: false, theme: 'system',
    hotkeyToggle: 'Ctrl+Alt+R', hotkeyPause: 'Ctrl+Alt+P',
  };
  const PREF_DEFAULTS = { closeToTray: true, launchAtStartup: false, floatingPill: false, trayHintShown: false };
  const S = {
    settings: { ...DEFAULTS },
    settingsReady: false,
    prefs: { ...PREF_DEFAULTS },
    hotkeys: null, capturing: null, capHint: '',
    devices: { inputs: [], outputs: [] },
    engine: { status: 'idle', elapsedMs: 0, recordingId: null },
    engineAt: performance.now(),
    recordings: [],
    view: 'record',
  };
  const app = $('#app');

  /* ───────────── tema ───────────── */
  const mq = window.matchMedia('(prefers-color-scheme: light)');
  let colorCache = null;
  function applyTheme() {
    const t = S.settings.theme || 'system';
    const resolved = t === 'system' ? (mq.matches ? 'light' : 'dark') : t;
    document.documentElement.dataset.theme = resolved;
    try { localStorage.setItem('eco-theme', t); } catch (_) { /* sin almacenamiento */ }
    colorCache = null;
    redrawAllWaves();
  }
  mq.addEventListener('change', () => { if ((S.settings.theme || 'system') === 'system') applyTheme(); });

  /* ───────────── tooltips propios ───────────── */
  const tipEl = $('#tip');
  let tipTimer = null, tipTarget = null;
  function showTip(target) {
    const text = target.getAttribute('data-tip');
    if (!text) return;
    tipTarget = target;
    tipEl.textContent = text;
    tipEl.hidden = false;
    const r = target.getBoundingClientRect();
    const tr = tipEl.getBoundingClientRect();
    let x = clamp(r.left + r.width / 2 - tr.width / 2, 8, window.innerWidth - tr.width - 8);
    let y = r.bottom + 8;
    if (y + tr.height > window.innerHeight - 8) y = r.top - tr.height - 8;
    tipEl.style.left = `${x}px`;
    tipEl.style.top = `${Math.max(8, y)}px`;
  }
  function hideTip() { clearTimeout(tipTimer); tipTimer = null; tipTarget = null; tipEl.hidden = true; }
  function scheduleTip(target, delay) { clearTimeout(tipTimer); tipTimer = setTimeout(() => showTip(target), delay); }
  document.addEventListener('mouseover', (e) => {
    const t = e.target.closest && e.target.closest('[data-tip]');
    if (t && t !== tipTarget) { hideTip(); scheduleTip(t, 350); } else if (!t) hideTip();
  });
  document.addEventListener('focusin', (e) => {
    const t = e.target.closest && e.target.closest('[data-tip]');
    if (t) { hideTip(); scheduleTip(t, 150); } else hideTip();
  });
  document.addEventListener('focusout', hideTip);
  document.addEventListener('pointerdown', hideTip, true);
  document.addEventListener('scroll', hideTip, true);
  function setTip(el, text) { if (text) el.setAttribute('data-tip', text); else el.removeAttribute('data-tip'); if (tipTarget === el && !text) hideTip(); }

  /* ───────────── toasts ───────────── */
  const toastBox = $('#toasts');
  function toast({ msg, kind = 'ok', action, ms }) {
    for (const t of $$('.toast', toastBox)) if (t.dataset.msg === msg) t.remove();
    const el = h('div', { class: `toast ${kind}`, 'data-msg': msg });
    let timer;
    const close = () => { clearTimeout(timer); el.classList.add('out'); setTimeout(() => el.remove(), 200); };
    el.append(h('span', { class: 'tdot' }), h('span', { class: 'tmsg' }, msg));
    if (action) el.append(h('button', { class: 'tbtn', type: 'button', onclick: () => { close(); action.fn(); } }, action.label));
    el.append(h('button', { class: 'tx', type: 'button', 'aria-label': 'Закрыть уведомление', onclick: close }, icon('close')));
    toastBox.append(el);
    while (toastBox.children.length > 3) toastBox.firstElementChild.remove();
    timer = setTimeout(close, ms ?? (kind === 'error' ? 8000 : 5500));
  }
  const toastError = (e) => toast({ msg: errMsg(e), kind: 'error' });

  /* ───────────── modal de confirmación ───────────── */
  let modalOpen = false;
  const CHANGELOG = `recorder_mic — direct MP3, версия 0.1.1
• Запуск передаёт выбранные формат и источники в backend атомарно; больше нет гонки с асинхронным сохранением настроек.
• Новые настройки: только микрофон, MP3; существующие настройки не сбрасываются. Программа имеет отдельный singleton-ID, старый Eco не перехватывает её запуск.
• MP3 кодирует встроенный rusty_mp3 1.1.0: Rust, без C/FFI, без FFmpeg и DLL.
• Библиотека: Apache-2.0, Mata Network / Remade With Rust. Тексты лицензий и исходники включены в поставку. Это новая библиотека: проверены синтетические сигналы, не заявлена эквивалентность качества LAME.
• По умолчанию новые настройки: рабочий стол и MP3; сохранённые настройки не меняются.
• Один активный источник в MP3 — прямая потоковая запись одного MP3 без промежуточного WAV и папки записи. Данные сбрасываются на диск каждую секунду.
• Микрофон: recmic_ГГГГ-ММ-ДД_ЧЧ-ММ, при совпадении — _2, _3…
• WAV 16/24/32f остаётся доступен. При ошибке прямой записи MP3 полученные байты остаются в MP3; показывается ошибка. Полнота и восстановление не гарантируются. Два источника сохраняют прежний WAV/микшерный конвейер.
• MP3: VBR, качество 2; режим выбран через публичный API без накопления целой записи в CBR reservoir. 96/192 кГц преобразуются в 48 кГц потоковым sinc-фильтром. WAV сохраняет исходный формат.
• Список, переименование и удаление поддерживают новые файлы и прежние папки сессий.
• Удаление затрагивает только файлы записи, не папку сохранения и не посторонние файлы.
• Индекс одиночных файлов хранится в AppData\\Eco\\library-v6, не на рабочем столе. Старый .eco-library.json читается для совместимости и не изменяется.
• Исходный проект: https://github.com/EndingVE/Eco-Windows-Recorder`;

  function changesDialog() {
    if (modalOpen) return;
    modalOpen = true;
    const prev = document.activeElement;
    const text = h('textarea', { readonly: '', 'aria-label': 'Описание изменений', style: 'width:100%;height:290px;resize:vertical;font:inherit' });
    text.value = CHANGELOG;
    const copy = h('button', { class: 'btn primary', type: 'button' }, 'Копировать');
    const close = h('button', { class: 'btn', type: 'button' }, 'Закрыть');
    const back = h('div', { class: 'modal-back' }, h('div', { class: 'modal', role: 'dialog', 'aria-modal': 'true', 'aria-label': 'Что нового' },
      h('h3', {}, 'Что нового'), text, h('div', { class: 'btn-row' }, copy, close)));
    const done = () => { document.removeEventListener('keydown', onKey, true); back.remove(); modalOpen = false; prev?.focus(); };
    const onKey = e => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); done(); }
      if (e.key === 'Tab') {
        const controls = [text, copy, close];
        e.preventDefault();
        controls[(controls.indexOf(document.activeElement) + (e.shiftKey ? 2 : 1)) % 3].focus();
      }
    };
    copy.addEventListener('click', async () => {
      try {
        if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(CHANGELOG);
        else { text.focus(); text.select(); if (!document.execCommand('copy')) throw new Error('Выделите текст и нажмите Ctrl+C.'); }
        copy.textContent = 'Скопировано';
      } catch (e) { text.focus(); text.select(); toastError(e); }
    });
    close.addEventListener('click', done);
    back.addEventListener('pointerdown', e => { if (e.target === back) done(); });
    document.addEventListener('keydown', onKey, true);
    $('#modalRoot').append(back);
    copy.focus();
  }
  $('#btnChanges').addEventListener('click', changesDialog);
  $('#btnProject').addEventListener('click', () => invoke('open_project').catch(toastError));
  function confirmDialog({ title, body, confirmLabel }) {
    return new Promise((resolve) => {
      const prev = document.activeElement;
      modalOpen = true;
      const cancel = h('button', { class: 'btn', type: 'button' }, 'Отмена');
      const ok = h('button', { class: 'btn primary', type: 'button' }, confirmLabel);
      const dlg = h('div', { class: 'modal', role: 'alertdialog', 'aria-modal': 'true', 'aria-labelledby': 'mdTitle', 'aria-describedby': 'mdBody' },
        h('h3', { id: 'mdTitle' }, title), body, h('div', { class: 'btn-row' }, cancel, ok));
      body.id = 'mdBody';
      const back = h('div', { class: 'modal-back' }, dlg);
      const done = (v) => {
        document.removeEventListener('keydown', onKey, true);
        modalOpen = false; back.remove();
        if (prev && prev.focus && document.contains(prev)) prev.focus();
        resolve(v);
      };
      const onKey = (e) => {
        if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); done(false); }
        else if (e.key === 'Tab') {
          e.preventDefault();
          (document.activeElement === cancel ? ok : cancel).focus();
        } else if (e.key === ' ' || e.key === 'p' || e.key === 'P') e.stopPropagation();
      };
      cancel.addEventListener('click', () => done(false));
      ok.addEventListener('click', () => done(true));
      back.addEventListener('pointerdown', (e) => { if (e.target === back) done(false); });
      document.addEventListener('keydown', onKey, true);
      $('#modalRoot').append(back);
      cancel.focus();
    });
  }

  /* ───────────── controles reutilizables ───────────── */
  function makeSeg(root, { items, role, label, onSelect, cls, noMouseFocus }) {
    root.className = 'seg' + (cls ? ` ${cls}` : '');
    root.style.setProperty('--n', items.length);
    root.setAttribute('role', role === 'tab' ? 'tablist' : 'radiogroup');
    root.setAttribute('aria-label', label);
    root.append(h('span', { class: 'seg-thumb', 'aria-hidden': 'true' }));
    let disabled = false;
    const attr = role === 'tab' ? 'aria-selected' : 'aria-checked';
    const btns = items.map((it, i) => {
      const b = h('button', { type: 'button', role: role === 'tab' ? 'tab' : 'radio', id: it.btnId, 'aria-controls': it.controls, 'data-id': it.id }, it.label);
      if (noMouseFocus) b.addEventListener('mousedown', (e) => e.preventDefault());
      b.addEventListener('click', () => { if (disabled) return; set(it.id); onSelect(it.id); });
      b.addEventListener('keydown', (e) => {
        if (disabled) return;
        const dir = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
        if (!dir) return;
        e.preventDefault();
        const j = (i + dir + items.length) % items.length;
        btns[j].focus(); set(items[j].id); onSelect(items[j].id);
      });
      root.append(b);
      return b;
    });
    function set(id) {
      const idx = items.findIndex((i) => i.id === id);
      root.classList.toggle('none', idx < 0);
      if (idx >= 0) root.style.setProperty('--i', idx);
      btns.forEach((b, i) => {
        const on = i === idx;
        b.setAttribute(attr, on ? 'true' : 'false');
        b.tabIndex = on || (idx < 0 && i === 0) ? 0 : -1;
      });
    }
    function setDisabled(on, tip) {
      disabled = on;
      root.classList.toggle('is-disabled', on);
      if (on) root.setAttribute('aria-disabled', 'true'); else root.removeAttribute('aria-disabled');
      setTip(root, on ? tip : '');
    }
    return { set, setDisabled, btns };
  }

  function bindSwitch(btn, onToggle) {
    btn.addEventListener('click', () => {
      if (btn.getAttribute('aria-disabled') === 'true') return;
      onToggle(btn.getAttribute('aria-checked') !== 'true');
    });
  }
  function setSwitch(btn, on, disabled, tip) {
    btn.setAttribute('aria-checked', on ? 'true' : 'false');
    btn.classList.toggle('is-disabled', !!disabled);
    if (disabled) btn.setAttribute('aria-disabled', 'true'); else btn.removeAttribute('aria-disabled');
    setTip(btn, disabled ? tip : '');
  }

  let openDropdown = null;
  function createDropdown({ label, onSelect }) {
    const uid = 'dd' + Math.random().toString(36).slice(2, 7);
    const text = h('span', { class: 'dd-text' });
    const bdg = h('span', { class: 'dd-badge' }); bdg.hidden = true;
    const btn = h('button', { type: 'button', class: 'dd-btn', 'aria-haspopup': 'listbox', 'aria-expanded': 'false', 'aria-label': label }, text, bdg, icon('chevron'));
    btn.lastElementChild.classList.add('chev');
    const root = h('div', { class: 'dd' }, btn);
    let options = [], selected = null, pop = null, activeIdx = 0, typed = '', typedAt = 0, disabled = false;

    // La insignia solo se muestra si el nombre cabe entero junto a ella; si no, prima el nombre (la insignia queda en el tooltip y en la lista).
    function fitBadge() {
      if (!bdg.textContent) { bdg.hidden = true; return; }
      bdg.hidden = false;
      if (text.scrollWidth > text.clientWidth + 1) bdg.hidden = true;
    }
    let lastW = 0;
    new ResizeObserver(() => { const w = btn.clientWidth; if (w && w !== lastW) { lastW = w; fitBadge(); } }).observe(btn);

    function setOptions(list, sel) {
      options = list; selected = sel;
      const cur = options.find((o) => o.id === sel);
      text.textContent = cur ? cur.name : 'Нет устройств';
      text.title = cur ? cur.name : '';
      bdg.textContent = cur && cur.badge ? cur.badge : ''; bdg.hidden = !(cur && cur.badge);
      if (cur && cur.badge) text.title = `${cur.name} (${cur.badge.toLowerCase()})`;
      fitBadge();
      if (pop) close(false);
    }
    function open() {
      if (disabled || pop || !options.length) return;
      if (openDropdown && openDropdown !== api) openDropdown.close(false);
      activeIdx = Math.max(0, options.findIndex((o) => o.id === selected));
      pop = h('ul', { class: 'dd-pop', role: 'listbox', tabindex: '-1', 'aria-label': label, 'aria-activedescendant': `${uid}-${activeIdx}` },
        options.map((o, i) => h('li', { class: 'dd-opt' + (i === activeIdx ? ' active' : ''), role: 'option', id: `${uid}-${i}`, 'aria-selected': o.id === selected ? 'true' : 'false', 'data-i': i },
          h('div', { class: 'txt' }, h('div', { class: 'nm', title: o.name }, o.name), o.meta ? h('div', { class: 'mt' }, o.meta) : null), icon('check'))));
      document.body.append(pop);
      const r = btn.getBoundingClientRect();
      pop.style.width = `${r.width}px`;
      pop.style.left = `${r.left}px`;
      const below = window.innerHeight - r.bottom - 12, above = r.top - 12;
      const natural = Math.min(pop.scrollHeight + 2, 280);
      const up = natural > below && above > below;
      const maxH = Math.max(120, Math.min(280, up ? above : below));
      pop.style.maxHeight = `${maxH}px`;
      const ph = Math.min(natural, maxH);
      pop.style.top = up ? `${r.top - ph - 6}px` : `${r.bottom + 6}px`;
      pop.classList.toggle('up', up);
      btn.setAttribute('aria-expanded', 'true');
      openDropdown = api;
      pop.focus();
      scrollActive();
      pop.addEventListener('keydown', onKey);
      pop.addEventListener('click', (e) => { const li = e.target.closest('.dd-opt'); if (li) choose(+li.dataset.i); });
      pop.addEventListener('mousemove', (e) => { const li = e.target.closest('.dd-opt'); if (li && +li.dataset.i !== activeIdx) setActive(+li.dataset.i, false); });
    }
    function close(refocus = true) {
      if (!pop) return;
      pop.remove(); pop = null;
      btn.setAttribute('aria-expanded', 'false');
      if (openDropdown === api) openDropdown = null;
      if (refocus) btn.focus();
    }
    function setActive(i, scroll = true) {
      activeIdx = clamp(i, 0, options.length - 1);
      $$('.dd-opt', pop).forEach((li, k) => li.classList.toggle('active', k === activeIdx));
      pop.setAttribute('aria-activedescendant', `${uid}-${activeIdx}`);
      if (scroll) scrollActive();
    }
    function scrollActive() { const li = pop && pop.children[activeIdx]; if (li) li.scrollIntoView({ block: 'nearest' }); }
    function choose(i) {
      const o = options[i];
      close();
      if (o && o.id !== selected) { selected = o.id; text.textContent = o.name; text.title = o.name; onSelect(o.id); }
    }
    function onKey(e) {
      e.stopPropagation();
      switch (e.key) {
        case 'ArrowDown': e.preventDefault(); setActive(activeIdx + 1); break;
        case 'ArrowUp': e.preventDefault(); setActive(activeIdx - 1); break;
        case 'Home': e.preventDefault(); setActive(0); break;
        case 'End': e.preventDefault(); setActive(options.length - 1); break;
        case 'Enter': case ' ': e.preventDefault(); choose(activeIdx); break;
        case 'Escape': e.preventDefault(); close(); break;
        case 'Tab': close(false); break;
        default:
          if (e.key.length === 1 && !e.ctrlKey && !e.altKey) {
            const now = Date.now();
            typed = now - typedAt > 700 ? e.key.toLowerCase() : typed + e.key.toLowerCase();
            typedAt = now;
            const i = options.findIndex((o) => o.name.toLowerCase().startsWith(typed));
            if (i >= 0) setActive(i);
          }
      }
    }
    btn.addEventListener('click', () => (pop ? close() : open()));
    btn.addEventListener('keydown', (e) => {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); open(); }
    });
    function setDisabled(on, tip) {
      disabled = on;
      btn.classList.toggle('is-disabled', on);
      if (on) btn.setAttribute('aria-disabled', 'true'); else btn.removeAttribute('aria-disabled');
      setTip(root, on ? tip : '');
      if (on) close(false);
    }
    const api = { root, btn, setOptions, setDisabled, close };
    return api;
  }
  document.addEventListener('pointerdown', (e) => {
    if (openDropdown && !e.target.closest('.dd-pop') && !openDropdown.root.contains(e.target)) openDropdown.close(false);
  }, true);
  window.addEventListener('resize', () => { if (openDropdown) openDropdown.close(false); redrawAllWaves(); });

  /* ───────────── tarjetas de fuente ───────────── */
  const SRC = {
    input: { cap: 'captureInput', dev: 'inputId', gain: 'inputGainDb', min: -12, max: 30, ticks: [0, 12], title: 'Микрофон', icon: 'mic', list: 'inputs', ddLabel: 'Устройство микрофона', swLabel: 'Записывать микрофон', gainLabel: 'Усиление микрофона' },
    output: { cap: 'captureOutput', dev: 'outputId', gain: 'outputGainDb', min: -12, max: 12, ticks: [0], title: 'Система', icon: 'speaker', list: 'outputs', ddLabel: 'Устройство вывода звука', swLabel: 'Записывать системный звук', gainLabel: 'Усиление системного звука' },
  };
  const UI = { input: {}, output: {} };
  const LOCK_DEV = 'Остановите запись, чтобы сменить устройство.';
  const LOCK_SRC = 'Остановите запись, чтобы изменить источники.';

  function buildSource(k) {
    const d = SRC[k], u = UI[k];
    u.sw = h('button', { class: 'switch', type: 'button', role: 'switch', 'aria-checked': 'true', 'aria-label': d.swLabel });
    u.db = h('span', { class: 'db', 'aria-hidden': 'true' });
    u.dd = createDropdown({ label: d.ddLabel, onSelect: (id) => updateSettings({ [d.dev]: id }) });
    u.fill = h('div', { class: 'meter-fill' });
    u.peak = h('div', { class: 'meter-peak' });
    u.meter = h('div', { class: 'meter', role: 'img', 'aria-label': `Nivel de ${d.title.toLowerCase()}` }, u.fill, u.peak);
    u.range = h('input', { type: 'range', min: String(d.min), max: String(d.max), step: '1', value: '0', 'aria-label': d.gainLabel, title: 'Doble clic para restablecer a 0 dB' });
    u.val = h('span', { class: 'val' }, '0 dB');
    const ticks = d.ticks.map((t) => {
      const el = h('span', { class: 'tick', 'aria-hidden': 'true' });
      el.style.left = `calc(7px + (100% - 14px) * ${(t - d.min) / (d.max - d.min)})`;
      return el;
    });
    const wrap = h('div', { class: 'range-wrap' }, ...ticks, u.range);
    const body = [u.dd.root, u.meter, h('div', { class: 'gain' }, h('span', { class: 'lbl' }, 'Усиление'), wrap, u.val)];
    if (k === 'input') {
      u.agcSw = h('button', { class: 'switch sm', type: 'button', role: 'switch', 'aria-checked': 'true', 'aria-labelledby': 'lblAgc' });
      u.agc = h('div', { class: 'agc', 'data-tip': 'Выравнивает громкость голоса' }, h('span', { class: 'lbl', id: 'lblAgc' }, 'Автоуровень голоса'), u.agcSw);
      u.warnTxt = h('span', { class: 't' });
      u.warnBtn = h('button', { class: 'btn sm', type: 'button' }, 'Установить 100 %');
      u.warn = h('div', { class: 'vol-warn', role: 'status', hidden: true }, u.warnTxt, u.warnBtn);
      body.push(u.warn);
      bindSwitch(u.agcSw, (on) => updateSettings({ inputAgc: on }));
      u.warnBtn.addEventListener('click', raiseMicVolume);
    }
    u.card = h('section', { class: 'card', 'aria-label': d.title },
      h('div', { class: 'source-head' }, h('span', { class: 'source-ico' }, icon(d.icon)), h('h3', { class: 'source-title' }, d.title), u.agc, u.db, u.sw),
      h('div', { class: 'source-body' }, ...body));
    bindSwitch(u.sw, (on) => updateSettings({ [d.cap]: on }));
    const setGain = (v, delay) => { setRange(k, v); updateSettings({ [d.gain]: v }, delay); };
    u.range.addEventListener('input', () => setGain(+u.range.value, 200));
    u.range.addEventListener('dblclick', () => setGain(0, 0));
    $('#sources').append(u.card);
  }
  const fmtDb = (v) => `${v > 0 ? '+' : v < 0 ? '\u2212' : ''}${Math.abs(v)} dB`;
  function setRange(k, v) {
    const u = UI[k], d = SRC[k];
    u.range.value = v;
    u.range.style.setProperty('--p', `${((v - d.min) / (d.max - d.min)) * 100}%`);
    u.range.setAttribute('aria-valuetext', fmtDb(v));
    u.val.textContent = fmtDb(v);
  }

  /* Volumen del micrófono en Windows (solo el dispositivo de captura). */
  const VOL = { level: null, muted: false, busy: false, inputId: null };
  async function refreshMicVolume() {
    if (VOL.busy) return;
    try {
      const v = await invoke('get_input_endpoint_volume');
      VOL.level = typeof v.level === 'number' ? v.level : null;
      VOL.muted = !!v.muted;
    } catch (_) { VOL.level = null; }
    renderMicVolume();
  }
  function renderMicVolume() {
    const u = UI.input;
    if (!u.warn) return;
    const bad = VOL.level != null && (VOL.muted || VOL.level < 0.7);
    u.warn.hidden = !(bad && S.settings.captureInput);
    if (bad) u.warnTxt.textContent = VOL.muted ? 'Микрофон отключён в Windows' : `Громкость микрофона в Windows: ${Math.round(VOL.level * 100)} %`;
  }
  async function raiseMicVolume() {
    VOL.busy = true;
    try {
      const v = await invoke('set_input_endpoint_volume', { level: 1 });
      VOL.level = v.level; VOL.muted = !!v.muted;
      toast({ msg: 'Громкость микрофона в Windows: 100 %' });
    } catch (e) { toastError(e); }
    VOL.busy = false;
    renderMicVolume();
  }

  function deviceOptions(k) {
    const d = SRC[k];
    const list = (S.devices[d.list] || []).map((x) => {
      const o = { id: x.id, name: x.name, meta: [x.sampleRate ? fmtRate(x.sampleRate) : '', x.channels ? fmtCh(x.channels) : ''].filter(Boolean).join(' · ') };
      if (x.id === 'default') {
        // Pseudo-dispositivo: nombre real del equipo + insignia «Predeterminado» (nunca paréntesis dobles).
        const m = /^Predeterminado del sistema \((.*)\)$/.exec(x.name || '');
        o.name = x.realName || (m ? m[1] : x.name);
        if (o.name === 'sin dispositivo') o.name = 'Нет устройства';
        o.badge = 'По умолчанию';
        o.meta = ['По умолчанию', o.meta].filter(Boolean).join(' · ');
      }
      return o;
    });
    const cur = S.settings[d.dev];
    if (cur && !list.some((o) => o.id === cur)) list.push({ id: cur, name: `${cur} (no disponible)`, meta: 'Desconectado' });
    return list;
  }

  const ddSig = { input: '', output: '' };
  function renderSources() {
    const locked = S.engine.status !== 'idle';
    for (const k of ['input', 'output']) {
      const d = SRC[k], u = UI[k];
      const on = !!S.settings[d.cap];
      u.card.classList.toggle('off', !on);
      setSwitch(u.sw, on, locked, LOCK_SRC);
      const opts = deviceOptions(k);
      const sig = JSON.stringify([opts, S.settings[d.dev]]);
      if (sig !== ddSig[k]) { ddSig[k] = sig; u.dd.setOptions(opts, S.settings[d.dev]); }
      u.dd.setDisabled(locked, LOCK_DEV);
      u.range.disabled = !on;
      const v = Math.round(clamp(+S.settings[d.gain] || 0, d.min, d.max));
      if (+u.range.value !== v || !u.range.style.getPropertyValue('--p')) setRange(k, v);
    }
    setSwitch(UI.input.agcSw, S.settings.inputAgc !== false, false);
    if (VOL.inputId !== S.settings.inputId) { VOL.inputId = S.settings.inputId; refreshMicVolume(); }
    renderMicVolume();
  }

  /* ───────────── modo / ajustes ───────────── */
  let modeSeg, fmtSeg, themeSeg, tabsSeg;
  function renderMode() {
    const { captureInput: i, captureOutput: o, mode } = S.settings;
    modeSeg.set(mode);
    const locked = S.engine.status !== 'idle';
    modeSeg.setDisabled(locked, 'Остановите запись, чтобы изменить режим.');
    const ext = S.settings.format === 'mp3' ? 'mp3' : 'wav';
    let html;
    if (!i && !o) html = 'Включите хотя бы один источник для записи.';
    else if (!(i && o)) html = `Один источник: <code>${i ? 'recmic' : 'recsys'}_ГГГГ-ММ-ДД_ЧЧ-ММ.${ext}</code>. ${ext === 'mp3' ? 'Сразу один MP3 в выбранной папке, без временной папки и WAV.' : 'Выбран WAV: во время записи используется папка сессии; для прямого одиночного MP3 выберите MP3.'}`;
    else if (mode === 'separate') html = `Два отдельных файла: <code>Microfono.${ext}</code> и <code>Sistema.${ext}</code>.`;
    else if (mode === 'mixed') html = `Один файл с обоими источниками: <code>Mezcla.${ext}</code>.`;
    else html = `Три файла: <code>Microfono.${ext}</code>, <code>Sistema.${ext}</code> и микс <code>Mezcla.${ext}</code>.`;
    if (i && o) html += ' Выбраны два источника: используется папка сессии. Для одного прямого MP3 отключите один источник.';
    const help = $('#modeHelp');
    if (help.innerHTML !== html) help.innerHTML = html;
  }
  function renderSettingsView() {
    const locked = S.engine.status !== 'idle';
    const dir = S.settings.outputDir || '';
    const p = $('#outPath');
    p.textContent = midTruncate(dir, 46) || 'Sin definir';
    p.title = dir;
    fmtSeg.set(S.settings.format);
    fmtSeg.setDisabled(locked, 'Остановите запись, чтобы изменить формат.');
    themeSeg.set(S.settings.theme);
    setSwitch($('#swMonitor'), !!S.settings.monitorWhenIdle, false);
    setSwitch($('#swAgc'), S.settings.inputAgc !== false, false);
    setSwitch($('#swDuck'), S.settings.mixVoicePriority !== false, false);
    setSwitch($('#swCloseTray'), !!S.prefs.closeToTray, false);
    setSwitch($('#swAutostart'), !!S.prefs.launchAtStartup, false);
    setSwitch($('#swPill'), !!S.prefs.floatingPill, false);
    const ch = $('#btnChoose');
    ch.classList.toggle('is-disabled', locked);
    if (locked) ch.setAttribute('aria-disabled', 'true'); else ch.removeAttribute('aria-disabled');
    setTip(ch, locked ? 'Остановите запись, чтобы изменить папку.' : '');
    renderHotkeys();
  }
  function renderSettingsAll() {
    renderSources();
    renderMode();
    renderSettingsView();
    renderEngine();
    applyTheme();
  }

  /* ───────────── guardado de ajustes ───────────── */
  let saveTimer = null, saving = false, dirty = false, resave = false, saveTask = null;
  function updateSettings(patch, delay = 0) {
    if (!S.settingsReady) { toastError('Дождитесь загрузки сохранённых настроек.'); return; }
    Object.assign(S.settings, patch);
    dirty = true;
    if (patch.theme !== undefined) applyTheme();
    renderSources(); renderMode(); renderSettingsView(); renderEngine();
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flushSettings, delay);
  }
  function flushSettings() {
    clearTimeout(saveTimer);
    if (saving) { resave = true; return saveTask; }
    if (!dirty) return Promise.resolve(true);
    saving = true;
    saveTask = (async () => {
      let success = true;
      do {
      resave = false; dirty = false;
      try {
        const res = await invoke('set_settings', { settings: { ...S.settings } });
        if (!dirty && res) S.settings = { ...DEFAULTS, ...res };
      } catch (e) {
        success = false;
        toastError(e);
        try { S.settings = { ...DEFAULTS, ...(await invoke('get_settings')) }; } catch (_) { /* se mantiene lo local */ }
        dirty = false;
      }
      } while (resave || dirty);
      return success;
    })().finally(() => {
      saving = false;
      renderSettingsAll();
    });
    return saveTask;
  }

  /* ───────────── ventana y bandeja (app-prefs) ───────────── */
  async function updatePrefs(patch) {
    S.prefs = { ...S.prefs, ...patch };
    renderSettingsView();
    try { S.prefs = { ...PREF_DEFAULTS, ...(await invoke('set_app_prefs', { prefs: { ...S.prefs } })) }; }
    catch (e) {
      toastError(e);
      try { S.prefs = { ...PREF_DEFAULTS, ...(await invoke('get_app_prefs')) }; } catch (_) { /* se mantiene lo local */ }
    }
    renderSettingsView();
  }

  /* ───────────── motor: estado y acciones ───────────── */
  function setEngine(st) {
    S.engine = { status: st.status, elapsedMs: st.elapsedMs || 0, recordingId: st.recordingId || null };
    S.engineAt = performance.now();
    renderSources(); renderMode(); renderSettingsView(); renderEngine();
  }
  const STATUS_TEXT = { idle: 'Готово к записи', recording: 'Идёт запись', paused: 'Пауза', finalizing: 'Завершение…' };
  function renderEngine() {
    const st = S.engine.status;
    app.dataset.status = st;
    const noSrc = !S.settings.captureInput && !S.settings.captureOutput;
    $('#statusText').textContent = st === 'idle' && noSrc ? 'Elige una fuente' : STATUS_TEXT[st] || '';
    $('#statusDot').setAttribute('aria-label', { idle: 'Неактивно', recording: 'Идёт запись', paused: 'Пауза', finalizing: 'Завершение' }[st] || '');
    const rec = $('#btnRec');
    const recDisabled = st === 'finalizing' || (st === 'idle' && (noSrc || !S.settingsReady));
    rec.classList.toggle('is-disabled', recDisabled);
    if (recDisabled) rec.setAttribute('aria-disabled', 'true'); else rec.removeAttribute('aria-disabled');
    const recLabel = st === 'idle' ? 'Начать запись' : st === 'finalizing' ? 'Завершение записи' : 'Остановить запись';
    rec.setAttribute('aria-label', recLabel);
    setTip($('#recWrap'), st === 'idle' && noSrc ? 'Activa al menos una fuente para grabar.' : st === 'finalizing' ? 'Guardando los archivos…' : `${recLabel} (Espacio)`);
    const pa = $('#btnPause');
    const canPause = st === 'recording' || st === 'paused';
    pa.classList.toggle('is-disabled', !canPause);
    if (canPause) pa.removeAttribute('aria-disabled'); else pa.setAttribute('aria-disabled', 'true');
    const paLabel = st === 'paused' ? 'Продолжить' : 'Пауза';
    pa.setAttribute('aria-label', paLabel);
    setTip($('#pauseWrap'), canPause ? `${paLabel} (P)` : 'Доступно во время записи.');
    const want = st === 'paused' ? 'play' : 'pause';
    const pi = $('#pauseIco');
    if (pi.dataset.icon !== want) { pi.dataset.icon = want; pi.innerHTML = svgOf(want); }
    $('#statusDot').title = '';
  }

  let busy = false;
  async function toggleRecord() {
    if (busy) return;
    const st = S.engine.status;
    const noSrc = !S.settings.captureInput && !S.settings.captureOutput;
    if (st === 'finalizing') return;
    if (st === 'idle' && !S.settingsReady) { toastError('Сохранённые настройки ещё не загружены; запись не начата.'); return; }
    if (st === 'idle' && noSrc) { toast({ msg: 'Activa al menos una fuente para grabar.', kind: 'error' }); return; }
    busy = true;
    try {
      if (st === 'idle') {
        if (!(await flushSettings())) throw new Error('Настройки не сохранены. Запись не начата; проверьте формат и источники.');
        setEngine(await invoke('start_recording', { settings: { ...S.settings } }));
      } else {
        setEngine({ ...S.engine, status: 'finalizing' });
        const rec = await invoke('stop_recording');
        onSaved(rec);
        setEngine(await invoke('get_state'));
      }
    } catch (e) {
      toastError(e);
      try { setEngine(await invoke('get_state')); } catch (_) { /* sin estado */ }
    } finally { busy = false; }
  }
  async function pauseResume() {
    if (busy) return;
    const st = S.engine.status;
    if (st !== 'recording' && st !== 'paused') return;
    busy = true;
    try { setEngine(await (st === 'recording' ? invoke('pause_recording') : invoke('resume_recording'))); }
    catch (e) { toastError(e); }
    finally { busy = false; }
  }

  /* ───────────── medidores, anillo y temporizador (rAF) ───────────── */
  const mk = () => ({ tgt: 0, pk: 0, active: false, disp: 0, peakDisp: 0, hold: 0, at: 0, dbTxt: '' });
  const M = { input: mk(), output: mk() };
  const toLvl = (v) => (v > 0 ? clamp((20 * Math.log10(v) + 60) / 60, 0, 1) : 0);
  function onLevels(p) {
    const now = performance.now();
    for (const k of ['input', 'output']) {
      const s = p && p[k]; if (!s) continue;
      M[k].tgt = toLvl(s.rms); M[k].pk = toLvl(s.peak); M[k].active = !!s.active; M[k].at = now;
    }
  }
  let lastFrame = performance.now(), lastDbAt = 0, lastTimer = '', lastLvl = -1;
  function frame(now) {
    const dt = Math.min(64, now - lastFrame); lastFrame = now;
    let ringLvl = 0;
    const st = S.engine.status;
    const live = st === 'recording' || st === 'paused' || S.settings.monitorWhenIdle;
    for (const k of ['input', 'output']) {
      const m = M[k], u = UI[k], on = !!S.settings[SRC[k].cap];
      const fresh = now - m.at < 400 && m.active && on && live;
      const tgt = fresh ? m.tgt : 0, pk = fresh ? m.pk : 0;
      m.disp += (tgt - m.disp) * (1 - Math.exp(-dt / (tgt > m.disp ? 35 : 230)));
      if (m.disp < 0.002) m.disp = 0;
      if (pk >= m.peakDisp) { m.peakDisp = pk; m.hold = now + 1000; }
      else if (now > m.hold) m.peakDisp = Math.max(m.disp, m.peakDisp - dt * 0.0006);
      m.peakDisp = Math.max(m.peakDisp, m.disp);
      u.fill.style.clipPath = `inset(0 ${((1 - m.disp) * 100).toFixed(2)}% 0 0 round 3px)`;
      u.peak.style.left = `${(m.peakDisp * 100).toFixed(2)}%`;
      u.peak.style.opacity = m.peakDisp > 0.01 ? '1' : '0';
      if (on) ringLvl = Math.max(ringLvl, m.disp);
      if (now - lastDbAt > 100) {
        const t = fresh && m.tgt > 0 ? `\u2212${Math.max(0, Math.round(60 - m.tgt * 60))} dB`.replace('\u22120 dB', '0 dB') : on ? '' : '';
        if (t !== m.dbTxt) { m.dbTxt = t; u.db.textContent = t; }
      }
    }
    if (now - lastDbAt > 100) lastDbAt = now;
    const lv = Math.round(ringLvl * 100) / 100;
    if (lv !== lastLvl) { lastLvl = lv; $('#recWrap').style.setProperty('--lvl', lv); }
    const ms = S.engine.status === 'idle' ? 0 : S.engine.status === 'recording' ? S.engine.elapsedMs + (now - S.engineAt) : S.engine.elapsedMs;
    const t = fmtTimer(ms);
    const key = `${t.main}.${t.dec}`;
    if (key !== lastTimer) { lastTimer = key; $('#tMain').textContent = t.main; $('#tDec').textContent = `.${t.dec}`; }
    requestAnimationFrame(frame);
  }

  /* ───────────── grabaciones ───────────── */
  const TRACK = { input: { chip: 'Mic', name: 'Микрофон' }, output: { chip: 'Система', name: 'Система' }, mix: { chip: 'Микс', name: 'Микс' } };
  const KIND_ORDER = { input: 0, output: 1, mix: 2 };
  const cards = new Map();
  const peaksCache = new Map();
  let currentPlayer = null;

  const sortedFiles = (rec) => [...(rec.files || [])].sort((a, b) => KIND_ORDER[a.kind] - KIND_ORDER[b.kind]);
  const primaryFile = (rec) => (rec.files || []).find((f) => f.kind === 'mix') || (rec.files || [])[0];
  const revealRec = (rec) => {
    const f = primaryFile(rec);
    return invoke('reveal_path', { path: f ? f.path : rec.folder }).catch(toastError);
  };

  let wavColors = null;
  function colors() {
    if (!wavColors || !colorCache) {
      const cs = getComputedStyle(document.documentElement);
      wavColors = { played: cs.getPropertyValue('--text').trim() || '#fff', rest: cs.getPropertyValue('--faint').trim() || '#666' };
      colorCache = true;
    }
    return wavColors;
  }
  const N_PEAKS = 480;
  const DECODE_MAX = 40 * 1024 * 1024; // hasta aquí: decodeAudioData completo; por encima: lectura en streaming del WAV

  async function decodePeaks(url) {
    const res = await fetch(url);
    if (!res.ok) throw new Error('HTTP ' + res.status);
    const ab = await new OfflineAudioContext(1, 1, 22050).decodeAudioData(await res.arrayBuffer());
    const out = new Float32Array(N_PEAKS), step = ab.length / N_PEAKS;
    const chs = Array.from({ length: ab.numberOfChannels }, (_, i) => ab.getChannelData(i));
    for (let i = 0; i < N_PEAKS; i++) {
      const s = Math.floor(i * step), e = Math.max(s + 1, Math.floor((i + 1) * step));
      let m = 0;
      for (const c of chs) for (let j = s; j < e; j++) { const v = Math.abs(c[j]); if (v > m) m = v; }
      out[i] = m;
    }
    return out;
  }

  // Archivos grandes: se recorre el WAV por trozos (sin cargarlo entero en memoria) y se calculan picos por tramo.
  async function streamPeaks(url) {
    const res = await fetch(url);
    if (!res.ok || !res.body) throw new Error('HTTP ' + res.status);
    const reader = res.body.getReader();
    const contentLen = +res.headers.get('content-length') || 0;
    const cat = (a, b) => { const m = new Uint8Array(a.length + b.length); m.set(a); m.set(b, a.length); return m; };
    let head = new Uint8Array(0), fmt = null, dataStart = -1, dataSize = 0;
    const parseHead = () => {
      const dv = new DataView(head.buffer, head.byteOffset, head.byteLength);
      if (head.length < 12) return false;
      let pos = 12;
      while (pos + 8 <= head.length) {
        const id = String.fromCharCode(head[pos], head[pos + 1], head[pos + 2], head[pos + 3]);
        const size = dv.getUint32(pos + 4, true);
        if (id === 'fmt ') {
          if (pos + 8 + 16 > head.length) return false;
          let tag = dv.getUint16(pos + 8, true);
          const bits = dv.getUint16(pos + 22, true);
          if (tag === 0xfffe && pos + 8 + 26 <= head.length) tag = dv.getUint16(pos + 8 + 24, true);
          fmt = { tag, ch: dv.getUint16(pos + 10, true), bits };
        } else if (id === 'data') { dataStart = pos + 8; dataSize = size; return !!fmt; }
        pos += 8 + size + (size & 1);
      }
      return false;
    };
    let ready = false, rd, bps, frameBytes, totalFrames, fpb, stride, f = 0, carry = null;
    const out = new Float32Array(N_PEAKS);
    const setup = () => {
      if (!fmt || (fmt.tag !== 1 && fmt.tag !== 3) || ![16, 24, 32].includes(fmt.bits)) throw new Error('Formato WAV no admitido');
      bps = fmt.bits / 8; frameBytes = bps * fmt.ch;
      const avail = contentLen ? contentLen - dataStart : Infinity;
      const bytes = dataSize === 0 || dataSize === 0xffffffff || dataSize > avail ? avail : dataSize;
      totalFrames = Math.floor(bytes / frameBytes);
      if (!isFinite(totalFrames) || totalFrames <= 0) throw new Error('WAV vacío');
      fpb = totalFrames / N_PEAKS; stride = Math.max(1, Math.floor(fpb / 1500));
      rd = fmt.bits === 16 ? (dv, o) => dv.getInt16(o, true) / 32768
        : fmt.bits === 24 ? (dv, o) => ((dv.getUint8(o) | (dv.getUint8(o + 1) << 8) | (dv.getInt8(o + 2) << 16)) / 8388608)
        : fmt.tag === 3 ? (dv, o) => dv.getFloat32(o, true) : (dv, o) => dv.getInt32(o, true) / 2147483648;
      ready = true;
    };
    const processBytes = (bytes) => {
      if (carry && carry.length) { bytes = cat(carry, bytes); carry = null; }
      const nF = Math.floor(bytes.length / frameBytes);
      const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
      for (let i = 0; i < nF && f < totalFrames; i++, f++) {
        if (f % stride) continue;
        let m = 0;
        for (let c = 0; c < fmt.ch; c++) { const v = Math.abs(rd(dv, i * frameBytes + c * bps)); if (v > m) m = v; }
        const k = Math.min(N_PEAKS - 1, Math.floor(f / fpb));
        if (m > out[k]) out[k] = m;
      }
      if (nF * frameBytes < bytes.length) carry = bytes.slice(nF * frameBytes);
    };
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      if (!ready) {
        head = cat(head, value);
        if (parseHead()) { setup(); processBytes(head.subarray(dataStart)); head = null; }
        else if (head.length > 262144) throw new Error('Cabecera WAV no válida');
      } else processBytes(value);
      if (ready && f >= totalFrames) { reader.cancel().catch(() => {}); break; }
    }
    if (!ready) throw new Error('WAV no válido');
    return out;
  }

  async function getPeaks(file) {
    const key = file.path + '|' + file.sizeBytes;
    if (peaksCache.has(key)) return peaksCache.get(key);
    const url = convertFileSrc(file.path);
    const job = (file.sizeBytes <= DECODE_MAX ? decodePeaks(url).catch(() => streamPeaks(url)) : streamPeaks(url)).catch(() => null);
    peaksCache.set(key, job);
    return job;
  }

  const players = new Set();
  if (window.__ECO_MOCK__) window.__ecoPlayers = players;
  function redrawAllWaves() { for (const P of players) drawWave(P); }

  function drawWave(P) {
    const cv = P.canvas, w = P.wave.clientWidth, hh = P.wave.clientHeight;
    if (!w || !hh) return;
    const dpr = window.devicePixelRatio || 1;
    if (cv.width !== Math.round(w * dpr) || cv.height !== Math.round(hh * dpr)) { cv.width = Math.round(w * dpr); cv.height = Math.round(hh * dpr); }
    const g = cv.getContext('2d');
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, hh);
    const C = colors();
    const barW = 2, gap = 2, n = Math.floor((w + gap) / (barW + gap)), mid = hh / 2;
    const peaks = P.peaks;
    let norm = 1;
    if (peaks) { let mx = 0; for (const v of peaks) if (v > mx) mx = v; norm = 1 / Math.max(mx, 0.08); }
    const playedX = P.prog * w;
    for (let i = 0; i < n; i++) {
      let v = 0;
      if (peaks) {
        const a = Math.floor((i / n) * peaks.length), b = Math.max(a + 1, Math.floor(((i + 1) / n) * peaks.length));
        for (let j = a; j < b; j++) if (peaks[j] > v) v = peaks[j];
        v = Math.pow(Math.min(1, v * norm), 0.85);
      }
      const x = i * (barW + gap), bh = Math.max(2, v * (hh - 4));
      g.globalAlpha = peaks || P.failed ? 1 : 0.5;
      g.fillStyle = x + barW / 2 <= playedX ? C.played : C.rest;
      g.beginPath();
      if (g.roundRect) g.roundRect(x, mid - bh / 2, barW, bh, 1); else g.rect(x, mid - bh / 2, barW, bh);
      g.fill();
    }
    g.globalAlpha = 1;
  }

  function playerDur(P) { return P.audio && isFinite(P.audio.duration) && P.audio.duration > 0 ? P.audio.duration : P.dur; }
  function updatePlayerUI(P) {
    const dur = playerDur(P), cur = P.audio ? P.audio.currentTime : 0;
    P.prog = dur ? clamp(cur / dur, 0, 1) : 0;
    P.time.textContent = `${fmtClock(cur)} / ${fmtClock(dur)}`;
    P.wave.setAttribute('aria-valuemax', String(Math.round(dur)));
    P.wave.setAttribute('aria-valuenow', String(Math.round(cur)));
    P.wave.setAttribute('aria-valuetext', `${fmtClock(cur)} de ${fmtClock(dur)}`);
    drawWave(P);
  }
  function ensureAudio(P) {
    if (P.audio) return P.audio;
    const a = new Audio();
    a.preload = 'metadata';
    a.src = convertFileSrc(P.file.path);
    a.addEventListener('loadedmetadata', () => updatePlayerUI(P));
    a.addEventListener('play', () => { P.playBtn.innerHTML = ''; P.playBtn.append(icon('pause')); P.playBtn.setAttribute('aria-label', `Pausar ${P.label}`); playLoop(P); });
    const stopped = () => { P.playBtn.innerHTML = ''; P.playBtn.append(icon('play')); P.playBtn.setAttribute('aria-label', `Reproducir ${P.label}`); updatePlayerUI(P); };
    a.addEventListener('pause', stopped);
    a.addEventListener('ended', () => { a.currentTime = 0; stopped(); });
    a.addEventListener('error', () => { stopped(); toast({ msg: 'No se pudo reproducir este archivo.', kind: 'error' }); });
    P.audio = a;
    return a;
  }
  function playLoop(P) {
    const tick = () => {
      if (!P.audio || P.audio.paused) return;
      updatePlayerUI(P);
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }
  function togglePlay(P) {
    const a = ensureAudio(P);
    if (a.paused) {
      if (currentPlayer && currentPlayer !== P && currentPlayer.audio) currentPlayer.audio.pause();
      currentPlayer = P;
      a.play().catch((e) => { if (e && e.name !== 'AbortError') toast({ msg: 'No se pudo reproducir este archivo.', kind: 'error' }); });
    } else a.pause();
  }
  function seekTo(P, frac) {
    const a = ensureAudio(P), dur = playerDur(P);
    if (!dur) return;
    a.currentTime = clamp(frac, 0, 1) * dur;
    updatePlayerUI(P);
  }
  function disposePlayer(P) {
    if (P.audio) { P.audio.pause(); P.audio.removeAttribute('src'); P.audio.load(); }
    if (currentPlayer === P) currentPlayer = null;
    P.ro.disconnect();
    players.delete(P);
  }

  function buildPlayer(rec, file) {
    const label = TRACK[file.kind].name.toLowerCase();
    const P = { rec, file, label, audio: null, peaks: null, failed: false, prog: 0, dur: (rec.durationMs || 0) / 1000 };
    P.playBtn = h('button', { class: 'play-btn', type: 'button', 'aria-label': `Reproducir ${label}` }, icon('play'));
    P.canvas = h('canvas', { 'aria-hidden': 'true' });
    P.wave = h('div', { class: 'wave', role: 'slider', tabindex: '0', 'aria-label': `Posición de ${label}`, 'aria-valuemin': '0', 'aria-valuemax': String(Math.round(P.dur)), 'aria-valuenow': '0' }, P.canvas);
    P.time = h('span', { class: 'time' }, `0:00 / ${fmtClock(P.dur)}`);
    P.el = h('div', { class: 'track' },
      h('div', { class: 'track-top' }, h('span', { class: 'track-name' }, TRACK[file.kind].name),
        h('span', { class: 'track-meta' }, [fmtRate(file.sampleRate), fmtCh(file.channels), fmtBytes(file.sizeBytes)].join(' · '))),
      h('div', { class: 'player' }, P.playBtn, P.wave, P.time));
    P.playBtn.addEventListener('click', () => togglePlay(P));
    const seekFromEvent = (e) => { const r = P.wave.getBoundingClientRect(); seekTo(P, (e.clientX - r.left) / r.width); };
    P.wave.addEventListener('pointerdown', (e) => { P.wave.setPointerCapture(e.pointerId); P.dragging = true; seekFromEvent(e); });
    P.wave.addEventListener('pointermove', (e) => { if (P.dragging) seekFromEvent(e); });
    const endDrag = () => { P.dragging = false; };
    P.wave.addEventListener('pointerup', endDrag);
    P.wave.addEventListener('pointercancel', endDrag);
    P.wave.addEventListener('keydown', (e) => {
      const dur = playerDur(P), cur = P.audio ? P.audio.currentTime : 0;
      let handled = true;
      if (e.key === 'ArrowRight') seekTo(P, (cur + 5) / dur);
      else if (e.key === 'ArrowLeft') seekTo(P, (cur - 5) / dur);
      else if (e.key === 'Home') seekTo(P, 0);
      else if (e.key === 'End') seekTo(P, 1);
      else if (e.key === ' ' || e.key === 'Enter') togglePlay(P);
      else handled = false;
      if (handled) { e.preventDefault(); e.stopPropagation(); }
    });
    P.ro = new ResizeObserver(() => drawWave(P));
    P.ro.observe(P.wave);
    players.add(P);
    getPeaks(file).then((pk) => { P.peaks = pk; P.failed = !pk; drawWave(P); });
    return P;
  }

  function buildCard(rec) {
    const e = { rec, open: false, players: [], built: false, sig: '', chipSig: '', editing: false };
    e.name = h('span', { class: 'rec-name', title: 'Doble clic para renombrar' });
    e.edit = h('button', { class: 'icon-btn sm rec-edit', type: 'button' }, icon('pencil'));
    e.title = h('div', { class: 'rec-title' }, e.name, e.edit);
    e.meta = h('div', { class: 'rec-meta' });
    e.chips = h('div', { class: 'chips' });
    e.chev = h('button', { class: 'icon-btn sm chev-btn', type: 'button', 'aria-expanded': 'false' }, icon('chevron'));
    e.tracks = h('div', { class: 'rec-tracks' });
    e.reveal = h('button', { class: 'btn ghost', type: 'button' }, icon('folder'), 'Показать в папке');
    e.del = h('button', { class: 'btn ghost danger', type: 'button' }, icon('trash'), 'Удалить');
    e.inner = h('div', { class: 'rec-inner' }, e.tracks, h('div', { class: 'rec-actions' }, e.reveal, e.del));
    e.inner.inert = true;
    const bodyId = `body-${rec.id}`;
    e.inner.id = bodyId;
    e.chev.setAttribute('aria-controls', bodyId);
    e.head = h('div', { class: 'rec-head' }, h('div', { class: 'rec-main' }, e.title, e.meta, e.chips), e.chev);
    e.el = h('article', { class: 'rec-item' }, e.head, h('div', { class: 'rec-collapse' }, e.inner));

    e.head.addEventListener('click', (ev) => { if (ev.target.closest('button,input')) return; if (e.editing) return; toggleOpen(e); });
    e.chev.addEventListener('click', () => toggleOpen(e));
    e.name.addEventListener('dblclick', () => startRename(e));
    e.edit.addEventListener('click', () => startRename(e));
    e.reveal.addEventListener('click', () => revealRec(e.rec));
    e.del.addEventListener('click', () => askDelete(e));
    return e;
  }

  function updateCard(e, rec) {
    e.rec = rec;
    if (!e.editing) {
      e.name.textContent = rec.name;
      e.edit.setAttribute('aria-label', `Renombrar «${rec.name}»`);
      e.edit.setAttribute('data-tip', 'Renombrar');
    }
    e.meta.textContent = `${fmtWhen(rec.createdAt)} · ${fmtClock((rec.durationMs || 0) / 1000)}`;
    e.chev.setAttribute('aria-label', `${e.open ? 'Ocultar' : 'Показать'} pistas de «${rec.name}»`);
    const files = sortedFiles(rec);
    const chipSig = files.map((f) => f.kind).join(',');
    if (chipSig !== e.chipSig) {
      e.chipSig = chipSig;
      e.chips.replaceChildren(...files.map((f) => h('span', { class: 'chip' + (f.kind === 'mix' ? ' mix' : '') }, TRACK[f.kind].chip)));
      if (!files.length) e.chips.append(h('span', { class: 'chip' }, 'Sin archivos'));
    }
    const sig = files.map((f) => `${f.path}|${f.sizeBytes}`).join(';');
    if (sig !== e.sig) {
      e.sig = sig;
      if (e.built) { disposeCardPlayers(e); e.built = false; if (e.open) ensurePlayers(e); }
    }
  }
  function disposeCardPlayers(e) { e.players.forEach(disposePlayer); e.players = []; e.tracks.replaceChildren(); }
  function ensurePlayers(e) {
    if (e.built) return;
    e.built = true;
    const files = sortedFiles(e.rec);
    e.players = files.map((f) => buildPlayer(e.rec, f));
    e.tracks.replaceChildren(...e.players.map((p) => p.el));
    if (!files.length) e.tracks.append(h('p', { class: 'help' }, 'В этой записи нет аудиофайлов.'));
  }
  function toggleOpen(e, force) {
    e.open = force ?? !e.open;
    e.el.classList.toggle('open', e.open);
    e.inner.inert = !e.open;
    e.chev.setAttribute('aria-expanded', String(e.open));
    e.chev.setAttribute('aria-label', `${e.open ? 'Ocultar' : 'Показать'} pistas de «${e.rec.name}»`);
    if (e.open) ensurePlayers(e);
    else e.players.forEach((p) => p.audio && !p.audio.paused && p.audio.pause());
  }

  function startRename(e) {
    if (e.editing) return;
    e.editing = true;
    const input = h('input', { class: 'rec-name-input', type: 'text', value: e.rec.name, maxlength: '120', 'aria-label': 'Название записи', spellcheck: 'false' });
    e.title.replaceChildren(input);
    input.focus(); input.select();
    let finished = false;
    const finish = async (commit) => {
      if (finished) return; finished = true;
      const val = input.value.trim();
      e.editing = false;
      e.title.replaceChildren(e.name, e.edit);
      if (commit && val && val !== e.rec.name) {
        e.name.textContent = val;
        try { upsertRecording(await invoke('rename_recording', { id: e.rec.id, name: val })); }
        catch (err) { toastError(err); e.name.textContent = e.rec.name; }
      }
      e.edit.focus();
    };
    input.addEventListener('keydown', (ev) => {
      ev.stopPropagation();
      if (ev.key === 'Enter') { ev.preventDefault(); finish(true); }
      else if (ev.key === 'Escape') { ev.preventDefault(); finish(false); }
    });
    input.addEventListener('blur', () => finish(true));
  }

  async function askDelete(e) {
    const rec = e.rec;
    const body = h('p', null, 'Будут удалены аудиофайлы записи ', h('span', { class: 'rn' }, `«${rec.name}»`), '. Если возможно, файлы попадут в корзину; иначе будут удалены безвозвратно. Папка сохранения и посторонние файлы не удаляются.');
    const ok = await confirmDialog({ title: 'Удалить эту запись?', body, confirmLabel: 'Удалить' });
    if (!ok) { e.del.focus(); return; }
    try {
      await invoke('delete_recording', { id: rec.id });
      S.recordings = S.recordings.filter((r) => r.id !== rec.id);
      renderRecordings();
      toast({ msg: 'Запись удалена.' });
    } catch (err) { toastError(err); }
  }

  function upsertRecording(rec) {
    const i = S.recordings.findIndex((r) => r.id === rec.id);
    if (i >= 0) S.recordings[i] = rec; else S.recordings.unshift(rec);
    S.recordings.sort((a, b) => (a.createdAt < b.createdAt ? 1 : a.createdAt > b.createdAt ? -1 : 0));
    renderRecordings();
  }
  function renderRecordings() {
    const list = $('#recList'), recs = S.recordings;
    $('#recEmpty').classList.toggle('show', recs.length === 0);
    $('#recToolbar').style.display = recs.length ? '' : 'none';
    $('#recCount').textContent = `Записей: ${recs.length}`;
    const els = [], seen = new Set();
    for (const rec of recs) {
      let e = cards.get(rec.id);
      if (!e) { e = buildCard(rec); cards.set(rec.id, e); }
      updateCard(e, rec);
      seen.add(rec.id); els.push(e.el);
    }
    for (const [id, e] of cards) if (!seen.has(id)) { disposeCardPlayers(e); cards.delete(id); }
    const same = list.children.length === els.length && els.every((el, i) => list.children[i] === el);
    if (!same) list.replaceChildren(...els);
  }
  async function refreshRecordings() {
    try { S.recordings = await invoke('list_recordings'); renderRecordings(); }
    catch (e) { toastError(e); }
  }

  const toasted = new Set();
  function onSaved(rec) {
    if (!rec) return;
    upsertRecording(rec);
    if (toasted.has(rec.id)) return;
    toasted.add(rec.id);
    toast({ msg: `Grabación guardada · ${fmtClock((rec.durationMs || 0) / 1000)}`, action: { label: 'Показать', fn: () => revealRec(rec) } });
  }

  /* ───────────── navegación ───────────── */
  function setView(v) {
    S.view = v;
    for (const id of ['record', 'recordings', 'settings']) {
      const on = id === v;
      const sec = $(`#view-${id}`);
      sec.classList.toggle('active', on);
      sec.inert = !on;
    }
    tabsSeg.set(v === 'settings' ? null : v);
    $('#btnSettings').setAttribute('aria-pressed', String(v === 'settings'));
    hideTip();
    if (v === 'recordings') refreshRecordings();
  }

  /* ───────────── atajos globales configurables ───────────── */
  const HK_DEFAULT = { toggle: 'Ctrl+Alt+R', pause: 'Ctrl+Alt+P' };
  const HK_FIELD = { toggle: 'hotkeyToggle', pause: 'hotkeyPause' };
  const HK_CODE_OK = /^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-9]|2[0-4])|Space|Enter|Tab|Backspace|Delete|Insert|Home|End|PageUp|PageDown|Arrow(Up|Down|Left|Right)|Numpad[A-Za-z0-9]+|Backquote|Backslash|Bracket(Left|Right)|Comma|Equal|Minus|Period|Quote|Semicolon|Slash|Pause|PrintScreen|ScrollLock|CapsLock|NumLock)$/;
  const HK_PRETTY = { Super: 'Win', Space: 'Пробел', ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→', Backquote: '`', BracketLeft: '[', BracketRight: ']', Comma: ',', Equal: '=', Minus: '-', Period: '.', Quote: "'", Semicolon: ';', Slash: '/', PageUp: 'Re Pág', PageDown: 'Av Pág', Delete: 'Supr', Insert: 'Ins', Backspace: 'Retroceso' };
  const hkParts = (accel) => String(accel || '').split('+').filter(Boolean);
  const hkAccel = (which) => (S.hotkeys && S.hotkeys[which] && S.hotkeys[which].accel) || S.settings[HK_FIELD[which]] || HK_DEFAULT[which];
  const hkLabel = (accel) => hkParts(accel).map((p) => HK_PRETTY[p] || p).join('+');
  const hkEls = {};

  function codeToToken(code) {
    if (/^Key[A-Z]$/.test(code)) return code.slice(3);
    if (/^Digit[0-9]$/.test(code)) return code.slice(5);
    return code;
  }
  function endCapture() {
    if (!S.capturing) return;
    S.capturing = null; S.capHint = '';
    renderHotkeys();
  }
  function startCapture(which) {
    if (S.capturing === which) { endCapture(); return; }
    S.capturing = which; S.capHint = '';
    renderHotkeys();
    hkEls[which].btn.focus();
  }
  function onCaptureKey(which, e) {
    if (S.capturing !== which) return;
    e.preventDefault(); e.stopPropagation();
    if (e.repeat) return;
    const bare = !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey;
    if (e.code === 'Escape' && bare) { endCapture(); return; }
    if (/^(Control|Alt|Shift|Meta|OS)/.test(e.code)) return; // solo modificadores: se sigue esperando
    const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super'].filter(Boolean);
    if (!mods.length) { S.capHint = 'Añade Ctrl, Alt o Mayús a la combinación.'; renderHotkeys(); return; }
    if (!HK_CODE_OK.test(e.code)) { S.capHint = 'Esa tecla no se puede usar en un atajo.'; renderHotkeys(); return; }
    const accel = [...mods, codeToToken(e.code)].join('+');
    hkEls[which].swallowUntil = Date.now() + 400; // evita que el keyup de Espacio/Enter reactive el botón
    endCapture();
    applyHotkey(which, accel);
  }
  async function refreshHotkeys() {
    try { S.hotkeys = await invoke('get_hotkey_status'); } catch (_) { /* sin estado de atajos */ }
  }
  async function applyHotkey(which, accel) {
    try {
      const res = await invoke('set_settings', { settings: { ...S.settings, [HK_FIELD[which]]: accel } });
      if (res) S.settings = { ...DEFAULTS, ...res };
      toast({ msg: `Atajo guardado: ${hkLabel(accel)}` });
    } catch (e) { toastError(e); }
    await refreshHotkeys();
    renderHotkeys();
  }
  function buildHotkeys() {
    for (const which of ['toggle', 'pause']) {
      const ctl = $(`.sc-ctl[data-hk="${which}"]`);
      const btn = h('button', { class: 'hk-btn', type: 'button', 'data-hk-btn': which });
      const reset = h('button', { class: 'btn ghost sm hk-reset', type: 'button', 'data-hk-reset': which, hidden: true }, 'Restablecer');
      const els = { ctl, btn, reset, row: $(`#hkRow-${which}`) };
      hkEls[which] = els;
      btn.addEventListener('click', () => {
        if (Date.now() < (els.swallowUntil || 0)) return;
        startCapture(which);
      });
      btn.addEventListener('keydown', (e) => onCaptureKey(which, e));
      btn.addEventListener('keyup', (e) => { if (Date.now() < (els.swallowUntil || 0)) e.preventDefault(); });
      btn.addEventListener('blur', () => { if (S.capturing === which) endCapture(); });
      reset.addEventListener('click', () => applyHotkey(which, HK_DEFAULT[which]));
      ctl.append(btn, reset);
    }
  }
  function renderHotkeys() {
    for (const which of ['toggle', 'pause']) {
      const e = hkEls[which];
      if (!e) continue;
      const cap = S.capturing === which;
      const accel = hkAccel(which);
      const st = S.hotkeys && S.hotkeys[which];
      const busy = !!st && st.ok === false;
      e.btn.classList.toggle('capturing', cap);
      e.btn.setAttribute('aria-label', cap ? 'Presiona la nueva combinación de teclas. Esc cancela.' : `Atajo ${which === 'toggle' ? 'iniciar o detener' : 'pausar o reanudar'}: ${hkLabel(accel)}. Cambiar`);
      e.btn.setAttribute('data-accel', accel);
      const key = cap ? 'cap' : accel;
      if (e.btn.dataset.shown !== key) {
        e.btn.dataset.shown = key;
        if (cap) e.btn.replaceChildren(h('span', { class: 'hk-cap' }, 'Presiona la combinación…'));
        else {
          const kids = [];
          hkParts(accel).forEach((p, i) => { if (i) kids.push(h('span', { class: 'plus' }, '+')); kids.push(h('kbd', null, HK_PRETTY[p] || p)); });
          e.btn.replaceChildren(...kids);
        }
      }
      const sub = $('.sub', e.row);
      const subText = cap ? (S.capHint || 'Esc para cancelar') : 'В любом приложении';
      if (sub.textContent !== subText) sub.textContent = subText;
      sub.classList.toggle('hint', cap && !!S.capHint);
      $('.hk-busy', e.row).hidden = !busy || cap;
      e.reset.hidden = accel === HK_DEFAULT[which] || cap;
    }
  }
  function warnBusyHotkeys() {
    const bad = ['toggle', 'pause'].filter((k) => S.hotkeys && S.hotkeys[k] && S.hotkeys[k].ok === false);
    if (!bad.length) return;
    const names = bad.map((k) => hkLabel(S.hotkeys[k].accel));
    const msg = bad.length === 2
      ? `Los atajos ${names[0]} y ${names[1]} están ocupados por otra aplicación. Cámbialos en Ajustes.`
      : `El atajo ${names[0]} está ocupado por otra aplicación. Cámbialo en Ajustes.`;
    toast({ msg, kind: 'error', ms: 14000, action: { label: 'Abrir Ajustes', fn: () => setView('settings') } });
  }

  /* ───────────── arranque ───────────── */
  function buildStatic() {
    $$('[data-icon]').forEach((n) => { n.innerHTML = svgOf(n.dataset.icon); });
    buildHotkeys();

    tabsSeg = makeSeg($('#tabs'), {
      role: 'tab', label: 'Secciones', noMouseFocus: true,
      items: [{ id: 'record', label: 'Запись', btnId: 'tab-record', controls: 'view-record' }, { id: 'recordings', label: 'Записи', btnId: 'tab-recordings', controls: 'view-recordings' }],
      onSelect: setView,
    });
    buildSource('input'); buildSource('output');
    modeSeg = makeSeg($('#modeSeg'), {
      role: 'radio', label: 'Режим записи',
      items: [{ id: 'separate', label: 'Раздельно' }, { id: 'mixed', label: 'Микс' }, { id: 'both', label: 'Оба' }],
      onSelect: (id) => updateSettings({ mode: id }),
    });
    fmtSeg = makeSeg($('#fmtSeg'), {
      role: 'radio', label: 'Формат аудио', cls: 'sm',
      items: [{ id: 'mp3', label: 'MP3' }, { id: 'wav16', label: 'WAV 16' }, { id: 'wav24', label: 'WAV 24' }, { id: 'wav32f', label: 'WAV 32f' }],
      onSelect: (id) => updateSettings({ format: id }),
    });
    themeSeg = makeSeg($('#themeSeg'), {
      role: 'radio', label: 'Тема', cls: 'sm',
      items: [{ id: 'system', label: 'Система' }, { id: 'dark', label: 'Тёмная' }, { id: 'light', label: 'Светлая' }],
      onSelect: (id) => updateSettings({ theme: id }),
    });
    bindSwitch($('#swMonitor'), (on) => updateSettings({ monitorWhenIdle: on }));
    bindSwitch($('#swAgc'), (on) => updateSettings({ inputAgc: on }));
    bindSwitch($('#swDuck'), (on) => updateSettings({ mixVoicePriority: on }));
    bindSwitch($('#swCloseTray'), (on) => updatePrefs({ closeToTray: on }));
    bindSwitch($('#swAutostart'), (on) => updatePrefs({ launchAtStartup: on }));
    bindSwitch($('#swPill'), (on) => updatePrefs({ floatingPill: on }));

    for (const id of ['btnRec', 'btnPause', 'btnSettings', 'btnMin', 'btnClose']) $(`#${id}`).addEventListener('mousedown', (e) => e.preventDefault());
    $('#btnRec').addEventListener('click', toggleRecord);
    $('#btnPause').addEventListener('click', pauseResume);
    $('#btnSettings').addEventListener('click', () => setView(S.view === 'settings' ? 'record' : 'settings'));
    $('#btnGoRecord').addEventListener('click', () => setView('record'));
    const openDir = () => invoke('open_output_dir').catch(toastError);
    $('#btnOpenDir').addEventListener('click', openDir);
    $('#btnOpenDir2').addEventListener('click', openDir);
    $('#btnChoose').addEventListener('click', async () => {
      if (S.engine.status !== 'idle') return;
      try {
        const dir = await invoke('choose_output_dir');
        if (dir) { S.settings = { ...DEFAULTS, ...(await invoke('get_settings')) }; renderSettingsAll(); }
      } catch (e) { toastError(e); }
    });

    let win = null;
    try { win = T.window.getCurrentWindow(); } catch (_) { /* sin ventana */ }
    $('#btnMin').addEventListener('click', () => { try { win.minimize(); } catch (e) { toastError(e); } });
    $('#btnClose').addEventListener('click', () => { try { win.close(); } catch (e) { toastError(e); } });

    document.addEventListener('keydown', (e) => {
      if (S.capturing || e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey || modalOpen) return;
      const t = e.target;
      if (e.code === 'Space' || e.key === ' ') {
        const ctl = t.closest && t.closest('input,textarea,select,button,a,[role=slider],[role=listbox],[role=tab],[role=radio],[role=switch],[contenteditable]');
        if (ctl && ctl.getAttribute('aria-disabled') !== 'true') return;
        e.preventDefault();
        if (!e.repeat) toggleRecord();
      } else if (e.key === 'p' || e.key === 'P') {
        if (t.closest && t.closest('input,textarea,select,[contenteditable],[role=listbox]')) return;
        if (!e.repeat) pauseResume();
      }
    });
  }

  async function init() {
    buildStatic();
    setView(MOCK.view || 'record');
    renderSettingsAll();
    renderRecordings();
    requestAnimationFrame(frame);
    setInterval(() => cards.forEach((e) => updateCard(e, e.rec)), 60000);
    // El usuario puede cambiar el volumen en Windows: se relee al volver a la ventana y cada 5 s en reposo.
    window.addEventListener('focus', refreshMicVolume);
    setInterval(() => { if (!document.hidden && S.engine.status === 'idle') refreshMicVolume(); }, 5000);

    await Promise.allSettled([
      listen('levels', (ev) => onLevels(ev.payload)),
      listen('state', (ev) => setEngine(ev.payload)),
      listen('recording-saved', (ev) => onSaved(ev.payload)),
      listen('devices-changed', (ev) => { S.devices = ev.payload; renderSources(); }),
      listen('engine-error', (ev) => toast({ msg: ev.payload && ev.payload.message ? ev.payload.message : 'Error en el motor de audio.', kind: 'error', ms: ev.payload && ev.payload.fatal ? 12000 : undefined })),
      listen('app-prefs-changed', (ev) => { S.prefs = { ...PREF_DEFAULTS, ...ev.payload }; renderSettingsView(); }),
      listen('hotkey', (ev) => {
        const b = $(ev.payload && ev.payload.action === 'pause' ? '#btnPause' : '#btnRec');
        b.classList.remove('pulse'); void b.offsetWidth; b.classList.add('pulse');
      }),
    ]);

    const [set, dev, st, recs, prf] = await Promise.allSettled([invoke('get_settings'), invoke('list_devices'), invoke('get_state'), invoke('list_recordings'), invoke('get_app_prefs'), refreshHotkeys()]);
    if (prf.status === 'fulfilled') S.prefs = { ...PREF_DEFAULTS, ...prf.value };
    if (set.status === 'fulfilled') { S.settings = { ...DEFAULTS, ...set.value }; S.settingsReady = true; } else toastError(set.reason);
    if (dev.status === 'fulfilled') S.devices = dev.value; else toastError(dev.reason);
    if (st.status === 'fulfilled') { S.engine = { status: st.value.status, elapsedMs: st.value.elapsedMs || 0, recordingId: st.value.recordingId || null }; S.engineAt = performance.now(); } else toastError(st.reason);
    if (recs.status === 'fulfilled') S.recordings = recs.value; else toastError(recs.reason);
    renderSettingsAll();
    renderRecordings();
    warnBusyHotkeys();

    if (MOCK.expand) {
      const all = MOCK.expand === 'all';
      S.recordings.forEach((r, i) => { if (all || String(i) === String(MOCK.expand)) { const e = cards.get(r.id); if (e) toggleOpen(e, true); } });
    }
    document.documentElement.dataset.ready = '1';
  }
  init().catch((e) => { console.error(e); toastError(e); });
})();
