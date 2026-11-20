// Capturas v3 (voz del micrófono): Edge headless + CDP, UI con mock.  Uso: node mic-shots.mjs
import { spawn } from 'node:child_process';
import { writeFileSync, mkdirSync } from 'node:fs';
import { setTimeout as sleep } from 'node:timers/promises';

import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, resolve } from 'node:path';
const HERE = dirname(fileURLToPath(import.meta.url));
const UI_INDEX = pathToFileURL(resolve(HERE, '../../ui/index.html')).href;
const EDGE = process.env.EDGE_PATH || 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe';
const OUT = process.env.ECO_SHOTS_DIR || resolve(HERE, 'out');
const BASE = UI_INDEX;
const PORT = 9341;
const PROFILE = (process.env.TMPDIR || process.env.TEMP) + '/eco-edge-mic-' + Date.now();
mkdirSync(OUT, { recursive: true });
const edge = spawn(EDGE, ['--headless=new', `--remote-debugging-port=${PORT}`, `--user-data-dir=${PROFILE}`, '--no-first-run', '--no-default-browser-check', '--allow-file-access-from-files', '--disable-extensions', 'about:blank'], { stdio: 'ignore' });
async function getWs() {
  for (let i = 0; i < 50; i++) { try { const t = (await (await fetch(`http://127.0.0.1:${PORT}/json`)).json()).find((x) => x.type === 'page'); if (t) return t.webSocketDebuggerUrl; } catch (_) {} await sleep(200); }
  throw new Error('Edge no arrancó');
}
const ws = new WebSocket(await getWs());
await new Promise((r) => (ws.onopen = r));
let id = 0; const pending = new Map(); const errors = [];
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  if (d.id && pending.has(d.id)) { const { res, rej } = pending.get(d.id); pending.delete(d.id); d.error ? rej(new Error(JSON.stringify(d.error))) : res(d.result); return; }
  if (d.method === 'Runtime.exceptionThrown') errors.push('exception: ' + JSON.stringify(d.params.exceptionDetails.exception?.description || d.params.exceptionDetails.text));
  if (d.method === 'Runtime.consoleAPICalled' && d.params.type === 'error') errors.push('console.error: ' + d.params.args.map((a) => a.value ?? a.description).join(' '));
  if (d.method === 'Log.entryAdded' && d.params.entry.level === 'error') errors.push('log: ' + d.params.entry.text + ' ' + (d.params.entry.url || ''));
};
const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; pending.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (expr) => { const r = await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true }); if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails)); return r.result.value; };
await send('Page.enable'); await send('Runtime.enable'); await send('Log.enable');

async function open(q, w = 440, h = 720) {
  await send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 1, mobile: false });
  await send('Page.navigate', { url: `${BASE}?mock${q}` });
  for (let i = 0; i < 60; i++) { if (await ev('document.documentElement.dataset.ready === "1"')) break; await sleep(100); }
  await sleep(500);
}
async function shot(name) {
  const r = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(`${OUT}/${name}.png`, Buffer.from(r.data, 'base64'));
  console.log('captura', name);
}
const layout = () => ev(`(() => { const d = document.documentElement; const v = document.querySelector('.view.active, .view[data-active], #view-record') ; return { sw: d.scrollWidth, cw: d.clientWidth, sh: d.scrollHeight, ch: d.clientHeight, viewScroll: [...document.querySelectorAll('.view')].filter(e => e.offsetParent).map(e => [e.id, e.scrollHeight, e.clientHeight]) }; })()`);
const setGain = (db) => ev(`(() => { const r = document.querySelector('#sources .card input[type=range]'); r.value = ${db}; r.dispatchEvent(new Event('input', {bubbles:true})); })()`);
const report = {};

// 1) tarjeta micrófono con slider en dB + switch «Voz automática» (volumen de Windows OK)
await open('&theme=dark');
await setGain(18); await sleep(500);
await shot('v3-mic-card');
report.card = { layout: await layout(), val: await ev(`document.querySelector('#sources .card .gain .val').textContent`), min: await ev(`document.querySelector('#sources .card input[type=range]').min`), max: await ev(`document.querySelector('#sources .card input[type=range]').max`), warnHidden: await ev(`document.querySelector('.vol-warn').hidden`), agc: await ev(`document.querySelector('#sources .card .switch[aria-labelledby=lblAgc]').getAttribute('aria-checked')`) };
await setGain(30); report.max30 = await ev(`document.querySelector('#sources .card .gain .val').textContent`);
await setGain(-12); report.min12 = await ev(`document.querySelector('#sources .card .gain .val').textContent`);
await setGain(0); report.zero = await ev(`document.querySelector('#sources .card .gain .val').textContent`);

// 2) aviso de volumen bajo de Windows (40 %)
await open('&theme=dark&micvol=40');
await setGain(18); await sleep(400);
await shot('v3-mic-aviso-volumen');
report.warn = { layout: await layout(), hidden: await ev(`document.querySelector('.vol-warn').hidden`), text: await ev(`document.querySelector('.vol-warn .t').textContent`), btn: await ev(`document.querySelector('.vol-warn .btn').textContent`) };
// clic en «Subir a 100 %»
const [bx, by] = await ev(`(() => { const r = document.querySelector('.vol-warn .btn').getBoundingClientRect(); return [r.left + r.width/2, r.top + r.height/2]; })()`);
for (const t of ['mousePressed', 'mouseReleased']) await send('Input.dispatchMouseEvent', { type: t, x: bx, y: by, button: 'left', buttons: t === 'mousePressed' ? 1 : 0, clickCount: 1 });
await sleep(600);
report.afterClick = { hidden: await ev(`document.querySelector('.vol-warn').hidden`), log: await ev(`window.__mockLog.filter(l => l.startsWith('set_input_endpoint_volume'))`) };
await shot('v3-mic-aviso-tras-subir');

// 3) aviso con el micrófono silenciado, tema claro
await open('&theme=light&micvol=100&micmute=1');
await setGain(12); await sleep(400);
await shot('v3-mic-silenciado-claro');
report.muted = { hidden: await ev(`document.querySelector('.vol-warn').hidden`), text: await ev(`document.querySelector('.vol-warn .t').textContent`), layout: await layout() };

// 4) Ajustes con el grupo Audio
await open('&theme=dark&view=settings');
await ev(`(() => { const l = [...document.querySelectorAll('.set-label')].find(e => e.textContent === 'Audio'); l.scrollIntoView({block:'center'}); })()`);
await sleep(400);
await shot('v3-mic-ajustes-audio');
report.settings = { layout: await layout(), agc: await ev(`document.querySelector('#swAgc').getAttribute('aria-checked')`), duck: await ev(`document.querySelector('#swDuck').getAttribute('aria-checked')`) };
// alternar y comprobar que se guarda
await ev(`document.querySelector('#swAgc').click()`); await ev(`document.querySelector('#swDuck').click()`); await sleep(500);
report.settings.afterToggle = await ev(`(() => { const l = window.__mockLog.filter(x => x.startsWith('set_settings')).pop() || ''; const s = JSON.parse(l.slice(13)); return { inputAgc: s.inputAgc, mixVoicePriority: s.mixVoicePriority, inputGainDb: s.inputGainDb }; })()`);
await shot('v3-mic-ajustes-audio-apagado');

// 5) ventana mínima 387x632 con el aviso visible: ¿hay scroll?
await open('&theme=dark&micvol=40', 387, 632);
report.min = { layout: await layout() };
await shot('v3-mic-387x632-aviso');
await open('&theme=dark', 387, 632);
report.minNoWarn = { layout: await layout() };
await shot('v3-mic-387x632');
await open('&theme=light&micvol=40', 440, 720);
await shot('v3-mic-claro-aviso');
report.light = { layout: await layout() };

report.consoleErrors = errors;
writeFileSync(`${OUT}/v3-mic-report.json`, JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 1));
ws.close(); edge.kill();
process.exit(0);
