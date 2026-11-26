use super::{Action, Sender};
use crate::profiles::Profile;
use std::ffi::{c_int, c_uint, CStr, CString};
use std::fmt::Write as _;
use std::io::{self, Read, Write};
use std::os::{fd::AsRawFd, unix::net::UnixStream};
use std::ptr::{null, null_mut};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

#[path = "gio.rs"]
mod gio;
#[path = "icon.rs"]
mod icon;
use gio::*;

const WATCHER: &CStr = c"org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &CStr = c"/StatusNotifierWatcher";
const ITEM: &CStr = c"org.kde.StatusNotifierItem";
const ITEM_PATH: &CStr = c"/StatusNotifierItem";
const MENU: &CStr = c"com.canonical.dbusmenu";
const MENU_PATH: &CStr = c"/MenuBar";

enum Command {
    Enable,
    Update(String),
    Stop,
}

pub(super) struct Worker {
    tx: mpsc::Sender<Command>,
    wake: UnixStream,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn spawn(send: Sender) -> io::Result<Self> {
        let (wake, reader) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        reader.set_nonblocking(true)?;
        let (tx, rx) = mpsc::channel();
        let thread = thread::Builder::new().name("vatra-sni".into()).spawn(move || unsafe {
            run(reader, rx, send);
        })?;
        Ok(Self { tx, wake, thread: Some(thread) })
    }

    fn send(&self, command: Command) {
        if self.tx.send(command).is_ok() {
            // If the socket is full there is already a wakeup waiting.
            let _ = (&self.wake).write_all(&[1]);
        }
    }

    pub fn enable(&self) { self.send(Command::Enable); }
    pub fn update(&self, wire: String) { self.send(Command::Update(wire)); }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.send(Command::Stop);
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}

#[derive(PartialEq)]
struct IconState {
    level: Option<u8>,
    charging: bool,
    color: [u8; 3],
    percent: u8,
    bolt: u8,
}

struct Reading {
    icon: IconState,
    mode: String,
    ready: bool,
    login: bool,
    tooltip: String,
}

impl Reading {
    fn parse(wire: &str) -> Option<Self> {
        let fields: Vec<_> = wire.trim_end_matches('\n').splitn(9, '\t').collect();
        let [level, charging, mode, ready, login, hex, percent, bolt, tip] = *fields.as_slice() else {
            return None;
        };
        let color = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        Some(Self {
            icon: IconState {
                level: level.parse().ok(), charging: charging == "1",
                color: [color(0)?, color(2)?, color(4)?],
                percent: percent.parse().ok()?, bolt: bolt.parse().ok()?,
            },
            mode: mode.into(), ready: ready == "1", login: login == "1",
            tooltip: tip.replace("\\n", "\n"),
        })
    }
}

struct Service {
    bus: Ptr,
    loop_: Ptr,
    reader: UnixStream,
    rx: Receiver<Command>,
    send: Sender,
    owner: Option<CString>,
    epoch: u64,
    probe: u64,
    available: bool,
    active: bool,
    registered: bool,
    pending: usize,
    stopping: bool,
    reading: Reading,
    pixmap: String,
    revision: u32,
}

impl Service {
    fn update(&mut self, wire: &str) {
        let Some(next) = Reading::parse(wire) else { return };
        let image_changed = self.reading.icon != next.icon;
        let tip_changed = self.reading.tooltip != next.tooltip;
        let menu_changed = self.reading.mode != next.mode || self.reading.ready != next.ready || self.reading.login != next.login;
        self.reading = next;
        if image_changed {
            self.pixmap = pixmap(&self.reading.icon);
            self.emit(ITEM_PATH, ITEM, c"NewIcon", null_mut());
        }
        if tip_changed { self.emit(ITEM_PATH, ITEM, c"NewToolTip", null_mut()); }
        if menu_changed {
            self.revision = self.revision.wrapping_add(1);
            self.emit(MENU_PATH, MENU, c"LayoutUpdated", variant(c"(ui)", &format!("({}, 0)", self.revision)));
        }
    }

    fn emit(&self, path: &CStr, interface: &CStr, signal: &CStr, value: Ptr) {
        unsafe {
            g_dbus_connection_emit_signal(self.bus, null(), path.as_ptr(), interface.as_ptr(),
                signal.as_ptr(), value, null_mut());
            release(value);
        }
    }

    fn activate_menu(&self, id: i32) -> bool {
        let action = match id {
            1 => Action::Open,
            3..=6 if self.reading.ready => Action::Mode(Profile::QUICK[(id - 3) as usize]),
            8 => Action::Autostart(!self.reading.login),
            9 => Action::Disable,
            10 => Action::Quit,
            _ => return false,
        };
        (self.send)(action);
        true
    }

    fn menu_props(&self, id: i32) -> Option<Vec<(&'static str, String)>> {
        let (label, toggle, checked) = match id {
            0 => return Some(vec![("children-display", quoted("submenu"))]),
            2 | 7 => return Some(vec![("type", quoted("separator"))]),
            1 => ("Open Vatra", "", false),
            3..=6 => {
                let mode = Profile::QUICK[(id - 3) as usize];
                (mode.name(), "radio", self.reading.mode == mode.id())
            }
            8 => ("Start at login", "checkmark", self.reading.login),
            9 => ("Disable tray", "", false),
            10 => ("Quit Vatra", "", false),
            _ => return None,
        };
        let mut props = vec![("label", quoted(label))];
        if !toggle.is_empty() {
            props.push(("toggle-type", quoted(toggle)));
            props.push(("toggle-state", u8::from(checked).to_string()));
        }
        if (3..=6).contains(&id) {
            props.push(("enabled", self.reading.ready.to_string()));
        }
        Some(props)
    }

    fn menu_dict(&self, id: i32, names: &[String]) -> Option<String> {
        Some(format!("{{{}}}", self.menu_props(id)?.into_iter()
            .filter(|(key, _)| names.is_empty() || names.iter().any(|name| name == key))
            .map(|(key, value)| format!("'{key}': <{value}>")).collect::<Vec<_>>().join(", ")))
    }

    fn enable(&mut self) {
        let Some(owner) = self.owner.clone().filter(|_| self.available) else { return };
        if self.registered {
            self.set_active(true);
            (self.send)(Action::SniReady(true));
            return;
        }
        self.set_active(true);
        self.pending += 1;
        let data = Box::into_raw(Box::new((self as *mut Service, self.epoch)));
        unsafe {
            // The path registration form binds the item to this connection's
            // unique D-Bus name. No second well-known name is needed.
            g_dbus_connection_call(self.bus, owner.as_ptr(), WATCHER_PATH.as_ptr(), WATCHER.as_ptr(),
                c"RegisterStatusNotifierItem".as_ptr(), variant(c"(s)", "('/StatusNotifierItem',)"),
                c"()".as_ptr(), 0, 2500, null_mut(), Some(registered), data.cast());
        }
    }
    fn set_active(&mut self, active: bool) {
        if self.active == active { return; }
        self.active = active;
        let status = if active { "Active" } else { "Passive" };
        self.emit(ITEM_PATH, ITEM, c"NewStatus", variant(c"(s)", &format!("({status:?},)")));
    }

    fn set_available(&mut self, available: bool) {
        if self.available == available { return; }
        self.available = available;
        self.epoch = self.epoch.wrapping_add(1);
        if !available { self.set_active(false); }
        (self.send)(Action::Watcher(available));
    }

    fn probe_host(&mut self) {
        let Some(owner) = self.owner.clone() else { return };
        self.probe = self.probe.wrapping_add(1);
        self.pending += 1;
        let data = Box::into_raw(Box::new((self as *mut Service, self.probe)));
        unsafe {
            g_dbus_connection_call(self.bus, owner.as_ptr(), WATCHER_PATH.as_ptr(),
                c"org.freedesktop.DBus.Properties".as_ptr(), c"Get".as_ptr(),
                variant(c"(ss)", "('org.kde.StatusNotifierWatcher', 'IsStatusNotifierHostRegistered')"),
                c"(v)".as_ptr(), 0, 2500, null_mut(), Some(host_checked), data.cast());
        }
    }

    unsafe fn finish_call(&mut self) {
        self.pending -= 1;
        if self.stopping && self.pending == 0 { g_main_loop_quit(self.loop_); }
    }

}

fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'").replace('\0', "�"))
}

fn variant(signature: &CStr, text: &str) -> Ptr {
    let Ok(text) = CString::new(text) else { return null_mut() };
    unsafe { g_variant_parse(signature.as_ptr(), text.as_ptr(), null(), null_mut(), null_mut()) }
}

pub fn xfce_panel_visible() -> Option<bool> {
    unsafe {
        let bus = g_bus_get_sync(2, null_mut(), null_mut());
        if bus.is_null() { return None; }
        g_dbus_connection_set_exit_on_close(bus, 0);
        let args = variant(c"(s)", "('io.github.VladCraft228.VatraPanel',)");
        let reply = g_dbus_connection_call_sync(bus, c"org.freedesktop.DBus".as_ptr(),
            c"/org/freedesktop/DBus".as_ptr(), c"org.freedesktop.DBus".as_ptr(),
            c"NameHasOwner".as_ptr(), args, c"(b)".as_ptr(), 0, 600, null_mut(), null_mut());
        let visible = if reply.is_null() { None } else {
            let value = g_variant_get_child_value(reply, 0);
            let visible = g_variant_get_boolean(value) != 0;
            release(value);
            Some(visible)
        };
        release(reply);
        release(args);
        g_object_unref(bus);
        visible
    }
}

pub fn notify(message: &str) -> bool {
    unsafe {
        let bus = g_bus_get_sync(2, null_mut(), null_mut()); // G_BUS_TYPE_SESSION
        if bus.is_null() { return false; }
        g_dbus_connection_set_exit_on_close(bus, 0);
        let args = variant(c"(susssasa{sv}i)", &format!(
            "('Vatra', 0, 'vatra', {}, '', [], {{'transient': <true>}}, 2500)",
            quoted(message)
        ));
        let reply = g_dbus_connection_call_sync(bus, c"org.freedesktop.Notifications".as_ptr(),
            c"/org/freedesktop/Notifications".as_ptr(), c"org.freedesktop.Notifications".as_ptr(),
            c"Notify".as_ptr(), args, c"(u)".as_ptr(), 0, 1500, null_mut(), null_mut());
        release(args);
        release(reply);
        g_object_unref(bus);
        !reply.is_null()
    }
}

unsafe fn release(value: Ptr) {
    if !value.is_null() { g_variant_unref(value); }
}

fn pixmap(state: &IconState) -> String {
    let bytes = icon::draw(state.level, state.charging, state.color, state.percent, state.bolt);
    let mut value = String::from("[(24, 24, [");
    for byte in bytes { let _ = write!(value, "{byte},"); }
    value.pop();
    value.push_str("])]");
    value
}

unsafe fn int(value: Ptr, index: usize) -> i32 {
    let child = g_variant_get_child_value(value, index);
    let result = g_variant_get_int32(child);
    release(child);
    result
}

unsafe fn string(value: Ptr, index: usize) -> String {
    let child = g_variant_get_child_value(value, index);
    let result = CStr::from_ptr(g_variant_get_string(child, null_mut())).to_string_lossy().into_owned();
    release(child);
    result
}

unsafe fn array<T>(value: Ptr, index: usize, item: impl Fn(Ptr, usize) -> T) -> Vec<T> {
    let child = g_variant_get_child_value(value, index);
    let values = (0..g_variant_n_children(child)).map(|i| item(child, i)).collect();
    release(child);
    values
}

unsafe extern "C" fn property(_: Ptr, _: Str, _: Str, interface: Str, key: Str, _: *mut Ptr, data: Ptr) -> Ptr {
    let state = &*data.cast::<Service>();
    let key = CStr::from_ptr(key).to_bytes();
    let (kind, value): (&CStr, String) = if CStr::from_ptr(interface) == ITEM {
        match key {
            b"Category" => (c"s", "'Hardware'".into()),
            b"Id" => (c"s", "'vatra'".into()),
            b"Title" => (c"s", "'Vatra'".into()),
            b"Status" => (c"s", if state.active { "'Active'" } else { "'Passive'" }.into()),
            b"IconPixmap" => (c"a(iiay)", state.pixmap.clone()),
            b"OverlayIconPixmap" | b"AttentionIconPixmap" => (c"a(iiay)", "[]".into()),
            b"IconName" | b"OverlayIconName" | b"AttentionIconName" | b"AttentionMovieName" => (c"s", "''".into()),
            b"ToolTip" => (c"(sa(iiay)ss)", format!("('', [], 'Vatra', {})", quoted(&state.reading.tooltip))),
            b"WindowId" => (c"i", "0".into()),
            b"ItemIsMenu" => (c"b", "false".into()),
            b"Menu" => (c"o", "'/MenuBar'".into()),
            _ => return null_mut(),
        }
    } else {
        match key {
            b"Version" => (c"u", "3".into()),
            b"TextDirection" => (c"s", "'ltr'".into()),
            b"Status" => (c"s", "'normal'".into()),
            b"IconThemePath" => (c"as", "[]".into()),
            _ => return null_mut(),
        }
    };
    // A getter transfers ownership to GDBus.
    variant(kind, &value)
}

unsafe extern "C" fn method(_: Ptr, _: Str, _: Str, interface: Str, name: Str, args: Ptr, call: Ptr, data: Ptr) {
    let state = &*data.cast::<Service>();
    let name = CStr::from_ptr(name).to_bytes();
    let reply = if CStr::from_ptr(interface) == ITEM {
        if matches!(name, b"Activate" | b"SecondaryActivate") { (state.send)(Action::Open); }
        Some((c"()", "()".into()))
    } else {
        menu_reply(state, name, args)
    };
    if let Some((kind, text)) = reply {
        let value = variant(kind, &text);
        if !value.is_null() {
            g_dbus_method_invocation_return_value(call, value);
            release(value);
            return;
        }
    }
    g_dbus_method_invocation_return_dbus_error(call, c"com.canonical.dbusmenu.Error.InvalidMenuItem".as_ptr(),
        c"Unknown menu item or property".as_ptr());
}

unsafe fn menu_reply(state: &Service, name: &[u8], args: Ptr) -> Option<(&'static CStr, String)> {
    Some(match name {
        b"GetLayout" => {
            let id = int(args, 0);
            let depth = int(args, 1);
            let names = array(args, 2, |a, i| string(a, i));
            let props = state.menu_dict(id, &names)?;
            let children = if id == 0 && depth != 0 {
                (1..=10).map(|id| format!("<({id}, @a{{sv}} {}, @av [])>", state.menu_dict(id, &names).unwrap_or_default()))
                    .collect::<Vec<_>>().join(", ")
            } else { String::new() };
            (c"(u(ia{sv}av))", format!("({}, ({id}, {props}, [{children}]))", state.revision))
        }
        b"GetGroupProperties" => {
            let ids = array(args, 0, |a, i| int(a, i));
            let names = array(args, 1, |a, i| string(a, i));
            let props = ids.into_iter().filter_map(|id| Some(format!("({id}, {})", state.menu_dict(id, &names)?)))
                .collect::<Vec<_>>().join(", ");
            (c"(a(ia{sv}))", format!("([{props}],)"))
        }
        b"GetProperty" => {
            let id = int(args, 0);
            let property = string(args, 1);
            let value = state.menu_props(id)?.into_iter().find(|(key, _)| *key == property)?.1;
            (c"(v)", format!("(<{value}>,)"))
        }
        b"Event" => {
            if string(args, 1) == "clicked" { state.activate_menu(int(args, 0)); }
            (c"()", "()".into())
        }
        b"EventGroup" => {
            let failed = array(args, 0, |a, i| {
                let event = g_variant_get_child_value(a, i);
                let id = int(event, 0);
                let failed = string(event, 1) == "clicked" && !state.activate_menu(id);
                release(event);
                failed.then_some(id)
            }).into_iter().flatten().map(|id| id.to_string()).collect::<Vec<_>>().join(", ");
            (c"(ai)", format!("([{failed}],)"))
        }
        b"AboutToShow" => (c"(b)", "(false,)".into()),
        b"AboutToShowGroup" => (c"(aiai)", "([], [])".into()),
        _ => return None,
    })
}

unsafe extern "C" fn appeared(_: Ptr, _: Str, owner: Str, data: Ptr) {
    let state = &mut *data.cast::<Service>();
    let owner = CStr::from_ptr(owner);
    if state.owner.as_deref() != Some(owner) {
        state.set_available(false);
        state.registered = false;
        state.owner = Some(owner.to_owned());
    }
    if !state.stopping { state.probe_host(); }
}

unsafe extern "C" fn vanished(_: Ptr, _: Str, data: Ptr) {
    let state = &mut *data.cast::<Service>();
    let was_available = state.available;
    state.probe = state.probe.wrapping_add(1);
    state.owner = None;
    state.registered = false;
    state.set_available(false);
    if !was_available && !state.stopping { (state.send)(Action::Watcher(false)); }
}

unsafe extern "C" fn registered(bus: Ptr, result: Ptr, data: Ptr) {
    let (ptr, epoch) = *Box::from_raw(data.cast::<(*mut Service, u64)>());
    let state = &mut *ptr;
    let reply = g_dbus_connection_call_finish(bus, result, null_mut());
    if !state.stopping && epoch == state.epoch {
        state.registered = !reply.is_null();
        if !state.registered { state.set_active(false); }
        (state.send)(Action::SniReady(state.registered));
    }
    release(reply);
    state.finish_call();
}

unsafe extern "C" fn host_changed(_: Ptr, sender: Str, _: Str, _: Str, _: Str, _: Ptr, data: Ptr) {
    let state = &mut *data.cast::<Service>();
    if !state.stopping && !sender.is_null() && state.owner.as_deref() == Some(CStr::from_ptr(sender)) {
        state.probe_host();
    }
}

unsafe extern "C" fn host_checked(bus: Ptr, result: Ptr, data: Ptr) {
    let (ptr, probe) = *Box::from_raw(data.cast::<(*mut Service, u64)>());
    let state = &mut *ptr;
    let reply = g_dbus_connection_call_finish(bus, result, null_mut());
    if !state.stopping && probe == state.probe {
        let available = if reply.is_null() {
            false
        } else {
            let wrapper = g_variant_get_child_value(reply, 0);
            let value = g_variant_get_variant(wrapper);
            let available = CStr::from_ptr(g_variant_get_type_string(value)) == c"b" && g_variant_get_boolean(value) != 0;
            release(value);
            release(wrapper);
            available
        };
        let was_available = state.available;
        state.set_available(available);
        if !available && !was_available { (state.send)(Action::Watcher(false)); }
    }
    release(reply);
    state.finish_call();
}

unsafe extern "C" fn wake(_: c_int, conditions: c_uint, data: Ptr) -> c_int {
    let state = &mut *data.cast::<Service>();
    let mut buffer = [0; 256];
    if conditions & 1 != 0 { let _ = state.reader.read(&mut buffer); }
    while let Ok(command) = state.rx.try_recv() {
        match command {
            Command::Enable if !state.stopping => state.enable(),
            Command::Update(wire) if !state.stopping => state.update(&wire),
            Command::Stop => state.stopping = true,
            _ => {}
        }
    }
    state.stopping |= conditions & (8 | 16 | 32) != 0;
    if state.stopping {
        if state.pending == 0 { g_main_loop_quit(state.loop_); }
        return 0;
    }
    1
}

unsafe fn run(reader: UnixStream, rx: Receiver<Command>, send: Sender) {
    let context = g_main_context_new();
    g_main_context_push_thread_default(context);
    let bus = g_bus_get_sync(2, null_mut(), null_mut()); // G_BUS_TYPE_SESSION
    if bus.is_null() {
        send(Action::Watcher(false));
    } else {
        g_dbus_connection_set_exit_on_close(bus, 0);
        run_service(context, bus, reader, rx, send);
        g_object_unref(bus);
    }
    g_main_context_pop_thread_default(context);
    g_main_context_unref(context);
}

unsafe fn run_service(context: Ptr, bus: Ptr, reader: UnixStream, rx: Receiver<Command>, send: Sender) {
    let xml = CString::new(include_str!("sni.xml")).expect("static D-Bus XML");
    let info = g_dbus_node_info_new_for_xml(xml.as_ptr(), null_mut());
    if info.is_null() { send(Action::Watcher(false)); return; }
    let loop_ = g_main_loop_new(context, 0);
    let reading = Reading::parse("-\t0\t-\t0\t0\tffb16b\t0\t0\tBattery status unavailable").expect("default reading");
    let mut state = Service {
        bus, loop_, reader, rx, send, owner: None, epoch: 0, probe: 0, pending: 0,
        available: false, active: false, registered: false, stopping: false,
        pixmap: pixmap(&reading.icon), reading, revision: 1,
    };
    let data = (&mut state as *mut Service).cast();
    static VTABLE: Vtable = Vtable { method: Some(method), get: Some(property), set: None, padding: [0; 8] };
    let register = |path: &CStr, interface: &CStr| {
        let interface = g_dbus_node_info_lookup_interface(info, interface.as_ptr());
        g_dbus_connection_register_object(bus, path.as_ptr(), interface, &VTABLE, data, None, null_mut())
    };
    let ids = [register(ITEM_PATH, ITEM), register(MENU_PATH, MENU)];
    if ids.contains(&0) {
        (state.send)(Action::Watcher(false));
    } else {
        let watcher = g_bus_watch_name_on_connection(bus, WATCHER.as_ptr(), 0, Some(appeared), Some(vanished), data, None);
        let signals = [c"StatusNotifierHostRegistered", c"StatusNotifierHostUnregistered"].map(|signal| {
            g_dbus_connection_signal_subscribe(bus, WATCHER.as_ptr(), WATCHER.as_ptr(), signal.as_ptr(),
                WATCHER_PATH.as_ptr(), null(), 0, Some(host_changed), data, None)
        });
        // GUnixFDSource dispatches a GUnixFDSourceFunc, not GSourceFunc.
        let callback = std::mem::transmute::<unsafe extern "C" fn(c_int, c_uint, Ptr) -> c_int, Callback>(wake);
        let source = g_unix_fd_source_new(state.reader.as_raw_fd(), 1 | 8 | 16 | 32);
        g_source_set_callback(source, Some(callback), data, None);
        g_source_attach(source, context);
        g_main_loop_run(loop_);
        g_source_destroy(source);
        g_source_unref(source);
        g_bus_unwatch_name(watcher);
        for id in signals { g_dbus_connection_signal_unsubscribe(bus, id); }
    }
    for id in ids.into_iter().filter(|id| *id != 0) { g_dbus_connection_unregister_object(bus, id); }
    g_dbus_node_info_unref(info);
    g_main_loop_unref(loop_);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, ChildStdin, Command as Process, Stdio};
    use std::sync::Arc;
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(8);

    struct Fixture(Child, ChildStdin, Receiver<String>);

    impl Fixture {
        fn send(&mut self, action: &str) {
            writeln!(self.1, "{action}").unwrap();
            assert_eq!(self.2.recv_timeout(WAIT).unwrap(), action);
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn call(destination: &str, path: &str, method: &str, args: &[&str]) -> String {
        let result = Process::new("gdbus")
            .args(["call", "--session", "--timeout", "4", "--dest", destination, "--object-path", path, "--method", method, "--"])
            .args(args).output().unwrap();
        assert!(result.status.success(), "{method}: {}", String::from_utf8_lossy(&result.stderr));
        String::from_utf8(result.stdout).unwrap()
    }

    fn registered_item(fixture: &Fixture) -> (String, String) {
        let registration = fixture.2.recv_timeout(WAIT).unwrap();
        let (name, path) = registration.split_once(' ').unwrap();
        assert!(name.starts_with(':'));
        assert_eq!(path, "/StatusNotifierItem");
        (name.into(), path.into())
    }

    fn fixture() -> Fixture {
        let mut child = Process::new("/usr/bin/python3")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/sni-watcher.py"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (log_tx, log_rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if log_tx.send(line).is_err() { break; }
            }
        });
        let fixture = Fixture(child, stdin, log_rx);
        assert_eq!(fixture.2.recv_timeout(WAIT).unwrap(), "started");
        fixture
    }

    #[test]
    fn isolated_sni_registration_and_recovery() {
        if std::env::var_os("VATRA_TEST_SNI").is_none() { return; }
        let mut fixture = fixture();
        let (tx, rx) = mpsc::channel();
        let worker = Worker::spawn(Arc::new(move |action| { let _ = tx.send(action); })).unwrap();

        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::Watcher(false)));
        fixture.send("acquire");
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::Watcher(false)));
        fixture.send("host-on");
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::Watcher(true)));
        worker.enable();
        let (name, path) = registered_item(&fixture);
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::SniReady(true)));

        let prop = |key| call(&name, &path, "org.freedesktop.DBus.Properties.Get", &["org.kde.StatusNotifierItem", key]);
        let before = prop("IconPixmap");
        assert!(before.contains("24, 24"));
        worker.update("68\t1\tbalanced\t1\t1\t80c1ff\t2\t1\tVatra: 68% charging".into());
        let mut updated = false;
        for _ in 0..30 {
            if prop("ToolTip").contains("68% charging") { updated = true; break; }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(updated);
        assert_ne!(prop("IconPixmap"), before);

        let layout = call(&name, "/MenuBar", "com.canonical.dbusmenu.GetLayout", &["0", "-1", "[]"]);
        assert!(layout.contains("Power Saver") && layout.contains("Start at login") && layout.contains("Quit Vatra"));
        let login = call(&name, "/MenuBar", "com.canonical.dbusmenu.GetProperty", &["8", "toggle-state"]);
        assert!(login.contains("<1>"));
        call(&name, &path, "org.kde.StatusNotifierItem.Activate", &["0", "0"]);
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::Open));
        call(&name, "/MenuBar", "com.canonical.dbusmenu.Event", &["4", "clicked", "<0>", "0"]);
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::Mode(Profile::Balanced)));
        call(&name, "/MenuBar", "com.canonical.dbusmenu.Event", &["8", "clicked", "<0>", "0"]);
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::Autostart(false)));

        fixture.send("host-off");
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::Watcher(false)));
        assert!(prop("Status").contains("Passive"));
        fixture.send("host-on");
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::Watcher(true)));
        worker.enable();
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::SniReady(true)));
        assert!(prop("Status").contains("Active"));
        drop(fixture);
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::Watcher(false)));
        let mut fixture = self::fixture();
        fixture.send("host-on");
        fixture.send("acquire");
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::Watcher(true)));
        worker.enable();
        assert_eq!(registered_item(&fixture), (name.clone(), path));
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::SniReady(true)));
        call(&name, "/MenuBar", "com.canonical.dbusmenu.Event", &["10", "clicked", "<0>", "0"]);
        assert!(matches!(rx.recv_timeout(WAIT).unwrap(), Action::Quit));
        drop(worker);
    }

    #[test]
    fn charging_badge_does_not_cover_any_percentage_glyph() {
        for level in 0..=100 {
            for percent in [1, 2] {
                let left = if percent == 2 { 9 } else { 0 };
                let right = if percent == 2 { 24 } else { 22 };
                let off = icon::draw(Some(level), false, [255, 177, 107], percent, 0);
                let on = icon::draw(Some(level), true, [255, 177, 107], percent, 0);
                assert_ne!(off, on);
                for y in 7..17 {
                    let row = y * 24 * 4;
                    assert_eq!(&off[row + left * 4..row + right * 4],
                               &on[row + left * 4..row + right * 4]);
                }
            }
        }
    }

    #[test]
    fn rendered_pixmap_changes_with_charge_and_appearance() {
        let low = icon::draw(Some(9), false, [255, 177, 107], 1, 0);
        let charging = icon::draw(Some(100), true, [128, 193, 255], 2, 1);
        assert_eq!(low.len(), 24 * 24 * 4);
        assert_ne!(low, charging);
        assert!(charging.chunks_exact(4).any(|pixel| pixel == [255, 128, 193, 255]));
    }
}
