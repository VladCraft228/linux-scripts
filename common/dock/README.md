# Linux Scripts Dock

An experimental macOS-inspired application dock for XFCE on X11.
The engine is written in Rust; Qt Quick/QML provides the interface. A small C++
adapter connects Qt's models, image rendering and event loop to CXX-Qt.

## Features

- Smooth upward icon magnification and a translucent rounded background.
- Fixed background geometry during hover and a stationary lower edge.
- Pinned and running application groups, active indicators and drag reordering.
- Import local `.desktop` launchers by dragging them onto the dock.
- Minimize/restore on click, explicit window selection and normal window closing.
- Hover cards with application icons and window titles (not screenshots).
- Bounded launch bounce, stopped by a matching new window, error or timeout.
- Intelligent hide based on overlap with visible windows on the current workspace.
- Live, persistent settings for size, magnification, opacity, offset, hide delay
  and the shared show/hide animation duration.
- System icon theme integration, single-instance protection and XFCE autostart.

## Platform

Debian 13 / XFCE / X11 is the initial target. Qt 6.8 or newer is required.
**Wayland is not supported**, including Plasma Wayland through XWayland.
The dock uses the primary monitor; hotplug, high DPI and very large icon sets
still need wider testing. Keep the existing XFCE panel available while testing.

## Build on Debian 13 / MX based on Debian 13

```bash
sudo apt update
sudo apt install build-essential cmake ninja-build pkg-config mold python3 \
  qt6-base-dev qt6-declarative-dev qmake6 \
  qml6-module-qtquick qml6-module-qtquick-window \
  qml6-module-qtqml-workerscript libglib2.0-dev libxcb1-dev
```

Install the stable Rust toolchain if it is not already available:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
```

Rust 1.100.0 and Qt 6.8.2 were used for the migration checks. Use a current stable
Rust toolchain; Debian's older distribution Rust compiler may not build the locked
dependency set. `Cargo.lock` is committed. The first build downloads crates.
CXX-Qt 0.8.1 is locked for the tested bridge/export integration. Qt and generated
bridge code still require a C++ compiler; this is a Rust engine, not a Rust-only UI.
`mold` is used for linking the Rust static library into the Qt executable.

```bash
bash build.sh
ctest --test-dir build --output-on-failure
./build/linux-scripts-dock
```

The build output remains `build/linux-scripts-dock`. Quit an older instance before
running the new one. `--bottom-offset 8` overrides and saves the offset; normally
omit it so the saved settings apply. `--icon-theme Papirus` is an optional override;
by default the dock follows XFCE's theme at startup.

## Install, autostart and remove

```bash
bash install.sh
~/.local/bin/linux-scripts-dock
```

Installation atomically replaces `~/.local/bin/linux-scripts-dock` and creates
`$XDG_CONFIG_HOME/autostart/linux-scripts-dock.desktop` (default `~/.config`).
The entry starts only in XFCE. An existing entry is preserved, including a disabled
one. The installer does not stop the current process or launch a second instance.
Restart the running dock manually to use an update.

Use `bash install.sh --no-autostart` to install without creating an entry.
Uncheck the entry in **Session and Startup → Application Autostart** to disable it.

```bash
bash uninstall.sh
```

Uninstall removes the installed executable and autostart entry. Quit any running
copy separately. User settings and imported launcher copies are preserved.

## Settings and migration

Right-click an icon or the background and select **Settings…**. Changes apply
immediately. Smaller **Show / hide animation** values mean faster movement;
**Hide delay** is the separate pause before hiding. Intelligent hide hides only
when a mapped normal window/dialog intersects the resting dock footprint. Windows
on other workspaces, minimized windows, desktop windows and other panels are
excluded. Hovering, menus, cards, dragging and settings keep the dock accessible.

The Rust version keeps the original Qt QSettings identity:
`LinuxScripts/linux-scripts-dock`, including pinned IDs, imported launcher paths,
intelligent-hide preference and appearance keys. It does not rewrite the user's
configuration into a new Rust-specific format. Back up settings before migration:

```bash
cp -a ~/.config/LinuxScripts ~/.config/LinuxScripts.before-rust
```

Run the command only if that directory exists and choose a fresh backup name if
needed. Custom XDG configuration paths are also respected by Qt.

## Launchers and grouping

Launchers are parsed and launched through GIO; `Exec` is not expanded by a shell.
Drops copy/register entries without executing them or moving the original. Local
readable `.desktop` files are limited to 32 per drop and 1 MiB per entry. Imported
copies survive source removal, but referenced icons/binaries are not packaged.

Matching prefers desktop identity hints and `StartupWMClass`, refusing ambiguous
fallbacks. Steam/Wine game identity remains a known limitation: a game may have a
different WM_CLASS from its launcher. Supply the correct `StartupWMClass` when
needed; the migration does not guess or merge unrelated games.

## Validation and scope

`ctest` runs Rust policy tests and the existing headless QML interaction test.
`cargo test --manifest-path rust/Cargo.toml --locked` runs the Rust core alone.
The isolated X11 test must **never** run against a user's live display:

```bash
xvfb-run -a build/dock-x11-grab-test
xvfb-run -a bash tests/smoke-x11.sh
```

Native smoke tests require `xvfb xauth openbox x11-utils xdotool`. They complement,
but do not replace, manual XFCE testing of drag-and-drop, the compositor, themes,
application matching and monitor changes. No CPU/RAM improvement is claimed from
the language change alone. See [architecture.md](docs/architecture.md) and
[testing.md](docs/testing.md).

No project license has been selected yet. Add a license before describing the
repository as licensed open-source software.

## Appearance customization and Thunar action

Settings now allow background opacity from **0–100%**, independent **Border**
on/off and **Corner radius** from 0–60 logical pixels. The rendered radius is
limited to half the background dimensions. A zero-alpha background can retain
its visible border. The drop-target highlight is temporarily shown during a drag
even when the normal border is disabled. Icons and running indicators retain their
own opacity, and the normal input area remains usable at zero background opacity.

Installation also merges an **Add to Linux Scripts Dock** action into Thunar's
context menu for `*.desktop` launcher files (for example in `/usr/share/applications`).
This integration is specific to Thunar, not a generic change to every desktop's
application menu. Existing **Pin application** in the Dock remains available for
running applications. Reopen/restart Thunar after installation to load the action.
Do not edit Thunar custom actions simultaneously with installation.

The action runs `linux-scripts-dock --pin-desktop /absolute/path/app.desktop`.
It registers/copies the launcher without launching the application. If Dock is
already running, a bounded, current-user local socket request updates that
instance; otherwise the command starts the Dock and adds the launcher. A running
older binary must be restarted after upgrading for this new command to work.

`python3` is required to merge the Thunar action. Other actions and comments are
preserved; changes are atomic and the previous `uca.xml` gets a timestamped
`.dock-backup-*` copy. Reinstalling does not duplicate the action. Use
`bash install.sh --no-context-menu` to skip this integration. Uninstall removes
only the action with the Dock's own unique ID. The original data and unrelated
Thunar actions remain intact. Corrupt XML is reported without overwriting it.

## Color wheel and Whisker Menu

Border color can be selected with a hue/saturation wheel and brightness control,
as well as the existing swatches and HEX field. Drag inside the wheel to choose a
color; use brightness to reach darker colors. Border width is independently
adjustable from 1–8 logical pixels; Border Off still disables the normal outline.
The existing translucent outline alpha is retained. These settings are saved.

For **Add to Linux Scripts Dock** in Whisker's application menu, see the optional
[MX Whisker integration](integrations/whisker/README.md). It patches MX Whisker
2.10.0 and builds a separate `.deb`; the normal Dock installer does not replace
system panel plugins. No launcher files are rewritten for this integration.

## Background color

Select **Border** or **Background** above the color wheel to choose what to edit.
Both colors have independent saved RGB values. The wheel, brightness, swatches
and HEX field apply to the selected target. Background opacity remains a separate
control; 0% keeps the chosen RGB value but makes the background invisible. The
initial background remains #2a303c, preserving the previous appearance.

## Settings sections and status indicators

Settings are split into **Panel**, **Icons**, **Behavior** and **Indicators** tabs.
The tabs and reset footer stay visible while longer content scrolls. The shared
color picker edits the panel's border/background in Panel, and active/running
indicator colors in Indicators. Each value is independently persisted.

Indicator styles are Pill (the previous default), Dot, Line, Square and Hidden.
Only the status mark below the icon changes; active icons do not get larger and
magnification, launch bounce and the fixed background geometry remain unchanged.
The running-window count badge remains available for groups with several windows.

**Restore defaults…** requires confirmation and restores appearance and behavior,
including intelligent hide and indicator settings. Pinned applications, imported
launcher files and their order are retained. Cancel leaves the settings untouched.

## Border opacity and About

**Panel → Border opacity** controls only the border (0–100%, default 38%).
Background opacity remains independent. A drag-target highlight remains visible
when the normal border is hidden. Restore defaults resets border opacity too.

Right-click the Dock and choose **About Dock…** for the runtime version, technology
summary, credits and links to the GitHub project, README, bug reports and author.
Credits: Vladyslav Havryliuk (VladCraft228), developed with assistance from ChatGPT
(OpenAI). External links open in the default browser only when clicked.

**Icons → Icon spacing** sets the gap between icons from 0–32 logical pixels
(default 10). Changes apply immediately and are saved. The dock's base width,
hover positions, reorder slots and external drop marker all use the same value.
Restore defaults returns spacing to 10 without changing pinned applications.

## Starting again and adding installed applications

Installation creates **Linux Scripts Dock** in the applications menu, with a
scalable icon. Search for `Dock` in Whisker to start it after closing it. This is
a separate `.desktop` entry from autostart, under
`$XDG_DATA_HOME/applications/linux-scripts-dock.desktop` (default `~/.local/share`).
Uninstall removes the menu entry and icon as well as the installed binary.

Right-click the Dock and choose **Add application…**. The searchable list shows
visible desktop applications and marks those already pinned. Selecting an entry
pins it without launching it or moving its desktop file. The catalog refreshes
on opening, so newly installed applications can appear without a Dock restart.
CLI-only tools and NoDisplay/desktop-inappropriate entries are not listed.
The Dock's own settings/About/chooser windows are excluded from its task list.
