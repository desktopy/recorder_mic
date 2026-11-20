// Pruebas funcionales contra el mock (Edge headless + CDP). Uso: node functional.mjs
import { spawn } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';

import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, resolve } from 'node:path';
const HERE = dirname(fileURLToPath(import.meta.url));
const UI_INDEX = pathToFileURL(resolve(HERE, '../../ui/index.html')).href;
const EDGE = process.env.EDGE_PATH || 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe';
const URL0 = UI_INDEX + '?mock&theme=dark';
const edge = spawn(EDGE, ['--headless=new', '--remote-debugging-port=9335', '--user-data-dir=' + (process.env.TEMP) + '/ecof' + Date.now(), '--autoplay-policy=no-user-gesture-required', '--allow-file-access-from-files', 'about:blank'], { stdio: 'ignore' });
await sleep(2500);
const t = (await (await fetch('http://127.0.0.1:9335/json')).json()).find((x) => x.type === 'page');
const ws = new WebSocket(t.webSocketDebuggerUrl); await new Promise((r) => (ws.onopen = r));
let id = 0; const p = new Map(); const errors = [];
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && p.has(d.id)) { p.get(d.id)(d); p.delete(d.id); } else if (d.method === 'Runtime.exceptionThrown' || (d.method === 'Runtime.consoleAPICalled' && ['error', 'warning'].includes(d.params.type)) || (d.method === 'Log.entryAdded' && ['error', 'warning'].includes(d.params.entry.level))) errors.push(JSON.stringify(d.params).slice(0, 300)); };
const send = (method, params = {}) => new Promise((r) => { const i = ++id; p.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (e) => { const r = await send('Runtime.evaluate', { expression: e, awaitPromise: true, returnByValue: true }); if (r.result.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails).slice(0, 300)); return r.result.result.value; };
await send('Runtime.enable'); await send('Page.enable'); await send('Log.enable');
await send('Emulation.setDeviceMetricsOverride', { width: 440, height: 720, deviceScaleFactor: 1, mobile: false });
const center = (sel) => ev(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return null; e.scrollIntoView({block:"nearest"}); const r = e.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()`);
const mouse = (type, x, y, cc = 1) => send('Input.dispatchMouseEvent', { type, x, y, button: 'left', buttons: type === 'mouseReleased' ? 0 : 1, clickCount: cc });
const click = async (sel, cc = 1) => { const c = await center(sel); if (!c) throw new Error('sin ' + sel); await mouse('mouseMoved', c[0], c[1]); for (let i = 1; i <= cc; i++) { await mouse('mousePressed', c[0], c[1], i); await mouse('mouseReleased', c[0], c[1], i); } };
const key = async (k, code, text) => { await send('Input.dispatchKeyEvent', { type: 'keyDown', key: k, code: code || k, text: text ?? (k.length === 1 ? k : undefined) }); await send('Input.dispatchKeyEvent', { type: 'keyUp', key: k, code: code || k }); };
const results = [];
const check = (name, ok, extra = '') => { results.push(ok); console.log(`${ok ? 'OK  ' : 'FALLA'} ${name} ${extra}`); };
const load = async (q = '') => { await send('Page.navigate', { url: URL0 + q }); for (let i = 0; i < 60; i++) { if (await ev('document.documentElement.dataset.ready === "1"').catch(() => false)) break; await sleep(100); } await sleep(500); };
const log = () => ev('window.__mockLog.slice()');
const status = () => ev('document.getElementById("app").dataset.status');

await load();
// 1. Espacio: iniciar / P pausar / Espacio detener
await key(' ', 'Space'); await sleep(400); check('Espacio inicia', (await status()) === 'recording');
await key('p', 'KeyP'); await sleep(300); check('P pausa', (await status()) === 'paused');
await key('p', 'KeyP'); await sleep(300); check('P reanuda', (await status()) === 'recording');
await sleep(1100);
const tm = await ev('document.getElementById("tMain").textContent + document.getElementById("tDec").textContent');
check('temporizador avanza', /^00:0[1-9]\.\d$/.test(tm), tm);
check('selector bloqueado grabando', await ev('document.querySelector("#sources .dd-btn").getAttribute("aria-disabled")') === 'true');
await click('#sources .dd-btn'); await sleep(200);
check('dropdown no abre bloqueado', await ev('!document.querySelector(".dd-pop")'));
await key(' ', 'Space'); await sleep(300); check('Espacio detiene -> finalizing', (await status()) === 'finalizing');
check('texto Finalizando…', (await ev('document.getElementById("statusText").textContent')) === 'Finalizando…');
await sleep(1500); check('vuelve a idle', (await status()) === 'idle');
check('toast guardada', await ev('!!document.querySelector(".toast .tbtn")'));
await click('.toast .tbtn'); await sleep(200);
check('toast Mostrar -> reveal_path', (await log()).some((l) => l.startsWith('reveal_path')));

// 2. Ajustes: slider con debounce, switch, modo, dropdown por teclado
await ev('window.__mockLog.length = 0');
await ev(`(() => { const r = document.querySelector('#sources .card:first-child input[type=range]'); r.value = 150; r.dispatchEvent(new Event('input', {bubbles:true})); r.value = 160; r.dispatchEvent(new Event('input', {bubbles:true})); })()`);
await sleep(100); check('debounce: aún sin guardar a 100 ms', (await log()).filter((l) => l.startsWith('set_settings')).length === 0);
await sleep(500);
let sets = (await log()).filter((l) => l.startsWith('set_settings'));
check('slider -> 1 set_settings con objeto completo', sets.length === 1 && JSON.parse(sets[0].slice(13)).inputGain === 1.6 && 'outputDir' in JSON.parse(sets[0].slice(13)) && 'theme' in JSON.parse(sets[0].slice(13)), String(sets.length));
await click('#sources .card:nth-child(2) .switch'); await sleep(300);
sets = (await log()).filter((l) => l.startsWith('set_settings'));
check('switch sistema off -> captureOutput=false', JSON.parse(sets[sets.length - 1].slice(13)).captureOutput === false);
check('tarjeta atenuada', await ev('document.querySelector("#sources .card:nth-child(2)").classList.contains("off")'));
check('ayuda modo con una fuente', (await ev('document.getElementById("modeHelp").textContent')).includes('una sola fuente'));
await click('#sources .card:nth-child(2) .switch'); await sleep(200);
await click('#modeSeg button[data-id=separate]'); await sleep(300);
check('modo separate guardado', JSON.parse((await log()).filter((l) => l.startsWith('set_settings')).pop().slice(13)).mode === 'separate');
check('ayuda modo separado', (await ev('document.getElementById("modeHelp").textContent')).includes('Dos archivos'));
await ev('document.querySelector("#sources .card:first-child .dd-btn").focus()');
await key('ArrowDown', 'ArrowDown'); await sleep(200);
check('ArrowDown abre listbox', await ev('!!document.querySelector(".dd-pop")'));
await key('ArrowDown', 'ArrowDown'); await key('Enter', 'Enter', '\r'); await sleep(400);
check('Enter elige opción -> inputId', JSON.parse((await log()).filter((l) => l.startsWith('set_settings')).pop().slice(13)).inputId === 'Micrófono (Realtek Audio)');
check('foco vuelve al botón', await ev('document.activeElement.classList.contains("dd-btn")'));

// 3. Ajustes: tema y carpeta
await click('#btnSettings'); await sleep(400);
check('ajustes activo', await ev('document.getElementById("view-settings").classList.contains("active")'));
await click('#themeSeg button[data-id=light]'); await sleep(400);
check('tema claro aplicado', (await ev('document.documentElement.dataset.theme')) === 'light');
await click('#themeSeg button[data-id=dark]'); await sleep(300);
await click('#btnChoose'); await sleep(500);
check('choose_output_dir actualiza ruta', (await ev('document.getElementById("outPath").title')).startsWith('D:'));
await click('#btnOpenDir'); await sleep(100);
check('open_output_dir', (await log()).includes('open_output_dir'));
await click('#fmtSeg button[data-id=wav32f]'); await sleep(300);
check('formato wav32f guardado', JSON.parse((await log()).filter((l) => l.startsWith('set_settings')).pop().slice(13)).format === 'wav32f');

// 4. Grabaciones: expandir, reproducir, seek, renombrar, eliminar
await click('#tab-recordings'); await sleep(500);
check('vista grabaciones', await ev('document.getElementById("view-recordings").classList.contains("active")'));
check('4+ tarjetas (incluye guardada)', (await ev('document.querySelectorAll(".rec-item").length')) === 5);
await click('.rec-item:first-child .chev-btn'); await sleep(1500);
check('expandida con 3 pistas', (await ev('document.querySelectorAll(".rec-item:first-child .track").length')) === 3);
await click('.rec-item:first-child .track:first-child .play-btn'); await sleep(900);
const t1 = await ev('[...__ecoPlayers][0].audio.currentTime');
check('reproduce (currentTime avanza)', t1 > 0.3, String(t1));
check('aria-label pausar', (await ev('document.querySelector(".rec-item:first-child .play-btn").getAttribute("aria-label")')).startsWith('Pausar'));
const r = await ev('(() => { const r = document.querySelector(".rec-item:first-child .wave").getBoundingClientRect(); return [r.left, r.width, r.top + r.height/2]; })()');
await mouse('mouseMoved', r[0] + r[1] * 0.8, r[2]); await mouse('mousePressed', r[0] + r[1] * 0.8, r[2]); await mouse('mouseReleased', r[0] + r[1] * 0.8, r[2]); await sleep(200);
const t2 = await ev('[...__ecoPlayers][0].audio.currentTime');
check('seek ~80 %', t2 > 17 && t2 < 22, String(t2));
await click('.rec-item:first-child .track:nth-child(2) .play-btn'); await sleep(500);
check('solo suena una pista', await ev('[...__ecoPlayers].filter(p => p.audio && !p.audio.paused).length') === 1);
await click('.rec-item:first-child .track:nth-child(2) .play-btn'); await sleep(200);
// renombrar con doble clic
await click('.rec-item:nth-child(2) .rec-name', 2); await sleep(200);
check('doble clic abre input', await ev('!!document.querySelector(".rec-name-input")'));
await ev('(() => { const i = document.querySelector(".rec-name-input"); i.value = "Llamada con proveedor"; })()');
await key('Enter', 'Enter', '\r'); await sleep(400);
check('renombrada', (await ev('document.querySelector(".rec-item:nth-child(2) .rec-name").textContent')) === 'Llamada con proveedor');
// eliminar
await click('.rec-item:nth-child(3) .chev-btn'); await sleep(500);
await click('.rec-item:nth-child(3) .btn.danger'); await sleep(300);
check('modal de confirmación propio', await ev('!!document.querySelector(".modal")'));
await key('Escape', 'Escape'); await sleep(300);
check('Esc cancela', await ev('!document.querySelector(".modal")') && (await ev('document.querySelectorAll(".rec-item").length')) === 5);
await click('.rec-item:nth-child(3) .btn.danger'); await sleep(300);
await click('.modal .btn.primary'); await sleep(500);
check('eliminada', (await ev('document.querySelectorAll(".rec-item").length')) === 4);
await click('.rec-item:first-child .btn.ghost:not(.danger)'); await sleep(100);
check('Mostrar en carpeta -> reveal_path', (await log()).filter((l) => l.startsWith('reveal_path')).length >= 1);

// 5. Estados vacíos / error / sin fuentes
await load('&empty=1&view=recordings');
check('estado vacío visible', await ev('document.getElementById("recEmpty").classList.contains("show")'));
await load('&err=1'); await sleep(1200);
check('engine-error -> toast error', await ev('!!document.querySelector(".toast.error")'));
await load();
await click('#sources .card:first-child .switch'); await click('#sources .card:nth-child(2) .switch'); await sleep(400);
check('sin fuentes: grabar deshabilitado', await ev('document.getElementById("btnRec").getAttribute("aria-disabled")') === 'true');
// hotkey / devices-changed
await load();
await ev(`(async () => { })()`);
// 6. Reduced motion: sin transform en vista (estático)
check('sin errores de consola en pruebas', errors.length === 0, errors.join('\n'));
console.log(`\n${results.filter(Boolean).length}/${results.length} pruebas OK`);
ws.close(); edge.kill(); process.exit(results.every(Boolean) ? 0 : 1);
