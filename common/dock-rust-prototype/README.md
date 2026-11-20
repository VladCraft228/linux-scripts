# Prychal Dock

A lightweight, customizable application dock for XFCE/X11. Application
logic, X11 integration, and UI rendering are written in Rust, using x11rb,
tiny-skia, and fontdue. No Qt, GTK, QML, CMake, or C++ application code is needed.

The source directory remains `common/dock-rust-prototype` during this rename.
The executable and application menu name are now **Prychal Dock** (`prychal-dock`).
Configuration, pins and imported launcher paths intentionally retain the previous
`linux-scripts-dock-rust-prototype` directory so existing preferences keep working.
The Qt-based `../dock` remains a separate implementation.

## Install and launch

On the XFCE/X11 development laptop, close the running Rust prototype, then:

```sh
bash install.sh
"$HOME/.local/bin/prychal-dock"
```

Installation builds and runs the existing checks through `build.sh`, atomically
installs the executable and SVG icon, creates **Prychal Dock**
in the application menu, and creates XFCE autostart by default. Existing autostart
entries are preserved, including entries disabled by the user. Use
`bash install.sh --no-autostart` to skip creating one; this does not delete an
existing entry. `--skip-build` installs an already built release binary; use it
only after successfully rebuilding the current source. No sudo is needed.
The installer does not stop/restart the running process. Quit the old Prychal Dock
before launching the updated executable.

Open the dock later from the application menu, or run:

```sh
"$HOME/.local/bin/prychal-dock"
"$HOME/.local/bin/prychal-dock" --settings
"$HOME/.local/bin/prychal-dock" --quit
"$HOME/.local/bin/prychal-dock" --version
```

There is one Prychal Dock instance per X11 screen. A repeated ordinary launch
opens the existing dock's settings; `--settings` starts the dock if necessary and
opens settings, while `--quit` never starts a new dock. X11 selection ownership
provides an atomic claim and releases automatically on process exit or X server
disconnect. Control messages use the same X11 connection without sockets, PID
files or polling. Launch-only flags affect a newly started process; they do not
reconfigure an existing instance.

Installed paths:

- `~/.local/bin/prychal-dock`
- `$XDG_DATA_HOME/applications/prychal-dock.desktop`
- `$XDG_DATA_HOME/icons/hicolor/scalable/apps/prychal-dock.svg`
- `$XDG_CONFIG_HOME/autostart/prychal-dock.desktop`

XDG defaults are `~/.local/share` and `~/.config`. The development executable is
still `target/release/prychal-dock`; existing settings, pins and imported
launchers retain their `linux-scripts-dock-rust-prototype` paths. This release
does not migrate or overwrite the Qt Dock executable, launcher or autostart.
If both editions have autostart enabled, choose one in XFCE Session and Startup.
`xdg-open` opens About links; it is not needed for dock rendering.

To uninstall:

```sh
bash uninstall.sh
```

This requests quit on the current X11 screen, removes this edition's installed
files and autostart, and preserves configuration, pins and imported launchers.
Development builds remain in the repository. Other X11 sessions must be closed
separately if they are running another instance.

Manual validation on the laptop: install, quit/reopen from the application menu,
launch again and confirm that settings open without a second panel, try
`--settings` and `--quit`, and verify autostart at the next XFCE login. Reinstall
with existing settings/autostart and check preservation. Shell syntax and patch
application are checked here; compilation and X11 behavior are validated locally
by the user.

## Scope

- Five initial shortcuts: Terminal, Files, Browser, Editor, App Finder.
- System icon theme from XFCE `xsettings` (`xfconf-query` once at startup), with
  GTK settings.ini and hicolor fallbacks. No GTK library is loaded.
- Desktop entry `Icon` names/paths, theme inheritance via freedesktop-icons,
  cached PNG/SVG rasterization, and `_NET_WM_ICON` or generic fallback icons.
- Running applications from `_NET_CLIENT_LIST`, grouped using desktop IDs,
  StartupWMClass, application IDs and WM_CLASS. Minimized windows remain listed.
- Left-click an inactive application to activate/restore a window; clicking the
  active application minimizes its active window. Different-workspace windows
  request a workspace switch before activation. The WM controls the final action.
- A blue pill marks the active application; a grey dot marks other running apps.
  Non-running initial shortcuts launch `xfce4-terminal`, `thunar`, `firefox`,
  `mousepad`, or `xfce4-appfinder`; missing commands are reported in the terminal.
- Width changes only with the number of visible applications. Mouse wheel scrolls
  the list when applications no longer fit on the screen.
- Hover magnification with upward growth, fixed rounded translucent background
  geometry during hover, and bottom-edge reveal.
- Intelligent hide: the dock remains visible on the desktop, when all windows are
  minimized, and whenever no ordinary window on the current workspace overlaps
  its base panel. Minimized/hidden windows, other workspaces, desktop/panel windows
  and the dock's own windows are excluded. Window frame extents are included.
- Hover, menu interactions and internal/external dragging keep the dock visible.
  Window overlap is recomputed after relevant X11 events, coalesced at 25 ms,
  rather than in a polling loop. `autoHide=false` still means always visible.
- Configurable hide delay (0–3000 ms, default 650) and the same full-travel
  duration for showing/hiding (60–500 ms, default 120). Reversals start from the
  current position. Magnification and neighbor motion retain their own timing.
- After 350 ms over a running application, a window chooser appears above that
  icon. Cards show the application icon, real window title, an active-window border
  and a close button; clicking the card activates that exact window. A Close all
  button is available for groups with multiple windows. Use the wheel if the
  group has more windows than fit on screen.
- Window cards now try real XComposite thumbnails, with XRender downscaling before
  a small image is read back. The existing compositor's redirected frame is tried
  first, then the client window; the dock never changes window redirection.
  Preview updates are capped at four per second for visible cards only. Closing
  the chooser stops captures entirely; unchanged images do not redraw the panel.
- At most 24 small thumbnails are cached in RAM. Named full-size compositor
  pixmaps and temporary XRender resources are freed after every capture. No
  screenshots are written to disk. Captures preserve aspect ratio.
- Minimized/unmapped windows can show only a previously captured thumbnail. A window never captured successfully uses its application icon. X11/GPU
  applications and compositor unredirect policies can also prevent capture.
- Settings → Window previews toggles thumbnails. Turning it off restores app icons
  and disables capture scheduling. It defaults to On for existing configurations.
  XComposite 0.2+, Render 0.10+, and a supported 32-bit ARGB format are required;
  unavailable backends fall back to icons without stopping the dock.
- The chosen app stays magnified while the pointer is over its chooser. A 220 ms
  grace period bridges the gap between icon and chooser; moving to another icon
  switches the hover target. Menus and dragging dismiss the chooser. It never
  grabs focus, keyboard or pointer. Closing requests go through the WM and allow
  normal unsaved-document prompts; the dock never force-kills the process.
- Non-running shortcuts retain input-transparent application-name labels. A normal
  icon click still activates/minimizes as before; hover to choose a specific window.
- Right-click an icon for Launch new window, Pin/Unpin, and Quit Dock.
  Right-click empty dock space for Settings and Quit Dock. Escape/outside click closes the menu;
  arrow keys and Enter navigate it. The selected icon stays magnified while the
  menu is open, and auto-hide is paused.
- Drag `.desktop` files from Thunar onto the dock to pin them without launching.
  XDND copy semantics preserve source files. Hovering over the bottom reveal strip
  opens the dock during an external drag; a blue highlight marks the drop target.
  Drop up to 32 local files, at most 1 MiB each. URI payloads are bounded to 64 KiB.
  All files must be valid Application entries with supported Exec commands.
  Other files, remote URLs, oversized/incremental transfers, or invalid launchers
  are rejected with a terminal message. A rejected batch does not change pins.
- Installed launchers are reused when possible. Standalone launchers are copied
  into `${XDG_DATA_HOME:-~/.local/share}/linux-scripts-dock-rust-prototype/launchers`;
  their original files are never moved or deleted. Copies are loaded on restart.
  Identical launcher content/commands are reused rather than pinned twice.
- Drag icons horizontally to reorder them, including placing a running icon between pins. A 6 px movement threshold separates
  dragging from a click; ordinary clicks activate on button release. Neighboring
  icons slide smoothly, and the dragged icon stays above them. Pinned order is saved
  once on release, not for every mouse movement. Unpinned running order is session-only
  and survives focus/notification updates while the dock remains running. A running icon can temporarily occupy a slot between pins. Closing the app
  removes that slot and leaves the pins in their saved relative order. Escape,
  right-click during drag, or release outside the dock restores the starting order.
  Closing the dragged app also cancels safely. Auto-hide is paused during a drag.
  For an overflowing list, the mouse wheel scrolls while holding the dragged icon.
- The icon row sits 6 px higher; the background geometry and animation timings
  are unchanged.
- Settings now open as a separate managed window from Right-click → Settings,
  with native XFWM decorations, resizing, a sidebar, sliders and toggle switches.
  Panel controls background/border colors, independent opacities, width and corner
  radius. Icons controls size/spacing/magnification. Behavior contains intelligent
  hide, delay, animation and previews. Indicators has separate colors and visual
  Pill / Dot / Line / Square / Hidden choices.
- Color controls open a dedicated view with a hue/saturation wheel, brightness
  slider, presets, sample and read-only HEX value. Back returns to its section.
  Sliders and color drags preview immediately and save on release; wheel changes
  and toggles save immediately. Escape cancels a drag, goes Back from a color page,
  or closes the settings window. Tab / Shift+Tab and arrow keys operate controls.
- The dock remains visible while settings are open. Closing/minimizing the window
  returns it to its normal visibility policy. Settings windows are excluded from
  the dock's application list. No keyboard grab is used by this settings window.
- Settings use the existing
  `${XDG_CONFIG_HOME:-~/.config}/linux-scripts-dock-rust-prototype/settings.json`.
  Reset settings preserves pins and their order; existing settings are retained.
  Border opacity remains independent from background opacity, including at 0%.
- Background height follows the chosen base icon size, never animated magnification;
  the bottom edge stays fixed. The raised icon baseline is retained. The icon size
  limit leaves headroom for maximum magnification in the 160 px canvas.
- Pins and their chosen order survive restarts. The prototype alone uses
  `${XDG_CONFIG_HOME:-~/.config}/linux-scripts-dock-rust-prototype/pins.json`.
  Missing configuration starts with the five initial shortcuts; an empty list
  means no pins. Pinning/unpinning writes atomically; unreadable/malformed
  configuration is reported and preserved. It never edits the Qt dock's settings.
- A matching desktop entry with a supported Exec command is required for pinning
  additional apps. Entries without a launcher show a disabled menu action.
- Desktop Exec commands launch as argument arrays without implicit shell
  evaluation. File/URL placeholders are omitted for a new empty launch; %c, %k,
  %i and %% are supported. Terminal=true uses xfce4-terminal. Unsupported field
  codes disable the launcher. DBus-only launchers are not supported yet.
- Ctrl+C in the launching terminal also quits.
- Event-driven window updates (25 ms coalescing), cached theme images, and an
  event-driven idle. No perpetual pointer polling or rendering loop.

Still omitted: keyboard navigation in the hover chooser,
multi-monitor placement, display hotplug, menu-grab recovery,
singleton locking, and Wayland. Direct icon clicks choose the first window when activating, or the active
window when minimizing; the hover chooser selects an exact window.
Steam/Wine and apps with unusual identifiers may still need matching rules.
SVGs are rasterized once at 96 px; SVG text and XPM files are not supported.
Theme changes require restarting the prototype. The desktop catalog is also
refreshed when launchers are imported.
The window is centered on the X11 screen, not necessarily the active monitor.
The edge strip can be covered by newly raised windows; production-grade stacking
and grab recovery remain outside this experiment.

CPU rendering is a design being tested, not a promise of lower CPU use than Qt's
GPU scene graph. Xorg/compositor and GPU costs are not attributed to this process.

## Build and try

Requires an installed Rust toolchain (Rust 1.85+; validated with 1.100), Cargo,
a linker, an X11 session, and the XFCE compositor. Menus use a system TrueType
font through fontdue; fontconfig or fonts-dejavu-core normally supplies it on XFCE. The dependencies are Rust
crates downloaded by Cargo. Existing dock development dependencies suffice.

```sh
cd ~/Projects/linux-scripts/common/dock-rust-prototype
bash build.sh
./target/release/prychal-dock
```

Close the production dock from its Quit menu first so the two docks do not
intercept each other's pointer events. No installation is necessary. Nothing
is added to autostart. Optional arguments:

```sh
./target/release/prychal-dock --no-autohide
./target/release/prychal-dock --duration 15
```

If the compositor is disabled, the application stops with a message instead of
showing an opaque rectangle. Enable compositing in XFCE Window Manager Tweaks.

## Repeatable resource measurements

`bench.py` uses Python's standard library and Linux `/proc`. Python is only the
measurement tool, not part of the dock's runtime. Sweep mode additionally uses
the system `libX11.so.6` and `libXtst.so.6` via ctypes (normally present on XFCE).
It moves the pointer but never clicks. Press Ctrl+C to abort the measurement. The pointer is restored on normal exit or Ctrl+C.
Physical mouse movement does not automatically abort the workload.

Each run writes CSV samples every approximately 250 ms and a JSON summary:

- CPU time and CPU percentage, where **100% means one fully busy logical core**.
  Process CPU time includes its threads.
- Mean PSS: shared memory divided among processes sharing each mapping.
- Mean RSS, maximum sampled RSS, and mean USS (private resident memory).
- Mean proportional swapped memory, thread count per sample, and context
  switches of each process's main thread.

A sampled RSS maximum is not the lifetime peak. PSS may vary with other running
programs. Only the chosen process is measured by default; launched applications
and child processes are not included. Permissions must allow reading its
`/proc/PID/smaps_rollup`. PID reuse and process exit abort the run.
No FPS, GPU memory, actual display presentation times, or energy consumption are
measured. A 60 Hz movement schedule is not a measured 60 FPS result.

For a useful first comparison, use the same laptop, compositor, display refresh
rate, power mode, five icons, base icon size 48, gap 10, magnification 55%, and no
other CPU-heavy work. Keep the production dock visible on an unobstructed desktop;
run the prototype with `--no-autohide` so both idle measurements are visible.
Do not launch applications during a measurement. Use release builds for both.

### 1. Measure the prototype

Close the production dock first. Run all commands in the same terminal:

```sh
cd ~/Projects/linux-scripts/common/dock-rust-prototype
./target/release/prychal-dock --no-autohide &
rust_pid=$!

python3 bench.py --pid "$rust_pid" --label rust-idle --seconds 30
python3 bench.py --pid "$rust_pid" --label rust-sweep --sweep --warmup 8 --seconds 30
```

During the sweep's eight-second warmup, put the pointer over the **middle icon**.
Then leave it alone. The script sweeps 232 pixels horizontally around that point
with a two-second full cycle. Keep the same sweep width for both docks.

Afterwards stop only the prototype you started:

```sh
kill "$rust_pid"
wait "$rust_pid" 2>/dev/null || true
```

### 2. Measure the installed Rust + Qt dock

Make sure no other production dock instance is running; otherwise the new process
will exit due to the instance lock and cannot be measured. Keep the same terminal:

```sh
~/.local/bin/linux-scripts-dock &
qt_pid=$!

python3 bench.py --pid "$qt_pid" --label qt-idle --seconds 30 \
  --compare results/rust-idle.json
python3 bench.py --pid "$qt_pid" --label qt-sweep --sweep --warmup 8 --seconds 30 \
  --compare results/rust-sweep.json
```

Again put the pointer over the middle icon during the sweep warmup. In idle mode,
leave it outside the dock. The Qt dock is left running after measurement.
Repeat each pair at least three times with unique labels, alternating order.
The Rust prototype lacks most production features, so a lower number does not
prove that Rust UI or this renderer is the better overall architecture.

Optionally repeat with `--extra-pid PID` for Xorg and/or the compositor. All chosen
processes are then summed in the report; use the same process scope in both runs.
This includes unrelated desktop activity and may need additional read permissions.
Check visual smoothness, tearing, response, and monitor behavior separately.

## Validation

`build.sh` builds a locked release and runs the Rust tests. Unit tests cover desktop
entry/Exec parsing, SVG rendering, EWMH icon decoding, hover falloff, and fixed background
geometry. A separate integration test under disposable Xvfb checks named theme icon
lookup, grouping multiple windows, minimize/restore requests, workspace switching,
and removing closed applications. It acts as a WM/compositor-selection fixture and
is ignored during a normal `cargo test`. Do not enable it on your desktop session.
The latest interaction and drag-and-drop updates are provided for local compilation and testing on XFCE; it
was reviewed and formatted here without building or running the GUI.

```sh
cargo test --locked
cargo clippy --all-targets -- -D warnings
```

For a manual check: launch Files from the dock, click its active icon to minimize,
click again to restore, open a second Files window and confirm a single icon,
then open another application, pin it from its right-click menu, close its windows,
quit the dock from its menu, and restart it to check the saved pin. Unpin and
restart again to check removal. Click outside the menu and press Escape to check
that mouse/keyboard input returns to other applications. Drag a pinned icon past
its neighbors and restart the dock to check the new order. Repeat with Escape
and with a drop outside the dock to check cancellation and unchanged saved order.
Confirm that a normal click still minimizes/restores without launching a duplicate.
Drag one launcher from `/usr/share/applications` and one standalone `.desktop` from
Thunar onto the dock, repeat a drop to check deduplication, and restart to confirm
persistence. Check that the originals remain in place and that no application
launches until clicked. Open Settings, change icon size/spacing/magnification and
opacity, hide delay, animation duration, and intelligent hide, then close the
menu and restart to verify persistence.
Set opacity to zero to check fully transparent background; use Reset settings to
return to defaults and confirm that pins and their order remain intact. Try an ordinary text file to check rejection. Check that running applications on another workspace can be activated.
Hover between icons to check that the background height stays fixed.

To remove the experiment, close it and delete this directory.
The production dock remains intact.

Behavior checks for this update: enable Intelligent hide, maximize a window and
move off the dock to check delayed hiding; restore or move the window away to
check immediate reveal. Minimize all windows, use Show Desktop, and switch
workspaces to check visibility. Hover adjacent icons to verify label placement
and unchanged magnification. Check equal show/hide timing at both 60 and 500 ms.
The latest behavior update was formatted and its patch checked here; compilation
and real XFCE behavior are being validated on the development laptop.

Window chooser checks: open two windows of one application, hover its dock icon,
move into the chooser and confirm that only the original icon stays magnified.
Activate each card, close one window, then check Close all on the remaining group.
Check one-window groups too, and open enough windows to exercise wheel scrolling.
Move off the chooser and then directly between dock icons to check dismissal.
The latest chooser update is provided for compilation and XFCE testing on the
user's laptop; only source formatting and patch application were checked here.

Thumbnail checks for this update: hover a mapped Thunar window or browser and
confirm that its actual content appears. Change its content while the chooser
is open to check low-rate updates. Minimize after a successful capture and check
that the last captured thumbnail remains visible; a never-captured minimized
window may still use its icon.
Toggle Window previews Off/On and verify persistence and unchanged close/select
controls. Terminal diagnostics report backend availability and one-off capture
failures. This update is prepared for compilation/testing on the user's XFCE
session; no local build or GUI capture was run here.

Appearance checks for this update: use each settings section and Back, change
all four color targets with the wheel and brightness slider, and restart to
verify persistence. Drag a color and press Escape before releasing to verify
cancellation. Set background opacity to zero and keep the border visible; toggle
border off, adjust its width/opacity, and test square/rounded corners. Cycle all
indicator styles with a running application. Reset settings must preserve pins.
This update was formatted and its patch checked without a local build; compile
and validate the actual UI on the XFCE development laptop.

Standalone settings checks: Right-click → Settings should open exactly one
ordinary decorated window; repeat to raise the same window. Resize, move, minimize,
reopen, and close via both Done and the titlebar. Change every sidebar section,
drag sliders, use the color wheel/brightness/presets, and confirm immediate dock
updates plus persistence after restart. Escape during a drag must restore the
previous saved state. Confirm that settings never appear as another dock item.
This UI update is provided for local compilation and testing on the user's XFCE
session; formatting and patch application are checked here without a build.

Settings placement now requests the current X11 screen center whenever opened,
including when raising an already open window. Explicit position hints and a WM
move request prevent smart placement from choosing a screen corner. XFWM frame
extents are included once available; subsequent manual moves/resizes remain free.
The updated source comparison is in [the parity audit](docs/parity-audit.md).

Launch feedback: actual launches bounce only the icon in 400 ms cycles with
decaying amplitude, independent of the panel and magnification. A new window in
the corresponding group ends the animation smoothly; a 12-second timeout bounds
unconfirmed launches. Activating/minimizing an existing window does not bounce.
Explicit Launch new window uses the group's pre-launch window IDs as its baseline.
At extreme icon/magnification settings bounce is clipped to available headroom.
Icons show a count badge for two or more windows (capped at 99+).

Spawn failures, early unsuccessful launcher exits and unconfirmed-launch timeouts
appear for six seconds above the dock without stealing focus, and are also logged
to stderr. A timeout is a missing window confirmation, not proof that launch failed;
launcher/window matching can still be imperfect for games and helper processes.
Import/pin errors still use terminal diagnostics.

Validate locally: launch a closed app, activate an existing app (no bounce), open
a second window (count badge), and confirm background geometry remains fixed.
Check an invalid executable launcher to see a visible error, and a launcher without
a matching new window to verify the bounded timeout. This update was formatted
and its patch checked here; compile and test on the XFCE development laptop.

Control arrows are drawn as vector paths, independent of font glyph coverage.
The managed Dock Settings window now appears as a temporary, unpinnable dock item,
including while minimized; click to minimize/restore and close to remove the item.
Other internal overlays remain excluded, and the settings event mask is preserved.
Application context menus offer Close window (active window when it belongs to the
group; otherwise the first tracked window), plus Close all windows for groups.
These send ordinary close requests, allowing applications to prompt for unsaved work.

Validate Back arrows, open/minimize/restore/close Settings, and close single/multiple
application windows from the context menu. Source formatting and patch application
were checked here; compile and validate the GUI on the development laptop.

About Dock is available from the dock context menu and the native window's About
sidebar page. It includes the Cargo package version, author Vladyslav Havryliuk
(VladCraft228), assistance credit for ChatGPT (OpenAI), the GitHub repository,
documentation, issue creation and author profile. Fixed HTTPS links open with
`xdg-open` without blocking the UI or invoking a shell. Opener failures are logged
to stderr. About shares the existing settings window and does not create another
dock item; settings and pins are preserved. Reset settings is hidden on this page.
Validate both entry points and the four link buttons on the XFCE development laptop.

## Installed application picker

Right-click the dock and choose **Add application...**, or use **Add app** in the
native window sidebar. Search installed desktop entries by display name or ID;
click a row or use Up/Down and Enter to pin it. Mouse-wheel scrolling and Clear
are supported. Pinning saves the existing desktop ID and never launches the app.
Already pinned entries offer an Unpin button, including identical privately imported
launchers, and cannot be added twice. New pins append to the current order.
Unpin removes only the saved pin; a running application's icon remains until its
windows close. Installed applications and imported launcher files are preserved.
The right-hand Add/Unpin button is independent from row selection, so clicking a
pinned row does not accidentally remove it.

The catalog is refreshed when opening the picker and revalidated before pinning.
XDG application directories and per-user overrides are respected; Hidden,
NoDisplay, OnlyShowIn, NotShowIn and TryExec filter menu visibility. Internal
imported launchers remain available to the dock but are not repeated in this list.
Only entries supported by the existing Exec parser are offered; DBus-only desktop
entries still need separate launch support. Display names use LC_ALL/LC_MESSAGES/
LANG locale and language variants when available.

Search negotiates XKB awareness on its X11 connection and uses each key's native
group/width mapping; it does not assume that the active group is English or that
each group has exactly two levels. Group and modifier state come from the
focused-window event, avoiding a later state query racing queued keystrokes.
This enables normal EN/UK switching within the picker without changing the
system layout, per-window layout policy or OSD. No polling or new runtime
libraries are introduced; the existing x11rb dependency enables its xkb feature.
Search handles Latin, Unicode and legacy Cyrillic keysyms (including Ukrainian),
Backspace and Ctrl+U. It uses ordinary focused-window events, without a keyboard
grab. Compose/dead-key sequences, clipboard paste and input-method preedit are
not implemented. Pin errors are shown in the picker; no new dependencies or
continuous animation loop are introduced.

Validate on XFCE: search a name/ID, add an unpinned app, confirm the icon appears
without launching, reopen/restart to verify persistence, and ensure Pinned rows
do not duplicate. Test wheel scrolling, keyboard input in EN/UK, and returning to
settings/About. Patch application is checked here; build and GUI validation are
performed on the user's development laptop.

Keyboard validation: focus the picker search, switch EN/UK using your configured
shortcut, type in each layout, then check Shift, Caps Lock, Backspace and Ctrl+U.
Also switch to another application and back to check the existing per-window
layout policy. Compile and validate on the XFCE laptop; no local build was run.

The Prychal Dock's SVG app icon is also embedded in the executable and published
as `_NET_WM_ICON` on the dock and managed settings window. Development launches,
XFWM titlebars/task switchers, the dock's own settings item, and the installed
application-menu launcher share the same brand; other apps retain their system
icon theme. Embedded rasterization is cached once.

The hover chooser no longer forces a 320 px minimum width: a single 212 px card
uses a 236 px popup, preserving thumbnail size while removing the wide side
margins. Header spacing and vertical padding are tighter (206 px popup height).
Multi-window scrolling, close targets and preview timing retain their behavior.
Validate one/multiple windows, edge placement, close buttons, and the Settings
titlebar icon after restarting the rebuilt binary. No local GUI build was run.

Preview control polish: multi-window choosers reserve an extra 10 px header gap
between Close all and the cards, while single-window choosers retain their compact
height. Close all has a 12 px right inset and centered text. Each close cross is a
vector path centered in its 21 px circle, independent of font glyph metrics; its
hit area matches that circle. Drawing, hit testing, popup placement and footer
position share the updated geometry. Check one/multiple windows and close controls
on the XFCE laptop after rebuilding; no local GUI build was run.

## Launcher counters and attention

- Blue, upper-right: number of open windows (two or more).
- Red, upper-left: application-supplied launcher counter, capped visually at 99+.
- Amber dot: the app/window requests attention, when no numeric counter is visible.

The X11 backend reads `_NET_WM_STATE_DEMANDS_ATTENTION` and the ICCCM UrgencyHint
on existing window-property events. Focused windows do not display their own
stale X11 urgency. Launcher counters/urgency use session-bus
`com.canonical.Unity.LauncherEntry.Update` signals with `count`, `count-visible`
and `urgent`. Counts are not inferred from titles or notification history.
Activating an app does not invent a read/unread reset: the publisher clears its
counter. The number's meaning is decided by the app (unread messages, pending
items, etc.). Not every application publishes this protocol, and no historical
count is available until an app sends its state.

This adds the pure-Rust `zbus` dependency and a lockfile update; the first build
will download/compile its dependencies. No libdbus development package, Python
helper, notification-service replacement or permanent animation loop is used.
A blocking D-Bus listener wakes the X11 poll loop through a nonblocking socket only
on state changes. State is bounded to 1024 sender/app records; duplicate updates
do not redraw. Sender disconnect removes its badges, and a disconnected bus clears
cached state and retries every 15 seconds while X11 attention continues working.
The bridge claims `com.canonical.Unity` only if free, without replacing/queuing
behind another dock, so cooperating apps can publish initial state. It never
claims `org.freedesktop.Notifications` or subscribes to notification contents.

Desktop IDs are matched to existing application groups, including exact-copy
imported launchers. Counts from multiple publishers are not summed: the latest
count update is used. Other apps retain their system icons.

Manual checks on XFCE: receive an update in a supporting messenger, verify the
red number/amber dot, read the messages, and exit the app to check clearing. Blue
window counts and launcher bounce should remain independent. A one-shot
`dbus-send` publisher exits immediately, so its badge is intentionally removed.
For a persistent manual publisher, use an ID shown in Add application:

```sh
cargo run --release --locked --example launcher-badge-demo -- YOUR_APP.desktop 5
# Or test an attention dot:
cargo run --release --locked --example launcher-badge-demo -- YOUR_APP.desktop urgent
```

Keep that terminal open; Enter clears the badge and exits. The app must already
be pinned/running in the dock. This is an optional manual test tool, not a runtime
dependency. Dependency metadata and patch application are checked here; the user
compiles and validates the actual UI on the development laptop.

Settings → Notifications controls launcher counters and attention dots separately.
Each has its own color wheel and size slider (counter 14–24 px, dot 6–14 px).
Changes preview immediately and persist; existing settings receive the previous
visual defaults automatically. Reset settings restores these defaults without
changing pins or unread state in the application. Hiding a visual indicator does
not clear the app's underlying state; re-enabling shows the latest received state.
Blue open-window counts are unaffected. Large counters stack above the icon when
needed to avoid overlapping the blue count, without changing the dock background.
Validate both toggles, color/size changes, Escape during slider/color dragging,
persistence and reset using a supporting app or the persistent badge demo.
No local application build or GUI tests were run.

## Rename compatibility

The installer migrates the old Rust Edition autostart entry to
`prychal-dock.desktop`, preserving custom arguments and enabled/disabled flags.
If a Prychal entry already exists it takes priority. The retired entry is saved
outside autostart in the existing configuration's `migration-backups` directory.
Old Rust Edition app-menu/icon files are removed, and the former
`~/.local/bin/linux-scripts-dock-rust` command becomes a symlink to `prychal-dock`.
Settings, pins, imported desktop files and stable X11 IPC names are preserved.
Close the old running process before installation to load the new name and binary.
Only this edition is migrated; the original Qt Dock is not modified.

Validate the Prychal launcher, About/version, restart persistence and existing
autostart flags on the laptop. Source syntax, shell syntax, dependency metadata
and patch application are checked here; no local build or GUI test was run.

## File drag hover activation

Keep holding a file from Thunar over a running app's icon for about 650 ms. The
dock activates/restores that app's most recently focused surviving window (or its
first tracked window if no focus history is available). Keep the mouse button held,
move into that window, and drop in the app's own upload/attachment area. The app
and file manager handle the actual transfer; the dock does not forward the file,
launch commands for it, or claim a successful file operation. A not-yet-running
launcher is not started by file hovering.

External hover tracks standard XDND root positions while the source owns the mouse;
no pointer grab or periodic pointer polling is added. Activation occurs once per
continuous hover, restarts its delay when moving to another icon, and is cancelled
when leaving, dropping, timing out or when the target closes. The existing chooser
is hidden during a file drag so it cannot intercept the file. Dock reveal and icon
magnification remain available, and activation never uses the minimize-on-click path.

Direct drops onto the dock still accept only local `.desktop` launchers for pinning.
For ordinary files, keep dragging into the restored application window; dropping
them onto the panel itself is rejected and leaves them at the source.

Validate on XFCE: swap two unpinned apps, change focus, and check the order stays;
then cancel a reorder with Escape/drop outside and check the starting order returns.
Check pinned reordering and persistence too. Drag a harmless file over a minimized
browser/messenger icon, pause until restored, and continue into its attachment area.
Check that a quick pass over icons does not activate them and holding still does
not repeatedly activate/minimize. Verify `.desktop` pinning still works. Source and
patch checks are performed here; compilation and live drag behavior are validated
on the user's laptop.
