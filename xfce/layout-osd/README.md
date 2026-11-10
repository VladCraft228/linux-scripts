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
git clone https://github.com/YOUR_USERNAME/linux-scripts.git
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

## License

See the repository's `LICENSE` file, if provided.
