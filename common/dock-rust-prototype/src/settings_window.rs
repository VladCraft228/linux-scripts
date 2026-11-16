//! A managed settings window with native XFWM decorations; no keyboard grab.
use crate::{
    colors, popup, rect,
    settings::{rgba, ColorTarget, Setting, Settings},
    Result,
};
use std::time::{Duration, Instant};
use tiny_skia::{Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};
use x11rb::{
    connection::Connection,
    protocol::{xproto::*, Event},
    wrapper::ConnectionExt as _,
};
const SIDEBAR: f32 = 164.;
const PRESETS: [u32; 8] = [
    0xffffff, 0, 0x272d39, 0x74c0ff, 0xf06f7f, 0xbd8aff, 0x48c9a5, 0xffc76a,
];
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Applications,
    Panel,
    Icons,
    Behavior,
    Indicators,
    Notifications,
    About,
}
#[derive(Clone, Copy)]
enum Toggle {
    Border,
    Hide,
    Previews,
    NotificationBadges,
    AttentionDots,
}
#[derive(Clone, Copy)]
enum Control {
    Search,
    ClearSearch,
    Application(usize),
    UnpinApplication(usize),
    Tab(Tab),
    Color(ColorTarget),
    Toggle(Toggle),
    Slider(Setting, u16, u16, u16),
    Style(u16),
    Wheel,
    Brightness,
    Preset(u32),
    Back,
    Link(&'static str),
    Reset,
    Close,
}
#[derive(Clone, Copy)]
struct Area {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}
impl Area {
    fn has(self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}
#[derive(Clone, Copy)]
struct Hit {
    area: Area,
    control: Control,
}
#[derive(Clone, Copy)]
struct Drag {
    control: Control,
    area: Area,
}
struct Picker {
    target: ColorTarget,
    h: f32,
    s: f32,
    v: f32,
    rgb: u32,
}
impl Picker {
    fn new(target: ColorTarget, rgb: u32) -> Self {
        let (h, s, v) = colors::from_rgb(rgb);
        Self {
            target,
            h,
            s,
            v,
            rgb,
        }
    }
}
pub enum Change {
    BrowseApplications,
    PinApplication(String),
    UnpinApplication(String),
    Preview(Settings),
    Commit(Settings),
    Cancel,
    Close,
}
pub struct SettingsWindow {
    pub window: u32,
    pub visible: bool,
    root: u32,
    gc: u32,
    width: u16,
    height: u16,
    tab: Tab,
    browser: crate::applications::Browser,
    search_active: bool,
    current: Settings,
    hits: Vec<Hit>,
    focus: Option<usize>,
    pointer: Option<(f32, f32)>,
    cursors: [u32; 5],
    cursor_index: usize,
    color: Option<Picker>,
    wheel: Option<Pixmap>,
    drag: Option<Drag>,
    last_emit: Instant,
    protocols: u32,
    delete: u32,
    activate: u32,
    moveresize: u32,
    normal_hints: u32,
    size_hints: u32,
    frame_extents: u32,
    center_on_map: bool,
    center_on_frame: bool,
    escape: u8,
    tab_key: u8,
    enter: u8,
    space: u8,
    left: u8,
    right: u8,
    up: u8,
    down: u8,
}
impl SettingsWindow {
    pub fn new<C: Connection>(
        c: &C,
        root: u32,
        visual: u32,
        colormap: u32,
        screen: (u16, u16),
    ) -> Result<Self> {
        crate::keyboard::initialize(c)?;
        let window = c.generate_id()?;
        let gc = c.generate_id()?;
        let width = 820u16.min(screen.0.saturating_sub(60)).max(700);
        let height = 640u16.min(screen.1.saturating_sub(100)).max(540);
        c.create_window(
            32,
            window,
            root,
            (screen.0.saturating_sub(width) / 2) as i16,
            (screen.1.saturating_sub(height) / 2) as i16,
            width,
            height,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new()
                .background_pixel(0xff181d27u32)
                .border_pixel(0)
                .colormap(colormap)
                .event_mask(
                    EventMask::EXPOSURE
                        | EventMask::STRUCTURE_NOTIFY
                        | EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE
                        | EventMask::POINTER_MOTION
                        | EventMask::KEY_PRESS
                        | EventMask::FOCUS_CHANGE
                        | EventMask::PROPERTY_CHANGE
                        | EventMask::ENTER_WINDOW
                        | EventMask::LEAVE_WINDOW,
                ),
        )?
        .check()?;
        c.create_gc(gc, window, &CreateGCAux::new())?.check()?;
        crate::icons::set_dock_icon(c, window)?;
        let atom =
            |s: &str| -> Result<u32> { Ok(c.intern_atom(false, s.as_bytes())?.reply()?.atom) };
        let protocols = atom("WM_PROTOCOLS")?;
        let delete = atom("WM_DELETE_WINDOW")?;
        c.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"Prychal Dock - Settings & About",
        )?;
        c.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"prychal-dock-settings\0prychal-dock\0",
        )?;
        c.change_property32(
            PropMode::REPLACE,
            window,
            protocols,
            AtomEnum::ATOM,
            &[delete],
        )?;
        c.change_property32(
            PropMode::REPLACE,
            window,
            atom("_NET_WM_PID")?,
            AtomEnum::CARDINAL,
            &[std::process::id()],
        )?;
        c.change_property32(
            PropMode::REPLACE,
            window,
            atom("_NET_WM_WINDOW_TYPE")?,
            AtomEnum::ATOM,
            &[atom("_NET_WM_WINDOW_TYPE_NORMAL")?],
        )?;
        let mut hints = [0u32; 18];
        hints[0] = 1 << 4;
        hints[5] = 700;
        hints[6] = 540;
        c.change_property32(
            PropMode::REPLACE,
            window,
            atom("WM_NORMAL_HINTS")?,
            atom("WM_SIZE_HINTS")?,
            &hints,
        )?;
        let min = c.setup().min_keycode;
        let max = c.setup().max_keycode;
        let keys = c.get_keyboard_mapping(min, max - min + 1)?.reply()?;
        let key = |symbol: u32| {
            keys.keysyms
                .chunks(keys.keysyms_per_keycode as usize)
                .position(|row| row.contains(&symbol))
                .map(|i| min + i as u8)
                .unwrap_or(0)
        };
        // X11 standard cursors: inherit/default, hand, horizontal resize, crosshair.
        let cursor_font = c.generate_id()?;
        c.open_font(cursor_font, b"cursor")?.check()?;
        let mut cursors = [0; 5];
        for (index, glyph) in [(1, 60u16), (2, 108u16), (3, 34u16), (4, 152u16)] {
            let cursor = c.generate_id()?;
            c.create_glyph_cursor(cursor, cursor_font, cursor_font, glyph, glyph + 1, 0, 0, 0, 65535, 65535, 65535)?.check()?;
            cursors[index] = cursor;
        }
        c.close_font(cursor_font)?;
        Ok(Self {
            window,
            visible: false,
            root,
            gc,
            width,
            height,
            tab: Tab::Panel,
            browser: crate::applications::Browser::default(),
            search_active: false,
            current: Settings::default(),
            hits: vec![],
            focus: None,
            pointer: None,
            cursors,
            cursor_index: 0,
            color: None,
            wheel: None,
            drag: None,
            last_emit: Instant::now(),
            protocols,
            delete,
            activate: atom("_NET_ACTIVE_WINDOW")?,
            moveresize: atom("_NET_MOVERESIZE_WINDOW")?,
            normal_hints: atom("WM_NORMAL_HINTS")?,
            size_hints: atom("WM_SIZE_HINTS")?,
            frame_extents: atom("_NET_FRAME_EXTENTS")?,
            center_on_map: false,
            center_on_frame: false,
            escape: key(0xff1b),
            tab_key: key(0xff09),
            enter: key(0xff0d),
            space: key(0x20),
            left: key(0xff51),
            right: key(0xff53),
            up: key(0xff52),
            down: key(0xff54),
        })
    }
    pub fn show<C: Connection>(
        &mut self,
        c: &C,
        font: &fontdue::Font,
        settings: Settings,
        time: u32,
    ) -> Result<()> {
        self.current = settings;
        let framed = self.center(c, true)?;
        self.center_on_map = !framed;
        self.center_on_frame = !framed;
        c.map_window(self.window)?;
        self.visible = true;
        self.draw(c, font)?;
        c.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ClientMessageEvent::new(32, self.window, self.activate, [2, time, 0, 0, 0]),
        )?
        .check()?;
        c.flush()?;
        Ok(())
    }
    fn margins<C: Connection>(&self, c: &C) -> Vec<u32> {
        c.get_property(
            false,
            self.window,
            self.frame_extents,
            AtomEnum::CARDINAL,
            0,
            4,
        )
        .ok()
        .and_then(|r| r.reply().ok())
        .and_then(|r| r.value32().map(|v| v.map(|n| n.min(16384)).collect()))
        .unwrap_or_default()
    }
    fn center<C: Connection>(&self, c: &C, hints: bool) -> Result<bool> {
        let screen = c.get_geometry(self.root)?.reply()?;
        let client = c.get_geometry(self.window)?.reply()?;
        let margins = self.margins(c);
        let framed = margins.len() == 4;
        let edge = |i: usize| margins.get(i).copied().unwrap_or(0);
        let position = center_position(
            (screen.width, screen.height),
            (client.width, client.height),
            [edge(0), edge(1), edge(2), edge(3)],
        );
        if hints {
            // USPosition and StaticGravity prevent XFWM smart placement overriding this request.
            let mut values = [0u32; 18];
            values[0] = (1 << 0) | (1 << 4) | (1 << 9);
            values[1] = position.0 as u32;
            values[2] = position.1 as u32;
            values[3] = client.width as u32;
            values[4] = client.height as u32;
            values[5] = 700;
            values[6] = 540;
            values[17] = 10;
            c.change_property32(
                PropMode::REPLACE,
                self.window,
                self.normal_hints,
                self.size_hints,
                &values,
            )?;
            c.configure_window(
                self.window,
                &ConfigureWindowAux::new().x(position.0).y(position.1),
            )?
            .check()?;
        }
        // The pager request also works after reparenting or when raising an existing window.
        let flags = 10 | (1 << 8) | (1 << 9) | (2 << 12);
        c.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ClientMessageEvent::new(
                32,
                self.window,
                self.moveresize,
                [flags, position.0 as u32, position.1 as u32, 0, 0],
            ),
        )?
        .check()?;
        c.flush()?;
        Ok(framed)
    }
    pub fn sync<C: Connection>(
        &mut self,
        c: &C,
        font: &fontdue::Font,
        settings: Settings,
    ) -> Result<()> {
        self.current = settings;
        if let Some(p) = self.color.as_mut() {
            let rgb = settings.color(p.target);
            if p.rgb != rgb {
                *p = Picker::new(p.target, rgb);
            }
        }
        if self.visible {
            self.draw(c, font)?;
        }
        Ok(())
    }
    pub fn show_settings<C: Connection>(
        &mut self,
        c: &C,
        font: &fontdue::Font,
        settings: Settings,
        time: u32,
    ) -> Result<()> {
        if matches!(self.tab, Tab::About | Tab::Applications) {
            self.tab = Tab::Panel;
            self.color = None;
            self.focus = None;
        }
        self.show(c, font, settings, time)
    }
    pub fn show_applications<C: Connection>(&mut self, c: &C, font: &fontdue::Font, settings: Settings, items: Vec<crate::applications::Application>, time: u32) -> Result<()> {
        self.release_pointer(c)?;
        self.tab = Tab::Applications;
        self.browser = crate::applications::Browser::new(items);
        self.search_active = true;
        self.color = None;
        self.focus = None;
        self.show(c, font, settings, time)
    }
    pub fn application_result(&mut self, key: &str, result: std::result::Result<bool, String>) {
        match result {
            Ok(added) => self.browser.pinned(key, added),
            Err(error) => self.browser.message = format!("Could not pin: {error}"),
        }
    }
    pub fn application_unpin_result(&mut self, key: &str, result: std::result::Result<bool, String>) {
        match result {
            Ok(removed) => {
                if let Some(item) = self.browser.items.iter_mut().find(|item| item.key == key) {
                    item.pinned = false;
                    self.browser.message = if removed { format!("Unpinned {} from Dock", item.name) } else { "Application is no longer pinned".into() };
                }
            }
            Err(error) => self.browser.message = format!("Could not unpin: {error}"),
        }
    }
    pub fn sync_application_pins(&mut self, applications: &[crate::applications::Application]) {
        for item in &mut self.browser.items {
            if let Some(current) = applications.iter().find(|current| current.key == item.key) { item.pinned = current.pinned; }
        }
    }
    fn application_page_size(&self) -> usize {
        ((self.height as usize).saturating_sub(300) / 44).max(1)
    }
    fn search_key<C: Connection>(&mut self, c: &C, font: &fontdue::Font, event: &KeyPressEvent) -> Result<Option<Change>> {
        let page = self.application_page_size();
        if event.detail == self.enter {
            return Ok(self.browser.selected_key().map(Change::PinApplication));
        }
        if event.detail == self.up || event.detail == self.down {
            self.browser.move_selection(if event.detail == self.up { -1 } else { 1 }, page);
        } else {
            let symbols = crate::keyboard::symbols(c, event.detail, u16::from(event.state))?;
            let symbol = symbols.plain;
            if symbol == 0xff08 {
                self.browser.query.pop();
                self.browser.changed();
            } else if event.state.contains(KeyButMask::CONTROL) {
                if crate::applications::character(symbols.shortcut).is_some_and(|c| c == 'u' || c == 'U') {
                    self.browser.query.clear(); self.browser.changed();
                }
            } else if !event.state.contains(KeyButMask::MOD1) {
                let caps = event.state.contains(KeyButMask::LOCK) && crate::applications::character(symbol).is_some_and(char::is_alphabetic);
                let shifted = event.state.contains(KeyButMask::SHIFT) ^ caps;
                let symbol = if shifted { symbols.shifted } else { symbols.plain };
                if let Some(ch) = crate::applications::character(symbol) {
                    if self.browser.query.chars().count() < 128 {
                        self.browser.query.push(ch); self.browser.changed();
                    }
                }
            }
        }
        self.draw(c, font)?;
        Ok(None)
    }
    pub fn show_about<C: Connection>(
        &mut self,
        c: &C,
        font: &fontdue::Font,
        settings: Settings,
        time: u32,
    ) -> Result<()> {
        self.release_pointer(c)?;
        self.tab = Tab::About;
        self.color = None;
        self.focus = None;
        self.show(c, font, settings, time)
    }
    fn release_pointer<C: Connection>(&mut self, c: &C) -> Result<()> {
        if self.drag.take().is_some() {
            c.ungrab_pointer(0u32)?;
            c.flush()?;
        }
        Ok(())
    }
    pub fn cancel_drag<C: Connection>(&mut self, c: &C) -> Result<()> {
        self.release_pointer(c)
    }
    pub fn hide<C: Connection>(&mut self, c: &C) -> Result<()> {
        self.release_pointer(c)?;
        self.visible = false;
        self.pointer = None;
        self.color = None;
        self.focus = None;
        c.unmap_window(self.window)?;
        c.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            UnmapNotifyEvent {
                response_type: UNMAP_NOTIFY_EVENT,
                sequence: 0,
                event: self.root,
                window: self.window,
                from_configure: false,
            },
        )?;
        c.flush()?;
        Ok(())
    }
    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        self.hits.iter().rposition(|hit| hit.area.has(x, y))
    }
    fn get(&self, setting: Setting) -> u16 {
        match setting {
            Setting::IconSize => self.current.icon_size,
            Setting::Spacing => self.current.spacing,
            Setting::Magnification => self.current.magnification,
            Setting::Opacity => self.current.opacity,
            Setting::HideDelay => self.current.hide_delay,
            Setting::Animation => self.current.animation_ms,
            Setting::BorderWidth => self.current.border_width,
            Setting::BorderOpacity => self.current.border_opacity,
            Setting::CornerRadius => self.current.corner_radius,
            Setting::IndicatorStyle => self.current.indicator_style,
            Setting::NotificationSize => self.current.notification_size,
            Setting::AttentionSize => self.current.attention_size,
        }
    }
    fn set_numeric(&mut self, setting: Setting, value: u16) {
        let delta = value as i32 - self.get(setting) as i32;
        self.current.adjust(setting, delta);
    }
    fn update_drag(&mut self, x: f32, y: f32) {
        let Some(d) = self.drag else { return };
        match d.control {
            Control::Slider(setting, min, max, step) => {
                let t = ((x - d.area.x - 16.) / (d.area.w - 32.)).clamp(0., 1.);
                let raw = min as f32 + t * (max - min) as f32;
                let value = ((raw - min as f32) / step as f32).round() as u16 * step + min;
                self.set_numeric(setting, value.clamp(min, max));
            }
            Control::Wheel | Control::Brightness => {
                if let Some(p) = self.color.as_mut() {
                    if matches!(d.control, Control::Wheel) {
                        let dx = x - (d.area.x + d.area.w / 2.);
                        let dy = y - (d.area.y + d.area.h / 2.);
                        p.h = (dy.atan2(dx) / std::f32::consts::TAU).rem_euclid(1.);
                        p.s = (dx.hypot(dy) / (d.area.w / 2.)).min(1.);
                    } else {
                        p.v = ((x - d.area.x) / d.area.w).clamp(0., 1.);
                    }
                    p.rgb = colors::hsv(p.h, p.s, p.v);
                    self.current.set_color(p.target, p.rgb);
                }
            }
            _ => {}
        }
    }
    fn activate<C: Connection>(
        &mut self,
        c: &C,
        font: &fontdue::Font,
        hit: Hit,
        x: f32,
        y: f32,
        time: u32,
    ) -> Result<Option<Change>> {
        match hit.control {
            Control::Search => { self.search_active = true; self.draw(c, font)?; }
            Control::ClearSearch => { self.browser.query.clear(); self.browser.changed(); self.search_active = true; self.draw(c, font)?; }
            Control::UnpinApplication(index) => {
                if let Some(&item) = self.browser.filtered().get(index) {
                    return Ok(Some(Change::UnpinApplication(self.browser.items[item].key.clone())));
                }
            }
            Control::Application(index) => {
                self.browser.selected = index;
                self.search_active = true;
                if let Some(key) = self.browser.selected_key() { return Ok(Some(Change::PinApplication(key))); }
                self.browser.message = "Already pinned to Dock".into();
                self.draw(c, font)?;
            }
            Control::Tab(tab) => {
                if tab == Tab::Applications { return Ok(Some(Change::BrowseApplications)); }
                self.search_active = false;
                self.tab = tab;
                self.color = None;
                self.focus = None;
                self.draw(c, font)?;
            }
            Control::Color(target) => {
                self.color = Some(Picker::new(target, self.current.color(target)));
                if self.wheel.is_none() {
                    self.wheel = Some(colors::wheel(110));
                }
                self.focus = None;
                self.draw(c, font)?;
            }
            Control::Back => {
                self.color = None;
                self.focus = None;
                self.draw(c, font)?;
            }
            Control::Toggle(toggle) => {
                match toggle {
                    Toggle::Border => self.current.border_enabled = !self.current.border_enabled,
                    Toggle::Hide => self.current.auto_hide = !self.current.auto_hide,
                    Toggle::Previews => self.current.previews = !self.current.previews,
                    Toggle::NotificationBadges => self.current.notification_badges = !self.current.notification_badges,
                    Toggle::AttentionDots => self.current.attention_dots = !self.current.attention_dots,
                }
                return Ok(Some(Change::Commit(self.current)));
            }
            Control::Style(style) => {
                self.current.indicator_style = style;
                return Ok(Some(Change::Commit(self.current)));
            }
            Control::Preset(rgb) => {
                if let Some(p) = self.color.as_mut() {
                    let target = p.target;
                    *p = Picker::new(target, rgb);
                    self.current.set_color(target, rgb);
                }
                return Ok(Some(Change::Commit(self.current)));
            }
            Control::Link(url) => {
                // Fixed URLs, no shell; wait off the UI thread to reap the helper.
                std::thread::spawn(move || {
                    match std::process::Command::new("xdg-open").arg(url).status() {
                        Ok(status) if status.success() => {}
                        Ok(status) => eprintln!("Open link: {status}"),
                        Err(error) => eprintln!("Open link: {error}"),
                    }
                });
            }
            Control::Reset => {
                self.current = Settings::default();
                if let Some(p) = self.color.as_mut() {
                    *p = Picker::new(p.target, self.current.color(p.target));
                }
                return Ok(Some(Change::Commit(self.current)));
            }
            Control::Close => return Ok(Some(Change::Close)),
            Control::Slider(..) | Control::Wheel | Control::Brightness => {
                if matches!(hit.control, Control::Slider(..)) && y < hit.area.y + 26. {
                    return Ok(None);
                }
                if matches!(hit.control, Control::Wheel)
                    && (x - hit.area.x - hit.area.w / 2.).hypot(y - hit.area.y - hit.area.h / 2.)
                        > hit.area.w / 2.
                {
                    return Ok(None);
                }
                let grabbed = c
                    .grab_pointer(
                        false,
                        self.window,
                        EventMask::POINTER_MOTION | EventMask::BUTTON_RELEASE,
                        GrabMode::ASYNC,
                        GrabMode::ASYNC,
                        0u32,
                        0u32,
                        time,
                    )?
                    .reply()?;
                if grabbed.status != GrabStatus::SUCCESS {
                    return Ok(None);
                }
                self.drag = Some(Drag {
                    control: hit.control,
                    area: hit.area,
                });
                self.update_drag(x, y);
                self.last_emit = Instant::now();
                return Ok(Some(Change::Preview(self.current)));
            }
        }
        Ok(None)
    }
    pub fn handle<C: Connection>(
        &mut self,
        c: &C,
        font: &fontdue::Font,
        event: &Event,
    ) -> Result<(bool, Option<Change>)> {
        let result = match event {
            Event::ClientMessage(e)
                if e.window == self.window
                    && e.format == 32
                    && e.type_ == self.protocols
                    && e.data.as_data32()[0] == self.delete =>
            {
                Some(Change::Close)
            }
            Event::Expose(e) if e.window == self.window => {
                if self.visible {
                    self.draw(c, font)?;
                }
                None
            }
            Event::PropertyNotify(e) if e.window == self.window && e.atom == self.frame_extents => {
                if self.center_on_frame && self.center(c, false)? {
                    self.center_on_frame = false;
                }
                None
            }
            Event::ConfigureNotify(e) if e.window == self.window => {
                if self.center_on_frame && self.margins(c).len() == 4 {
                    self.center_on_frame = false;
                    self.center(c, false)?;
                }
                if self.width != e.width || self.height != e.height {
                    let cancel = self.drag.is_some();
                    self.release_pointer(c)?;
                    self.width = e.width.max(1);
                    self.height = e.height.max(1);
                    if self.visible {
                        self.draw(c, font)?;
                    }
                    if cancel {
                        Some(Change::Cancel)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Event::UnmapNotify(e) if e.window == self.window => {
                self.visible = false;
                let cancel = self.drag.is_some();
                self.release_pointer(c)?;
                if cancel {
                    Some(Change::Cancel)
                } else {
                    None
                }
            }
            Event::MapNotify(e) if e.window == self.window => {
                self.visible = true;
                if self.center_on_map {
                    self.center_on_map = false;
                    if self.center(c, false)? {
                        self.center_on_frame = false;
                    }
                }
                self.draw(c, font)?;
                None
            }
            Event::ButtonPress(e) if e.event == self.window => {
                if self.tab == Tab::Applications && e.event_x as f32 >= SIDEBAR && e.event_y >= 210 && (e.detail == 4 || e.detail == 5) {
                    self.browser.scroll(if e.detail == 4 { -3 } else { 3 }, self.application_page_size());
                    self.draw(c, font)?;
                    return Ok((true, None));
                }
                if !self.visible {
                    return Ok((true, None));
                }
                let point = (e.event_x as f32, e.event_y as f32);
                self.pointer = Some(point);
                if let Some(i) = self.hit(point.0, point.1) {
                    self.focus = Some(i);
                    let hit = self.hits[i];
                    self.search_active = matches!(hit.control, Control::Search | Control::ClearSearch | Control::Application(_) | Control::UnpinApplication(_));
                    if e.detail == 1 {
                        self.activate(c, font, hit, point.0, point.1, e.time)?
                    } else if e.detail == 4 || e.detail == 5 {
                        if let Control::Slider(setting, _, _, step) = hit.control {
                            self.current.adjust(
                                setting,
                                if e.detail == 4 {
                                    step as i32
                                } else {
                                    -(step as i32)
                                },
                            );
                            Some(Change::Commit(self.current))
                        } else if matches!(hit.control, Control::Brightness | Control::Wheel) {
                            if let Some(p) = self.color.as_mut() {
                                p.v = (p.v + if e.detail == 4 { 0.025 } else { -0.025 })
                                    .clamp(0., 1.);
                                p.rgb = colors::hsv(p.h, p.s, p.v);
                                self.current.set_color(p.target, p.rgb);
                            }
                            Some(Change::Commit(self.current))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Event::EnterNotify(e) if e.event == self.window => {
                self.pointer = Some((e.event_x as f32, e.event_y as f32));
                self.draw(c, font)?;
                None
            }
            Event::LeaveNotify(e) if e.event == self.window => {
                if self.drag.is_none() {
                    self.pointer = None;
                    self.draw(c, font)?;
                }
                None
            }
            Event::MotionNotify(e) if e.event == self.window => {
                let previous = self.pointer.and_then(|(x,y)|self.hover_hit(x,y));
                self.pointer = Some((e.event_x as f32, e.event_y as f32));
                let next = self.pointer.and_then(|(x,y)|self.hover_hit(x,y));
                if self.drag.is_none() && previous != next { self.draw(c,font)?; }
                if self.drag.is_some() {
                    self.update_drag(e.event_x as f32, e.event_y as f32);
                    if self.last_emit.elapsed() >= Duration::from_millis(16) {
                        self.last_emit = Instant::now();
                        Some(Change::Preview(self.current))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Event::ButtonRelease(e) if e.event == self.window => {
                if e.detail == 1 && self.drag.is_some() {
                    self.update_drag(e.event_x as f32, e.event_y as f32);
                    self.release_pointer(c)?;
                    Some(Change::Commit(self.current))
                } else {
                    None
                }
            }
            Event::FocusOut(e) if e.event == self.window => {
                if self.drag.is_some() && e.mode == NotifyMode::NORMAL {
                    self.release_pointer(c)?;
                    Some(Change::Cancel)
                } else {
                    None
                }
            }
            Event::KeyPress(e) if e.event == self.window => {
                if self.drag.is_some() && e.detail != self.escape {
                    return Ok((true, None));
                }
                if !self.visible {
                    return Ok((true, None));
                }
                if self.tab == Tab::Applications && self.search_active && e.detail != self.escape && e.detail != self.tab_key {
                    return Ok((true, self.search_key(c, font, e)?));
                }
                if e.detail == self.escape {
                    if self.drag.is_some() {
                        self.release_pointer(c)?;
                        Some(Change::Cancel)
                    } else if self.color.is_some() {
                        self.color = None;
                        self.focus = None;
                        self.draw(c, font)?;
                        None
                    } else {
                        Some(Change::Close)
                    }
                } else if e.detail == self.tab_key {
                    self.search_active = false;
                    if !self.hits.is_empty() {
                        let reverse = e.state.contains(KeyButMask::SHIFT);
                        self.focus = Some(match self.focus {
                            Some(i) if reverse => (i + self.hits.len() - 1) % self.hits.len(),
                            Some(i) => (i + 1) % self.hits.len(),
                            None => 0,
                        });
                        self.draw(c, font)?;
                    }
                    None
                } else if let Some(hit) = self.focus.and_then(|i| self.hits.get(i)).copied() {
                    if e.detail == self.left
                        || e.detail == self.right
                        || e.detail == self.up
                        || e.detail == self.down
                    {
                        if let Control::Slider(setting, _, _, step) = hit.control {
                            let positive = e.detail == self.right || e.detail == self.up;
                            self.current.adjust(
                                setting,
                                if positive {
                                    step as i32
                                } else {
                                    -(step as i32)
                                },
                            );
                            Some(Change::Commit(self.current))
                        } else {
                            None
                        }
                    } else if e.detail == self.enter || e.detail == self.space {
                        if matches!(
                            hit.control,
                            Control::Slider(..) | Control::Wheel | Control::Brightness
                        ) {
                            None
                        } else {
                            self.activate(
                                c,
                                font,
                                hit,
                                hit.area.x + hit.area.w / 2.,
                                hit.area.y + hit.area.h / 2.,
                                e.time,
                            )?
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => return Ok((false, None)),
        };
        Ok((true, result))
    }
    fn hover_hit(&self, x:f32, y:f32) -> Option<usize> {
        self.hit(x,y).filter(|&index| {
            let hit=&self.hits[index];
            if matches!(hit.control, Control::Wheel) {
                (x-hit.area.x-hit.area.w/2.).hypot(y-hit.area.y-hit.area.h/2.) <= hit.area.w/2.
            } else { true }
        })
    }
    pub fn draw<C: Connection>(&mut self, c: &C, font: &fontdue::Font) -> Result<()> {
        let mut image = Pixmap::new(self.width as u32, self.height as u32)
            .ok_or("Settings allocation failed")?;
        image.fill(tiny_skia::Color::from_rgba8(24, 29, 39, 255));
        self.hits.clear();
        let w = self.width as f32;
        let h = self.height as f32;
        rect(&mut image, 0., 96., SIDEBAR, h - 96., 0., (20, 25, 34, 255));
        rect(&mut image, 0., 95., w, 1., 0., (49, 58, 73, 255));
        text(
            &mut image,
            font,
            if self.tab == Tab::Applications { "Add application" } else if self.tab == Tab::About {
                "About Prychal Dock"
            } else {
                "Dock Settings"
            },
            24.,
            45.,
            24.,
            [239, 245, 255],
        );
        text(
            &mut image,
            font,
            if self.tab == Tab::About {
                "Credits & project links"
            } else {
                "Prychal Dock  ·  Live preview"
            },
            25.,
            72.,
            12.,
            [148, 167, 194],
        );
        rect(&mut image, w - 78., 31., 52., 27., 9., (48, 65, 89, 255));
        for (i, color) in [0x74c0ff, 0xbd8aff, 0x48c9a5].into_iter().enumerate() {
            rect(
                &mut image,
                w - 70. + i as f32 * 14.,
                37.,
                10.,
                15.,
                3.,
                rgba(color, 255),
            );
        }
        for (i, (tab, label)) in [
            (Tab::Panel, "Panel"),
            (Tab::Icons, "Icons"),
            (Tab::Behavior, "Behavior"),
            (Tab::Indicators, "Indicators"),
            (Tab::Notifications, "Notifications"),
            (Tab::Applications, "Add app"),
            (Tab::About, "About"),
        ]
        .into_iter()
        .enumerate()
        {
            let a = Area {
                x: 16.,
                y: 120. + i as f32 * 52.,
                w: 132.,
                h: 42.,
            };
            let selected = self.tab == tab;
            if selected {
                rect(&mut image, a.x, a.y, a.w, a.h, 9., (46, 75, 114, 255));
                rect(
                    &mut image,
                    a.x,
                    a.y + 11.,
                    3.,
                    20.,
                    1.5,
                    (116, 192, 255, 255),
                );
            }
            rect(
                &mut image,
                a.x + 14.,
                a.y + 15.,
                11.,
                11.,
                3.,
                if selected {
                    (116, 192, 255, 255)
                } else {
                    (92, 111, 139, 255)
                },
            );
            text(
                &mut image,
                font,
                label,
                a.x + 38.,
                a.y + 27.,
                13.,
                if selected {
                    [240, 247, 255]
                } else {
                    [162, 180, 205]
                },
            );
            self.hits.push(Hit {
                area: a,
                control: Control::Tab(tab),
            });
        }
        text(
            &mut image,
            font,
            "Prychal Dock",
            24.,
            h - 50.,
            12.,
            [164, 181, 204],
        );
        text(
            &mut image,
            font,
            "Native X11 settings",
            24.,
            h - 31.,
            10.,
            [100, 119, 146],
        );
        let x = SIDEBAR + 24.;
        let content = w - x - 24.;
        if self.color.is_some() {
            self.draw_color(&mut image, font, x, content)?;
        } else if self.tab == Tab::Applications {
            self.draw_applications(&mut image, font, x, content);
        } else if self.tab == Tab::About {
            self.draw_about(&mut image, font, x, content);
        } else {
            let (title, subtitle) = match self.tab {
                Tab::Panel => ("Panel", "Background, border and corners"),
                Tab::Icons => ("Icons", "Size, spacing and magnification"),
                Tab::Behavior => ("Behavior", "Hide, motion and window previews"),
                Tab::Indicators => ("Indicators", "Active and running applications"),
                Tab::Notifications => ("Notifications", "Application counters and attention dots"),
                Tab::Applications => unreachable!("Applications has its own layout"),
                Tab::About => unreachable!("About has its own layout"),
            };
            text(&mut image, font, title, x, 125., 18., [234, 242, 253]);
            text(&mut image, font, subtitle, x, 144., 11., [144, 164, 191]);
            let rows: Vec<(&str, Control)> = match self.tab {
                Tab::Panel => vec![
                    ("Background color", Control::Color(ColorTarget::Background)),
                    (
                        "Background opacity",
                        Control::Slider(Setting::Opacity, 0, 100, 5),
                    ),
                    ("Border", Control::Toggle(Toggle::Border)),
                    ("Border color", Control::Color(ColorTarget::Border)),
                    (
                        "Border opacity",
                        Control::Slider(Setting::BorderOpacity, 0, 100, 5),
                    ),
                    (
                        "Border width",
                        Control::Slider(Setting::BorderWidth, 1, 8, 1),
                    ),
                    (
                        "Corner radius",
                        Control::Slider(Setting::CornerRadius, 0, 40, 1),
                    ),
                ],
                Tab::Icons => vec![
                    ("Icon size", Control::Slider(Setting::IconSize, 24, 64, 2)),
                    ("Icon spacing", Control::Slider(Setting::Spacing, 0, 32, 1)),
                    (
                        "Magnification",
                        Control::Slider(Setting::Magnification, 0, 100, 5),
                    ),
                ],
                Tab::Behavior => vec![
                    ("Intelligent hide", Control::Toggle(Toggle::Hide)),
                    (
                        "Hide delay",
                        Control::Slider(Setting::HideDelay, 0, 3000, 50),
                    ),
                    (
                        "Show / hide duration",
                        Control::Slider(Setting::Animation, 60, 500, 10),
                    ),
                    ("Window previews", Control::Toggle(Toggle::Previews)),
                ],
                Tab::Notifications => vec![
                    ("Application counters", Control::Toggle(Toggle::NotificationBadges)),
                    ("Counter color", Control::Color(ColorTarget::Notification)),
                    ("Counter size", Control::Slider(Setting::NotificationSize, 14, 24, 1)),
                    ("Attention dots", Control::Toggle(Toggle::AttentionDots)),
                    ("Attention color", Control::Color(ColorTarget::Attention)),
                    ("Attention dot size", Control::Slider(Setting::AttentionSize, 6, 14, 1)),
                ],
                Tab::Applications | Tab::About => vec![],
                Tab::Indicators => vec![
                    ("Active color", Control::Color(ColorTarget::Active)),
                    ("Running color", Control::Color(ColorTarget::Running)),
                ],
            };
            let step = ((h - 236.) / rows.len() as f32).clamp(44., 62.);
            for (i, (label, control)) in rows.iter().copied().enumerate() {
                let a = Area {
                    x,
                    y: 154. + i as f32 * step,
                    w: content,
                    h: step - 6.,
                };
                rect(&mut image, a.x, a.y, a.w, a.h, 9., (33, 41, 55, 255));
                text(
                    &mut image,
                    font,
                    label,
                    a.x + 14.,
                    a.y + 19.,
                    12.,
                    [219, 230, 246],
                );
                match control {
                    Control::Slider(setting, min, max, _) => {
                        let value = self.get(setting);
                        let unit = match setting {
                            Setting::Opacity | Setting::Magnification | Setting::BorderOpacity => {
                                "%"
                            }
                            Setting::Animation | Setting::HideDelay => " ms",
                            _ => " px",
                        };
                        right_text(
                            &mut image,
                            font,
                            &format!("{value}{unit}"),
                            a.x + a.w - 16.,
                            a.y + 19.,
                            11.,
                            [136, 195, 255],
                        );
                        let track = a.w - 32.;
                        let progress =
                            (value.saturating_sub(min) as f32 / (max - min) as f32).clamp(0., 1.);
                        rect(
                            &mut image,
                            a.x + 16.,
                            a.y + 29.,
                            track,
                            4.,
                            2.,
                            (61, 75, 95, 255),
                        );
                        if progress > 0. {
                            rect(
                                &mut image,
                                a.x + 16.,
                                a.y + 29.,
                                track * progress,
                                4.,
                                2.,
                                (116, 192, 255, 255),
                            );
                        }
                        rect(
                            &mut image,
                            a.x + 16. + track * progress - 6.,
                            a.y + 25.,
                            12.,
                            12.,
                            6.,
                            (231, 243, 255, 255),
                        );
                    }
                    Control::Toggle(toggle) => {
                        let on = match toggle {
                            Toggle::Border => self.current.border_enabled,
                            Toggle::Hide => self.current.auto_hide,
                            Toggle::Previews => self.current.previews,
                            Toggle::NotificationBadges => self.current.notification_badges,
                            Toggle::AttentionDots => self.current.attention_dots,
                        };
                        let bx = a.x + a.w - 62.;
                        let by = a.y + (a.h - 24.) / 2.;
                        rect(
                            &mut image,
                            bx,
                            by,
                            44.,
                            24.,
                            12.,
                            if on {
                                (91, 168, 237, 255)
                            } else {
                                (63, 77, 97, 255)
                            },
                        );
                        rect(
                            &mut image,
                            bx + if on { 23. } else { 3. },
                            by + 3.,
                            18.,
                            18.,
                            9.,
                            (245, 249, 255, 255),
                        );
                    }
                    Control::Color(target) => {
                        let rgb = self.current.color(target);
                        right_text(
                            &mut image,
                            font,
                            &format!("#{rgb:06X}"),
                            a.x + a.w - 66.,
                            a.y + a.h / 2. + 4.,
                            11.,
                            [146, 166, 194],
                        );
                        rect(
                            &mut image,
                            a.x + a.w - 50.,
                            a.y + 7.,
                            32.,
                            a.h - 14.,
                            6.,
                            rgba(rgb, 255),
                        );
                    }
                    _ => {}
                }
                self.hits.push(Hit { area: a, control });
            }
            if self.tab == Tab::Indicators {
                text(
                    &mut image,
                    font,
                    "Indicator style",
                    x,
                    308.,
                    12.,
                    [192, 210, 234],
                );
                let cell = (content - 32.) / 5.;
                for (style, label) in ["Pill", "Dot", "Line", "Square", "Hidden"]
                    .into_iter()
                    .enumerate()
                {
                    let a = Area {
                        x: x + style as f32 * (cell + 8.),
                        y: 324.,
                        w: cell,
                        h: 84.,
                    };
                    let chosen = self.current.indicator_style == style as u16;
                    rect(
                        &mut image,
                        a.x,
                        a.y,
                        a.w,
                        a.h,
                        10.,
                        if chosen {
                            (47, 80, 121, 255)
                        } else {
                            (33, 41, 55, 255)
                        },
                    );
                    mark(
                        &mut image,
                        a.x + a.w / 2. - 15.,
                        a.y + 28.,
                        style as u16,
                        true,
                        self.current.active_color,
                    );
                    mark(
                        &mut image,
                        a.x + a.w / 2. + 15.,
                        a.y + 28.,
                        style as u16,
                        false,
                        self.current.running_color,
                    );
                    centered(
                        &mut image,
                        font,
                        label,
                        a.x + a.w / 2.,
                        a.y + 65.,
                        11.,
                        if chosen {
                            [234, 245, 255]
                        } else {
                            [151, 173, 202]
                        },
                    );
                    self.hits.push(Hit {
                        area: a,
                        control: Control::Style(style as u16),
                    });
                }
            }
        }
        rect(
            &mut image,
            SIDEBAR,
            h - 72.,
            w - SIDEBAR,
            1.,
            0.,
            (48, 59, 76, 255),
        );
        if self.color.is_some() {
            let a = Area {
                x,
                y: h - 54.,
                w: 88.,
                h: 34.,
            };
            button(&mut image, font, a, "← Back", false);
            self.hits.push(Hit {
                area: a,
                control: Control::Back,
            });
        } else if !matches!(self.tab, Tab::About | Tab::Applications) {
            text(
                &mut image,
                font,
                "Saved automatically",
                x,
                h - 33.,
                11.,
                [133, 155, 184],
            );
        }
        let reset = Area {
            x: w - 258.,
            y: h - 54.,
            w: 132.,
            h: 34.,
        };
        let close = Area {
            x: w - 110.,
            y: h - 54.,
            w: 86.,
            h: 34.,
        };
        if !matches!(self.tab, Tab::About | Tab::Applications) {
            button(&mut image, font, reset, "Reset settings", false);
            self.hits.push(Hit {
                area: reset,
                control: Control::Reset,
            });
        }
        button(&mut image, font, close, "Done", true);

        self.hits.push(Hit {
            area: close,
            control: Control::Close,
        });
        let hovered = self.pointer.and_then(|(x,y)|self.hover_hit(x,y)).and_then(|i|self.hits.get(i));
        let control = self.drag.as_ref().map(|drag|drag.control).or_else(||hovered.map(|hit|hit.control));
        let cursor_index = match control {
            Some(Control::Slider(..) | Control::Brightness) => 2,
            Some(Control::Wheel) => 3,
            Some(Control::Search) => 4,
            Some(_) => 1,
            None => 0,
        };
        if cursor_index != self.cursor_index {
            c.change_window_attributes(self.window, &ChangeWindowAttributesAux::new().cursor(self.cursors[cursor_index]))?;
            self.cursor_index = cursor_index;
        }
        if let Some(hit) = hovered {
            let a=hit.area;
            let mut paint=Paint::default();
            paint.set_color_rgba8(166,207,255,24);
            let path=if matches!(hit.control,Control::Wheel) {
                let mut path=PathBuilder::new(); path.push_circle(a.x+a.w/2.,a.y+a.h/2.,a.w/2.); path.finish().unwrap()
            } else { crate::rounded(a.x,a.y,a.w,a.h,8.) };
            image.fill_path(&path,&paint,tiny_skia::FillRule::Winding,Transform::identity(),None);
            paint.set_color_rgba8(140,192,250,145);
            image.stroke_path(&path,&paint,&Stroke { width:1., ..Default::default() },Transform::identity(),None);
        }
        if let Some(hit) = self.focus.and_then(|i| self.hits.get(i)) {
            let a = hit.area;
            let path = if matches!(hit.control, Control::Wheel) {
                let mut p = PathBuilder::new();
                p.push_circle(a.x + a.w / 2., a.y + a.h / 2., a.w / 2. + 2.);
                p.finish().unwrap()
            } else {
                crate::rounded(a.x + 1., a.y + 1., a.w - 2., a.h - 2., 8.)
            };
            let mut paint = Paint::default();
            paint.set_color_rgba8(116, 192, 255, 210);
            image.stroke_path(
                &path,
                &paint,
                &Stroke {
                    width: 1.4,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }
        popup::upload(c, self.window, self.gc, &image)?;
        c.flush()?;
        Ok(())
    }
    fn draw_applications(&mut self, image: &mut Pixmap, font: &fontdue::Font, x: f32, width: f32) {
        let filtered = self.browser.filtered();
        let page = self.application_page_size();
        self.browser.offset = self.browser.offset.min(filtered.len().saturating_sub(page));
        text(image, font, "Installed applications", x, 130., 20., [239,245,255]);
        text(image, font, "Search by name or application ID; click a row to pin.", x, 152., 12., [148,167,194]);
        let search = Area { x, y: 168., w: width - 76., h: 36. };
        rect(image, search.x, search.y, search.w, search.h, 8., if self.search_active { (39,57,80,255) } else { (33,41,55,255) });
        let mut label = self.browser.query.clone();
        while label.chars().map(|c|font.metrics(c,13.).advance_width).sum::<f32>() > search.w - 32. { label.remove(0); }
        text(image, font, if label.is_empty() { "Search applications..." } else { &label }, x+12., 192.,13.,[219,230,246]);
        if self.search_active {
            let advance: f32 = label.chars().map(|ch| font.metrics(ch,13.).advance_width).sum();
            rect(image,x+12.+advance,177.,1.,19.,0.,(175,216,250,255));
        }
        self.hits.push(Hit { area: search, control: Control::Search });
        let clear = Area { x: x + width - 68., y: 168., w: 68., h: 36. };
        button(image,font,clear,"Clear",false);
        self.hits.push(Hit { area:clear,control:Control::ClearSearch });
        for (position, &index) in filtered.iter().enumerate().skip(self.browser.offset).take(page) {
            let item = &self.browser.items[index];
            let area = Area { x, y: 220. + (position-self.browser.offset) as f32*44., w:width, h:40. };
            rect(image,area.x,area.y,area.w,area.h,8.,if position==self.browser.selected { (43,65,94,255) } else { (33,41,55,255) });
            let mut name = item.name.clone();
            while name.chars().map(|c|font.metrics(c,13.).advance_width).sum::<f32>() > width - 110. { name.pop(); }
            text(image,font,&name,x+12.,area.y+17.,13.,[234,242,253]);
            let mut id = item.key.clone();
            while id.chars().map(|c|font.metrics(c,10.).advance_width).sum::<f32>() > width - 110. { id.pop(); }
            text(image,font,&id,x+12.,area.y+32.,10.,[148,167,194]);
            let action = Area { x: x+width-82., y:area.y+5., w:74., h:30. };
            button(image,font,action,if item.pinned { "Unpin" } else { "Add" },false);
            self.hits.push(Hit { area, control:Control::Application(position) });
            self.hits.push(Hit { area:action, control:if item.pinned { Control::UnpinApplication(position) } else { Control::Application(position) } });
        }
        if filtered.is_empty() { text(image,font,"No matching applications",x+12.,248.,13.,[148,167,194]); }
        let bottom = self.height as f32 - 85.;
        text(image,font,&format!("{} results · Scroll or use Up / Down; Enter to add",filtered.len()),x,bottom,11.,[148,167,194]);
        let mut message = self.browser.message.clone();
        while message.chars().map(|c|font.metrics(c,11.).advance_width).sum::<f32>() > width - 116. { message.pop(); }
        text(image,font,&message,x,self.height as f32-32.,11.,[175,216,250]);
    }
    fn draw_about(&mut self, image: &mut Pixmap, font: &fontdue::Font, x: f32, width: f32) {
        for (label, y, size, color) in [
            ("Prychal Dock", 130., 22., [239, 245, 255]),
            (
                concat!("Version ", env!("CARGO_PKG_VERSION")),
                157.,
                12.,
                [148, 167, 194],
            ),
            (
                "A customizable application dock for Linux.",
                194.,
                13.,
                [219, 230, 246],
            ),
            ("Rust · X11 · XFCE", 216., 12., [148, 167, 194]),
            (
                "Created by Vladyslav Havryliuk (VladCraft228).",
                258.,
                13.,
                [239, 245, 255],
            ),
            (
                "Developed with assistance from ChatGPT (OpenAI).",
                281.,
                12.,
                [190, 207, 230],
            ),
            (
                "Part of linux-scripts: practical Linux desktop tools.",
                309.,
                12.,
                [148, 167, 194],
            ),
        ] {
            text(image, font, label, x, y, size, color);
        }
        rect(image, x, 233., width, 1., 0., (48, 59, 76, 255));
        for (index, (label, url)) in [
            (
                "GitHub repository & other tools",
                "https://github.com/VladCraft228/linux-scripts",
            ),
            (
                "Documentation",
                "https://github.com/VladCraft228/linux-scripts#readme",
            ),
            (
                "Report a bug / suggest a feature",
                "https://github.com/VladCraft228/linux-scripts/issues/new",
            ),
            ("Author on GitHub", "https://github.com/VladCraft228"),
        ]
        .into_iter()
        .enumerate()
        {
            let area = Area {
                x,
                y: 330. + index as f32 * 36.,
                w: width,
                h: 30.,
            };
            button(image, font, area, label, false);
            self.hits.push(Hit {
                area,
                control: Control::Link(url),
            });
        }
    }
    fn draw_color(
        &mut self,
        image: &mut Pixmap,
        font: &fontdue::Font,
        x: f32,
        content: f32,
    ) -> Result<()> {
        let p = self.color.as_ref().ok_or("Missing settings color picker")?;
        text(image, font, p.target.label(), x, 125., 18., [234, 242, 253]);
        text(
            image,
            font,
            "Drag the wheel or brightness slider",
            x,
            145.,
            11.,
            [144, 164, 191],
        );
        let (cx, cy, r) = (x + 122., 292., 110.);
        if let Some(wheel) = &self.wheel {
            image.draw_pixmap(
                (cx - r) as i32,
                (cy - r) as i32,
                wheel.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
        let theta = p.h * std::f32::consts::TAU;
        let mut path = PathBuilder::new();
        path.push_circle(cx + r * p.s * theta.cos(), cy + r * p.s * theta.sin(), 6.);
        if let Some(path) = path.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(0, 0, 0, 235);
            image.stroke_path(
                &path,
                &paint,
                &Stroke {
                    width: 3.,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
            paint.set_color_rgba8(255, 255, 255, 255);
            image.stroke_path(
                &path,
                &paint,
                &Stroke {
                    width: 1.5,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }
        centered(
            image,
            font,
            "Hue & saturation",
            cx,
            430.,
            11.,
            [144, 164, 191],
        );
        self.hits.push(Hit {
            area: Area {
                x: cx - r,
                y: cy - r,
                w: 2. * r,
                h: 2. * r,
            },
            control: Control::Wheel,
        });
        let right = x + 270.;
        let width = (content - 270.).max(1.);
        rect(image, right, 188., width, 82., 10., rgba(p.rgb, 255));
        text(
            image,
            font,
            "Selected color",
            right,
            297.,
            11.,
            [144, 164, 191],
        );
        text(
            image,
            font,
            &format!("#{:06X}", p.rgb),
            right,
            327.,
            24.,
            [238, 246, 255],
        );
        text(image, font, "Brightness", right, 355., 12., [205, 220, 240]);
        right_text(
            image,
            font,
            &format!("{}%", (p.v * 100.).round() as u32),
            right + width,
            355.,
            11.,
            [144, 185, 232],
        );
        let pixels = width.round() as u32;
        for column in 0..pixels {
            rect(
                image,
                right + column as f32,
                370.,
                1.,
                12.,
                0.,
                rgba(
                    colors::hsv(
                        p.h,
                        p.s,
                        column as f32 / (pixels.saturating_sub(1).max(1)) as f32,
                    ),
                    255,
                ),
            );
        }
        rect(
            image,
            right + p.v * width - 3.,
            365.,
            6.,
            22.,
            3.,
            (239, 247, 255, 255),
        );
        self.hits.push(Hit {
            area: Area {
                x: right,
                y: 362.,
                w: width,
                h: 30.,
            },
            control: Control::Brightness,
        });
        let step = (width / 8.).min(38.);
        let size = (step - 8.).max(8.);
        for (i, rgb) in PRESETS.into_iter().enumerate() {
            let a = Area {
                x: right + i as f32 * step,
                y: 414.,
                w: size,
                h: 26.,
            };
            if rgb == p.rgb {
                rect(
                    image,
                    a.x - 2.,
                    a.y - 2.,
                    a.w + 4.,
                    a.h + 4.,
                    6.,
                    (227, 240, 255, 255),
                );
            }
            rect(image, a.x, a.y, a.w, a.h, 4., rgba(rgb, 255));
            self.hits.push(Hit {
                area: a,
                control: Control::Preset(rgb),
            });
        }
        Ok(())
    }
}
fn text(
    image: &mut Pixmap,
    font: &fontdue::Font,
    label: &str,
    x: f32,
    y: f32,
    size: f32,
    color: [u8; 3],
) {
    popup::text(image, font, label, x, y, size, color);
}
fn right_text(
    image: &mut Pixmap,
    font: &fontdue::Font,
    label: &str,
    right: f32,
    y: f32,
    size: f32,
    color: [u8; 3],
) {
    let width: f32 = label
        .chars()
        .map(|ch| font.metrics(ch, size).advance_width)
        .sum();
    text(image, font, label, right - width, y, size, color);
}
fn centered(
    image: &mut Pixmap,
    font: &fontdue::Font,
    label: &str,
    x: f32,
    y: f32,
    size: f32,
    color: [u8; 3],
) {
    let width: f32 = label
        .chars()
        .map(|ch| font.metrics(ch, size).advance_width)
        .sum();
    text(image, font, label, x - width / 2., y, size, color);
}
fn button(image: &mut Pixmap, font: &fontdue::Font, a: Area, label: &str, primary: bool) {
    rect(
        image,
        a.x,
        a.y,
        a.w,
        a.h,
        8.,
        if primary {
            (73, 135, 204, 255)
        } else {
            (40, 51, 69, 255)
        },
    );
    centered(
        image,
        font,
        label,
        a.x + a.w / 2.,
        a.y + 22.,
        12.,
        if primary {
            [245, 250, 255]
        } else {
            [188, 208, 234]
        },
    );
}
fn mark(image: &mut Pixmap, x: f32, y: f32, style: u16, active: bool, rgb: u32) {
    let (w, h, r) = match style {
        0 => (if active { 20. } else { 7. }, 4., 2.),
        1 => {
            let d = if active { 8. } else { 5. };
            (d, d, d / 2.)
        }
        2 => (if active { 22. } else { 12. }, 2., 0.),
        3 => {
            let d = if active { 6. } else { 4. };
            (d, d, 0.)
        }
        _ => return,
    };
    rect(image, x - w / 2., y - h / 2., w, h, r, rgba(rgb, 255));
}

fn center_position(screen: (u16, u16), client: (u16, u16), frame: [u32; 4]) -> (i32, i32) {
    let outer_width = client.0 as u32 + frame[0] + frame[1];
    let outer_height = client.1 as u32 + frame[2] + frame[3];
    (
        ((screen.0 as u32).saturating_sub(outer_width) / 2 + frame[0]) as i32,
        ((screen.1 as u32).saturating_sub(outer_height) / 2 + frame[2]) as i32,
    )
}
#[cfg(test)]
mod positioning_tests {
    use super::*;
    #[test]
    fn centers_the_frame_instead_of_the_client() {
        assert_eq!(
            center_position((1920, 1080), (820, 640), [0, 0, 0, 0]),
            (550, 220)
        );
        assert_eq!(
            center_position((1920, 1080), (820, 640), [4, 4, 28, 4]),
            (550, 232)
        );
        assert_eq!(
            center_position((640, 480), (820, 640), [4, 4, 28, 4]),
            (4, 28)
        );
    }
}
