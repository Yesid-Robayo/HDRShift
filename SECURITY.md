# Security · Seguridad

[English](#english) · [Español](#español)

## English

### How to report a security problem

If you find a security flaw (for example, a way to make HDRShift run something that isn't in your app list, to write files
outside its folder or to get administrator rights), **do not post it in an *issue***. Use GitHub's private report:

1. The repository's **Security** tab.
2. **Report a vulnerability**.
3. Describe what happens, how to reproduce it and with which version.

Only the author will see it. You will get an answer as soon as possible and, once it is fixed, a new version will be
released.

### Supported versions

Only the latest version published in *Releases* receives fixes.

### What HDRShift protects

- **It only changes the Windows HDR setting**, through the same Windows request as the Settings switch. It sends nothing to
  your monitor by itself and doesn't touch Automatic Color Management.
- **It doesn't touch your games.** It doesn't inject anything, read their memory or change their files. To know whether an
  app is open it only reads the list of processes and their paths, as Task Manager does.
- **No administrator rights, no drivers, no services installed.** The only registry changes are under
  `HKEY_CURRENT_USER`: a `Run` value if you turn on starting with Windows or mark an app to turn on by itself, and a `RunOnce`
  value that exists only while HDR is left to restore.
- **Nothing is executed from a message.** Other processes can only ask for an app already in your list by its id, a switch change, ending a session, forgetting what's left to restore, a reload or quitting. Orders are size-limited and rejected if they aren't valid.
- **No telemetry, no automatic updates, no accounts.** It never connects to the internet. The links in the window, like GitHub
  or Ko-fi, only open your browser when you click them.
- **A broken file isn't lost**: if `config.json` can't be read, it's first copied to `config.bad.json`. The program and
  arguments in it are run as they are, so treat that file like a shortcut.
- **The log has no personal data**: no PC or Windows user name, no paths. It does include the names you gave your apps and
  your monitor's model.
- **Releases are built by GitHub Actions** from this repository's code, with a published SHA256 and a provenance attestation
  you can verify (see the README). Dependencies are locked in `Cargo.lock` with their checksums.

### What it does not protect

- The exe isn't signed with a paid certificate: check the SHA256 or the attestation before running it.
- HDRShift opens the program you put in your list, with its arguments. Only add programs you trust.
- Any program running as your user can read or change the files in `%APPDATA%\HDRShift`, including your app list.

## Español

### Cómo avisar de un problema de seguridad

Si encuentras un fallo de seguridad (por ejemplo, una forma de hacer que HDRShift ejecute algo que no esté en tu lista de
apps, escriba archivos fuera de su carpeta u obtenga permisos de administrador), **no lo publiques en un *issue***. Usa el
aviso privado de GitHub:

1. La pestaña **Security** del repositorio.
2. **Report a vulnerability**.
3. Describe qué pasa, cómo repetirlo y con qué versión.

Solo lo verá el autor. Recibirás una respuesta lo antes posible y, una vez arreglado, saldrá una versión nueva.

### Versiones con soporte

Solo la última versión publicada en *Releases* recibe arreglos.

### Qué protege HDRShift

- **Solo cambia el ajuste de HDR de Windows**, con la misma petición que el interruptor de Configuración. No le envía nada a
  tu monitor por su cuenta y no toca la gestión automática de color.
- **No toca tus juegos.** No inyecta nada, no lee su memoria ni cambia sus archivos. Para saber si una app está abierta solo
  consulta la lista de procesos y sus rutas, como el Administrador de tareas.
- **Sin permisos de administrador, sin drivers y sin servicios instalados.** Los únicos cambios en el registro están en
  `HKEY_CURRENT_USER`: un valor en `Run` si activas el inicio con Windows o marcas una app con Encender solo, y un valor en
  `RunOnce` que solo existe mientras queda HDR por restaurar.
- **Nada se ejecuta a partir de un mensaje.** Otros procesos solo pueden pedir por su id una app que ya esté en tu lista, un cambio del interruptor, terminar una sesión, olvidar lo que queda por restaurar, recargar o salir. Las órdenes tienen un tamaño máximo y se rechazan si no son
  válidas.
- **Sin telemetría, sin actualizaciones automáticas y sin cuentas.** Nunca se conecta a internet. Los enlaces de la ventana, como GitHub o
  Ko-fi, solo abren tu navegador cuando los pulsas.
- **Un archivo dañado no se pierde**: si `config.json` no se puede leer, antes se copia a `config.bad.json`. El programa y los
  argumentos que contiene se ejecutan tal cual, así que trata ese archivo como a un acceso directo.
- **El log no tiene datos personales**: ni nombre del PC o del usuario de Windows, ni rutas. Sí incluye los nombres que les
  pusiste a tus apps y el modelo de tu monitor.
- **Las versiones las compila GitHub Actions** desde el código de este repositorio, con un SHA256 publicado y una atestación
  de origen verificable (mira el README). Las dependencias están fijadas en `Cargo.lock` con sus sumas de comprobación.

### Lo que no protege

- El exe no está firmado con un certificado de pago: comprueba el SHA256 o la atestación antes de ejecutarlo.
- HDRShift abre el programa que pongas en tu lista, con sus argumentos. Añade solo programas de confianza.
- Cualquier programa que se ejecute con tu usuario puede leer o cambiar los archivos de `%APPDATA%\HDRShift`, incluida tu
  lista de apps.
