# Architecture: Rust engine and Qt Quick presentation

## Boundary

Rust owns application identity matching, grouping, pin order, desktop-entry
import/launch, launch deadlines and all native X11 integration. The existing
`qml/Main.qml` is unchanged by the migration. The model's public role names and
invokables remain compatible with that QML.

- `rust/src/model.rs`: policy, GIO application catalog and launching, bounded
  imports, monotonic pending-launch records and settings validation.
- `rust/src/x11.rs`: an x11rb connection, EWMH/ICCCM events/actions, input shaping,
  native pointer presence, window icons and overlap detection.
- `rust/src/engine.rs`: synchronous command dispatch and changed-state snapshots.
- `rust/src/bridge.rs`: the CXX-Qt QObject entry point.
- `src/runtime.*`: Qt event-loop adapters, compatible QSettings storage, system
  icon drawing and transfer of changed snapshots to the two list adapters.
- `src/applicationmodel.*`, `src/platform/x11/windowmodel.*`: Qt role/property
  adapters. They contain no grouping or X11 protocol implementation.
- `src/main.cpp`: Qt process startup, theme, surface placement and image providers.

Qt remains a C++ library. CXX-Qt generates C++ bridge code, and the Qt-specific
presentation adapter remains C++. This does not make Qt or its dependencies
memory-safe; the migrated application policy and X11 protocol handling use Rust.
There is no handwritten unsafe Rust in the engine; the binding declaration marks
the external Qt type as required by CXX. GIO and Qt bindings use FFI internally.

## Dispatch and scheduling

All engine calls run synchronously on the GUI thread through a CXX-Qt method. A
compact JSON message is the in-process serialization boundary, not a socket or
subprocess. No messages are queued into Rust while a mutable Rust call is active.
The Rust engine returns changed task/group/configuration snapshots and dirty icon
data; unchanged model snapshots are not transmitted back to Qt.

QSocketNotifier watches the Rust X11 connection fd. Event bursts schedule one
25 ms refresh; there is no idle window polling. X11 calls can buffer protocol
events while reading replies, so dispatch drains those events before returning.
The 80 ms pointer fallback runs only while hidden or recovering from a popup
pointer grab. A single-shot launch timer targets the next Rust monotonic deadline.
Qt timers implement scheduling, not policy. A broken X11 connection requires a
restart; the application does not silently reconnect with stale window IDs.

## Compatibility

Qt's existing QSettings serializer is retained deliberately: it understands the
original QStringList and QVariantMap encoding. Rust receives a normalized config
snapshot and returns updates, which Qt persists with the same keys and identity.
Icons continue through QIcon and the system theme. Native ARGB icon payloads are
bounds-checked in Rust and copied into owned QImage storage by the adapter.
Configuration write errors are surfaced; Rust memory state may then differ from
persisted state until restart. Imported copies can remain orphaned after a failed
configuration write. Do not claim disk/config persistence after a reported error.

The normal resting background rectangle is the only intelligent-hide footprint;
magnification and hide translations cannot move it. Active current-workspace
windows are considered regardless of stacking occlusion. Hidden, iconic,
skip-taskbar utility, desktop and panel windows are excluded, as before.

## Build

CMake builds the Rust static library through Cargo `--locked --features qt`, then
links the Qt executable using mold. The CXX-Qt 0.8.1 export layout is pinned by
Cargo.lock; changing CXX-Qt requires rechecking the CMake include/export paths.
`cargo test` without `qt` tests policy independently from the Qt development SDK.
The GUI still requires Qt 6.8 and X11. There is no Wayland backend in this release.

## Local launcher requests

`--pin-desktop` either imports files before showing a new Dock or forwards their
file URLs to the existing instance. The Qt local-socket adapter accepts only a
bounded, newline-framed `pinFiles` request, limited to 32 URL strings; Rust still
validates each file and the complete batch. The listener lives in the user runtime
directory with current-user access. Socket errors never spawn a second Dock.
No arbitrary command execution is exposed by this transport.
