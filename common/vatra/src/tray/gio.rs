// Narrow GIO ABI used by the tray worker. All objects and callbacks live on
// its GLib main context. GVariant and GObject have different ownership APIs.
use std::ffi::{c_char, c_int, c_uint, c_void};

pub type Ptr = *mut c_void;
pub type Str = *const c_char;
// GLib represents a GVariantType with a pointer to its type string.
pub type VariantType = *const c_char;
pub type Callback = unsafe extern "C" fn(Ptr) -> c_int;
pub type Async = unsafe extern "C" fn(Ptr, Ptr, Ptr);
pub type NameAppeared = unsafe extern "C" fn(Ptr, Str, Str, Ptr);
pub type NameVanished = unsafe extern "C" fn(Ptr, Str, Ptr);
pub type Signal = unsafe extern "C" fn(Ptr, Str, Str, Str, Str, Ptr, Ptr);
pub type Method = unsafe extern "C" fn(Ptr, Str, Str, Str, Str, Ptr, Ptr, Ptr);
pub type Property = unsafe extern "C" fn(Ptr, Str, Str, Str, Str, *mut Ptr, Ptr) -> Ptr;
pub type SetProperty = unsafe extern "C" fn(Ptr, Str, Str, Str, Str, Ptr, *mut Ptr, Ptr) -> c_int;

#[repr(C)]
pub struct Vtable {
    pub method: Option<Method>,
    pub get: Option<Property>,
    pub set: Option<SetProperty>,
    pub padding: [usize; 8],
}

#[link(name = "gio-2.0")]
extern "C" {
    pub fn g_bus_get_sync(bus: c_int, cancellable: Ptr, error: *mut Ptr) -> Ptr;
    pub fn g_bus_watch_name_on_connection(bus: Ptr, name: Str, flags: c_uint, appeared: Option<NameAppeared>,
        vanished: Option<NameVanished>, data: Ptr, free: Option<unsafe extern "C" fn(Ptr)>) -> c_uint;
    pub fn g_bus_unwatch_name(id: c_uint);
    pub fn g_dbus_node_info_new_for_xml(xml: Str, error: *mut Ptr) -> Ptr;
    pub fn g_dbus_node_info_lookup_interface(info: Ptr, name: Str) -> Ptr;
    pub fn g_dbus_node_info_unref(info: Ptr);
    pub fn g_dbus_connection_set_exit_on_close(bus: Ptr, value: c_int);
    pub fn g_dbus_connection_register_object(bus: Ptr, path: Str, info: Ptr, table: *const Vtable, data: Ptr,
        free: Option<unsafe extern "C" fn(Ptr)>, error: *mut Ptr) -> c_uint;
    pub fn g_dbus_connection_unregister_object(bus: Ptr, id: c_uint) -> c_int;
    pub fn g_dbus_connection_call(bus: Ptr, name: Str, path: Str, interface: Str, method: Str,
        args: Ptr, reply: VariantType, flags: c_uint, timeout: c_int, cancel: Ptr, callback: Option<Async>, data: Ptr);
    pub fn g_dbus_connection_call_finish(bus: Ptr, result: Ptr, error: *mut Ptr) -> Ptr;
    pub fn g_dbus_connection_signal_subscribe(bus: Ptr, sender: Str, interface: Str, member: Str,
        path: Str, arg0: Str, flags: c_uint, callback: Option<Signal>, data: Ptr,
        free: Option<unsafe extern "C" fn(Ptr)>) -> c_uint;
    pub fn g_dbus_connection_signal_unsubscribe(bus: Ptr, id: c_uint);
    pub fn g_dbus_connection_emit_signal(bus: Ptr, destination: Str, path: Str, interface: Str, name: Str,
        args: Ptr, error: *mut Ptr) -> c_int;
    pub fn g_dbus_method_invocation_return_value(invocation: Ptr, value: Ptr);
    pub fn g_dbus_method_invocation_return_dbus_error(invocation: Ptr, name: Str, message: Str);
}
#[link(name = "gobject-2.0")]
extern "C" { pub fn g_object_unref(object: Ptr); }
#[link(name = "glib-2.0")]
extern "C" {
    pub fn g_main_context_new() -> Ptr;
    pub fn g_main_context_push_thread_default(context: Ptr);
    pub fn g_main_context_pop_thread_default(context: Ptr);
    pub fn g_main_context_unref(context: Ptr);
    pub fn g_main_loop_new(context: Ptr, running: c_int) -> Ptr;
    pub fn g_main_loop_run(loop_: Ptr);
    pub fn g_main_loop_quit(loop_: Ptr);
    pub fn g_main_loop_unref(loop_: Ptr);
    pub fn g_unix_fd_source_new(fd: c_int, conditions: c_uint) -> Ptr;
    pub fn g_source_set_callback(source: Ptr, callback: Option<Callback>, data: Ptr, free: Option<unsafe extern "C" fn(Ptr)>);
    pub fn g_source_attach(source: Ptr, context: Ptr) -> c_uint;
    pub fn g_source_destroy(source: Ptr);
    pub fn g_source_unref(source: Ptr);
    pub fn g_variant_parse(ty: VariantType, text: Str, limit: Str, end: *mut Str, error: *mut Ptr) -> Ptr;
    pub fn g_variant_get_child_value(value: Ptr, position: usize) -> Ptr;
    pub fn g_variant_n_children(value: Ptr) -> usize;
    pub fn g_variant_get_variant(value: Ptr) -> Ptr;
    pub fn g_variant_get_type_string(value: Ptr) -> Str;
    pub fn g_variant_get_boolean(value: Ptr) -> c_int;
    pub fn g_variant_get_int32(value: Ptr) -> i32;
    pub fn g_variant_get_string(value: Ptr, length: *mut usize) -> Str;
    pub fn g_variant_unref(value: Ptr);
}
