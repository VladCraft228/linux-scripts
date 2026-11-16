use crate::{
    apps::Item,
    colors, rect,
    settings::{ColorTarget, Setting, Settings},
    Result,
};
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};
use tiny_skia::Pixmap;
use x11rb::{
    connection::{Connection, RequestConnection},
    protocol::xproto::*,
    wrapper::ConnectionExt as _,
};
const WIDTH: u16 = 260;
const HEADER: i16 = 38;
const ROW: i16 = 32;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Panel,
    Icons,
    Behavior,
    Indicators,
    Color(ColorTarget),
}
#[derive(Clone, Copy)]
enum DragPart {
    Wheel,
    Value,
}
struct Picker {
    target: ColorTarget,
    h: f32,
    s: f32,
    v: f32,
    rgb: u32,
    drag: Option<DragPart>,
    emitted: Instant,
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
            drag: None,
            emitted: Instant::now(),
        }
    }
    fn action(&mut self, commit: bool) -> Action {
        self.rgb = colors::hsv(self.h, self.s, self.v);
        Action::SetColor(self.target, self.rgb, commit)
    }
    fn update(&mut self, x: i16, y: i16) {
        match self.drag {
            Some(DragPart::Wheel) => {
                let dx = x as f32 - 130.;
                let dy = y as f32 - 136.;
                self.h = (dy.atan2(dx) / std::f32::consts::TAU).rem_euclid(1.);
                self.s = (dx.hypot(dy) / 86.).min(1.);
            }
            Some(DragPart::Value) => self.v = ((x as f32 - 24.) / 211.).clamp(0., 1.),
            None => {}
        }
    }
}
const COLOR_HEIGHT: u32 = 384;
const BACK_Y: i16 = 344;
const PRESETS: [u32; 8] = [
    0xffffff, 0x000000, 0x272d39, 0x74c0ff, 0xf06f7f, 0xbd8aff, 0x48c9a5, 0xffc76a,
];
#[derive(Clone)]
pub enum Action {
    Launch(String),
    CloseWindow(String, u32),
    CloseAll(String),
    Pin(String),
    Settings,
    About,
    AddApplication,
    Page(Page),
    SetColor(ColorTarget, u32, bool),
    ToggleBorder,
    Stay,
    Adjust(Setting, i32),
    ToggleAutoHide,
    TogglePreviews,
    ResetSettings,
    Close,
    Quit,
}
struct Row {
    label: String,
    enabled: bool,
    action: Action,
}
pub struct Popup {
    pub window: u32,
    pub visible: bool,
    pub key: Option<String>,
    font: fontdue::Font,
    rows: Vec<Row>,
    name: String,
    selected: Option<usize>,
    settings_page: bool,
    page: Page,
    picker: Option<Picker>,
    color_wheel: Option<Pixmap>,
    escape: u8,
    up: u8,
    down: u8,
    enter: u8,
    left: u8,
    right: u8,
    anchor: (i16, i16),
    screen: (u16, u16),
}
impl Popup {
    pub fn new<C: Connection>(c: &C, root: u32, visual: u32, colormap: u32) -> Result<Self> {
        let data = Command::new("fc-match")
            .args(["-f", "%{file}", "sans"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|p| fs::read(p.trim()).ok())
            .or_else(|| fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").ok())
            .ok_or("No menu font found (install fonts-dejavu-core/fontconfig)")?;
        let font = fontdue::Font::from_bytes(data, fontdue::FontSettings::default())
            .map_err(|e| format!("Menu font: {e}"))?;
        let window = c.generate_id()?;
        c.create_window(
            32,
            window,
            root,
            0,
            0,
            WIDTH,
            160,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new()
                .override_redirect(1)
                .background_pixel(0)
                .border_pixel(0)
                .colormap(colormap)
                .event_mask(
                    EventMask::EXPOSURE
                        | EventMask::POINTER_MOTION
                        | EventMask::BUTTON_PRESS
                        | EventMask::KEY_PRESS,
                ),
        )?
        .check()?;
        c.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"Prychal Dock Menu",
        )?;
        c.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"prychal-dock-menu\0prychal-dock-menu\0",
        )?;
        let atom =
            |s: &str| -> Result<u32> { Ok(c.intern_atom(false, s.as_bytes())?.reply()?.atom) };
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
            &[atom("_NET_WM_WINDOW_TYPE_POPUP_MENU")?],
        )?;
        let min = c.setup().min_keycode;
        let max = c.setup().max_keycode;
        let keys = c.get_keyboard_mapping(min, max - min + 1)?.reply()?;
        let keycode = |symbol: u32| -> u8 {
            keys.keysyms
                .chunks(keys.keysyms_per_keycode as usize)
                .position(|row| row.contains(&symbol))
                .map(|i| min + i as u8)
                .unwrap_or(0)
        };
        Ok(Self {
            window,
            visible: false,
            key: None,
            font,
            rows: vec![],
            name: String::new(),
            selected: None,
            settings_page: false,
            page: Page::Home,
            picker: None,
            color_wheel: None,
            escape: keycode(0xff1b),
            up: keycode(0xff52),
            down: keycode(0xff54),
            enter: keycode(0xff0d),
            left: keycode(0xff51),
            right: keycode(0xff53),
            anchor: (0, 0),
            screen: (0, 0),
        })
    }
    pub fn show<C: Connection>(
        &mut self,
        c: &C,
        gc: u32,
        item: Option<&Item>,
        anchor: (i16, i16),
        screen: (u16, u16),
        time: u32,
    ) -> Result<()> {
        self.settings_page = false;
        self.page = Page::Home;
        self.picker = None;
        self.anchor = anchor;
        self.screen = screen;
        self.key = item.map(|i| i.key.clone());
        self.rows.clear();
        self.selected = None;
        self.name = item
            .map(|i| i.name.clone())
            .unwrap_or_else(|| "Prychal Dock".into());
        if let Some(item) = item {
            self.rows.push(Row {
                label: "Launch new window".into(),
                enabled: item.launcher.is_some(),
                action: Action::Launch(item.key.clone()),
            });
            self.rows.push(Row {
                label: (if item.pinned {
                    "Unpin from Dock"
                } else if item.launcher.is_some() {
                    "Pin to Dock"
                } else {
                    "Pin unavailable (no launcher)"
                })
                .into(),
                enabled: item.pinned || item.launcher.is_some(),
                action: Action::Pin(item.key.clone()),
            });
        }
        if let Some(item) = item.filter(|item| !item.windows.is_empty()) {
            let active = c
                .get_property(
                    false,
                    c.get_geometry(self.window)?.reply()?.root,
                    c.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?.atom,
                    AtomEnum::WINDOW,
                    0,
                    1,
                )?
                .reply()?
                .value32()
                .and_then(|mut values| values.next());
            let id = active
                .filter(|id| item.windows.contains(id))
                .unwrap_or(item.windows[0]);
            self.rows.push(Row {
                label: if active.is_some_and(|id| item.windows.contains(&id))
                    && item.windows.len() > 1
                {
                    "Close active window"
                } else {
                    "Close window"
                }
                .into(),
                enabled: true,
                action: Action::CloseWindow(item.key.clone(), id),
            });
            if item.windows.len() > 1 {
                self.rows.push(Row {
                    label: "Close all windows".into(),
                    enabled: true,
                    action: Action::CloseAll(item.key.clone()),
                });
            }
        }
        self.rows.push(Row { label: "Add application...".into(), enabled: true, action: Action::AddApplication });
        self.rows.push(Row {
            label: "Settings...".into(),
            enabled: true,
            action: Action::Settings,
        });
        self.rows.push(Row {
            label: "About Dock...".into(),
            enabled: true,
            action: Action::About,
        });
        self.rows.push(Row {
            label: "Quit Dock".into(),
            enabled: true,
            action: Action::Quit,
        });
        let height = (HEADER + self.rows.len() as i16 * ROW + 8) as u16;
        let x =
            (anchor.0 as i32 - WIDTH as i32 / 2).clamp(0, screen.0.saturating_sub(WIDTH) as i32);
        let y = (anchor.1 as i32 - height as i32).clamp(0, screen.1.saturating_sub(height) as i32);
        c.configure_window(
            self.window,
            &ConfigureWindowAux::new()
                .x(x)
                .y(y)
                .width(WIDTH as u32)
                .height(height as u32)
                .stack_mode(StackMode::ABOVE),
        )?
        .check()?;
        c.map_window(self.window)?.check()?;
        let grab = c
            .grab_pointer(
                false,
                self.window,
                EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                0u32,
                0u32,
                time,
            )?
            .reply()?;
        if grab.status != GrabStatus::SUCCESS {
            c.unmap_window(self.window)?;
            c.flush()?;
            return Err("Could not open menu: pointer is grabbed by another application".into());
        }
        self.visible = true;
        let keyboard = c
            .grab_keyboard(false, self.window, time, GrabMode::ASYNC, GrabMode::ASYNC)?
            .reply()?;
        if keyboard.status != GrabStatus::SUCCESS {
            self.close(c)?;
            return Err("Could not grab menu keyboard".into());
        }
        self.draw(c, gc)
    }
    pub fn open_page<C: Connection>(
        &mut self,
        c: &C,
        gc: u32,
        page: Page,
        settings: Settings,
    ) -> Result<()> {
        self.page = page;
        self.selected = None;
        self.picker = None;
        self.show_settings(c, gc, settings)
    }
    fn height(&self) -> u32 {
        if matches!(self.page, Page::Color(_)) && self.settings_page {
            COLOR_HEIGHT
        } else {
            (HEADER + self.rows.len() as i16 * ROW + 8) as u32
        }
    }
    pub fn show_settings<C: Connection>(
        &mut self,
        c: &C,
        gc: u32,
        settings: Settings,
    ) -> Result<()> {
        if !self.settings_page {
            self.selected = None;
        }
        self.settings_page = true;
        let row = |label: String, action: Action| Row {
            label,
            enabled: true,
            action,
        };
        let back = || row("← Back".into(), Action::Page(Page::Home));
        self.rows = match self.page {
            Page::Home => {
                self.name = "Settings".into();
                vec![
                    row("Panel  →".into(), Action::Page(Page::Panel)),
                    row("Icons  →".into(), Action::Page(Page::Icons)),
                    row("Behavior  →".into(), Action::Page(Page::Behavior)),
                    row("Indicators  →".into(), Action::Page(Page::Indicators)),
                    row("Reset settings".into(), Action::ResetSettings),
                    row("Close settings".into(), Action::Close),
                ]
            }
            Page::Panel => {
                self.name = "Panel   (left − / right +)".into();
                vec![
                    row(
                        "Background color  →".into(),
                        Action::Page(Page::Color(ColorTarget::Background)),
                    ),
                    row(
                        format!("Background opacity: {}%", settings.opacity),
                        Action::Adjust(Setting::Opacity, 5),
                    ),
                    row(
                        format!(
                            "Border: {}",
                            if settings.border_enabled { "On" } else { "Off" }
                        ),
                        Action::ToggleBorder,
                    ),
                    row(
                        "Border color  →".into(),
                        Action::Page(Page::Color(ColorTarget::Border)),
                    ),
                    row(
                        format!("Border opacity: {}%", settings.border_opacity),
                        Action::Adjust(Setting::BorderOpacity, 5),
                    ),
                    row(
                        format!("Border width: {} px", settings.border_width),
                        Action::Adjust(Setting::BorderWidth, 1),
                    ),
                    row(
                        format!("Corner radius: {} px", settings.corner_radius),
                        Action::Adjust(Setting::CornerRadius, 1),
                    ),
                    back(),
                ]
            }
            Page::Icons => {
                self.name = "Icons   (left − / right +)".into();
                vec![
                    row(
                        format!("Icon size: {} px", settings.icon_size),
                        Action::Adjust(Setting::IconSize, 2),
                    ),
                    row(
                        format!("Icon spacing: {} px", settings.spacing),
                        Action::Adjust(Setting::Spacing, 1),
                    ),
                    row(
                        format!("Magnification: {}%", settings.magnification),
                        Action::Adjust(Setting::Magnification, 5),
                    ),
                    back(),
                ]
            }
            Page::Behavior => {
                self.name = "Behavior".into();
                vec![
                    row(
                        format!(
                            "Intelligent hide: {}",
                            if settings.auto_hide { "On" } else { "Off" }
                        ),
                        Action::ToggleAutoHide,
                    ),
                    row(
                        format!("Hide delay: {} ms", settings.hide_delay),
                        Action::Adjust(Setting::HideDelay, 50),
                    ),
                    row(
                        format!("Show/hide: {} ms", settings.animation_ms),
                        Action::Adjust(Setting::Animation, 10),
                    ),
                    row(
                        format!(
                            "Window previews: {}",
                            if settings.previews { "On" } else { "Off" }
                        ),
                        Action::TogglePreviews,
                    ),
                    back(),
                ]
            }
            Page::Indicators => {
                self.name = "Indicators".into();
                vec![
                    row(
                        format!("Style: {}   − / +", settings.indicator_name()),
                        Action::Adjust(Setting::IndicatorStyle, 1),
                    ),
                    row(
                        "Active color  →".into(),
                        Action::Page(Page::Color(ColorTarget::Active)),
                    ),
                    row(
                        "Running color  →".into(),
                        Action::Page(Page::Color(ColorTarget::Running)),
                    ),
                    back(),
                ]
            }
            Page::Color(target) => {
                self.name = target.label().into();
                let rgb = settings.color(target);
                if self
                    .picker
                    .as_ref()
                    .is_none_or(|p| p.target != target || p.rgb != rgb)
                {
                    self.picker = Some(Picker::new(target, rgb));
                }
                if self.color_wheel.is_none() {
                    self.color_wheel = Some(colors::wheel(86));
                }
                let parent = match target {
                    ColorTarget::Background | ColorTarget::Border => Page::Panel,
                    _ => Page::Indicators,
                };
                vec![row("← Back".into(), Action::Page(parent))]
            }
        };
        let height = self.height();
        let x = (self.anchor.0 as i32 - WIDTH as i32 / 2)
            .clamp(0, self.screen.0.saturating_sub(WIDTH) as i32);
        let y = (self.anchor.1 as i32 - height as i32)
            .clamp(0, (self.screen.1 as u32).saturating_sub(height) as i32);
        c.configure_window(
            self.window,
            &ConfigureWindowAux::new().x(x).y(y).height(height),
        )?
        .check()?;
        self.draw(c, gc)
    }
    pub fn wheel(&mut self, x: i16, y: i16, up: bool) -> Option<Action> {
        if let Some(p) = self.picker.as_mut() {
            if x >= 0 && x < WIDTH as i16 && y >= 0 && y < COLOR_HEIGHT as i16 {
                p.v = (p.v + if up { 0.025 } else { -0.025 }).clamp(0., 1.);
                return Some(p.action(true));
            }
            return None;
        }
        let index = self.hit(x, y)?;
        match self.rows[index].action.clone() {
            Action::Adjust(setting, step) => {
                Some(Action::Adjust(setting, if up { step } else { -step }))
            }
            _ => None,
        }
    }
    pub fn close<C: Connection>(&mut self, c: &C) -> Result<()> {
        if self.visible {
            c.ungrab_pointer(0u32)?;
            c.ungrab_keyboard(0u32)?;
            c.unmap_window(self.window)?;
            c.flush()?;
            self.visible = false;
            if let Some(p) = self.picker.as_mut() {
                p.drag = None;
            }
        }
        Ok(())
    }
    fn hit(&self, x: i16, y: i16) -> Option<usize> {
        if self.settings_page && matches!(self.page, Page::Color(_)) {
            return (x >= 6 && x < WIDTH as i16 - 6 && (BACK_Y..BACK_Y + ROW).contains(&y))
                .then_some(0);
        }
        if x < 6 || x >= WIDTH as i16 - 6 || y < HEADER {
            return None;
        }
        let row = ((y - HEADER) / ROW) as usize;
        (row < self.rows.len() && self.rows[row].enabled).then_some(row)
    }
    pub fn motion<C: Connection>(
        &mut self,
        c: &C,
        gc: u32,
        x: i16,
        y: i16,
    ) -> Result<Option<Action>> {
        if let Some(p) = self.picker.as_mut().filter(|p| p.drag.is_some()) {
            p.update(x, y);
            let action = p.action(false);
            if p.emitted.elapsed() >= Duration::from_millis(16) {
                p.emitted = Instant::now();
                return Ok(Some(action));
            }
            return Ok(None);
        }
        let row = self.hit(x, y);
        if row != self.selected {
            self.selected = row;
            self.draw(c, gc)?;
        }
        Ok(None)
    }
    pub fn click(&mut self, x: i16, y: i16) -> Option<Action> {
        if self.settings_page && matches!(self.page, Page::Color(_)) {
            if x < 0 || x >= WIDTH as i16 || y < 0 || y >= COLOR_HEIGHT as i16 {
                return None;
            }
            let p = self.picker.as_mut()?;
            if (x as f32 - 130.).hypot(y as f32 - 136.) <= 86. {
                p.drag = Some(DragPart::Wheel);
                p.update(x, y);
                p.emitted = Instant::now();
                return Some(p.action(false));
            }
            if (24..=236).contains(&x) && (234..=256).contains(&y) {
                p.drag = Some(DragPart::Value);
                p.update(x, y);
                p.emitted = Instant::now();
                return Some(p.action(false));
            }
            if (314..338).contains(&y) {
                for (i, color) in PRESETS.iter().enumerate() {
                    let left = 16 + i as i16 * 28;
                    if (left..left + 24).contains(&x) {
                        *p = Picker::new(p.target, *color);
                        return Some(p.action(true));
                    }
                }
            }
            if self.hit(x, y).is_none() {
                return Some(Action::Stay);
            }
        }
        self.hit(x, y).map(|i| match self.rows[i].action.clone() {
            Action::Adjust(setting, step) => {
                Action::Adjust(setting, if x < WIDTH as i16 / 2 { -step } else { step })
            }
            action => action,
        })
    }
    pub fn release(&mut self) -> Option<Action> {
        let p = self.picker.as_mut()?;
        p.drag.take()?;
        Some(p.action(true))
    }
    pub fn font(&self) -> &fontdue::Font {
        &self.font
    }
    pub fn is_escape(&self, key: u8) -> bool {
        key == self.escape
    }
    pub fn key<C: Connection>(&mut self, c: &C, gc: u32, key: u8) -> Result<Option<Action>> {
        if key == self.escape {
            self.close(c)?;
            return Ok(None);
        }
        if key == self.left || key == self.right {
            if let Some(p) = self.picker.as_mut() {
                p.v = (p.v + if key == self.left { -0.025 } else { 0.025 }).clamp(0., 1.);
                return Ok(Some(p.action(true)));
            }
            return Ok(self
                .selected
                .and_then(|i| match self.rows[i].action.clone() {
                    Action::Adjust(setting, step) => Some(Action::Adjust(
                        setting,
                        if key == self.left { -step } else { step },
                    )),
                    _ => None,
                }));
        }
        if key == self.enter {
            return Ok(self.selected.map(|i| self.rows[i].action.clone()));
        }
        if key == self.up || key == self.down {
            let enabled: Vec<_> = self
                .rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.enabled)
                .map(|(i, _)| i)
                .collect();
            if !enabled.is_empty() {
                let position = self
                    .selected
                    .and_then(|s| enabled.iter().position(|&i| i == s));
                let next = if key == self.up {
                    position.map_or(enabled.len() - 1, |p| {
                        (p + enabled.len() - 1) % enabled.len()
                    })
                } else {
                    position.map_or(0, |p| (p + 1) % enabled.len())
                };
                self.selected = Some(enabled[next]);
                self.draw(c, gc)?;
            }
        }
        Ok(None)
    }
    fn draw_color<C: Connection>(&self, c: &C, gc: u32) -> Result<()> {
        let p = self.picker.as_ref().ok_or("Missing color picker")?;
        let mut image =
            Pixmap::new(WIDTH as u32, COLOR_HEIGHT).ok_or("Color picker allocation failed")?;
        rect(
            &mut image,
            0.,
            0.,
            WIDTH as f32,
            COLOR_HEIGHT as f32,
            12.,
            (31, 36, 46, 250),
        );
        text(
            &mut image,
            &self.font,
            &self.name,
            14.,
            25.,
            13.,
            [225, 234, 247],
        );
        if let Some(wheel) = &self.color_wheel {
            image.draw_pixmap(
                44,
                50,
                wheel.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                tiny_skia::Transform::identity(),
                None,
            );
        }
        let theta = p.h * std::f32::consts::TAU;
        let (x, y) = (
            130. + 86. * p.s * theta.cos(),
            136. + 86. * p.s * theta.sin(),
        );
        let mut path = tiny_skia::PathBuilder::new();
        path.push_circle(x, y, 5.);
        if let Some(path) = path.finish() {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(0, 0, 0, 235);
            image.stroke_path(
                &path,
                &paint,
                &tiny_skia::Stroke {
                    width: 3.,
                    ..Default::default()
                },
                tiny_skia::Transform::identity(),
                None,
            );
            paint.set_color_rgba8(255, 255, 255, 255);
            image.stroke_path(
                &path,
                &paint,
                &tiny_skia::Stroke {
                    width: 1.4,
                    ..Default::default()
                },
                tiny_skia::Transform::identity(),
                None,
            );
        }
        for column in 0..212 {
            rect(
                &mut image,
                24. + column as f32,
                238.,
                1.,
                14.,
                0.,
                crate::settings::rgba(colors::hsv(p.h, p.s, column as f32 / 211.), 255),
            );
        }
        rect(
            &mut image,
            24. + p.v * 211. - 2.,
            235.,
            4.,
            20.,
            2.,
            (255, 255, 255, 255),
        );
        text(
            &mut image,
            &self.font,
            "Brightness",
            24.,
            230.,
            10.,
            [174, 189, 212],
        );
        rect(
            &mut image,
            24.,
            270.,
            48.,
            30.,
            6.,
            crate::settings::rgba(p.rgb, 255),
        );
        text(
            &mut image,
            &self.font,
            &format!("#{:06X}", p.rgb),
            88.,
            291.,
            15.,
            [243, 247, 255],
        );
        for (i, color) in PRESETS.iter().enumerate() {
            let x = 16. + i as f32 * 28.;
            if *color == p.rgb {
                rect(&mut image, x - 2., 312., 28., 28., 6., (255, 255, 255, 255));
            }
            rect(
                &mut image,
                x,
                314.,
                24.,
                24.,
                4.,
                crate::settings::rgba(*color, 255),
            );
        }
        if self.selected == Some(0) {
            rect(
                &mut image,
                6.,
                BACK_Y as f32,
                WIDTH as f32 - 12.,
                ROW as f32 - 2.,
                7.,
                (61, 109, 173, 255),
            );
        }
        text(
            &mut image,
            &self.font,
            "← Back",
            14.,
            BACK_Y as f32 + 21.,
            13.,
            [241, 244, 249],
        );
        upload(c, self.window, gc, &image)?;
        c.flush()?;
        Ok(())
    }
    pub fn draw<C: Connection>(&self, c: &C, gc: u32) -> Result<()> {
        if self.settings_page && matches!(self.page, Page::Color(_)) {
            return self.draw_color(c, gc);
        }
        let height = self.height();
        let mut pm = Pixmap::new(WIDTH as u32, height).ok_or("Menu allocation failed")?;
        rect(
            &mut pm,
            0.,
            0.,
            WIDTH as f32,
            height as f32,
            12.,
            (31, 36, 46, 250),
        );
        text(
            &mut pm,
            &self.font,
            &self.name,
            14.,
            25.,
            13.,
            [173, 186, 205],
        );
        rect(
            &mut pm,
            12.,
            33.,
            WIDTH as f32 - 24.,
            1.,
            0.,
            (88, 98, 115, 150),
        );
        for (i, row) in self.rows.iter().enumerate() {
            let y = HEADER as f32 + i as f32 * ROW as f32;
            if self.selected == Some(i) {
                rect(
                    &mut pm,
                    6.,
                    y,
                    WIDTH as f32 - 12.,
                    ROW as f32 - 2.,
                    7.,
                    (61, 109, 173, 255),
                );
            }
            text(
                &mut pm,
                &self.font,
                &row.label,
                14.,
                y + 21.,
                13.,
                if row.enabled {
                    [241, 244, 249]
                } else {
                    [121, 131, 147]
                },
            );
        }
        upload(c, self.window, gc, &pm)?;
        c.flush()?;
        Ok(())
    }
}
pub(crate) fn text(
    pm: &mut Pixmap,
    font: &fontdue::Font,
    label: &str,
    mut x: f32,
    baseline: f32,
    size: f32,
    color: [u8; 3],
) {
    for ch in label.chars() {
        if ch == '←' || ch == '→' {
            let advance = font.metrics(ch, size).advance_width.max(size * 0.8);
            if x + advance > pm.width() as f32 - 12. {
                break;
            }
            let left = x + 1.;
            let right = x + advance - 2.;
            let cy = baseline - size * 0.32;
            let tip = if ch == '←' { left } else { right };
            let back = if ch == '←' {
                left + size * 0.25
            } else {
                right - size * 0.25
            };
            let mut path = tiny_skia::PathBuilder::new();
            path.move_to(left, cy);
            path.line_to(right, cy);
            path.move_to(back, cy - size * 0.25);
            path.line_to(tip, cy);
            path.line_to(back, cy + size * 0.25);
            if let Some(path) = path.finish() {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color_rgba8(color[0], color[1], color[2], 255);
                pm.stroke_path(
                    &path,
                    &paint,
                    &tiny_skia::Stroke {
                        width: 1.4,
                        line_cap: tiny_skia::LineCap::Round,
                        line_join: tiny_skia::LineJoin::Round,
                        ..Default::default()
                    },
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
            x += advance;
            continue;
        }
        let (metrics, bitmap) = font.rasterize(ch, size);
        if x + metrics.advance_width > pm.width() as f32 - 12. {
            break;
        }
        let left = x as i32 + metrics.xmin;
        let top = baseline as i32 - metrics.ymin - metrics.height as i32;
        for y in 0..metrics.height {
            for dx in 0..metrics.width {
                let a = bitmap[y * metrics.width + dx];
                if a == 0 {
                    continue;
                }
                let (px, py) = (left + dx as i32, top + y as i32);
                if px < 0 || py < 0 || px >= pm.width() as i32 || py >= pm.height() as i32 {
                    continue;
                }
                let index = 4 * (py as usize * pm.width() as usize + px as usize);
                let pixel = &mut pm.data_mut()[index..index + 4];
                for k in 0..3 {
                    pixel[k] = ((color[k] as u32 * a as u32 + pixel[k] as u32 * (255 - a as u32))
                        / 255) as u8;
                }
                pixel[3] = (a as u32 + pixel[3] as u32 * (255 - a as u32) / 255) as u8;
            }
        }
        x += metrics.advance_width;
    }
}
pub fn upload<C: Connection + RequestConnection>(
    c: &C,
    window: u32,
    gc: u32,
    pm: &Pixmap,
) -> Result<()> {
    let mut pixels = pm.data().to_vec();
    let little = c.setup().image_byte_order == ImageOrder::LSB_FIRST;
    for p in pixels.chunks_exact_mut(4) {
        let word = u32::from_be_bytes([p[3], p[0], p[1], p[2]]);
        p.copy_from_slice(&if little {
            word.to_le_bytes()
        } else {
            word.to_be_bytes()
        });
    }
    let stride = pm.width() as usize * 4;
    let rows =
        ((c.maximum_request_bytes().saturating_sub(128)) / stride).clamp(1, pm.height() as usize);
    for (n, data) in pixels.chunks(rows * stride).enumerate() {
        c.put_image(
            ImageFormat::Z_PIXMAP,
            window,
            gc,
            pm.width() as u16,
            (data.len() / stride) as u16,
            0,
            (n * rows) as i16,
            0,
            32,
            data,
        )?;
    }
    Ok(())
}
