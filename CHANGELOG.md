# Changelog

**English** · [Español](CHANGELOG.es.md)

What each version of HDRShift brought and fixed, newest first. Published versions have their exe, their notes in English
and Spanish and their SHA256 in [Releases](https://github.com/Yesid-Robayo/HDRShift/releases).

When a new version is published, it's added at the top with the same content as its release notes.

## 0.1.0

First version. It's a personal project in beta: **you use it at your own risk** and it may not work with your monitor.

### HDR per app

- **Add your apps**, even by picking one that's already open, choose their monitors and create a shortcut with the app's
  own icon.
- **HDR before the app:** it turns on and is verified before opening the app, since many games read it only at startup.
- **Back to how it was:** when the app closes, HDR returns to its previous state. If it was already on, it's left alone.
- **Launchers:** a game counts as open while anything from its folder is running, so a launcher can open it and exit.
- **Several apps at once:** HDR stays on until the last one closes.

### Switch and detection

- **Manual switch** in the window, in the tray and as a one-click shortcut.
- **Optional automatic detection, per app,** for apps opened from other launchers. Off by default.
- **Windows notification** when HDR turns on for an app.
- **Adjustable wait per app,** for monitors that take longer to switch.
- **Nothing resident by default:** the process exits on its own when no app needs HDR.

### Safety

- **A single owner** of HDR, so sessions can't overwrite each other.
- **Written down first:** what has to be restored is saved before changing HDR.
- **Verified:** a change only counts after reading the real state back, whatever Windows answers.
- **Automatic recovery** after a crash, a power cut, a missing monitor or signing out of Windows.
- **A moved monitor is still found** if the cable changes port.

### App

- **Dark and light themes**, or the same one as Windows, and **Spanish and English**.
- **Portable mode**, with your data next to the exe, and **uninstall** from the settings.
- A single exe of about 5 MB, with no runtime and no administrator rights.
