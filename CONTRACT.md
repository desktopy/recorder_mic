# Eco — contrato backend ↔ UI (fuente de verdad)

App de escritorio Windows 11, minimalista. Graba **entrada** (micrófono, cualquier dispositivo de captura) y **salida** (audio del sistema por WASAPI loopback del dispositivo de reproducción elegido). Todo el texto visible en **español neutro LATAM**.

## Stack fijo

- Tauri **v2** + Rust (`src-tauri/`). Audio: `cpal` (host WASAPI) + `hound` (WAV). Resampleo para la mezcla: `rubato` o interpolación propia de calidad (sinc o cúbica, no lineal ingenua).
- UI: HTML/CSS/JS **estático sin bundler ni npm** en `ui/` (`frontendDist: "../ui"`, `app.withGlobalTauri: true`, sin `devUrl`). La UI usa `window.__TAURI__.core.invoke`, `window.__TAURI__.event.listen`, `window.__TAURI__.core.convertFileSrc`, `window.__TAURI__.window.getCurrentWindow()`.
- Plugins: `tauri-plugin-dialog` (elegir carpeta), `tauri-plugin-global-shortcut` (atajos globales), `tauri-plugin-opener` (abrir carpeta / revelar archivo).
- `assetProtocol` habilitado con scope `["**"]` para reproducir WAV desde la UI vía `convertFileSrc`.
- Ventana: label `main`, título `Eco`, 440×720, minWidth 380, minHeight 600, `decorations: false`, `shadow: true`, `transparent: false`, `resizable: true`, `center: true`. La UI dibuja su propia barra de título (drag region + minimizar + cerrar).
- Capabilities: `core:default`, `core:window:allow-start-dragging`, `core:window:allow-minimize`, `core:window:allow-close`, `core:window:allow-toggle-maximize`, `dialog:default`, `opener:default` + permiso para revelar/abrir rutas, `global-shortcut:default`.

## Tipos (JSON, camelCase en el cable)

```ts
Device   = { id: string, name: string, isDefault: boolean, sampleRate: number, channels: number }
// id = "default" significa «seguir el predeterminado de Windows»; otros ids = nombre del dispositivo.
Devices  = { inputs: Device[], outputs: Device[] }   // inputs[0] y outputs[0] SIEMPRE son el pseudo-dispositivo id "default", name "Predeterminado del sistema (<nombre real>)"

Settings = {
  inputId: string, outputId: string,          // default "default"
  captureInput: boolean, captureOutput: boolean, // default true, true
  mode: "separate" | "mixed" | "both",        // default "both": pistas separadas + mezcla
  format: "wav16" | "wav24" | "wav32f",       // default "wav24"
  outputDir: string,                          // default %USERPROFILE%\Music\Eco
  inputGainDb: number, outputGainDb: number,  // dB: micrófono −12…+30, sistema −12…+12, default 0 (ver Extensiones → Audio de voz; sustituyen a inputGain/outputGain lineales)
  inputAgc: boolean, mixVoicePriority: boolean, // default true, true (ver Extensiones → Audio de voz)
  monitorWhenIdle: boolean,                   // default true: niveles en vivo sin grabar
  theme: "system" | "dark" | "light"          // default "system" (solo lo usa la UI; backend lo persiste)
}

TrackFile = { kind: "input" | "output" | "mix", path: string, sizeBytes: number, sampleRate: number, channels: number }
Recording = { id: string, name: string, createdAt: string /*ISO 8601 local*/, durationMs: number, files: TrackFile[], folder: string }

EngineState = { status: "idle" | "recording" | "paused" | "finalizing", elapsedMs: number, recordingId: string | null }
Levels    = { input: { rms: number, peak: number, active: boolean }, output: { rms: number, peak: number, active: boolean } } // 0..1 lineal, post-ganancia
```

## Comandos (`invoke(nombre, args)`)

| Comando | Args | Devuelve |
|---|---|---|
| `list_devices` | — | `Devices` |
| `get_settings` | — | `Settings` |
| `set_settings` | `{ settings: Settings }` | `Settings` (validado, persistido en `%APPDATA%\Eco\settings.json`; si cambian dispositivos o monitor, reabre streams) |
| `get_state` | — | `EngineState` |
| `start_recording` | — | `EngineState` (error string si no hay ninguna fuente activa o falla un dispositivo) |
| `pause_recording` / `resume_recording` | — | `EngineState` |
| `stop_recording` | — | `Recording` (ya finalizada en disco, incluida la mezcla si aplica) |
| `list_recordings` | — | `Recording[]` ordenadas de más nueva a más vieja |
| `rename_recording` | `{ id, name }` | `Recording` |
| `delete_recording` | `{ id }` | `null` (manda la carpeta de la sesión a la Papelera si es posible; si no, borra) |
| `reveal_path` | `{ path }` | `null` (abre el Explorador con el archivo seleccionado) |
| `open_output_dir` | — | `null` |
| `choose_output_dir` | — | `string \| null` (diálogo nativo de carpeta; si elige, persiste en settings) |

Errores: los comandos devuelven `Err(String)` con mensaje en español apto para mostrar al usuario.

## Eventos (`listen(nombre, cb)`, payload en `event.payload`)

- `levels` → `Levels`, ~30 Hz mientras haya streams abiertos (grabando, o idle con `monitorWhenIdle`).
- `state` → `EngineState`, en cada cambio y cada ~250 ms mientras graba/pausa.
- `recording-saved` → `Recording`.
- `devices-changed` → `Devices` (sondeo cada ~3 s; emitir solo si cambió la lista).
- `engine-error` → `{ message: string, fatal: boolean }` (p.ej. se desconectó el micrófono: si fatal, el backend detiene y guarda lo grabado).
- `hotkey` → `{ action: "toggle" | "pause" }` (informativo; el backend ya ejecutó la acción).

## Atajos globales

Por defecto `Ctrl+Alt+R` (iniciar / detener) y `Ctrl+Alt+P` (pausar / reanudar); configurables (ver *Extensiones → Atajos configurables*).

## Archivos en disco

Una carpeta por sesión: `<outputDir>\Eco 2026-10-03 14-22-05\` con `Microfono.wav`, `Sistema.wav`, `Mezcla.wav` (según modo y fuentes) y `session.json` (`{ id, name, createdAt, durationMs }`). `list_recordings` escanea las subcarpetas con `session.json`. `name` por defecto: `Grabación 03/10 14:22`.

## Reglas de audio que no se negocian

1. **Loopback en silencio**: WASAPI loopback no entrega paquetes cuando nada suena. Abre en paralelo un stream de salida en ese mismo dispositivo que reproduce ceros, para que el loopback siga entregando muestras. Además, rellena con silencio cualquier hueco detectado por reloj. La pista `Sistema.wav` debe durar lo mismo que la grabación aunque no suene nada.
2. Cada pista se graba en su formato nativo (sample rate y canales del dispositivo) con el `format` elegido. Nada de bloqueos ni E/S en el callback de audio: el callback empuja a un ring buffer / canal y un hilo escritor escribe el WAV.
3. Pausa: las muestras durante la pausa se descartan en las dos pistas, de forma alineada.
4. Mezcla (`mixed` o `both`): al detener, offline: resamplea ambas pistas a 48 kHz estéreo (mono → duplicar), alinea por el instante del primer frame de cada pista, suma con ganancias y limitador suave (sin clipping duro). En `mixed` se borran las pistas separadas tras mezclar; en `both` se conservan.
5. Si solo hay una fuente activa, no se genera mezcla.

## Extensiones

Cambios sobre el contrato original surgidos de las pruebas de integración con el exe real. Son compatibles hacia atrás.

- **`Device.realName?: string`** (opcional). Solo lo trae el pseudo-dispositivo `id: "default"`: nombre real del dispositivo al que apunta hoy el predeterminado de Windows, sin el prefijo. `name` conserva el formato `Predeterminado del sistema (<nombre real>)`. La UI muestra `realName` como etiqueta principal y una insignia/subtexto «Predeterminado»; si el campo falta, lo deriva de `name`. Los dispositivos reales no incluyen el campo.
- **`set_settings` valida `outputDir`**: devuelve `Err` en español si la ruta contiene caracteres ilegales de Windows (`< > " | ? *`, `:` fuera de la unidad, controles) o si ya existe y es un archivo. No crea la carpeta (se crea al grabar). Vacío sigue significando «restaurar el default».
- **Atajo ocupado** (reemplaza al aviso `engine-error` original): si otra aplicación ya tiene el atajo, Eco arranca igual, ese atajo queda con `ok=false` y no emite `engine-error`; la UI lo detecta con `get_hotkey_status` y muestra un aviso con acción «Abrir Ajustes».
- **Forma de onda**: no hizo falta `get_waveform`. `fetch()` sobre `convertFileSrc` (`http://asset.localhost/…`) funciona desde `http://tauri.localhost` (respuesta `type: "cors"`, HTTP 200) con la config actual de `assetProtocol`.
- **Atajos configurables**. `Settings` gana `hotkeyToggle: string` y `hotkeyPause: string` en el formato del plugin (`Ctrl+Alt+R`, `Ctrl+Alt+Shift+R`, `Alt+F9`…; modificadores `Ctrl`, `Alt`, `Shift`, `Super`; la tecla va al final). Defaults `Ctrl+Alt+R` y `Ctrl+Alt+P`; un `settings.json` anterior sin estos campos carga con los defaults (serde `default`); vacío = default; si lo guardado no es válido o ambos son iguales, al arrancar se vuelve a los defaults. Requieren al menos un modificador.
  - `set_settings` re-registra los atajos antes de guardar. Si el nuevo no es válido o no se puede registrar (otra app lo usa) devuelve `Err` en español («No se pudo registrar el atajo X: otra aplicación ya lo está usando. Se conserva el anterior.»), no guarda nada y deja registrado el anterior. Un atajo sin cambios que sigue ocupado se reintenta en silencio y no hace fallar el guardado (así se pueden cambiar otros ajustes).
  - **`get_hotkey_status`** → `{ toggle: { accel: string, ok: boolean }, pause: { accel: string, ok: boolean } }`. `ok=false` significa que el sistema no aceptó el registro (ocupado por otra app).
  - UI (Ajustes → Atajos de teclado): cada atajo global es un botón con la combinación; al hacer clic entra en modo «Presiona la combinación…», captura el siguiente `keydown` con al menos un modificador (Esc cancela, sin modificador pide uno) y llama a `set_settings`. Mientras captura se desactivan los atajos de ventana (Espacio / P). Si `ok=false` se muestra «Ocupado por otra app» en ámbar; «Restablecer» vuelve al default. Si al arrancar algún atajo está ocupado, aviso con acción «Abrir Ajustes».
- **Rutas con separador de Windows**. Todo lo que devuelve el backend usa la barra invertida como separador: `outputDir` (normalizado al validar y guardado así en `settings.json`), `Recording.folder` y `TrackFile.path`. Se aceptan `/` de entrada; se colapsan separadores repetidos y se quita el final salvo en la raíz (`C:` + separador). Se respetan los prefijos de red y de ruta larga.
- **Vista Grabar sin scroll**: a 387×632 px CSS (ventana mínima con escala de Windows ~114 %) y a 440×720 no hay scroll vertical; el botón principal mide ≥ 84 px en todas las alturas.

### Audio de voz

Objetivo: que la voz salga a buen nivel aunque el micrófono sea flojo (p. ej. array de laptop con el volumen de Windows al 26 %). Cadena del micrófono, en este orden: **ganancia manual → AGC («Voz automática») → limitador −1 dBFS → medidores y WAV**. Los medidores y `Microfono.wav` reciben la señal ya procesada.

- **Ganancia en dB**. `Settings.inputGainDb` (−12…+30) y `outputGainDb` (−12…+12), default 0. `validated()` los recorta; NaN → 0. **Migración**: un `settings.json` (o un `set_settings`) que traiga los campos antiguos `inputGain`/`outputGain` (lineales) los convierte con `20·log10(g)` (`1.0` → `0 dB`, `0.5` → `−6 dB`, `0` → `−120 dB` que luego se recorta al mínimo) cuando se valida; los campos antiguos solo se leen, nunca se vuelven a escribir. Mantiene compatibilidad serde: faltan campos → defaults.
- **Voz automática** (`inputAgc`, default `true`; solo micrófono). Objetivo ≈ −18 dBFS RMS de voz; ganancia del AGC de −20 a **+30 dB** (se suma a la manual, que va antes); ataque ~10 ms (baja rápido), subida ~400 ms (~120 ms si está a más de 6 dB del objetivo). Solo sube con voz: detector por nivel con piso de ruido adaptable (baja al instante, sube 1,5 dB/s, o salta al nivel si el ruido es estacionario ~0,6 s), umbral +8 dB sobre el piso (mín. −66 dBFS), histéresis y espera de 250 ms. Hasta comprobar que la señal varía como el habla (≥ 6 dB en 0,5 s de actividad) no pasa de +6 dB, y en silencio la ganancia se relaja hacia lo que deje el fondo en ≤ −52 dBFS. El AGC corre dentro del callback de captura pero sin asignar memoria ni tomar locks (coste constante por muestra; ver `audio/dsp.rs`), así que no bloquea el callback; no hay hilo de proceso aparte.
- **Limitador** (`audio/dsp.rs::Limiter`): look-ahead de 2 ms, techo −1 dBFS (0,8913), recuperación ~80 ms, ganancia común a los canales. Se aplica a las dos pistas separadas (micrófono y sistema) antes de escribir el WAV, también con el AGC apagado, así que wav16/wav24 ya no recortan de forma dura aunque la ganancia sea alta. Latencia añadida: 2 ms. La mezcla sigue teniendo su limitador suave propio.
- **Prioridad de voz** (`mixVoicePriority`, default `true`). Solo afecta a la mezcla (`Mezcla.wav`, offline): `Microfono.wav` detecta voz con el mismo detector y el audio del sistema baja 8 dB (ataque 50 ms, recuperación 500 ms, espera de 250 ms tras la voz). Las pistas separadas no se tocan.
- **Volumen del micrófono en Windows** (IAudioEndpointVolume del dispositivo de captura elegido en `inputId`; `"default"` = el predeterminado de Windows). Nuevos comandos (en `src-tauri/src/commands_audio.rs`, lógica en `eco_audio::audio::endpoint`):

  | Comando | Args | Devuelve |
  |---|---|---|
  | `get_input_endpoint_volume` | — | `{ deviceName: string, level: number /*0..1*/, muted: boolean, boost: null }` |
  | `set_input_endpoint_volume` | `{ level: number }` (0..1) | igual que arriba, con el estado resultante; si `level` > 0 también quita el silencio |

  `boost` va siempre `null` (el refuerzo del driver no se puede leer de forma genérica). Solo toca el dispositivo de captura, nunca el volumen de reproducción. Errores en español (`Err(String)`); en otras plataformas siempre `Err`.
- **UI**. Tarjeta Micrófono: slider en dB (−12…+30, marcas en 0 y +12, doble clic = 0 dB, lectura «+18 dB»), el switch «Voz automática» en la cabecera de la tarjeta y, si el nivel de Windows es < 70 % o está silenciado, un aviso «El volumen del micrófono en Windows está en 40 %» (o «El micrófono está silenciado en Windows») con el botón «Subir a 100 %». Se relee al cambiar de dispositivo, al volver el foco a la ventana y cada 5 s en reposo; si el comando falla el aviso no se muestra. Tarjeta Sistema: slider −12…+12 dB. Ajustes → grupo «Audio»: switches «Voz automática» y «Prioridad de voz en la mezcla». La vista Grabar sigue sin scroll a 440×720 y 387×632, con o sin aviso.
- **Mock** (`ui/mock-tauri.js`): implementa ambos comandos; `?micvol=40` fija el volumen de Windows simulado (%), `?micmute=1` lo silencia; por defecto 100 %.
- **Sonda**: `cargo run --release --example mic_probe -- [segundos] [gananciaDb] [speak]` graba el micrófono real con la voz automática activada y desactivada y compara pico/RMS; solo lee el volumen de Windows.

### Bandeja y widget

Eco vive en el área de notificación (junto al reloj). Cerrar la ventana la oculta; el motor sigue activo. Todo lo de esta sección es del *app crate* (`src-tauri/src/{tray,flyout,icons,prefs,app_commands}.rs`); los `Settings` del motor no cambian.

- **Preferencias de la app** (`%APPDATA%\Eco\app-prefs.json`, aparte de `settings.json`; faltante o corrupto = defaults):
  `AppPrefs = { closeToTray: boolean /*true*/, launchAtStartup: boolean /*false*/, floatingPill: boolean /*false*/, trayHintShown: boolean /*false*/, pillX?: number|null, pillY?: number|null }`.
  `pillX/pillY` (píxeles físicos del área visible de la píldora) los gestiona el backend; `set_app_prefs` los ignora, y `trayHintShown` nunca vuelve a `false` desde la UI.
  - `get_app_prefs` → `AppPrefs` (`launchAtStartup` refleja el registro real).
  - `set_app_prefs { prefs: AppPrefs }` → `AppPrefs` guardado. Si cambia `launchAtStartup` crea/borra la entrada `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Eco` (`<ruta>\Eco.exe --hidden`); si el registro falla devuelve `Err` en español y no guarda. Emite `app-prefs-changed` (payload `AppPrefs`) para sincronizar Ajustes, menú y widget.
  - `show_main_window` → `null`: muestra, restaura y enfoca la ventana principal y cierra el flyout. `hide_widget` → `null`: cierra el flyout (Esc).
- **Ícono de bandeja** (`tray-icon`): tres variantes dibujadas por código (anillo + centro) a 16/20/24/32 px según el tamaño del ícono pequeño del sistema: reposo (punto del color del tema), grabando (punto coral `#FF5A4E`) y en pausa (dos barras ámbar). Se regenera si cambia el tema de la barra de tareas (`SystemUsesLightTheme`). Tooltip vivo (~250 ms): `Eco — Listo`, `Eco — Grabando 02:13`, `Eco — En pausa 02:13`, `Eco — Guardando…`. Nota: Windows 11 compone el nombre accesible del botón con el primer tooltip registrado («Eco — Listo») más el actual.
- **Clic izquierdo**: abre o cierra el flyout `widget` (`ui/widget.html`, 320×220 lógicos, sin decoraciones, siempre encima, sin botón en la barra de tareas) anclado al rectángulo del ícono que reporta el evento de la bandeja. Posición (`flyout.rs`, píxeles físicos): borde de la barra deducido de lo que quita al área de trabajo (si no quita nada, borde más cercano al ícono); centrado sobre el ícono y limitado al área de trabajo del monitor del ícono, con margen de 12 px lógicos × escala. Barra abajo → encima de ella; arriba → debajo; izquierda/derecha → al costado. El tamaño y la posición se aplican midiendo con Win32 (el marco invisible de las ventanas sin decoraciones de tao no coincide con su modelo de tamaño). Se oculta al perder el foco y con Esc; un clic en el ícono que acaba de quitarle el foco cuenta como «cerrar». Entrada: fundido + 10 px de desplazamiento (180 ms); se vuelve a lanzar con el evento `widget-shown` (`{ hint: boolean }`).
  Contenido: estado + temporizador, botón grabar/detener, pausa, medidores Mic / Sistema con su switch (bloqueados mientras se graba, igual que la vista Grabar) y enlace «Abrir Eco». Usa solo los comandos y eventos de este contrato (`get_settings`, `set_settings`, `get_state`, `start/pause/resume/stop_recording`, `levels`, `state`, `engine-error`) y los tokens de `ui/tokens.css` (extraídos de `styles.css`, que los importa). Tema compartido con la ventana principal (`localStorage['eco-theme']`).
- **Clic derecho**: menú nativo: `Iniciar grabación` / `Detener grabación` (deshabilitado mientras finaliza), `Pausar` / `Reanudar` (deshabilitado si no graba), `Abrir Eco`, `Abrir carpeta de grabaciones`, separador, `Iniciar con Windows` (con marca), `Salir`. Los textos se actualizan con el estado. **Doble clic** en el ícono: abre la ventana principal.
- **Cerrar a la bandeja**: el botón X de la barra propia y Alt+F4 ocultan la ventana principal si `closeToTray`. La primera vez se muestra el flyout con «Eco sigue en la bandeja» (se retira solo a los 6 s) y se marca `trayHintShown`. Con `closeToTray=false`, cerrar equivale a «Salir». **Salir** detiene y guarda si está grabando y termina el proceso (no queda ninguno).
- **Inicio con Windows**: `tauri-plugin-autostart` con el argumento `--hidden`. Con `--hidden` (y bandeja disponible) la ventana principal no se muestra al arrancar. Toggle en Ajustes → «Ventana y bandeja» y en el menú, sincronizados.
- **Instancia única** (`tauri-plugin-single-instance`): abrir `Eco.exe` otra vez muestra la ventana existente (salvo que traiga `--hidden`) y el segundo proceso termina.
- **Píldora flotante** (`floatingPill`, default `false`): mientras graba o está en pausa aparece la ventana `pill` (`ui/pill.html`, 216×52 con sombra, transparente, siempre encima, sin foco ni botón en la barra de tareas) arrastrable, con punto (ámbar en pausa), temporizador y botón detener. Recuerda su posición en `app-prefs` (si queda fuera de todos los monitores vuelve al centro superior). Desaparece al detener.
- **Ajustes → «Ventana y bandeja»**: tres switches (`Cerrar a la bandeja`, `Iniciar con Windows`, `Píldora flotante al grabar`) que llaman a `set_app_prefs`.
- **Capabilities**: `capabilities/widget.json` (ventanas `widget` y `pill`: `core:default`, `core:window:allow-start-dragging`). La ventana `main` se crea oculta (`visible: false`) y se muestra al arrancar salvo con `--hidden`.
- **Mock** (`ui/mock-tauri.js`): implementa `get_app_prefs`, `set_app_prefs` (emite `app-prefs-changed`), `show_main_window` y `hide_widget`; `?autostart=1` y `?pill=1` fijan prefs iniciales. `widget.html?mock&state=recording&theme=light` y `pill.html?mock` cargan los widgets en un navegador.
