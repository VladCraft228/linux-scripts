# Prychal Dock roadmap

The existing Rust renderer experiment is becoming a practical dock, incrementally
matching the behavior of the Qt-based original. Preserve a working version at
each step; build and validate on the XFCE/X11 development laptop. Keep Rust
Edition settings independent from the Qt implementation during this work.

## Implemented for local validation

- Pure Rust X11 rendering, magnification and fixed base panel geometry.
- System icon themes, running applications, grouping and minimize/restore.
- Persistent pinned reordering and session-only running icon placement, including between pins, with cancellation.
- XDND file-hover activation/restoration of running application windows.
- Local `.desktop` drag-and-drop import and deduplication.
- A separate native settings window with sidebar sections, sliders, switches,
  live preview, color controls, resize/close support, and reset retaining pins.
- Background/border/indicator color wheels, border toggle/width/opacity, corner
  radius, and multiple active/running indicator designs.
- Intelligent overlap hide, configurable delay and symmetric slide timing.
- Application-name labels above the corresponding icon.
- Hover window chooser with real titles, exact activation and per-window/grouped
  close requests; hover stays attached to the original app.
- XComposite/XRender thumbnails for visible cards at up to 4 FPS, a bounded cache
  for previously captured windows, and app-icon fallback when capture is unavailable.

See [the detailed parity audit](parity-audit.md) for the source-backed gap list.

- Bounded launch bounce, multi-window count badges and visible launch errors.
- About page with version, author/ChatGPT credits and project links.
- Native installed-app picker with name/ID search and persistent pinning.
- X11 attention dots and application-supplied D-Bus launcher counters.
- X11 single instance with settings/quit commands; installer/uninstaller, menu
  launcher, SVG icon and default XFCE autostart, with separate edition paths.

## Next parity milestones

1. Bottom offset, insertion position for desktop drops, typed HEX and reset confirmation.
2. Stronger launcher matching (including concrete Steam/Wine cases), multi-monitor selection/hotplug.
3. Optional external pin IPC and Thunar custom-action integration.

Wayland needs its own compositor/backend integration; replacing Qt alone does
not provide equivalent window-control protocols on Plasma Wayland.
