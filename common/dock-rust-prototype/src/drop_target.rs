//! Bounded, copy-only XDND receiver. The source keeps ownership of its files.
use crate::Result;
use std::{
    ffi::OsString,
    os::unix::ffi::OsStringExt,
    path::PathBuf,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{xproto::*, Event},
    wrapper::ConnectionExt as _,
};
const MAX_BYTES: usize = 64 * 1024;
const MAX_FILES: usize = 32;
struct Atoms {
    enter: u32,
    position: u32,
    leave: u32,
    drop: u32,
    status: u32,
    finished: u32,
    types: u32,
    selection: u32,
    uri: u32,
    copy: u32,
    property: u32,
}
struct Session {
    version: u32,
    source: u32,
    target: u32,
    offered: bool,
    accepted: bool,
    waiting: bool,
    time: u32,
    deadline: Instant,
}
pub struct DropTarget {
    a: Atoms,
    session: Option<Session>,
    window: u32,
    edge: u32,
}
impl DropTarget {
    pub fn new<C: Connection>(c: &C, window: u32, edge: u32) -> Result<Self> {
        let atom =
            |s: &str| -> Result<u32> { Ok(c.intern_atom(false, s.as_bytes())?.reply()?.atom) };
        let aware = atom("XdndAware")?;
        for w in [window, edge] {
            c.change_property32(PropMode::REPLACE, w, aware, AtomEnum::ATOM, &[5])?;
        }
        Ok(Self {
            a: Atoms {
                enter: atom("XdndEnter")?,
                position: atom("XdndPosition")?,
                leave: atom("XdndLeave")?,
                drop: atom("XdndDrop")?,
                status: atom("XdndStatus")?,
                finished: atom("XdndFinished")?,
                types: atom("XdndTypeList")?,
                selection: atom("XdndSelection")?,
                uri: atom("text/uri-list")?,
                copy: atom("XdndActionCopy")?,
                property: atom("_LINUX_SCRIPTS_DOCK_DROP")?,
            },
            session: None,
            window,
            edge,
        })
    }
    pub fn active(&self) -> bool {
        self.session.is_some()
    }
    pub fn accepting(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.offered)
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.session.as_ref().map(|s| s.deadline)
    }
    fn send<C: Connection>(&self, c: &C, source: u32, kind: u32, data: [u32; 5]) -> Result<()> {
        // A source can disappear mid-drag. Catch BadWindow here, not in the app loop.
        if let Ok(cookie) = c.send_event(
            false,
            source,
            EventMask::NO_EVENT,
            ClientMessageEvent::new(32, source, kind, data),
        ) {
            let _ = cookie.check();
        }
        c.flush()?;
        Ok(())
    }
    pub fn complete<C: Connection>(&mut self, c: &C, success: bool) -> Result<()> {
        if let Some(s) = self.session.take() {
            self.send(
                c,
                s.source,
                self.a.finished,
                [
                    s.target,
                    if s.version >= 5 {
                        u32::from(success)
                    } else {
                        0
                    },
                    if success && s.version >= 5 {
                        self.a.copy
                    } else {
                        0
                    },
                    0,
                    0,
                ],
            )?;
        }
        c.delete_property(self.window, self.a.property)?;
        c.flush()?;
        Ok(())
    }
    pub fn expire<C: Connection>(&mut self, c: &C, now: Instant) -> Result<bool> {
        if self.deadline().is_some_and(|t| now >= t) {
            if self.session.as_ref().is_some_and(|s| s.waiting) {
                self.complete(c, false)?;
            } else {
                self.session = None;
            }
            eprintln!("Desktop drop timed out");
            return Ok(true);
        }
        Ok(false)
    }
    pub fn handle<C: Connection>(&mut self, c: &C, event: &Event) -> Result<Option<Vec<PathBuf>>> {
        if let Event::DestroyNotify(e) = event {
            if self.session.as_ref().is_some_and(|s| s.source == e.window) {
                self.session = None;
            }
        }
        if let Event::ClientMessage(e) = event {
            if e.format != 32 || (e.window != self.window && e.window != self.edge) {
                return Ok(None);
            }
            let d = e.data.as_data32();
            if e.type_ == self.a.enter {
                if self.session.as_ref().is_some_and(|s| s.waiting) {
                    return Ok(None);
                }
                if self.session.as_ref().is_some_and(|s| s.source != d[0]) {
                    return Ok(None);
                }
                let version = d[1] >> 24;
                if !(3..=5).contains(&version) {
                    self.session = None;
                    return Ok(None);
                }
                let types = if d[1] & 1 != 0 {
                    c.get_property(false, d[0], self.a.types, AtomEnum::ATOM, 0, 64)?
                        .reply()
                        .ok()
                        .filter(|r| r.bytes_after == 0)
                        .and_then(|r| r.value32().map(|v| v.collect::<Vec<_>>()))
                        .unwrap_or_default()
                } else {
                    d[2..5].to_vec()
                };
                if let Ok(cookie) = c.get_window_attributes(d[0]) {
                    if let Ok(attributes) = cookie.reply() {
                        if let Ok(cookie) = c.change_window_attributes(
                            d[0],
                            &ChangeWindowAttributesAux::new().event_mask(
                                attributes.your_event_mask | EventMask::STRUCTURE_NOTIFY,
                            ),
                        ) {
                            let _ = cookie.check();
                        }
                    }
                }
                self.session = Some(Session {
                    version,
                    source: d[0],
                    target: e.window,
                    offered: types.contains(&self.a.uri),
                    accepted: false,
                    waiting: false,
                    time: 0,
                    deadline: Instant::now() + Duration::from_secs(30),
                });
            } else if self
                .session
                .as_ref()
                .is_some_and(|s| s.source == d[0] && s.target == e.window)
            {
                if e.type_ == self.a.position {
                    let s = self.session.as_mut().unwrap();
                    if s.waiting {
                        return Ok(None);
                    }
                    s.accepted = s.offered;
                    s.deadline = Instant::now() + Duration::from_secs(30);
                    let (source, target, accepted) = (s.source, s.target, s.accepted);
                    self.send(
                        c,
                        source,
                        self.a.status,
                        [
                            target,
                            if accepted { 3 } else { 2 },
                            0,
                            0,
                            if accepted { self.a.copy } else { 0 },
                        ],
                    )?;
                } else if e.type_ == self.a.leave {
                    if !self.session.as_ref().unwrap().waiting {
                        self.session = None;
                    }
                } else if e.type_ == self.a.drop {
                    let s = self.session.as_mut().unwrap();
                    if s.waiting {
                        return Ok(None);
                    }
                    if !s.accepted {
                        self.complete(c, false)?;
                        return Ok(None);
                    }
                    s.waiting = true;
                    s.time = d[2];
                    s.deadline = Instant::now() + Duration::from_secs(3);
                    c.delete_property(self.window, self.a.property)?;
                    c.convert_selection(
                        self.window,
                        self.a.selection,
                        self.a.uri,
                        self.a.property,
                        d[2],
                    )?;
                    c.flush()?;
                }
            }
        } else if let Event::SelectionNotify(e) = event {
            let expected = self
                .session
                .as_ref()
                .is_some_and(|s| s.waiting && (s.time == 0 || s.time == e.time));
            if !expected
                || e.requestor != self.window
                || e.selection != self.a.selection
                || e.target != self.a.uri
            {
                return Ok(None);
            }
            if e.property != self.a.property {
                self.complete(c, false)?;
                return Err("Source did not provide a readable desktop-file drop".into());
            }
            let reply = c
                .get_property(
                    false,
                    self.window,
                    self.a.property,
                    AtomEnum::ANY,
                    0,
                    (MAX_BYTES / 4 + 1) as u32,
                )?
                .reply()?;
            c.delete_property(self.window, self.a.property)?;
            // INCR is intentionally refused: launcher URI lists fit the fixed bound.
            if reply.type_ != self.a.uri
                || reply.format != 8
                || reply.bytes_after != 0
                || reply.value.len() > MAX_BYTES
            {
                self.complete(c, false)?;
                return Err("Drop is too large or uses an unsupported transfer format".into());
            }
            match local_files(&reply.value) {
                Ok(files) => return Ok(Some(files)),
                Err(e) => {
                    self.complete(c, false)?;
                    return Err(e);
                }
            }
        }
        Ok(None)
    }
}
fn local_files(data: &[u8]) -> Result<Vec<PathBuf>> {
    let text = std::str::from_utf8(data)?;
    let mut files = Vec::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let prefix = line.get(..7).ok_or("Expected a local file:// URI")?;
        if !prefix.eq_ignore_ascii_case("file://") || line.contains('?') || line.contains('#') {
            return Err("Only local file:// desktop launchers can be added".into());
        }
        let (authority, relative) = line[7..]
            .split_once('/')
            .ok_or("File URI has no absolute path")?;
        if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
            return Err("Remote file hosts are not supported".into());
        }
        let encoded = format!("/{relative}");
        let bytes = encoded.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'%'
                && !(bytes.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
                    && bytes.get(i + 2).is_some_and(u8::is_ascii_hexdigit))
            {
                return Err("Malformed percent escape in file URI".into());
            }
        }
        let decoded: Vec<u8> = percent_encoding::percent_decode_str(&encoded).collect();
        if decoded.contains(&0) {
            return Err("NUL byte in file URI".into());
        }
        let path = PathBuf::from(OsString::from_vec(decoded));
        if path.extension().is_none_or(|s| s != "desktop") {
            return Err(
                "Drop .desktop launchers, not applications, folders, or other files".into(),
            );
        }
        if !files.contains(&path) {
            files.push(path);
        }
        if files.len() > MAX_FILES {
            return Err("Drop at most 32 desktop files at once".into());
        }
    }
    if files.is_empty() {
        return Err("The drop contains no desktop launchers".into());
    }
    Ok(files)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_uri_lists() {
        let paths = local_files(
            b"# launchers\r\nfile:///tmp/My%20App.desktop\r\nfile:///tmp/My%20App.desktop\r\n",
        )
        .unwrap();
        assert_eq!(paths, vec![PathBuf::from("/tmp/My App.desktop")]);
        assert!(local_files(b"https://example.org/app.desktop").is_err());
        assert!(local_files(b"file://remote/tmp/app.desktop").is_err());
        assert!(local_files(b"file:///tmp/app.txt").is_err());
        assert!(local_files(b"file:///tmp/app.desktop?bad").is_err());
        assert!(local_files(b"file:///tmp/%ZZ.desktop").is_err());
        assert!(local_files(b"file:///tmp/%00.desktop").is_err());
        assert_eq!(
            local_files(b"file://localhost/tmp/a%23b.desktop").unwrap(),
            vec![PathBuf::from("/tmp/a#b.desktop")]
        );
    }
}
