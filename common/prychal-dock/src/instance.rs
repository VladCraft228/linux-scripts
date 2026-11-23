//! Per-X11-screen single instance and small control protocol, without lock files.
use crate::Result;
use x11rb::{
    connection::Connection,
    protocol::{xproto::*, Event},
};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Command { Settings, Quit }
pub struct Instance {
    window: u32,
    selection: u32,
    message: u32,
}
impl Instance {
    /// Selection ownership lives on the X server and is released on disconnect.
    /// Grab the server only for this short claim/check transaction; never input.
    pub fn claim<C: Connection>(c: &C, root: u32, screen: usize, command: Command) -> Result<Option<Self>> {
        let selection = c.intern_atom(false, format!("_LINUX_SCRIPTS_DOCK_RUST_EDITION_S{screen}").as_bytes())?.reply()?.atom;
        let message = c.intern_atom(false, b"_LINUX_SCRIPTS_DOCK_RUST_CONTROL_V1")?.reply()?.atom;
        c.grab_server()?.check()?;
        let result = (|| -> Result<Option<Self>> {
            let owner = c.get_selection_owner(selection)?.reply()?.owner;
            if owner != 0 {
                c.send_event(false, owner, EventMask::NO_EVENT, ClientMessageEvent::new(
                    32, owner, message, [1, if command == Command::Quit { 2 } else { 1 }, 0, 0, 0],
                ))?.check()?;
                eprintln!("Prychal Dock is already running; control request sent.");
                return Ok(None);
            }
            if command == Command::Quit {
                eprintln!("Prychal Dock is not running on this X11 screen.");
                return Ok(None);
            }
            let window = c.generate_id()?;
            c.create_window(0, window, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0,
                &CreateWindowAux::new().override_redirect(1))?.check()?;
            c.set_selection_owner(window, selection, 0u32)?.check()?;
            if c.get_selection_owner(selection)?.reply()?.owner != window {
                return Err("Could not claim the Prychal Dock instance selection".into());
            }
            Ok(Some(Self { window, selection, message }))
        })();
        let release = c.ungrab_server();
        let flushed = c.flush();
        release?;
        flushed?;
        result
    }
    pub fn command(&self, event: &Event) -> Option<Command> {
        if let Event::ClientMessage(event) = event {
            if event.window == self.window && event.type_ == self.message && event.format == 32 {
                let data = event.data.as_data32();
                if data[0] == 1 {
                    return match data[1] { 1 => Some(Command::Settings), 2 => Some(Command::Quit), _ => None };
                }
            }
        }
        None
    }
    pub fn lost(&self, event: &Event) -> bool {
        matches!(event, Event::SelectionClear(event) if event.owner == self.window && event.selection == self.selection)
    }
}
