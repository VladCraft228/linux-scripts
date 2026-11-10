# Layout OSD for XFCE

A lightweight keyboard layout and Caps Lock on-screen display.

## Requirements

- XFCE on X11
- Python 3
- GTK3 Python bindings
- X11 libraries
- setxkbmap

### Dependencies (Debian / MX Linux)

    sudo apt install python3-gi gir1.2-gtk-3.0 libx11-6 x11-xkb-utils

## Installation

    mkdir -p ~/.local/bin ~/.config/autostart
    cp layout-osd.py ~/.local/bin/

Create ~/.config/autostart/layout-osd.desktop
with the following content:

    [Desktop Entry]
    Type=Application
    Name=Layout OSD
    Exec=python3 /home/YOUR_USERNAME/.local/bin/layout-osd.py
    Icon=input-keyboard
    Terminal=false
    OnlyShowIn=XFCE;

Replace YOUR_USERNAME with your actual Linux username.

## Configuration

Edit these variables in layout-osd.py:

    OSD_WIDTH = 180
    OSD_HEIGHT = 110
    OSD_DURATION = 800

## Compatibility

Designed for X11. Wayland is not supported.
