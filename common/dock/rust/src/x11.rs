//! Safe X11 protocol client. Qt owns the surface; Rust owns X11 policy/events.
use crate::model::{Action, Task};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    os::fd::AsRawFd,
};
use x11rb::{
    connection::Connection,
    protocol::{
        shape::{self, ConnectionExt as _},
        xproto::{self, ConnectionExt as _},
        Event,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Rect {
    fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
    fn intersects(self, b: Self) -> bool {
        self.width > 0.
            && self.height > 0.
            && b.width > 0.
            && b.height > 0.
            && self.x < b.x + b.width
            && b.x < self.x + self.width
            && self.y < b.y + b.height
            && b.y < self.y + self.height
    }
    fn scaled(self, s: f64) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
            width: self.width * s,
            height: self.height * s,
        }
    }
}
#[derive(Serialize)]
pub struct Icon {
    pub wid: u32,
    pub theme: String,
    pub letter: String,
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u32>,
}
pub struct Backend {
    pub conn: RustConnection,
    root: u32,
    atoms: HashMap<String, u32>,
    pub window: u32,
    pub scale: f64,
    pub width: f64,
    pub height: f64,
    region: Vec<Rect>,
    area: Rect,
    pub hidden: bool,
    pub recovering: bool,
    pub inside: bool,
    pub edge: bool,
    pub desktop_visible: bool,
    pub obstructed: bool,
    pub tasks: Vec<Task>,
    dirty: HashSet<u32>,
    pub images: Vec<Icon>,
    revision: u64,
}
impl Backend {
    pub fn new() -> Result<Self> {
        let (conn, screen) = x11rb::connect(None)?;
        let root = conn.setup().roots[screen].root;
        conn.shape_query_version()?.reply()?;
        conn.change_window_attributes(
            root,
            &xproto::ChangeWindowAttributesAux::new().event_mask(
                xproto::EventMask::PROPERTY_CHANGE | xproto::EventMask::SUBSTRUCTURE_NOTIFY,
            ),
        )?
        .check()?;
        conn.flush()?;
        Ok(Self {
            conn,
            root,
            atoms: HashMap::new(),
            window: 0,
            scale: 1.,
            width: 0.,
            height: 0.,
            region: vec![],
            area: Rect::default(),
            hidden: false,
            recovering: false,
            inside: false,
            edge: false,
            desktop_visible: true,
            obstructed: false,
            tasks: vec![],
            dirty: HashSet::new(),
            images: vec![],
            revision: 0,
        })
    }
    pub fn fd(&self) -> i32 {
        self.conn.stream().as_raw_fd()
    }
    fn atom(&mut self, name: &str) -> Result<u32> {
        if let Some(a) = self.atoms.get(name) {
            return Ok(*a);
        }
        let a = self.conn.intern_atom(false, name.as_bytes())?.reply()?.atom;
        self.atoms.insert(name.into(), a);
        Ok(a)
    }
    fn bytes(&mut self, id: u32, name: &str, kind: u32, limit: u32) -> Vec<u8> {
        let Ok(atom) = self.atom(name) else {
            return vec![];
        };
        self.conn
            .get_property(false, id, atom, kind, 0, limit)
            .ok()
            .and_then(|c| c.reply().ok())
            .filter(|r| r.type_ == kind)
            .map(|r| r.value)
            .unwrap_or_default()
    }
    fn words(&mut self, id: u32, name: &str, kind: u32) -> Vec<u32> {
        self.bytes(id, name, kind, 4096)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_ne_bytes(*b))
            .collect()
    }
    fn text(&mut self, id: u32, name: &str, kind: u32) -> String {
        String::from_utf8_lossy(&self.bytes(id, name, kind, 4096))
            .trim_end_matches('\0')
            .into()
    }
    pub fn attach(&mut self, window: u32, scale: f64, width: f64, height: f64) -> Result<()> {
        self.window = window;
        self.dimensions(scale, width, height);
        self.conn
            .change_window_attributes(
                window,
                &xproto::ChangeWindowAttributesAux::new().event_mask(
                    xproto::EventMask::ENTER_WINDOW
                        | xproto::EventMask::LEAVE_WINDOW
                        | xproto::EventMask::POINTER_MOTION
                        | xproto::EventMask::STRUCTURE_NOTIFY,
                ),
            )?
            .check()?;
        let ty = self.atom("_NET_WM_WINDOW_TYPE")?;
        let dock = self.atom("_NET_WM_WINDOW_TYPE_DOCK")?;
        self.conn
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                ty,
                xproto::AtomEnum::ATOM,
                &[dock],
            )?
            .check()?;
        self.conn.flush()?;
        Ok(())
    }
    pub fn dimensions(&mut self, scale: f64, width: f64, height: f64) {
        self.scale = scale.max(0.1);
        self.width = width;
        self.height = height;
    }
    pub fn set_area(&mut self, area: Rect) -> bool {
        let area = area.scaled(self.scale);
        let changed = self.area != area;
        self.area = area;
        changed
    }
    pub fn shape(&mut self, region: Vec<Rect>) -> Result<()> {
        if self.window == 0 {
            return Ok(());
        }
        let region: Vec<_> = region
            .into_iter()
            .map(|r| Rect {
                x: r.x.floor(),
                y: r.y.floor(),
                width: (r.x + r.width).ceil() - r.x.floor(),
                height: (r.y + r.height).ceil() - r.y.floor(),
            })
            .collect();
        let scaled: Vec<_> = region.iter().map(|r| r.scaled(self.scale)).collect();
        let rects: Vec<_> = scaled
            .iter()
            .filter(|r| r.width > 0. && r.height > 0.)
            .map(|r| xproto::Rectangle {
                x: r.x.floor().clamp(-32768., 32767.) as i16,
                y: r.y.floor().clamp(-32768., 32767.) as i16,
                width: ((r.x + r.width).ceil() - r.x.floor()).clamp(0., 65535.) as u16,
                height: ((r.y + r.height).ceil() - r.y.floor()).clamp(0., 65535.) as u16,
            })
            .collect();
        self.region = region;
        self.conn
            .shape_rectangles(
                shape::SO::SET,
                shape::SK::INPUT,
                xproto::ClipOrdering::UNSORTED,
                self.window,
                0,
                0,
                &rects,
            )?
            .check()?;
        self.conn.flush()?;
        self.query_pointer()?;
        Ok(())
    }
    pub fn query_pointer(&mut self) -> Result<()> {
        if self.window == 0 {
            return Ok(());
        }
        let p = self.conn.query_pointer(self.window)?.reply()?;
        let x = p.win_x as f64 / self.scale;
        let y = p.win_y as f64 / self.scale;
        self.inside = p.same_screen
            && x >= 0.
            && y >= 0.
            && x < self.width
            && y < self.height
            && self.region.iter().any(|r| r.contains(x, y));
        self.edge = self.inside && y >= self.height - 3.;
        Ok(())
    }
    pub fn drain(&mut self) -> Result<bool> {
        let mut changed = false;
        while let Some(event) = self.conn.poll_for_event()? {
            match event {
                Event::EnterNotify(e) if e.event == self.window => {
                    if e.mode != xproto::NotifyMode::GRAB {
                        self.recovering = false;
                    }
                    self.query_pointer()?;
                }
                Event::LeaveNotify(e) if e.event == self.window => {
                    if e.mode != xproto::NotifyMode::GRAB {
                        self.recovering = false;
                    }
                    self.query_pointer()?;
                }
                Event::MotionNotify(e) if e.event == self.window => {
                    self.recovering = false;
                    self.query_pointer()?;
                }
                Event::ConfigureNotify(_)
                | Event::MapNotify(_)
                | Event::UnmapNotify(_)
                | Event::DestroyNotify(_) => {
                    changed = true;
                    if self.window != 0 {
                        self.query_pointer()?;
                    }
                }
                Event::PropertyNotify(e) => {
                    if [self.atom("_NET_WM_ICON")?, self.atom("WM_CLASS")?].contains(&e.atom) {
                        self.dirty.insert(e.window);
                    }
                    for name in [
                        "_NET_WM_PID",
                        "_NET_CLIENT_LIST",
                        "_NET_ACTIVE_WINDOW",
                        "_NET_WM_NAME",
                        "WM_NAME",
                        "_NET_WM_STATE",
                        "_NET_WM_WINDOW_TYPE",
                        "_NET_WM_ICON",
                        "WM_CLASS",
                        "_NET_SHOWING_DESKTOP",
                        "_NET_CURRENT_DESKTOP",
                        "_NET_WM_DESKTOP",
                        "WM_STATE",
                        "_NET_FRAME_EXTENTS",
                        "_GTK_APPLICATION_ID",
                        "_KDE_NET_WM_DESKTOP_FILE",
                    ] {
                        if self.atom(name)? == e.atom {
                            changed = true;
                            break;
                        }
                    }
                }
                _ => (),
            }
        }
        Ok(changed)
    }
    pub fn refresh(&mut self) -> Result<()> {
        let clients = self.words(
            self.root,
            "_NET_CLIENT_LIST",
            xproto::AtomEnum::WINDOW.into(),
        );
        let active = self
            .words(
                self.root,
                "_NET_ACTIVE_WINDOW",
                xproto::AtomEnum::WINDOW.into(),
            )
            .first()
            .copied()
            .unwrap_or(0);
        let current = self
            .words(
                self.root,
                "_NET_CURRENT_DESKTOP",
                xproto::AtomEnum::CARDINAL.into(),
            )
            .first()
            .copied()
            .unwrap_or(0);
        let showing = self
            .words(
                self.root,
                "_NET_SHOWING_DESKTOP",
                xproto::AtomEnum::CARDINAL.into(),
            )
            .first()
            .copied()
            .unwrap_or(0)
            != 0;
        let utf8 = self.atom("UTF8_STRING")?;
        let atom = u32::from(xproto::AtomEnum::ATOM);
        let cardinal = u32::from(xproto::AtomEnum::CARDINAL);
        let dock = self.atom("_NET_WM_WINDOW_TYPE_DOCK")?;
        let desktop = self.atom("_NET_WM_WINDOW_TYPE_DESKTOP")?;
        let normal = self.atom("_NET_WM_WINDOW_TYPE_NORMAL")?;
        let dialog = self.atom("_NET_WM_WINDOW_TYPE_DIALOG")?;
        let hidden = self.atom("_NET_WM_STATE_HIDDEN")?;
        let skip = self.atom("_NET_WM_STATE_SKIP_TASKBAR")?;
        let wm_state = self.atom("WM_STATE")?;
        let mut footprint = self.area;
        if self.window != 0 {
            if let Ok(origin) = self
                .conn
                .translate_coordinates(self.window, self.root, 0, 0)?
                .reply()
            {
                footprint.x += origin.dst_x as f64;
                footprint.y += origin.dst_y as f64;
            }
        }
        let mut visible = false;
        let mut obstructed = false;
        let mut tasks = vec![];
        for id in clients {
            // Settings/About/chooser surfaces belong to the Dock, not app tasks.
            if id == self.window
                || self.words(id, "_NET_WM_PID", cardinal).first() == Some(&std::process::id())
            {
                continue;
            }
            let _ = self.conn.change_window_attributes(
                id,
                &xproto::ChangeWindowAttributesAux::new().event_mask(
                    xproto::EventMask::PROPERTY_CHANGE | xproto::EventMask::STRUCTURE_NOTIFY,
                ),
            );
            let types = self.words(id, "_NET_WM_WINDOW_TYPE", atom);
            let states = self.words(id, "_NET_WM_STATE", atom);
            if types.contains(&dock) || types.contains(&desktop) {
                continue;
            }
            let workspace = self
                .words(id, "_NET_WM_DESKTOP", cardinal)
                .first()
                .copied()
                .unwrap_or(current);
            let iconic = self.words(id, "WM_STATE", wm_state).first() == Some(&3);
            if (types.is_empty() || types.contains(&normal) || types.contains(&dialog))
                && (workspace == current || workspace == u32::MAX)
                && !iconic
                && !states.contains(&hidden)
                && !states.contains(&skip)
            {
                let mapped = self
                    .conn
                    .get_window_attributes(id)?
                    .reply()
                    .is_ok_and(|a| a.map_state == xproto::MapState::VIEWABLE);
                visible |= mapped;
                if mapped && !obstructed {
                    let geometry = self.conn.get_geometry(id)?.reply();
                    let translated = self
                        .conn
                        .translate_coordinates(id, self.root, 0, 0)?
                        .reply();
                    if let (Ok(g), Ok(origin)) = (geometry, translated) {
                        let mut rect = Rect {
                            x: origin.dst_x as f64,
                            y: origin.dst_y as f64,
                            width: g.width as f64,
                            height: g.height as f64,
                        };
                        let frame = self.words(id, "_NET_FRAME_EXTENTS", cardinal);
                        if frame.len() == 4 {
                            rect.x -= frame[0] as f64;
                            rect.y -= frame[2] as f64;
                            rect.width += (frame[0] as f64) + (frame[1] as f64);
                            rect.height += (frame[2] as f64) + (frame[3] as f64);
                        }
                        obstructed = rect.intersects(footprint);
                    }
                }
            }
            if states.contains(&skip) {
                continue;
            }
            let mut title = self.text(id, "_NET_WM_NAME", utf8);
            if title.is_empty() {
                title = self.text(id, "WM_NAME", xproto::AtomEnum::STRING.into());
            }
            let classes = self.bytes(id, "WM_CLASS", xproto::AtomEnum::STRING.into(), 4096);
            let mut names = vec![];
            for c in classes.split(|c| *c == 0) {
                let name = String::from_utf8_lossy(c).trim().to_lowercase();
                if !name.is_empty() && !names.contains(&name) {
                    names.push(name);
                }
            }
            let key = names
                .last()
                .cloned()
                .unwrap_or("application-x-executable".into());
            let mut desktop_id = self.text(id, "_GTK_APPLICATION_ID", utf8);
            if desktop_id.is_empty() {
                desktop_id = self.text(id, "_KDE_NET_WM_DESKTOP_FILE", utf8);
            }
            let old = self.tasks.iter().find(|t| t.window_id == id);
            let reload = old.is_none_or(|t| t.icon_key != key) || self.dirty.contains(&id);
            let icon_url = if reload {
                self.revision += 1;
                self.load_icon(id, &key);
                format!("image://task/{id}/{}", self.revision)
            } else {
                old.unwrap().icon_url.clone()
            };
            tasks.push(Task {
                window_id: id,
                window_title: if title.is_empty() { key.clone() } else { title },
                icon_url,
                window_active: id == active,
                class_names: names,
                desktop_id,
                icon_key: key,
            });
        }
        self.tasks = tasks;
        self.obstructed = obstructed && !showing;
        self.desktop_visible = showing || !visible;
        self.dirty.clear();
        self.conn.flush()?;
        Ok(())
    }
    fn load_icon(&mut self, id: u32, key: &str) {
        let words: Vec<_> = self
            .bytes(
                id,
                "_NET_WM_ICON",
                xproto::AtomEnum::CARDINAL.into(),
                262144,
            )
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_ne_bytes(*b))
            .collect();
        let mut best = None;
        let mut i = 0;
        while i + 1 < words.len() {
            let w = words[i] as usize;
            let h = words[i + 1] as usize;
            if w == 0 || h == 0 || w > 1024 || h > 1024 || w * h > words.len() - i - 2 {
                break;
            }
            if best.is_none_or(|(_, bw, _)| w >= 96 && (bw < 96 || w < bw)) {
                best = Some((i, w, h));
            }
            i += w * h + 2;
        }
        let (width, height, pixels) = best
            .map(|(i, w, h)| (w as u16, h as u16, words[i + 2..i + 2 + w * h].to_vec()))
            .unwrap_or((0, 0, vec![]));
        self.images.push(Icon {
            wid: id,
            theme: key.into(),
            letter: key.chars().next().unwrap_or('?').to_uppercase().collect(),
            width,
            height,
            pixels,
        });
    }
    pub fn action(&mut self, action: Action) -> Result<()> {
        let (id, name, data) = match action {
            Action::Activate(id) => (id, "_NET_ACTIVE_WINDOW", [2, 0, 0, 0, 0]),
            Action::Minimize(id) => (id, "WM_CHANGE_STATE", [3, 0, 0, 0, 0]),
            Action::Close(id) => (id, "_NET_CLOSE_WINDOW", [0, 2, 0, 0, 0]),
        };
        if !self.tasks.iter().any(|t| t.window_id == id) {
            return Ok(());
        }
        let ty = self.atom(name)?;
        let event = xproto::ClientMessageEvent::new(32, id, ty, data);
        self.conn
            .send_event(
                false,
                self.root,
                xproto::EventMask::SUBSTRUCTURE_REDIRECT | xproto::EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )?
            .check()?;
        self.conn.flush()?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_footprint_overlap() {
        let a = Rect {
            x: 10.,
            y: 100.,
            width: 180.,
            height: 84.,
        };
        assert!(!a.intersects(Rect {
            x: 10.,
            y: 0.,
            width: 180.,
            height: 100.
        }));
        assert!(a.intersects(Rect {
            x: 10.,
            y: 99.,
            width: 180.,
            height: 10.
        }));
    }
}
