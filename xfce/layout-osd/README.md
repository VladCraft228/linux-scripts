# Layout OSD for XFCE

A lightweight, macOS-style keyboard layout and Caps Lock on-screen display for XFCE on X11, built with Python and GTK3.

## Features

- Keyboard layout change notifications (EN, UK, RU, and more)
- Caps Lock ON/OFF notifications
- Consistent 180 × 110 px popup
- Rounded corners and transparent background
- Automatic positioning on the active monitor
- XFCE autostart support
- Lightweight and configurable

## Requirements

- XFCE desktop environment
- X11 session (Wayland is not supported)
- Python 3
- GTK3 Python bindings (PyGObject)
- X11 libraries
- `setxkbmap`

### Dependencies (Debian / MX Linux)

```bash
sudo apt install python3-gi gir1.2-gtk-3.0 libx11-6 x11-xkb-utils
```

## Installation

**1. Clone the repository**

```bash
git clone https://github.com/VladCraft228/linux-scripts.git
```

**2. Navigate to the Layout OSD directory**

```bash
cd linux-scripts/xfce/layout-osd
```

**3. Run the installer**

```bash
./install.sh
```

The installer will:

- Check required dependencies.
- Copy `layout-osd.py` to `~/.local/bin/`.
- Create `~/.config/autostart/layout-osd.desktop`.
- Launch Layout OSD immediately when running XFCE.

No root privileges are required for the installer.

## Configuration

Edit the installed script:

```bash
nano ~/.local/bin/layout-osd.py
```

Available settings:

```python
OSD_WIDTH = 180
OSD_HEIGHT = 110
OSD_DURATION = 800
POLL_INTERVAL = 100
```

Changes take effect after restarting the script or logging in again.

## Autostart

Layout OSD automatically starts when you log into XFCE.

You can manage it through:

**Settings → Session and Startup → Application Autostart**

## Compatibility

Developed and tested for MX Linux with XFCE on X11.

Wayland is not supported because the application relies on X11/XKB APIs.

## Stopping and removal

```bash
python3 ~/.local/bin/layout-osd.py --stop
./uninstall.sh
```

Only one instance can run per user. The installer stops an existing instance
before updating it. The installer also handles the original version by matching the exact installed
script path and current user.

Autostart respects `XDG_CONFIG_HOME` (default: `~/.config`).
The instance lock is stored under `XDG_CACHE_HOME` (default: `~/.cache`).

## Known limitations

- Layouts are read at startup. Restart after changing the configured layout list.
- Focus-change filtering is best effort: changes are sampled every 100 ms.
- Transparency requires an enabled compositor.
- Caps Lock detection uses the standard X11 Lock modifier mapping.
- This is an X11 utility, including when launched manually; Wayland is rejected.
- GUI behavior is tested by the project author on MX Linux XFCE.

## License

A license has not yet been selected for this repository.
