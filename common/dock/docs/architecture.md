# Architecture decision: technical prototype

Status: agreed technology; first implementation pending real XFCE validation.

## Agreed direction

C++17 owns native integration and models; Qt Quick/QML owns presentation.
Qt 6.8 is the minimum, matching the initial Debian 13 system.
The technical prototype uses a small X11 WindowModel to validate the difficult
platform boundaries before extracting application grouping and launcher services.

The X11 connection is separate from Qt's connection. It subscribes to EWMH root
properties and client metadata. QSocketNotifier drains events and a single-shot
25 ms timer coalesces bursts. The timer is stopped when idle. Input masks are
sent only when geometry changes, using SHAPE's input region rather than clipping
the rendered window. Input rectangles include the panel and expanded icons.

QML computes magnification from stable baseline icon centers. Neighboring icons
move with their animated widths. Icon source dimensions remain fixed during
animation. This first implementation can still require tuning to avoid cursor
oscillation at the shaped boundary; that must be tested on a real compositor.

## Next steps after validation

1. Extract PlatformBackend: window events/actions and dock-surface integration.
2. Add ApplicationCatalog: desktop IDs, desktop-file launching, matching rules.
3. Add DockModel: persistent launchers grouped with their windows.
4. Add reorder, full context menus, and basic autohide with an edge trigger.
5. Add settings and performance measurement before calling the dock an MVP.
6. Implement a Plasma/KWin Wayland backend separately from X11.

Wayland surface placement and task/window management are separate capabilities.
Do not promise generic Wayland support based on layer-shell alone.

No application launching through shell-expanded desktop-file Exec strings is
implemented in this prototype. Evaluate the dependency cost of KService before
choosing the desktop-entry launcher implementation.
