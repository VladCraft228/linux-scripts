# XFCE manual validation

Build in Release mode and start from a terminal. Keep Plank enabled initially.

1. Confirm the panel appears above Plank and does not take keyboard focus.
2. Move over icons slowly and quickly: icons should expand and settle without
   flicker, overlap, clipping, or continuous movement after the pointer stops.
3. Leave the dock from its top and side: icons should return to their base size.
4. Click desktop items through the transparent window margins. They should
   receive input; the panel and visible icons should remain clickable.
5. Open and close two terminals and a browser; confirm entries follow windows.
6. Change a window's title and activate windows using Alt+Tab; verify title and
   active indicators update without disturbing the pointer.
7. Minimize a window, then click its icon. Confirm the window manager restores it.
8. Test a window on another workspace. Record the behavior of XFCE's activation
   policy rather than assuming the dock can override it.
9. Right-click the panel, select Quit, and verify the process exits.
10. Start a second instance; confirm it exits without opening another panel.
11. Compare WhiteSur and another installed icon theme. A missing themed icon
    should fall back to a window icon or generated tile.
12. Repeat with display scaling and monitor hotplug; report misplaced or clipped
    panels. Full multi-monitor support is not part of this technical prototype.

For performance, compare the same visible window set with Plank. Measure idle,
hidden/occluded, and pointer-motion CPU separately, plus RSS/PSS and frame pacing.
Do not infer RAM savings from the language or toolkit alone.
