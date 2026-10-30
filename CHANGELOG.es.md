# Cambios por versión

**Español** · [English](CHANGELOG.md)

Qué trajo y qué arregló cada versión de HDRShift, de la más nueva a la más vieja. Las versiones publicadas tienen su exe, sus
notas en inglés y en español y su SHA256 en [Releases](https://github.com/Yesid-Robayo/HDRShift/releases).

Al publicar una versión nueva, se añade aquí arriba con lo mismo que sus notas.

## 0.1.0

Primera versión. Es un proyecto personal en beta: **se usa bajo tu propio riesgo** y puede que no funcione con tu monitor.

### HDR por app

- **Añade tus apps**, elige sus monitores y crea un acceso directo con el icono de la app.
- **HDR antes que la app:** se enciende y se comprueba antes de abrirla, porque muchos juegos solo lo miran al arrancar.
- **Vuelve como estaba:** al cerrarse la app, el HDR regresa a su estado anterior. Si ya estaba encendido, no se toca.
- **Lanzadores:** un juego cuenta como abierto mientras haya algo de su carpeta en marcha, así que un lanzador puede abrirlo y
  cerrarse.
- **Varias apps a la vez:** el HDR sigue encendido hasta que se cierra la última.

### Interruptor y detección

- **Interruptor manual** en la ventana, en la bandeja y como acceso directo de un clic.
- **Detección automática opcional** para apps abiertas desde Steam u otros lanzadores. Viene apagada.
- **Nada residente por defecto:** el proceso sale solo cuando ninguna app necesita HDR.

### Seguridad

- **Un solo dueño** del HDR, para que las sesiones no puedan pisarse.
- **Primero se apunta:** lo que hay que restaurar se guarda antes de cambiar el HDR.
- **Comprobado:** un cambio solo cuenta tras leer el estado real, responda lo que responda Windows.
- **Recuperación automática** tras un cuelgue, un apagón, un monitor ausente o cerrar la sesión de Windows.
- **Encuentra el monitor aunque cambie de puerto** el cable.

### App

- **Tema oscuro y claro**, o el mismo de Windows, y **español e inglés**.
- **Modo portable**, con tus datos junto al exe, y **desinstalar** desde los ajustes.
- Un solo exe de unos 5 MB, sin runtime ni permisos de administrador.
