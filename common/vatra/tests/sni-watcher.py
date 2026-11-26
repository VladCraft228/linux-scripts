"""Isolated StatusNotifierWatcher fixture controlled over stdin."""
import sys
from gi.repository import Gio, GLib

NAME = "org.kde.StatusNotifierWatcher"
PATH = "/StatusNotifierWatcher"
BUS = "org.freedesktop.DBus"
INFO = Gio.DBusNodeInfo.new_for_xml(f"""
<node><interface name="{NAME}">
    <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
    <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
    <signal name="StatusNotifierHostRegistered"/>
    <signal name="StatusNotifierHostUnregistered"/>
</interface></node>
""")
conn = Gio.bus_get_sync(Gio.BusType.SESSION, None)
loop = GLib.MainLoop()
host = False


def register(_conn, sender, _path, _interface, _method, parameters, call):
    (item,) = parameters.unpack()
    print(f"{sender} {item}", flush=True)
    call.return_value(None)


def property_value(*_args):
    return GLib.Variant("b", host)


conn.register_object(PATH, INFO.interfaces[0], register, property_value, None)


def handle_stdin(_fd, _condition):
    global host
    action = sys.stdin.readline().strip()
    if not action or action == "quit":
        loop.quit()
        return GLib.SOURCE_REMOVE
    if action in {"acquire", "release"}:
        method = "RequestName" if action == "acquire" else "ReleaseName"
        args = GLib.Variant("(su)", (NAME, 4)) if action == "acquire" else GLib.Variant("(s)", (NAME,))
        reply = conn.call_sync(BUS, "/org/freedesktop/DBus", BUS, method, args,
                               None, Gio.DBusCallFlags.NONE, 2000, None)
        assert reply.unpack() == (1,)
    elif action in {"host-on", "host-off"}:
        host = action == "host-on"
        signal = "StatusNotifierHostRegistered" if host else "StatusNotifierHostUnregistered"
        conn.emit_signal(None, PATH, NAME, signal, None)
    else:
        raise ValueError(action)
    print(action, flush=True)
    return GLib.SOURCE_CONTINUE


GLib.io_add_watch(sys.stdin.fileno(), GLib.PRIORITY_DEFAULT, GLib.IO_IN | GLib.IO_HUP, handle_stdin)
print("started", flush=True)
loop.run()
