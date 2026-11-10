#!/usr/bin/env python3
import ctypes
import fcntl
import os
from pathlib import Path
import signal
import sys
import re
import subprocess

# A per-user lock also provides a narrowly scoped stop command for the installer.
lock_dir = Path(os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache"))) / "layout-osd"
lock_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
lock_file = (lock_dir / "instance.lock").open("a+")
try:
    fcntl.flock(lock_file, fcntl.LOCK_EX | fcntl.LOCK_NB)
except BlockingIOError:
    if sys.argv[1:] == ["--stop"]:
        lock_file.seek(0)
        try:
            pid = int(lock_file.read().strip())
            args = Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")
            if len(args) > 1 and Path(os.fsdecode(args[1])).resolve() == Path(__file__).resolve():
                os.kill(pid, signal.SIGTERM)
            else:
                raise SystemExit("Cannot verify the running Layout OSD process.")
        except (ValueError, ProcessLookupError, FileNotFoundError):
            pass
        raise SystemExit(0)
    raise SystemExit("Layout OSD is already running.")
if sys.argv[1:] == ["--stop"]:
    raise SystemExit(0)
if sys.argv[1:]:
    raise SystemExit("Usage: layout-osd.py [--stop]")
lock_file.seek(0)
lock_file.truncate()
lock_file.write(str(os.getpid()))
lock_file.flush()

if os.environ.get("XDG_SESSION_TYPE", "").lower() == "wayland" or os.environ.get("WAYLAND_DISPLAY"):
    raise SystemExit("Layout OSD requires an X11 session; Wayland is not supported.")
if not os.environ.get("DISPLAY"):
    raise SystemExit("Layout OSD requires an X11 display.")
os.environ["GDK_BACKEND"] = "x11"

import gi

gi.require_version("Gdk", "3.0")
gi.require_version("Gtk", "3.0")

from gi.repository import Gdk, GLib, Gtk

# Налаштування OSD
OSD_WIDTH = 180
OSD_HEIGHT = 110
OSD_DURATION = 800
POLL_INTERVAL = 100

# X11
XKB_CORE_KEYBOARD = 0x100

x11 = ctypes.CDLL("libX11.so.6")

x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
x11.XOpenDisplay.restype = ctypes.c_void_p

x11.XkbGetState.argtypes = [
    ctypes.c_void_p,
    ctypes.c_uint,
    ctypes.c_void_p,
]
x11.XkbGetState.restype = ctypes.c_int

x11.XGetInputFocus.argtypes = [
    ctypes.c_void_p,
    ctypes.POINTER(ctypes.c_ulong),
    ctypes.POINTER(ctypes.c_int),
]
x11.XGetInputFocus.restype = ctypes.c_int


x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
x11.XCloseDisplay.restype = ctypes.c_int


class XkbState(ctypes.Structure):
    _fields_ = [
        ("group", ctypes.c_ubyte),
        ("locked_group", ctypes.c_ubyte),
        ("base_group", ctypes.c_ushort),
        ("latched_group", ctypes.c_ushort),
    ] + [
        (name, ctypes.c_ubyte)
        for name in (
            "mods", "base_mods", "latched_mods", "locked_mods",
            "compat_state", "grab_mods", "compat_grab_mods",
            "lookup_mods", "compat_lookup_mods",
        )
    ] + [("ptr_buttons", ctypes.c_ushort)]


display = x11.XOpenDisplay(None)
if not display:
    raise SystemExit("Cannot open the X11 display.")


def get_xkb_state():
    state = XkbState()

    if x11.XkbGetState(
        display,
        XKB_CORE_KEYBOARD,
        ctypes.byref(state),
    ) != 0:
        return None, None

    # Use the X11 Lock modifier rather than assuming an indicator index.
    caps_locked = bool(state.locked_mods & 0x02)

    return state.group, caps_locked


def get_focus():
    window = ctypes.c_ulong()
    revert = ctypes.c_int()

    x11.XGetInputFocus(
        display,
        ctypes.byref(window),
        ctypes.byref(revert),
    )

    return window.value


def get_layouts():
    try:
        output = subprocess.check_output(
            ["setxkbmap", "-query"],
            text=True,
            stderr=subprocess.DEVNULL,
        )

        match = re.search(r"^layout:\s*(.+)$", output, re.M)

        if match:
            return match.group(1).strip().split(",")

    except (OSError, subprocess.CalledProcessError):
        pass

    return ["us", "ua"]


LAYOUTS = get_layouts()

NAMES = {
    "ua": ("UK", "Українська"),
    "us": ("EN", "English"),
    "gb": ("EN", "English"),
    "ru": ("RU", "Російська"),
    "de": ("DE", "Deutsch"),
    "pl": ("PL", "Polski"),
}


class LayoutOSD(Gtk.Window):
    def __init__(self):
        super().__init__(type=Gtk.WindowType.TOPLEVEL)

        # Вікно-сповіщення без захоплення фокусу
        self.set_type_hint(Gdk.WindowTypeHint.NOTIFICATION)
        self.set_decorated(False)
        self.set_keep_above(True)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)
        self.set_focus_on_map(False)
        self.set_accept_focus(False)
        self.set_can_focus(False)
        self.set_resizable(False)
        self.set_default_size(OSD_WIDTH, OSD_HEIGHT)
        self.set_size_request(OSD_WIDTH, OSD_HEIGHT)
        self.set_app_paintable(True)

        self.get_style_context().add_class("layout-osd-window")

        screen = self.get_screen()
        visual = screen.get_rgba_visual()

        if visual and screen.is_composited():
            self.set_visual(visual)

        # Прозоре вікно та темний OSD
        css = b"""
        window.layout-osd-window {
            background-color: transparent;
            background-image: none;
            border: none;
            box-shadow: none;
        }

        .osd {
            background-color: rgba(28, 36, 53, 0.94);
            background-image: none;
            border-radius: 20px;
            border: none;
            box-shadow: none;
        }

        .code {
            font-size: 30px;
            font-weight: bold;
            color: #ffffff;
        }

        .name {
            font-size: 12px;
            color: #b8c5da;
        }

        .caps-on {
            font-size: 30px;
            font-weight: bold;
            color: #f5ad42;
        }

        .caps-off {
            font-size: 30px;
            font-weight: bold;
            color: #ffffff;
        }
        """

        provider = Gtk.CssProvider()
        provider.load_from_data(css)

        Gtk.StyleContext.add_provider_for_screen(
            screen,
            provider,
            Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
        )

        # Фіксований контейнер OSD
        self.frame = Gtk.Box(
            orientation=Gtk.Orientation.VERTICAL,
            spacing=3,
        )

        self.frame.get_style_context().add_class("osd")
        self.frame.set_size_request(OSD_WIDTH, OSD_HEIGHT)
        self.frame.set_halign(Gtk.Align.FILL)
        self.frame.set_valign(Gtk.Align.FILL)

        # Основний напис
        self.main_label = Gtk.Label()
        self.main_label.set_halign(Gtk.Align.CENTER)
        self.main_label.get_style_context().add_class("code")

        # Підпис
        self.subtitle = Gtk.Label()
        self.subtitle.set_halign(Gtk.Align.CENTER)
        self.subtitle.get_style_context().add_class("name")

        # Вертикальне центрування обох написів
        self.content = Gtk.Box(
            orientation=Gtk.Orientation.VERTICAL,
            spacing=3,
        )

        self.content.set_halign(Gtk.Align.CENTER)
        self.content.set_valign(Gtk.Align.CENTER)

        self.content.pack_start(
            self.main_label, False, False, 0
        )
        self.content.pack_start(
            self.subtitle, False, False, 0
        )

        self.frame.pack_start(
            self.content, True, True, 0
        )

        self.add(self.frame)

        # Початковий стан
        self.last_group, self.last_caps = get_xkb_state()
        self.last_focus = get_focus()
        self.hide_id = None

        GLib.timeout_add(POLL_INTERVAL, self.check)

    def check(self):
        group, caps = get_xkb_state()
        focus = get_focus()

        if group is None:
            return True

        focus_changed = focus != self.last_focus
        group_changed = group != self.last_group

        caps_changed = (
            caps is not None
            and self.last_caps is not None
            and caps != self.last_caps
        )

        # Caps Lock має пріоритет
        if caps_changed:
            self.show_caps(caps)

        # Не показуємо перемикання при зміні фокусу
        elif group_changed and not focus_changed:
            self.show_language(group)

        self.last_group = group

        if caps is not None:
            self.last_caps = caps

        self.last_focus = focus

        return True

    def show_language(self, group):
        layout = (
            LAYOUTS[group]
            if group < len(LAYOUTS)
            else "unknown"
        )

        code, name = NAMES.get(
            layout,
            (layout.upper(), layout),
        )

        self.main_label.set_text(code)
        self.subtitle.set_text(name)

        style = self.main_label.get_style_context()

        style.remove_class("caps-on")
        style.remove_class("caps-off")
        style.add_class("code")

        self.present_osd()

    def show_caps(self, enabled):
        self.main_label.set_text(
            "ON" if enabled else "OFF"
        )
        self.subtitle.set_text("CAPS LOCK")

        style = self.main_label.get_style_context()

        style.remove_class("code")
        style.remove_class("caps-on")
        style.remove_class("caps-off")

        style.add_class(
            "caps-on" if enabled else "caps-off"
        )

        self.present_osd()

    def present_osd(self):
        self.show_all()
        self.move_to_center()

        if self.hide_id is not None:
            GLib.source_remove(self.hide_id)

        self.hide_id = GLib.timeout_add(
            OSD_DURATION,
            self.dismiss,
        )

    def move_to_center(self):
        screen = Gdk.Screen.get_default()
        display = Gdk.Display.get_default()

        seat = display.get_default_seat()
        pointer = seat.get_pointer()

        _, x, y = pointer.get_position()

        monitor = screen.get_monitor_at_point(x, y)
        rect = screen.get_monitor_geometry(monitor)

        self.move(
            rect.x + (rect.width - OSD_WIDTH) // 2,
            rect.y + (rect.height - OSD_HEIGHT) // 2,
        )

    def dismiss(self):
        self.hide()
        self.hide_id = None
        return False


if __name__ == "__main__":
    window = LayoutOSD()
    try:
        Gtk.main()
    finally:
        x11.XCloseDisplay(display)
