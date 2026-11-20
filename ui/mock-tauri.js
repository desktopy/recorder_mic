/* Eco — Tauri falso para pruebas en navegador.
 * Solo actúa si window.__TAURI__ NO existe y la URL lleva ?mock. En Tauri real no hace nada.
 * Parámetros opcionales: view=record|recordings|settings, state=recording|paused, theme=light|dark|system,
 * expand=all|<índice>, empty=1 (sin grabaciones), err=1 (dispara un engine-error), autostart=1 / pill=1 (app-prefs),
 * micvol=40 (volumen del micrófono en Windows, %), micmute=1 (micrófono silenciado en Windows).
 * También lo cargan widget.html y pill.html (p. ej. widget.html?mock&state=recording&theme=light). */
(function () {
  'use strict';
  if (window.__TAURI__ || !/[?&]mock\b/.test(location.search)) return;

  var q = new URLSearchParams(location.search);
  window.__ECO_MOCK__ = { view: q.get('view') || 'record', expand: q.get('expand') || '' };
  window.__mockLog = [];
  var log = function (m) { window.__mockLog.push(m); };

  var pad = function (n) { return String(n).padStart(2, '0'); };
  var isoLocal = function (d) {
    return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()) + 'T' + pad(d.getHours()) + ':' + pad(d.getMinutes()) + ':' + pad(d.getSeconds());
  };
  var clone = function (o) { return JSON.parse(JSON.stringify(o)); };

  /* ── dispositivos ── */
  var devices = {
    inputs: [
      { id: 'default', name: 'Predeterminado del sistema (Micrófono (Realtek Audio))', realName: 'Micrófono (Realtek Audio)', isDefault: true, sampleRate: 48000, channels: 2 },
      { id: 'Micrófono (Realtek Audio)', name: 'Micrófono (Realtek Audio)', isDefault: false, sampleRate: 48000, channels: 2 },
      { id: 'Auriculares con micrófono (Logitech G435)', name: 'Auriculares con micrófono (Logitech G435)', isDefault: false, sampleRate: 44100, channels: 1 },
    ],
    outputs: [
      { id: 'default', name: 'Predeterminado del sistema (Altavoces (Realtek Audio))', realName: 'Altavoces (Realtek Audio)', isDefault: true, sampleRate: 48000, channels: 2 },
      { id: 'Altavoces (Realtek Audio)', name: 'Altavoces (Realtek Audio)', isDefault: false, sampleRate: 48000, channels: 2 },
      { id: 'Monitor LG (HDMI)', name: 'Monitor LG (HDMI)', isDefault: false, sampleRate: 48000, channels: 2 },
      { id: 'Auriculares (Logitech G435)', name: 'Auriculares (Logitech G435)', isDefault: false, sampleRate: 48000, channels: 2 },
    ],
  };

  /* ── ajustes ── */
  var settings = {
    inputId: 'default', outputId: 'default', captureInput: true, captureOutput: true, mode: 'both', format: 'mp3',
    outputDir: 'C:\\Users\\usuario\\Music\\Eco', inputGainDb: 0, outputGainDb: 0, inputAgc: true, mixVoicePriority: true, monitorWhenIdle: true,
    theme: q.get('theme') || 'system',
    hotkeyToggle: 'Ctrl+Alt+R', hotkeyPause: 'Ctrl+Alt+P',
  };
  /* Atajos ocupados por otra app: ?hkbusy=Ctrl+Alt+R (separados por coma). Por defecto, ninguno. */
  var hkBusy = (q.get('hkbusy') || '').split(',').filter(Boolean);
  var hkOk = function (a) { return hkBusy.indexOf(a) < 0; };
  var hkSeen = { Ctrl: 1, Alt: 1, Shift: 1, Super: 1 };
  var hkValid = function (a) { var p = String(a || '').split('+'); return p.length >= 2 && p.slice(0, -1).every(function (m) { return hkSeen[m]; }); };

  /* ── preferencias de ventana y bandeja (app-prefs) ── */
  var appPrefs = { closeToTray: true, launchAtStartup: q.get('autostart') === '1', floatingPill: q.get('pill') === '1', trayHintShown: false };

  /* ── volumen del micrófono en Windows (?micvol=40 y ?micmute=1; por defecto 100 %) ── */
  var micVol = { level: q.get('micvol') ? Math.max(0, Math.min(1, +q.get('micvol') / 100)) : 1, muted: !!q.get('micmute') };

  /* ── grabaciones de ejemplo ── */
  var now = new Date();
  var ago = function (min) { return new Date(now.getTime() - min * 60000); };
  var sessionDir = function (d) {
    return 'C:\\Users\\usuario\\Music\\Eco\\Eco ' + d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()) + ' ' + pad(d.getHours()) + '-' + pad(d.getMinutes()) + '-' + pad(d.getSeconds());
  };
  var defaultName = function (d) { return 'Grabación ' + pad(d.getDate()) + '/' + pad(d.getMonth() + 1) + ' ' + pad(d.getHours()) + ':' + pad(d.getMinutes()); };
  var track = function (dir, kind, durMs, rate, ch, bytes) {
    var file = { input: 'Microfono.wav', output: 'Sistema.wav', mix: 'Mezcla.wav' }[kind];
    return { kind: kind, path: dir + '\\' + file, sizeBytes: Math.round((durMs / 1000) * rate * ch * bytes), sampleRate: rate, channels: ch };
  };
  var mkRec = function (id, name, date, durMs, kinds) {
    var dir = sessionDir(date);
    return {
      id: id, name: name || defaultName(date), createdAt: isoLocal(date), durationMs: durMs, folder: dir,
      files: kinds.map(function (k) { return track(dir, k, durMs, k === 'input' ? 44100 : 48000, k === 'input' ? 1 : 2, 3); }),
    };
  };
  var recordings = q.get('empty') ? [] : [
    mkRec('rec-1', null, ago(24), 724300, ['input', 'output', 'mix']),
    mkRec('rec-2', 'Reunión con el equipo de diseño y producto — revisión trimestral', ago(60 * 26 + 5), 3912000, ['input', 'output']),
    mkRec('rec-3', 'Entrevista a Camila', ago(60 * 24 * 4 + 120), 1805000, ['mix']),
    mkRec('rec-4', null, ago(60 * 24 * 21 + 33), 48600, ['input']),
  ];

  /* ── motor simulado ── */
  var eng = { status: 'idle', base: 0, since: 0, id: null };
  var startState = q.get('state');
  if (startState === 'recording' || startState === 'paused') {
    eng.status = startState; eng.base = 83400; eng.since = performance.now(); eng.id = 'rec-live';
  }
  var elapsed = function () { return Math.round(eng.base + (eng.status === 'recording' ? performance.now() - eng.since : 0)); };
  var stateObj = function () { return { status: eng.status, elapsedMs: elapsed(), recordingId: eng.id }; };

  var listeners = {};
  var emit = function (name, payload) {
    (listeners[name] || []).slice().forEach(function (cb) { try { cb({ event: name, payload: clone(payload) }); } catch (e) { console.error(e); } });
  };
  var delay = function (v, ms) { return new Promise(function (res) { setTimeout(function () { res(v); }, ms || 15); }); };
  var reject = function (msg, ms) { return new Promise(function (_, rej) { setTimeout(function () { rej(msg); }, ms || 15); }); };

  /* niveles ~30 Hz */
  var tt = 0;
  setInterval(function () {
    var live = eng.status !== 'idle' || settings.monitorWhenIdle;
    if (!live) return;
    tt += 0.033;
    var speech = Math.max(0, Math.sin(tt * 2.3) * Math.sin(tt * 0.7 + 1)) + (Math.random() < 0.02 ? 0.6 : 0);
    var rmsIn = (0.012 + 0.2 * speech) * Math.pow(10, settings.inputGainDb / 20) * (0.75 + 0.25 * Math.random());
    var beat = 0.5 + 0.5 * Math.sin(tt * 6.1);
    var rmsOut = (0.06 + 0.09 * beat) * Math.pow(10, settings.outputGainDb / 20) * (0.9 + 0.1 * Math.random());
    var pk = function (r) { return Math.min(1, r * (1.5 + Math.random() * 0.9)); };
    emit('levels', {
      input: { rms: Math.min(1, rmsIn), peak: pk(rmsIn), active: settings.captureInput },
      output: { rms: Math.min(1, rmsOut), peak: pk(rmsOut), active: settings.captureOutput },
    });
  }, 33);
  setInterval(function () { if (eng.status === 'recording' || eng.status === 'paused') emit('state', stateObj()); }, 250);

  /* ── WAV sintético (en memoria) ── */
  var wavCache = {};
  var hash = function (s) { var h = 2166136261; for (var i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); } return (h >>> 0) / 4294967295; };
  function makeWav(path) {
    if (wavCache[path]) return wavCache[path];
    var sr = 8000, dur = 24, n = sr * dur, seed = hash(path) * 10;
    var kind = /Mezcla/.test(path) ? 'mix' : /Sistema/.test(path) ? 'sys' : 'mic';
    var pcm = new Int16Array(n);
    for (var i = 0; i < n; i++) {
      var t = i / sr, v = 0;
      if (kind !== 'sys') {
        var burst = Math.sin(t * 2 * Math.PI * 1.3 + seed) + Math.sin(t * 2 * Math.PI * 0.37 + seed * 2);
        var env = Math.max(0, Math.min(1, (burst - 0.3) * 1.4));
        var f0 = 120 + 30 * Math.sin(t * 3 + seed);
        v += env * 0.55 * (Math.sin(2 * Math.PI * f0 * t) + 0.5 * Math.sin(4 * Math.PI * f0 * t) + 0.3 * Math.sin(6 * Math.PI * f0 * t));
        v += (Math.random() - 0.5) * 0.02;
      }
      if (kind !== 'mic') {
        var trem = 0.55 + 0.45 * Math.sin(2 * Math.PI * 0.21 * t + seed);
        v += 0.3 * trem * (Math.sin(2 * Math.PI * 220 * t) + Math.sin(2 * Math.PI * 277 * t) + Math.sin(2 * Math.PI * 330 * t)) / 3 * (kind === 'mix' ? 1.2 : 1.6);
      }
      pcm[i] = Math.max(-1, Math.min(1, v * 0.5)) * 32767;
    }
    var buf = new ArrayBuffer(44 + pcm.length * 2), dv = new DataView(buf);
    var str = function (o, s) { for (var k = 0; k < s.length; k++) dv.setUint8(o + k, s.charCodeAt(k)); };
    str(0, 'RIFF'); dv.setUint32(4, 36 + pcm.length * 2, true); str(8, 'WAVE'); str(12, 'fmt ');
    dv.setUint32(16, 16, true); dv.setUint16(20, 1, true); dv.setUint16(22, 1, true); dv.setUint32(24, sr, true);
    dv.setUint32(28, sr * 2, true); dv.setUint16(32, 2, true); dv.setUint16(34, 16, true); str(36, 'data'); dv.setUint32(40, pcm.length * 2, true);
    new Int16Array(buf, 44).set(pcm);
    return (wavCache[path] = URL.createObjectURL(new Blob([buf], { type: 'audio/wav' })));
  }

  /* ── comandos ── */
  var commands = {
    list_devices: function () { return delay(clone(devices)); },
    get_settings: function () { return delay(clone(settings)); },
    set_settings: function (a) {
      var s = a && a.settings;
      if (!s) return reject('Ajustes inválidos.');
      var oldT = settings.hotkeyToggle, oldP = settings.hotkeyPause;
      if (!hkValid(s.hotkeyToggle) || !hkValid(s.hotkeyPause)) return reject('El atajo no es válido: necesita al menos un modificador.');
      if (s.hotkeyToggle === s.hotkeyPause) return reject('Los atajos de iniciar/detener y de pausar no pueden ser el mismo.');
      if (s.hotkeyToggle !== oldT && !hkOk(s.hotkeyToggle)) return reject('No se pudo registrar el atajo ' + s.hotkeyToggle + ': otra aplicación ya lo está usando. Se conserva el anterior.');
      if (s.hotkeyPause !== oldP && !hkOk(s.hotkeyPause)) return reject('No se pudo registrar el atajo ' + s.hotkeyPause + ': otra aplicación ya lo está usando. Se conserva el anterior.');
      settings = Object.assign({}, settings, s);
      settings.inputGainDb = Math.max(-12, Math.min(30, settings.inputGainDb));
      settings.outputGainDb = Math.max(-12, Math.min(12, settings.outputGainDb));
      log('set_settings ' + JSON.stringify(settings));
      return delay(clone(settings), 20);
    },
    get_input_endpoint_volume: function () {
      return delay({ deviceName: 'Micrófono (Realtek Audio)', level: micVol.level, muted: micVol.muted, boost: null });
    },
    set_input_endpoint_volume: function (a) {
      micVol.level = Math.max(0, Math.min(1, +a.level));
      if (micVol.level > 0) micVol.muted = false;
      log('set_input_endpoint_volume ' + micVol.level);
      return delay({ deviceName: 'Micrófono (Realtek Audio)', level: micVol.level, muted: micVol.muted, boost: null });
    },
    get_hotkey_status: function () {
      return delay({ toggle: { accel: settings.hotkeyToggle, ok: hkOk(settings.hotkeyToggle) }, pause: { accel: settings.hotkeyPause, ok: hkOk(settings.hotkeyPause) } });
    },
    get_app_prefs: function () { return delay(clone(appPrefs)); },
    set_app_prefs: function (a) {
      var p = a && a.prefs;
      if (!p) return reject('Preferencias inválidas.');
      appPrefs = Object.assign({}, appPrefs, { closeToTray: !!p.closeToTray, launchAtStartup: !!p.launchAtStartup, floatingPill: !!p.floatingPill });
      log('set_app_prefs ' + JSON.stringify(appPrefs));
      var res = clone(appPrefs); emit('app-prefs-changed', res);
      return delay(res, 20);
    },
    show_main_window: function () { log('show_main_window'); return delay(null); },
    hide_widget: function () { log('hide_widget'); return delay(null); },
    get_state: function () { return delay(stateObj()); },
    start_recording: function () {
      if (eng.status !== 'idle') return reject('Ya hay una grabación en curso.');
      if (!settings.captureInput && !settings.captureOutput) return reject('Activa al menos una fuente para grabar.');
      eng = { status: 'recording', base: 0, since: performance.now(), id: 'rec-' + Date.now() };
      var st = stateObj(); emit('state', st);
      return delay(st, 40);
    },
    pause_recording: function () {
      if (eng.status !== 'recording') return reject('No hay una grabación activa.');
      eng.base = elapsed(); eng.status = 'paused';
      var st = stateObj(); emit('state', st); return delay(st);
    },
    resume_recording: function () {
      if (eng.status !== 'paused') return reject('La grabación no está en pausa.');
      eng.since = performance.now(); eng.status = 'recording';
      var st = stateObj(); emit('state', st); return delay(st);
    },
    stop_recording: function () {
      if (eng.status !== 'recording' && eng.status !== 'paused') return reject('No hay ninguna grabación en curso.');
      var dur = elapsed(); eng.base = dur; eng.status = 'finalizing'; emit('state', stateObj());
      return new Promise(function (res) {
        setTimeout(function () {
          var d = new Date();
          var kinds = [];
          var both = settings.captureInput && settings.captureOutput;
          if (settings.mode !== 'mixed' || !both) { if (settings.captureInput) kinds.push('input'); if (settings.captureOutput) kinds.push('output'); }
          if (both && settings.mode !== 'separate') kinds.push('mix');
          var rec = mkRec(eng.id, null, d, dur, kinds);
          recordings.unshift(rec);
          eng = { status: 'idle', base: 0, since: 0, id: null };
          emit('state', stateObj());
          emit('recording-saved', rec);
          res(clone(rec));
        }, 800);
      });
    },
    list_recordings: function () { return delay(clone(recordings)); },
    rename_recording: function (a) {
      var r = recordings.find(function (x) { return x.id === a.id; });
      if (!r) return reject('No se encontró la grabación.');
      r.name = String(a.name).trim() || r.name;
      return delay(clone(r));
    },
    delete_recording: function (a) {
      var i = recordings.findIndex(function (x) { return x.id === a.id; });
      if (i < 0) return reject('No se encontró la grabación.');
      recordings.splice(i, 1);
      return delay(null);
    },
    reveal_path: function (a) { log('reveal_path ' + a.path); return delay(null); },
    open_output_dir: function () { log('open_output_dir'); return delay(null); },
    choose_output_dir: function () {
      var next = settings.outputDir.indexOf('D:') === 0 ? 'C:\\Users\\usuario\\Music\\Eco' : 'D:\\Proyectos\\Audio\\Grabaciones de reuniones\\Eco';
      settings.outputDir = next;
      return delay(next, 120);
    },
  };

  var fakeWindow = {
    minimize: function () { log('window.minimize'); return Promise.resolve(); },
    close: function () { log('window.close'); return Promise.resolve(); },
    toggleMaximize: function () { log('window.toggleMaximize'); return Promise.resolve(); },
    startDragging: function () { return Promise.resolve(); },
  };

  window.__TAURI__ = {
    core: {
      invoke: function (cmd, args) {
        var fn = commands[cmd];
        if (!fn) return reject('Comando desconocido: ' + cmd);
        return fn(args || {});
      },
      convertFileSrc: function (p) { return makeWav(p); },
    },
    event: {
      listen: function (name, cb) {
        (listeners[name] = listeners[name] || []).push(cb);
        return Promise.resolve(function () { listeners[name] = listeners[name].filter(function (x) { return x !== cb; }); });
      },
    },
    window: { getCurrentWindow: function () { return fakeWindow; } },
  };

  if (q.get('err')) {
    setTimeout(function () {
      emit('engine-error', { message: 'Se desconectó «Micrófono (Realtek Audio)». Se guardó lo grabado hasta ese momento.', fatal: false });
    }, 900);
  }
})();
