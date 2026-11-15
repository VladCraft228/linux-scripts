# Rust migration verification

## Automated

- `cargo test --manifest-path rust/Cargo.toml --locked`: matching/ambiguity,
  grouping and pin persistence in the model, minimize/restore decisions, explicit
  window validation, close-all targets, reordering, previews, setting constraints,
  launch completion on a new window, launch timeout, rejected URLs and transactional
  batch validation, import deduplication and launch after source removal.
- `ctest --test-dir build --output-on-failure`: Rust core plus the unchanged QML
  interface exercised with a model fixture. Checks cover drag reorder, cards,
  close action, deferred URI drops, intelligent-hide state, magnification,
  background geometry, bounce bounds and the settings slider.
- `xvfb-run -a build/dock-x11-grab-test`: real Rust/Qt/X11 integration, pointer
  grab recovery, overlap and client movement, minimized/iconic state, Show Desktop
  and workspace changes. Run only on a dedicated test display.
- `xvfb-run -a bash tests/smoke-x11.sh`: actual application startup with Openbox,
  dock type, no QML errors, single instance and explicit Wayland rejection.

QML fixtures cannot prove native Thunar drag/drop or compositor behavior.
The X11 regression uses a test QSettings identity; application smoke tests use
isolated XDG configuration/data/runtime directories.

## Manual XFCE checklist before replacing a working C++ installation

1. Back up the existing configuration and imported launchers. Quit the old dock.
2. Start the Rust executable from `build/` before installing it. Confirm pinned
   icons/order, appearance values and the system icon theme are preserved.
3. Launch native applications, switch/minimize/restore, open multiple windows,
   close one/all through cards, and check the active indicator. Confirm known
   Steam game matching limitations have not been misrepresented as fixed.
4. Exercise icon hover at both ends, preview selection, reorder and desktop-file
   import from Thunar. Cancel a drop and reject mixed invalid files.
5. Check bounce on launch and immediate launch failure. No bounce when merely
   activating a running window; no indefinite bounce after a missing window.
6. Move/resize/maximize a window across the normal footprint. Check intelligent
   hide on empty workspaces and Show Desktop. Check reveal while a menu holds
   a pointer grab. Both animation directions use the selected duration.
7. Change every setting and restart. Confirm persistence and settings window
   closure without exiting the dock.
8. Install, log out/in and confirm exactly one autostart instance. Disable its
   checkbox, reinstall and verify the disabled entry remains preserved. Uninstall
   must remove the binary and autostart entry while preserving user data.
9. Measure idle and animated CPU/RAM on the target XFCE session; no performance
   improvement is assumed solely because the engine is Rust.

## Customization and file-manager integration

- `dock-rust-core` covers opacity 0 and independent border/radius persistence.
- The QML fixture checks transparent background, zero radius and border off.
- `dock-thunar-action` verifies preserving unrelated actions/comments, idempotence,
  backups, safe removal and malformed-XML refusal.
- `dock-launcher-ipc` uses connected local socket pairs for protocol tests and
  also tests a real listener when local socket binding is permitted.
- Native startup smoke now imports a launcher on cold start without executing it.
  It also sends a second launcher to the existing instance when the environment
  supports local sockets; a skip is printed otherwise.

On XFCE, restart the updated Dock and Thunar, right-click an actual `.desktop`
file, choose **Add to Linux Scripts Dock**, and verify one persistent icon, no
application launch and no source movement. Repeat while Dock is closed. Verify
other Thunar actions remain present. This action is for Thunar launcher files;
Whisker Menu and xfdesktop use separate context-menu systems.
