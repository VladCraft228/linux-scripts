# Dock — technical prototype

An experimental Qt 6 Quick dock for XFCE on X11, developed inside linux-scripts.
This is the architecture-validation stage, not yet a replacement for Plank.

## What works in this prototype

- A translucent rounded panel and smoothly magnified neighboring icons.
- Running X11 windows, their titles, icons, and active-window indicators.
- Click a window icon to request activation from the window manager.
- Event-driven window updates through XCB and QSocketNotifier.
- Input-only X11 shaping: transparent margins pass clicks through.
- Freedesktop icon themes, with window-icon and generated-icon fallbacks.
- Single-instance guard and a right-click Quit action.

One icon currently represents **one window**, including windows on other
workspaces. Application grouping, desktop-file matching, pinned launchers,
drag-and-drop, autohide, settings, and Wayland support belong to later stages.
The window manager ultimately decides whether to honor activation requests.

## Requirements

- Debian 13 / MX Linux based on Debian 13, XFCE X11.
- Qt 6.8 or later, C++17, CMake 3.21 or later.
- An enabled compositor for transparency (XFCE Window Manager Tweaks → Compositor).
- X11 SHAPE extension and an EWMH-compatible window manager.

Debian / MX build and runtime dependencies:

```bash
sudo apt update && sudo apt install \
  build-essential cmake ninja-build pkg-config \
  qt6-base-dev qt6-declarative-dev \
  qml6-module-qtquick qml6-module-qtquick-window \
  qml6-module-qtqml-workerscript \
  libxcb1-dev libxcb-ewmh-dev libxcb-shape0-dev
```

The prototype uses core XCB/EWMH properties directly; xcb-ewmh is reserved for
the next backend iteration and is not linked yet.

## Build and run without installation

```bash
cd ~/Projects/linux-scripts/common/dock
bash build.sh && ./build/linux-scripts-dock --icon-theme WhiteSur-light
```

By default the prototype sits above the bottom edge with a 96 px offset to leave
room for an existing Plank. It neither disables Plank nor reserves screen space.
It uses the primary screen initially. It does not yet provide monitor selection.

To move it closer to the edge:

```bash
./build/linux-scripts-dock --bottom-offset 8
```

Right-click the panel or an icon, then click **Quit dock prototype**. Alternatively,
press Ctrl+C in the launching terminal. Click an icon or right-click again to
dismiss the Quit action without exiting.

## Optional user installation

```bash
bash install.sh
~/.local/bin/linux-scripts-dock --icon-theme WhiteSur-light
```

Installation builds the executable and copies it to `~/.local/bin/`. It does not
start the dock, add autostart, install dependencies, or change desktop settings.

To remove the installed executable:

```bash
bash uninstall.sh
```

## Validation and diagnostics

Follow [the manual test checklist](docs/testing.md) on real XFCE hardware.
The included CI workflow is configured for a Debian 13 build, a headless QML
test, and a basic X11/WM smoke test. It has not yet run on GitHub. These checks
do not verify compositor performance or real pointer interaction.

Current validation: the C++ executable builds with Qt 6.8.2 and the headless QML
test passes, including model population and reset. Full X11 integration remains
unverified: the development environment could not provide a reachable X server.

```bash
ctest --test-dir build --output-on-failure
```

For Qt rendering diagnostics:

```bash
QSG_INFO=1 ./build/linux-scripts-dock 2>&1 | tee /tmp/linux-scripts-dock.log
```

The technical prototype refreshes window metadata with bounded synchronous X11
reads after relevant events. Icon results are reused until icon/class changes.
There is no permanent polling timer or animation loop while idle.
Very large window lists, asynchronous metadata loading, complete icon-theme
matching, and high-DPI/multi-monitor behavior require further validation.

## Architecture and license

See [architecture.md](docs/architecture.md). This directory does not declare a
project license yet; select one before announcing an open-source release.
