# Prychal Dock parity audit

Date: 2026-11-16. Compared the current working copies used for this development
thread, not a newly fetched GitHub revision. No build or GUI tests were run for
this audit; the user validates changes on XFCE/X11.

Original: `common/dock` (Rust core + CXX-Qt adapter + Qt Quick/QML).
Prychal Dock: `common/dock-rust-prototype` (Rust X11/rendering/UI).

## Present in both editions

System icon themes; pinned/running application grouping; minimize/restore;
persistent pinning and drag reordering; `.desktop` file import with private
copies/deduplication; active/running indicators; fixed background geometry during
magnification; intelligent hide; configurable hide delay and symmetric slide
speed; per-window activation/close and grouped close; appearance color controls;
separate settings UI; reset without deleting pins.

These are functional counterparts, not pixel-identical implementations.

## Missing or incomplete compared with the original

| Feature | Original implementation | Prychal Dock status |
| --- | --- | --- |
| Launch bounce | `qml/Main.qml` launchBounce/bounceSettle, core pending-launch state and 12 s timeout | Implemented for local validation: 400 ms decaying cycles, new-window detection, 12 s timeout and smooth settling |
| Add application picker | `openApplicationChooser`, searchable installed-app list, `available_applications` | Implemented for local validation: native searchable installed-app page, persistent pinning and duplicate checks |
| About and credits | About window, version, author/ChatGPT credit, repository/docs/issues links | Implemented as a native About page with context-menu entry, package version, credits and external links |
| Installation and removal | `install.sh` / `uninstall.sh`, installed binary, app-menu launcher and SVG icon | Implemented for local validation with distinct Prychal Dock filenames; settings preserved |
| Autostart | Installer creates XFCE autostart and preserves existing entry | Implemented: created by default, optional opt-out, existing entry preserved |
| Single instance | `src/main.cpp` QLockFile | Implemented: atomic per-X11-screen selection ownership; repeated launch raises settings |
| External pin command and IPC | `--pin-desktop`, `src/launcheripc.cpp` local socket forwarding | Partial: X11 IPC supports settings/quit; external pin command remains missing |
| Thunar custom action | Installer-managed action using the external pin command | Missing; native file drop already works |
| Bottom offset | `bottomOffset` preference, slider, CLI option and geometry integration | Missing: vertical placement is fixed |
| Multi-window icon badge | `tile.runningCount > 1` count badge in QML | Implemented for local validation: icon count from two windows, capped at 99+ |
| Rich application context menu | Window-title list, close active/all, direct hide toggle, Add application and About | Partial: launch/pin/settings/quit exist; window activation/closing is available in the hover chooser |
| Drop insertion position | QML insertion marker and `pinDesktopFiles(urls, dropPosition)` | Partial: drop imports append new pins; reordering afterwards works |
| Copy/link drop actions | QML accepts a copy or link offered by the source | Partial: native XDND receiver currently accepts copy semantics only |
| Typed HEX color input | Editable validated `TextInput` / `setPickerHex` | Partial: wheel/brightness/presets exist; HEX is read-only |
| Reset confirmation | Confirm/Cancel stage before resetting settings | Missing: native Reset settings applies immediately |
| Size/radius ranges | Original icons 24–80 px and radius 0–60 px | Partial: Prychal Dock icons 24–64 and radius 0–40, with a fixed 160 px dock canvas; enlarge the canvas/headroom before raising limits |
| Monitor/resolution changes | Qt `screenChanged` and QScreen geometry signals reposition the dock | Partial: Prychal Dock dock uses X11 screen geometry captured at startup; settings centering now queries current root geometry on opening |
| HiDPI and image formats | Qt logical pixels/device scaling and QIcon loaders | Partial: custom renderer currently uses physical pixels and cached PNG/SVG images; XPM and equivalent scaling behavior are not implemented |
| Pointer cursors | QML uses PointingHandCursor on interactive targets | Partial: custom surfaces mostly inherit the normal cursor |
| Desktop entry launching | GIO DesktopAppInfo/AppLaunchContext | Partial: Prychal Dock parses a supported Exec subset; DBus-only launchers and localized display-name handling need parity work |
| User-visible launch/import errors | QML `root.message` with a message timer | Partial: launch errors and unconfirmed-launch timeouts now appear above the dock; import/pin errors still use terminal diagnostics |

Sources for this table: [original UI](../../dock/qml/Main.qml),
[original model](../../dock/rust/src/model.rs),
[original startup](../../dock/src/main.cpp),
[original runtime](../../dock/src/runtime.cpp),
[original installer](../../dock/install.sh), and the Prychal Dock source modules.

## Prychal Dock additions

The current original's preview cards use application icons (`modelData.iconUrl`).
Prychal Dock now has XComposite/XRender thumbnails at up to four updates per
second while its chooser is open, small cached frames, and icon fallback.
The Cached label was removed at the user's request. Prychal Dock can also scroll
through more hover cards, while the original limits its hover preview to four
and exposes further windows through the menu.

## Not regressions to attribute to this migration

- Neither edition implements Plasma Wayland window control yet.
- The optional Whisker-menu source patch was explicitly deferred by the user;
  it is not a required original feature to reinstate now.
- Steam/Wine matching was already imperfect in the original. Do not claim the
  Qt original solved every game launcher case; collect concrete mismatches.
- Appearance defaults differ slightly between editions. Existing user choices
  should be preserved rather than silently overwritten to force visual parity.

## Suggested remaining batches

1. Validate the added launch state/bounded bounce, multi-window badges and visible launch messages on XFCE.
2. Bottom offset, remaining menu parity and drop position.
3. Validate the new single-instance settings/quit IPC and installer/uninstaller,
   launcher, icon and default autostart on XFCE. External pin IPC/Thunar remain
   separate follow-up work. Use distinct paths while both editions coexist.
4. Monitor/hotplug/HiDPI behavior, cursor polish, editable HEX,
   reset confirmation and remaining desktop-entry compatibility.

The settings-centering fix included with this audit requests explicit X11
placement on each opening and uses frame extents when XFWM supplies them. It does
not continuously recenter the window while the user moves or resizes it.

Other-application popup-grab recovery was removed from the required Prychal Dock
plan at the user's request: no failure has been reproduced in the native edition.
Revisit only if a concrete regression is observed.
