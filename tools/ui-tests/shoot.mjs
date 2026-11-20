// Harness de capturas y verificación (Edge headless + CDP). Uso: node shoot.mjs
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
const PORT = 9333;
const PROFILE = (process.env.TMPDIR || process.env.TEMP) + '/eco-edge-profile-' + Date.now();
mkdirSync(OUT, { recursive: true });

const edge = spawn(EDGE, [`--headless=new`, `--remote-debugging-port=${PORT}`, `--user-data-dir=${PROFILE}`, '--no-first-run', '--no-default-browser-check',
  '--autoplay-policy=no-user-gesture-required', '--allow-file-access-from-files', '--disable-extensions', 'about:blank'], { stdio: 'ignore' });

async function getWs() {
  for (let i = 0; i < 50; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${PORT}/json`);
      const t = (await r.json()).find((x) => x.type === 'page');
      if (t) return t.webSocketDebuggerUrl;
    } catch (_) {}
    await sleep(200);
  }
  throw new Error('Edge no arrancó');
}
const ws = new WebSocket(await getWs());
await new Promise((r) => (ws.onopen = r));
let id = 0; const pending = new Map(); const events = [];
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  if (d.id && pending.has(d.id)) { const { res, rej } = pending.get(d.id); pending.delete(d.id); d.error ? rej(new Error(JSON.stringify(d.error))) : res(d.result); }
  else if (d.method) events.push(d);
};
const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; pending.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (expr) => { const r = await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true }); if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails)); return r.result.value; };
await send('Page.enable'); await send('Runtime.enable'); await send('Log.enable'); await send('Network.enable');

async function center(sel) { return ev(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return null; const r = e.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()`); }
async function mouse(type, x, y) { await send('Input.dispatchMouseEvent', { type, x, y, button: 'left', buttons: type === 'mouseReleased' ? 0 : 1, clickCount: 1 }); }
async function click(sel) { const c = await center(sel); if (!c) throw new Error('sin elemento ' + sel); await mouse('mouseMoved', c[0], c[1]); await mouse('mousePressed', c[0], c[1]); await mouse('mouseReleased', c[0], c[1]); }
async function hover(sel) { const c = await center(sel); await mouse('mouseMoved', c[0], c[1]); }
async function key(k, code) { await send('Input.dispatchKeyEvent', { type: 'keyDown', key: k, code: code || k, text: k.length === 1 ? k : undefined }); await send('Input.dispatchKeyEvent', { type: 'keyUp', key: k, code: code || k }); }

const report = [];
async function shot(name, { w = 440, h = 720, query = '', scheme = 'dark', reduced = false, actions, wait = 1400 } = {}) {
  events.length = 0;
  await send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 1, mobile: false });
  await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: scheme }, { name: 'prefers-reduced-motion', value: reduced ? 'reduce' : 'no-preference' }] });
  await send('Page.navigate', { url: `${BASE}?mock${query}` });
  for (let i = 0; i < 60; i++) { if (await ev('document.documentElement.dataset.ready === "1"').catch(() => false)) break; await sleep(100); }
  await sleep(wait);
  if (actions) await actions();
  const m = await ev(`(() => {
    const out = { theme: document.documentElement.dataset.theme, hscroll: document.documentElement.scrollWidth > innerWidth, over: [] };
    const act = document.querySelector('.view.active');
    out.viewScrollH = act ? act.scrollHeight - act.clientHeight : 0;
    for (const e of document.querySelectorAll('.view.active *, .titlebar *, .nav *')) {
      const r = e.getBoundingClientRect(); if (!r.width) continue;
      if (r.right > innerWidth + 0.5 || r.left < -0.5) out.over.push((e.className || e.tagName) + ' ' + Math.round(r.left) + '..' + Math.round(r.right));
      if (e.scrollWidth > e.clientWidth + 1 && getComputedStyle(e).overflowX === 'visible' && e.children.length === 0 && e.clientWidth > 0) out.over.push('texto desborda: ' + (e.className || e.tagName));
    }
    out.over = out.over.slice(0, 8);
    return out; })()`);
  const shotRes = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(`${OUT}/${name}.png`, Buffer.from(shotRes.data, 'base64'));
  const errs = events.filter((e) =>
    (e.method === 'Runtime.exceptionThrown') ||
    (e.method === 'Runtime.consoleAPICalled' && ['error', 'warning', 'assert'].includes(e.params.type)) ||
    (e.method === 'Log.entryAdded' && ['error', 'warning'].includes(e.params.entry.level)) ||
    (e.method === 'Network.loadingFailed' && !e.params.canceled))
    .map((e) => JSON.stringify(e.params).slice(0, 300));
  report.push({ name, ...m, consoleErrors: errs.length, errs });
  console.log(`${name}: theme=${m.theme} hscroll=${m.hscroll} scrollExtra=${m.viewScrollH}px overflow=${JSON.stringify(m.over)} consoleErrors=${errs.length}`);
  errs.forEach((e) => console.log('   ERR', e));
}

const only = process.argv[2];
const S = {
  '01-grabar-idle-oscuro': () => shot('01-grabar-idle-oscuro', { query: '&theme=dark' }),
  '02-grabar-grabando-oscuro': () => shot('02-grabar-grabando-oscuro', { query: '&theme=dark&state=recording' }),
  '03-grabaciones-lista-oscuro': () => shot('03-grabaciones-lista-oscuro', { query: '&theme=dark&view=recordings' }),
  '03b-grabaciones-expandida-oscuro': () => shot('03b-grabaciones-expandida-oscuro', { query: '&theme=dark&view=recordings&expand=0', wait: 5000 }),
  '03c-grabaciones-todas-380x900': () => shot('03c-grabaciones-todas-380x900', { w: 380, h: 900, query: '&theme=dark&view=recordings&expand=all', wait: 5000 }),
  '04-ajustes-oscuro': () => shot('04-ajustes-oscuro', { query: '&theme=dark&view=settings' }),
  '05-claro-grabando': () => shot('05-claro-grabando', { query: '&theme=light&state=recording', scheme: 'light' }),
  '05b-claro-grabaciones': () => shot('05b-claro-grabaciones', { query: '&theme=light&view=recordings&expand=0', scheme: 'light', wait: 4500 }),
  '05c-claro-ajustes': () => shot('05c-claro-ajustes', { query: '&theme=light&view=settings', scheme: 'light' }),
  '06-380x600-idle': () => shot('06-380x600-idle', { w: 380, h: 600, query: '&theme=dark' }),
  '06b-380x600-grabando': () => shot('06b-380x600-grabando', { w: 380, h: 600, query: '&theme=dark&state=recording' }),
  '06c-380x600-grabaciones': () => shot('06c-380x600-grabaciones', { w: 380, h: 600, query: '&theme=dark&view=recordings&expand=0', wait: 4500 }),
  '06d-380x600-ajustes': () => shot('06d-380x600-ajustes', { w: 380, h: 600, query: '&theme=dark&view=settings' }),
  '07-dropdown-abierto': () => shot('07-dropdown-abierto', { query: '&theme=dark', actions: async () => { await click('#sources .card:first-child .dd-btn'); await sleep(300); } }),
  '07b-dropdown-claro': () => shot('07b-dropdown-claro', { query: '&theme=light', scheme: 'light', actions: async () => { await click('#sources .card:nth-child(2) .dd-btn'); await sleep(300); } }),
  '08-modal-eliminar': () => shot('08-modal-eliminar', { query: '&theme=dark&view=recordings&expand=1', wait: 2000, actions: async () => { await click('.rec-item:nth-child(2) .btn.danger'); await sleep(350); } }),
  '09-vacio': () => shot('09-vacio', { query: '&theme=dark&view=recordings&empty=1' }),
  '10-toast-guardada': () => shot('10-toast-guardada', { query: '&theme=dark', actions: async () => { await click('#btnRec'); await sleep(1800); await click('#btnRec'); await sleep(1500); } }),
  '11-toast-error': () => shot('11-toast-error', { query: '&theme=dark&err=1', wait: 1800 }),
  '12-bloqueado-tooltip': () => shot('12-bloqueado-tooltip', { query: '&theme=dark&state=recording', actions: async () => { await hover('#sources .card:first-child .dd'); await sleep(700); } }),
  '13-pausa': () => shot('13-pausa', { query: '&theme=dark&state=paused' }),
  '14-sistema-claro': () => shot('14-sistema-claro', { query: '', scheme: 'light' }),
  '15-sin-fuente': () => shot('15-sin-fuente', { query: '&theme=dark', actions: async () => { await click('#sources .card:first-child .switch'); await click('#sources .card:nth-child(2) .switch'); await sleep(500); } }),
  '16-reducido': () => shot('16-reducido', { query: '&theme=dark&state=recording', reduced: true }),
};
for (const [k, fn] of Object.entries(S)) if (!only || k.startsWith(only)) await fn();

writeFileSync(`${OUT}/report.json`, JSON.stringify(report, null, 1));
const total = report.reduce((a, r) => a + r.consoleErrors, 0);
console.log(`\nTOTAL errores de consola: ${total} en ${report.length} capturas`);
ws.close(); edge.kill();
process.exit(0);
