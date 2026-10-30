# Architecture · Arquitectura

[English](#english) · [Español](#español)

## English

### Overview

HDRShift is a Rust workspace with two crates:

| Crate | Contents |
|---|---|
| `core` (`hdrshift-core`) | The engine, with no graphics: displays, sessions, app watching, configuration. |
| `app` (`HDRShift.exe`) | The service, the tray and the window. |

### One exe, several roles

| Command | Role |
|---|---|
| no arguments | Window. A separate process that only exists while it's open. |
| `--launch <id>` | Gives the order to open an app. |
| `--toggle` | Gives the order to flip the manual switch. |
| `--service` | Starts the service with no order. |
| `--recover` | Puts HDR back and exits. Windows runs it from `RunOnce`. If it can't, it stays as the service. |
| `--reopen` | Window that first waits for the previous one to close. |

The service is the only process allowed to change HDR. A named mutex guarantees there is one per session. A shortcut first
tries to hand its order to the running service through `WM_COPYDATA`. If there isn't one, that same process becomes the
service, so nothing extra is launched. The service exits by itself when it has nothing to do, unless the user asked it to stay
in the tray.

### The session engine

`core/src/session.rs` is the heart of the app, and it can be tested without a screen because it only sees two traits:
`Displays` and `LedgerStore`.

- A `Hold` is one reason to keep HDR on: an app or the manual switch of a monitor.
- `Ledger::turned_on` is the list of monitors HDRShift must turn off. It's a debt: it only contains what HDRShift itself
  turned on.

The rules that come from those two ideas:

1. A monitor's debt is saved **before** changing it.
2. HDR that was already on never enters the debt, so it's never turned off.
3. A monitor is restored when no `Hold` uses it any more.
4. A debt is removed only after reading the real state back.
5. Anything that fails stays in the debt and is retried.
6. If the debt can't be saved, HDR isn't touched.
7. An unreadable monitor counts as missing, never as off.

### Why the answer from Windows isn't trusted

Measured on an ASUS XG27AQDMES with Windows build 26300: with the screen off, `DisplayConfigSetDeviceInfo` answers
`ERROR_ACCESS_DENIED` (5) **and applies the change anyway**. `Engine::switch` therefore ignores the answer as a verdict: it
reads the state back, and the error code is only used to explain a failure.

The state read is the `highDynamicRangeUserEnabled` bit, the user's setting. With the screen off it changes, while the active
colour mode only changes once the screen is on.

### Windows requests

`core/src/display.rs` uses the CCD API. On Windows 11 24H2 or later it uses `GET_ADVANCED_COLOR_INFO_2` and `SET_HDR_STATE`,
which separate HDR from Automatic Color Management. Before that it uses `GET_ADVANCED_COLOR_INFO` and
`SET_ADVANCED_COLOR_STATE`.

The older request is **never** a fallback on modern Windows: if the new one fails, the old one would change Automatic Color
Management instead of HDR.

A monitor's identity is its device path, because the adapter id changes between boots. If the cable moves to another port the
path changes too, so a saved monitor is also matched by model when only one monitor has that model.

### Watching apps

`core/src/watch.rs` decides whether an app is open. The launched process isn't enough, because many launchers exit after
opening the game. A process counts as the app if it is:

- the exact exe;
- anything in the exe's folder or subfolders, unless that folder is shared by unrelated programs (`Downloads`, `Windows`...);
- the extra process name set by the user.

Full paths are compared, so another program's `Launcher.exe` isn't the app. A path is read once per process. The service takes
a process snapshot every 2 seconds while there's something to watch, and an app counts as closed 5 seconds after its last
process is gone, to leave room for a launcher handing over to the game.

### Recovery

| Situation | What happens |
|---|---|
| The service dies | The next service keeps the sessions of apps still open and restores the rest. |
| The PC shuts down | The service restores on `WM_ENDSESSION`. If it can't, `RunOnce` runs `--recover` at the next sign-in. |
| The monitor isn't there | The debt stays and is retried when the display changes and every 30 seconds. The user can forget it from the window if that monitor is gone. |
| The window opens with debt and no service | It starts the service, which recovers. |

### Files

Everything is in `%APPDATA%\HDRShift`, or in `HDRShiftData` next to the exe in portable mode.

| File | Contents |
|---|---|
| `config.json` | Apps and settings. Written by the window. |
| `ledger.json` | The debt and the active sessions. It only exists while there's something to restore or watch. |
| `status.json` | What the service publishes for the window. |
| `hdrshift.log` | The log. |

Files are written atomically (temporary file + rename).

### Window

The window never changes HDR. It reads the real state of the monitors every second, reads `status.json` to know who is holding
HDR, and sends orders to the service. The colours of both themes are in `app/src/ui/theme.rs`.

## Español

### Visión general

HDRShift es un workspace de Rust con dos crates:

| Crate | Contenido |
|---|---|
| `core` (`hdrshift-core`) | El motor, sin gráficos: monitores, sesiones, vigilancia de apps, configuración. |
| `app` (`HDRShift.exe`) | El servicio, la bandeja y la ventana. |

### Un exe, varios papeles

| Orden | Papel |
|---|---|
| sin argumentos | Ventana. Un proceso aparte que solo existe mientras está abierta. |
| `--launch <id>` | Da la orden de abrir una app. |
| `--toggle` | Da la orden de cambiar el interruptor manual. |
| `--service` | Arranca el servicio sin ninguna orden. |
| `--recover` | Deja el HDR como estaba y sale. Lo lanza Windows desde `RunOnce`. Si no puede, sigue como servicio. |
| `--reopen` | Ventana que espera antes a que se cierre la anterior. |

El servicio es el único proceso que puede cambiar el HDR. Un mutex con nombre garantiza que haya uno por sesión. Un acceso
directo primero intenta pasarle su orden al servicio por `WM_COPYDATA`. Si no hay ninguno, ese mismo proceso se convierte en el
servicio, así que no se lanza nada de más. El servicio sale solo cuando no tiene nada que hacer, salvo que el usuario haya
pedido quedarse en la bandeja.

### El motor de sesiones

`core/src/session.rs` es el corazón de la app, y se puede probar sin pantalla porque solo conoce dos rasgos: `Displays` y
`LedgerStore`.

- Una `Hold` es un motivo para mantener el HDR: una app o el interruptor manual de un monitor.
- `Ledger::turned_on` es la lista de monitores que HDRShift debe apagar. Es una deuda: solo contiene lo que encendió HDRShift.

Las reglas que salen de esas dos ideas:

1. La deuda de un monitor se guarda **antes** de cambiarlo.
2. El HDR que ya estaba encendido nunca entra en la deuda, así que nunca se apaga.
3. Un monitor se restaura cuando ninguna `Hold` lo usa.
4. Una deuda solo se borra tras leer el estado real.
5. Lo que falla se queda en la deuda y se reintenta.
6. Si la deuda no se puede guardar, no se toca el HDR.
7. Un monitor que no se puede leer cuenta como ausente, nunca como apagado.

### Por qué no se cree la respuesta de Windows

Medido en un ASUS XG27AQDMES con Windows build 26300: con la pantalla apagada, `DisplayConfigSetDeviceInfo` responde
`ERROR_ACCESS_DENIED` (5) **y aplica el cambio igualmente**. Por eso `Engine::switch` no usa la respuesta como veredicto: lee el
estado, y el código de error solo sirve para explicar un fallo.

El estado leído es el bit `highDynamicRangeUserEnabled`, el ajuste del usuario. Con la pantalla apagada ese bit cambia, mientras
que el modo de color activo solo cambia cuando la pantalla está encendida.

### Peticiones a Windows

`core/src/display.rs` usa la API CCD. En Windows 11 24H2 o posterior usa `GET_ADVANCED_COLOR_INFO_2` y `SET_HDR_STATE`, que
separan el HDR de la gestión automática de color. Antes usa `GET_ADVANCED_COLOR_INFO` y `SET_ADVANCED_COLOR_STATE`.

La petición antigua **nunca** es un plan B en Windows moderno: si la nueva falla, la antigua cambiaría la gestión automática de
color en lugar del HDR.

La identidad de un monitor es su ruta de dispositivo, porque el identificador del adaptador cambia entre arranques. Si el cable
cambia de puerto la ruta también cambia, así que un monitor guardado también se reconoce por su modelo cuando solo hay uno de
ese modelo.

### Vigilar las apps

`core/src/watch.rs` decide si una app sigue abierta. El proceso lanzado no basta, porque muchos lanzadores se cierran tras abrir
el juego. Un proceso cuenta como la app si es:

- el exe exacto;
- cualquier cosa de la carpeta del exe o sus subcarpetas, salvo que la compartan programas sin relación (`Descargas`,
  `Windows`...);
- el nombre de proceso extra que indicó el usuario.

Se comparan rutas completas, así que el `Launcher.exe` de otro programa no es la app. Cada ruta se lee una vez por proceso. El
servicio mira los procesos cada 2 segundos mientras hay algo que vigilar, y una app cuenta como cerrada 5 segundos después de
que desaparezca su último proceso, para dejar margen al relevo entre lanzador y juego.

### Recuperación

| Situación | Qué pasa |
|---|---|
| Muere el servicio | El siguiente conserva las sesiones de las apps aún abiertas y restaura el resto. |
| Se apaga el PC | El servicio restaura con `WM_ENDSESSION`. Si no puede, `RunOnce` lanza `--recover` en el siguiente inicio de sesión. |
| No está el monitor | La deuda se queda y se reintenta al cambiar la pantalla y cada 30 segundos. El usuario puede olvidarla desde la ventana si ese monitor ya no existe. |
| Se abre la ventana con deuda y sin servicio | Arranca el servicio, que recupera. |

### Archivos

Todo está en `%APPDATA%\HDRShift`, o en `HDRShiftData` junto al exe en modo portable.

| Archivo | Contenido |
|---|---|
| `config.json` | Apps y ajustes. Lo escribe la ventana. |
| `ledger.json` | La deuda y las sesiones activas. Solo existe mientras hay algo que restaurar o vigilar. |
| `status.json` | Lo que publica el servicio para la ventana. |
| `hdrshift.log` | El log. |

Los archivos se escriben de forma atómica (temporal + renombrar).

### Ventana

La ventana nunca cambia el HDR. Lee el estado real de los monitores cada segundo, lee `status.json` para saber quién mantiene el
HDR y le da órdenes al servicio. Los colores de los dos temas están en `app/src/ui/theme.rs`.
