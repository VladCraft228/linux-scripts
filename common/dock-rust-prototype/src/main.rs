//! Prychal Dock — a lightweight, customizable application dock for X11.
mod apps;
mod badges;
mod applications;
mod colors;
mod drop_target;
mod icons;
mod instance;
mod keyboard;
mod launches;
mod notice;
mod pins;
mod popup;
mod previews;
mod settings;
mod settings_window;
mod tooltip;
mod visibility;
mod windows;
use apps::{Apps, Item};
use settings::Settings;
use std::{
    error::Error,
    time::{Duration, Instant},
};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};
use x11rb::{
    connection::Connection,
    protocol::{render, shape, xproto::*, Event},
    wrapper::ConnectionExt as _,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const H: u16 = 160;
const BASELINE: f32 = 134.;
const FRAME: Duration = Duration::from_micros(16_667);
#[derive(Clone, Copy)]
struct Layout {
    count: usize,
    width: u16,
    base: f32,
    step: f32,
    magnification: f32,
    opacity: u8,
    appearance: Settings,
}
impl Layout {
    #[cfg(test)]
    fn new(count: usize) -> Self {
        Self::configured(count, Settings::default())
    }
    fn configured(count: usize, settings: Settings) -> Self {
        Self {
            count,
            width: count as u16 * (settings.icon_size + settings.spacing) + 130,
            base: settings.icon_size as f32,
            step: settings.step(),
            magnification: settings.magnification as f32 / 100.,
            opacity: (settings.opacity as f32 * 2.55).round() as u8,
            appearance: settings,
        }
    }
    fn panel_height(self) -> f32 {
        self.base + 24.
    }
    fn panel_top(self) -> f32 {
        148. - self.panel_height()
    }
    fn center(self, i: usize) -> f32 {
        self.width as f32 / 2. + (i as f32 - (self.count as f32 - 1.) / 2.) * self.step
    }
    fn panel_width(self) -> f32 {
        self.count as f32 * self.step + 40.
    }
    fn panel_x(self) -> f32 {
        (self.width as f32 - self.panel_width()) / 2.
    }
    fn influence(self, x: Option<f32>, i: usize) -> f32 {
        x.map(|x| (-((x - self.center(i)) / (self.step * 28. / 58.)).powi(2)).exp())
            .unwrap_or(0.)
    }
    fn hit(self, x: f32) -> Option<usize> {
        (0..self.count).find(|&i| (x - self.center(i)).abs() <= self.step / 2.)
    }
}
struct Drag {
    key: String,
    start_x: i16,
    start_y: i16,
    root_x: i16,
    eligible: bool,
    active: bool,
    order: Vec<String>,
}
fn finish_drag<C: Connection>(c: &C, drag: &mut Option<Drag>) -> Result<Option<Drag>> {
    let ended = drag.take();
    if ended.as_ref().is_some_and(|d| d.active) {
        c.ungrab_pointer(0u32)?;
        c.ungrab_keyboard(0u32)?;
        c.flush()?;
    }
    Ok(ended)
}
fn sync_visuals(
    keys: &[String],
    old: &[String],
    scales: &mut Vec<f32>,
    centers: &mut Vec<f32>,
    layout: Layout,
) {
    let states: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(i, key)| {
            old.iter()
                .position(|k| k == key)
                .map(|j| (scales[j], centers[j]))
                .unwrap_or((0., layout.center(i)))
        })
        .collect();
    *scales = states.iter().map(|s| s.0).collect();
    *centers = states.iter().map(|s| s.1).collect();
}
fn rounded(x: f32, y: f32, w: f32, h: f32, radius: f32) -> tiny_skia::Path {
    let r = radius.min(w / 2.).min(h / 2.);
    let k = 0.552_284_8;
    let mut p = PathBuilder::new();
    p.move_to(x + r, y);
    p.line_to(x + w - r, y);
    p.cubic_to(x + w - r + k * r, y, x + w, y + r - k * r, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.cubic_to(
        x + w,
        y + h - r + k * r,
        x + w - r + k * r,
        y + h,
        x + w - r,
        y + h,
    );
    p.line_to(x + r, y + h);
    p.cubic_to(x + r - k * r, y + h, x, y + h - r + k * r, x, y + h - r);
    p.line_to(x, y + r);
    p.cubic_to(x, y + r - k * r, x + r - k * r, y, x + r, y);
    p.close();
    p.finish().unwrap()
}
fn fill(pm: &mut Pixmap, path: &tiny_skia::Path, rgba: (u8, u8, u8, u8)) {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(rgba.0, rgba.1, rgba.2, rgba.3));
    pm.fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);
}
fn rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, c: (u8, u8, u8, u8)) {
    fill(pm, &rounded(x, y, w, h, r), c);
}
#[derive(Default)]
struct Decorations<'a> {
    active: u32,
    font: Option<&'a fontdue::Font>,
    bounces: &'a [f32],
    badges: &'a [badges::Badge],
}
fn bounce_offset(size: f32, offset: f32) -> f32 {
    offset.min((BASELINE - size - 4.).max(0.))
}
fn draw(
    layout: Layout,
    scales: &[f32],
    centers: &[f32],
    dragged: Option<usize>,
    visibility: f32,
    items: &[Item],
    decorations: Decorations<'_>,
) -> Pixmap {
    let mut pm = Pixmap::new(layout.width as u32, H as u32).unwrap();
    let dy = (1. - visibility) * H as f32;
    let appearance = layout.appearance;
    let radius = (appearance.corner_radius as f32).min(layout.panel_height() / 2.);
    rect(
        &mut pm,
        layout.panel_x(),
        layout.panel_top() + dy,
        layout.panel_width(),
        layout.panel_height(),
        radius,
        settings::rgba(appearance.background, layout.opacity),
    );
    if appearance.border_enabled && appearance.border_opacity > 0 {
        let width = appearance.border_width as f32;
        let inset = width / 2.;
        let path = rounded(
            layout.panel_x() + inset,
            layout.panel_top() + dy + inset,
            layout.panel_width() - width,
            layout.panel_height() - width,
            (radius - inset).max(0.),
        );
        let mut paint = Paint::default();
        let rgba = settings::rgba(
            appearance.border_color,
            (appearance.border_opacity as f32 * 2.55).round() as u8,
        );
        paint.set_color_rgba8(rgba.0, rgba.1, rgba.2, rgba.3);
        pm.stroke_path(
            &path,
            &paint,
            &tiny_skia::Stroke {
                width,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
    let mut order: Vec<_> = (0..items.len()).filter(|i| Some(*i) != dragged).collect();
    if let Some(i) = dragged {
        order.push(i);
    }
    for i in order {
        let item = &items[i];
        let size = layout.base * (1. + layout.magnification * scales[i]);
        let x = centers[i] - size / 2.;
        let lift = if dragged == Some(i) {
            0.
        } else {
            bounce_offset(size, decorations.bounces.get(i).copied().unwrap_or(0.))
        };
        let y = BASELINE - size - lift + dy;
        if let Some(icon) = &item.icon {
            let transform = Transform::from_translate(x, y)
                .pre_scale(size / icon.width() as f32, size / icon.height() as f32);
            let paint = tiny_skia::PixmapPaint {
                quality: tiny_skia::FilterQuality::Bilinear,
                ..Default::default()
            };
            pm.draw_pixmap(0, 0, icon.as_ref().as_ref(), &paint, transform, None);
        } else {
            rect(&mut pm, x, y, size, size, size * 0.23, (78, 104, 143, 255));
            for row in 0..2 {
                for col in 0..2 {
                    rect(
                        &mut pm,
                        x + size * (0.22 + col as f32 * 0.33),
                        y + size * (0.22 + row as f32 * 0.33),
                        size * 0.23,
                        size * 0.23,
                        2.,
                        (235, 244, 255, 255),
                    );
                }
            }
        }
        if item.windows.len() > 1 {
            if let Some(font) = decorations.font {
                let label = if item.windows.len() > 99 {
                    "99+".into()
                } else {
                    item.windows.len().to_string()
                };
                let tw: f32 = label
                    .chars()
                    .map(|ch| font.metrics(ch, 11.).advance_width)
                    .sum();
                let bw = (tw + 10.).max(18.);
                let bx = x + size - bw + 3.;
                let by = y.max(4.);
                rect(&mut pm, bx, by, bw, 18., 9., (42, 111, 188, 255));
                popup::text(
                    &mut pm,
                    font,
                    &label,
                    bx + (bw - tw) / 2.,
                    by + 13.,
                    11.,
                    [255, 255, 255],
                );
            }
        }
        if let Some(badge) = decorations.badges.get(i) {
            if badge.count > 0 && appearance.notification_badges {
                if let Some(font) = decorations.font {
                    let label = if badge.count > 99 { "99+".into() } else { badge.count.to_string() };
                    let h = appearance.notification_size as f32;
                    let font_size = h * 11. / 18.;
                    let width: f32 = label.chars().map(|ch|font.metrics(ch,font_size).advance_width).sum();
                    let bw = (width+h*10./18.).max(h);
                    let bx = x-3.;
                    let blue_width = if item.windows.len() > 1 {
                        let label = if item.windows.len() > 99 { "99+".into() } else { item.windows.len().to_string() };
                        (label.chars().map(|ch|font.metrics(ch,11.).advance_width).sum::<f32>()+10.).max(18.)
                    } else { 0. };
                    // Stack counters vertically only when the chosen size would overlap.
                    let by = (y - if blue_width > 0. && bw+blue_width > size+6. { h } else { 0. }).max(4.);
                    rect(&mut pm,bx,by,bw,h,h/2.,settings::rgba(appearance.notification_color,255));
                    let baseline = font.horizontal_line_metrics(font_size).map_or(by+h*13./18., |m|by+h/2.+(m.ascent+m.descent)/2.);
                    let rgb = appearance.notification_color;
                    let luma = ((rgb>>16)&255)*299 + ((rgb>>8)&255)*587 + (rgb&255)*114;
                    let text_color = if luma > 170_000 { [25,32,44] } else { [255,255,255] };
                    popup::text(&mut pm,font,&label,bx+(bw-width)/2.,baseline,font_size,text_color);
                }
            } else if badge.urgent && appearance.attention_dots {
                let d = appearance.attention_size as f32;
                rect(&mut pm,x-2.,y.max(4.),d+4.,d+4.,(d+4.)/2.,(34,39,49,255));
                rect(&mut pm,x,y.max(4.)+2.,d,d,d/2.,settings::rgba(appearance.attention_color,255));
            }
        }
        if !item.windows.is_empty() && appearance.indicator_style != 4 {
            let active = item.windows.contains(&decorations.active);
            let color = settings::rgba(
                if active {
                    appearance.active_color
                } else {
                    appearance.running_color
                },
                if active { 255 } else { 220 },
            );
            let (w, h, r) = match appearance.indicator_style {
                1 => {
                    let d = if active { 8. } else { 5. };
                    (d, d, d / 2.)
                }
                2 => (if active { 24. } else { 12. }, 2., 0.),
                3 => {
                    let d = if active { 6. } else { 4. };
                    (d, d, 0.)
                }
                _ => (if active { 22. } else { 7. }, 4., 2.),
            };
            rect(
                &mut pm,
                centers[i] - w / 2.,
                BASELINE + 4. + dy,
                w,
                h,
                r,
                color,
            );
        }
    }
    pm
}
fn main() {
    if let Err(e) = run() {
        eprintln!("prychal-dock: {e}");
        std::process::exit(1)
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help") {
        println!("Prychal Dock\nUsage: prychal-dock [--settings | --quit] [--no-autohide] [--duration SECONDS]\n--version: print version. Repeated launch opens the running dock's settings.\nXFCE X11 with compositor required.");
        return Ok(());
    }
    if args.iter().any(|a| a == "--version") {
        println!("Prychal Dock {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let mut force_visible = false;
    let mut show_settings = false;
    let mut quit = false;
    let mut duration = None;
    let mut a = 0;
    while a < args.len() {
        match args[a].as_str() {
            "--no-autohide" => force_visible = true,
            "--settings" => show_settings = true,
            "--quit" => quit = true,
            "--duration" => {
                a += 1;
                let seconds: f64 = args.get(a).ok_or("missing duration")?.parse()?;
                if !seconds.is_finite() || !(0.1..=86400.).contains(&seconds) {
                    return Err("duration must be 0.1..86400 seconds".into());
                }
                duration = Some(Duration::from_secs_f64(seconds));
            }
            _ => return Err(format!("unknown option: {}", args[a]).into()),
        }
        a += 1;
    }
    if quit && (show_settings || force_visible || duration.is_some()) {
        return Err("--quit cannot be combined with launch options".into());
    }
    if std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland") {
        return Err("this prototype supports native X11 only".into());
    }
    let (conn, sn) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[sn];
    let root = screen.root;
    let Some(instance) = instance::Instance::claim(&conn, root, sn, if quit { instance::Command::Quit } else { instance::Command::Settings })? else { return Ok(()); };
    let mut preferences = Settings::load()?;
    let mut auto = if force_visible { false } else { preferences.auto_hide };
    let mut apps = Apps::new(&conn)?;
    conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?
    .check()?;
    apps.refresh(&conn, root, 0);
    let mut capacity =
        (screen.width_in_pixels.saturating_sub(130) as usize / preferences.step() as usize).max(1);
    let mut offset = 0usize;
    let mut layout = Layout::configured(apps.items.len().min(capacity), preferences);
    let mut w = layout.width;
    let mut x = (screen.width_in_pixels.saturating_sub(w) / 2) as i16;
    let atom =
        |s: &str| -> Result<u32> { Ok(conn.intern_atom(false, s.as_bytes())?.reply()?.atom) };
    let cm = atom(&format!("_NET_WM_CM_S{sn}"))?;
    if conn.get_selection_owner(cm)?.reply()?.owner == 0 {
        return Err("enable the XFCE compositor first (Window Manager Tweaks → Compositor)".into());
    }
    let formats = render::query_pict_formats(&conn)?.reply()?;
    let visual = screen
        .allowed_depths
        .iter()
        .filter(|d| d.depth == 32)
        .flat_map(|d| &d.visuals)
        .find(|v| {
            v.class == VisualClass::TRUE_COLOR
                && v.red_mask == 0xff0000
                && v.green_mask == 0xff00
                && v.blue_mask == 0xff
                && formats.screens[sn]
                    .depths
                    .iter()
                    .flat_map(|d| &d.visuals)
                    .any(|rv| {
                        rv.visual == v.visual_id
                            && formats.formats.iter().any(|f| {
                                f.id == rv.format
                                    && f.direct.alpha_mask == 255
                                    && f.direct.alpha_shift == 24
                            })
                    })
        })
        .ok_or("no compatible 32-bit ARGB visual")?;
    let window = conn.generate_id()?;
    let edge = conn.generate_id()?;
    let cmap = conn.generate_id()?;
    let gc = conn.generate_id()?;
    conn.create_colormap(ColormapAlloc::NONE, cmap, root, visual.visual_id)?
        .check()?;
    let y = screen.height_in_pixels.saturating_sub(H) as i16;
    conn.create_window(
        32,
        window,
        root,
        x,
        y,
        w,
        H,
        0,
        WindowClass::INPUT_OUTPUT,
        visual.visual_id,
        &CreateWindowAux::new()
            .background_pixel(0)
            .border_pixel(0)
            .colormap(cmap)
            .override_redirect(1)
            .event_mask(
                EventMask::EXPOSURE
                    | EventMask::POINTER_MOTION
                    | EventMask::ENTER_WINDOW
                    | EventMask::LEAVE_WINDOW
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::KEY_PRESS,
            ),
    )?
    .check()?;
    conn.create_gc(gc, window, &CreateGCAux::new())?.check()?;
    icons::set_dock_icon(&conn, window)?;
    conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        b"Prychal Dock",
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        b"prychal-dock\0prychal-dock\0",
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atom("_NET_WM_PID")?,
        AtomEnum::CARDINAL,
        &[std::process::id()],
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atom("_NET_WM_WINDOW_TYPE")?,
        AtomEnum::ATOM,
        &[atom("_NET_WM_WINDOW_TYPE_DOCK")?],
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atom("_NET_WM_STATE")?,
        AtomEnum::ATOM,
        &[
            atom("_NET_WM_STATE_ABOVE")?,
            atom("_NET_WM_STATE_SKIP_TASKBAR")?,
            atom("_NET_WM_STATE_SKIP_PAGER")?,
        ],
    )?;
    conn.create_window(
        0,
        edge,
        root,
        x + layout.panel_x() as i16,
        screen.height_in_pixels.saturating_sub(2) as i16,
        layout.panel_width() as u16,
        2,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new()
            .override_redirect(1)
            .event_mask(EventMask::ENTER_WINDOW | EventMask::LEAVE_WINDOW),
    )?
    .check()?;
    let mut dnd = drop_target::DropTarget::new(&conn, window, edge)?;
    let mut popup = popup::Popup::new(&conn, root, visual.visual_id, cmap)?;
    let mut tooltip = tooltip::Tooltip::new(&conn, root, visual.visual_id, cmap)?;
    conn.map_window(window)?;
    if auto {
        conn.map_window(edge)?;
    }
    conn.flush()?;
    println!(
        "Prychal Dock: PID={} window={} geometry={}x{}+{}+{}; right-click for menu",
        std::process::id(),
        window,
        w,
        H,
        x,
        y
    );
    let mut window_panel = windows::Windows::new(&conn, root, visual.visual_id, cmap)?;
    window_panel.set_previews(preferences.previews);
    let mut settings_ui = settings_window::SettingsWindow::new(
        &conn,
        root,
        visual.visual_id,
        cmap,
        (screen.width_in_pixels, screen.height_in_pixels),
    )?;
    apps.settings_window = Some(settings_ui.window);
    if show_settings {
        settings_ui.show_settings(&conn, popup.font(), Settings { auto_hide: auto, ..preferences }, 0)?;
    }
    let start = Instant::now();
    let mut last = start;
    let mut slide = visibility::Slide::new(1., start, preferences.animation_ms);
    let mut overlap_at = Some(start);
    let mut blocked = false;
    let mut edge_hovered = false;
    let mut hover_key: Option<String> = None;
    let mut tooltip_at: Option<Instant> = None;
    let mut panel_hide_at: Option<Instant> = None;
    let mut next_frame = start;
    let mut scales = vec![0.; layout.count];
    let mut centers: Vec<_> = (0..layout.count).map(|i| layout.center(i)).collect();
    let mut visual_keys: Vec<_> = apps
        .items
        .iter()
        .take(layout.count)
        .map(|i| i.key.clone())
        .collect();
    let mut drag: Option<Drag> = None;
    let mut refresh_at: Option<Instant> = None;
    let mut scroll_changed = false;
    let mut settings_changed = false;
    let mut color_original: Option<Settings> = None;
    let mut settings_original: Option<Settings> = None;
    let mut vis = 1_f32;
    let mut target_vis = 1_f32;
    let mut pointer = None;
    let mut hide_at: Option<Instant> = None;
    let mut dirty = true;
    let mut launches = launches::Launches::new();
    let mut notice = notice::Notice::new(&conn, root, visual.visual_id, cmap)?;
    let mut pending = None;
    let mut launcher_badges = badges::Badges::new()?;
    loop {
        dirty |= launcher_badges.drain();
        loop {
            let event = if pending.is_some() {
                pending.take()
            } else {
                conn.poll_for_event()?
            };
            let Some(event) = event else { break };
            if instance.lost(&event) { return Ok(()); }
            if let Some(command) = instance.command(&event) {
                match command {
                    instance::Command::Quit => return Ok(()),
                    instance::Command::Settings => {
                        settings_ui.show_settings(&conn, popup.font(), Settings { auto_hide: auto, ..preferences }, 0)?;
                        refresh_at = Some(Instant::now());
                        dirty = true;
                    }
                }
                continue;
            }
            let (handled, change) = match settings_ui.handle(&conn, popup.font(), &event) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("Settings window: {error}");
                    (true, Some(settings_window::Change::Cancel))
                }
            };
            if handled {
                if matches!(
                    &event,
                    Event::MapNotify(_) | Event::UnmapNotify(_) | Event::PropertyNotify(_)
                ) {
                    refresh_at = Some(Instant::now());
                    dirty = true;
                }
                let changed = change.is_some();
                if let Some(change) = change {
                    match change {
                        settings_window::Change::BrowseApplications => {
                            let items = apps.available_applications();
                            settings_ui.show_applications(&conn, popup.font(), Settings { auto_hide: auto, ..preferences }, items, 0)?;
                        }
                        settings_window::Change::UnpinApplication(key) => {
                            let result = apps.unpin_application(&key).map_err(|error| error.to_string());
                            settings_ui.application_unpin_result(&key, result);
                            settings_ui.sync_application_pins(&apps.available_applications());
                            refresh_at = Some(Instant::now());
                            dirty = true;
                        }
                        settings_window::Change::PinApplication(key) => {
                            let result = apps.pin_application(&key).map_err(|error| error.to_string());
                            settings_ui.application_result(&key, result);
                            settings_ui.sync_application_pins(&apps.available_applications());
                            refresh_at = Some(Instant::now());
                            dirty = true;
                        }
                        settings_window::Change::Preview(next) => {
                            if settings_original.is_none() {
                                settings_original = Some(Settings {
                                    auto_hide: auto,
                                    ..preferences
                                });
                            }
                            preferences = next;
                            auto = next.auto_hide;
                            settings_changed = true;
                        }
                        settings_window::Change::Commit(next) => match next.save() {
                            Ok(()) => {
                                preferences = next;
                                auto = next.auto_hide;
                                settings_changed = true;
                                settings_original = None;
                            }
                            Err(error) => {
                                if let Some(original) = settings_original.take() {
                                    preferences = original;
                                    auto = original.auto_hide;
                                    settings_changed = true;
                                }
                                eprintln!("Save settings: {error}");
                            }
                        },
                        settings_window::Change::Cancel => {
                            settings_ui.cancel_drag(&conn)?;
                            if let Some(original) = settings_original.take() {
                                preferences = original;
                                auto = original.auto_hide;
                                settings_changed = true;
                            }
                        }
                        settings_window::Change::Close => {
                            if let Some(original) = settings_original.take() {
                                preferences = original;
                                auto = original.auto_hide;
                                settings_changed = true;
                            }
                            settings_ui.hide(&conn)?;
                        }
                    }
                    settings_ui.sync(
                        &conn,
                        popup.font(),
                        Settings {
                            auto_hide: auto,
                            ..preferences
                        },
                    )?;
                }
                // Render the dock between live settings updates, even with queued motion.
                if changed {
                    break;
                }
                continue;
            }
            if window_panel.visible {
                let handled = match &event {
                    Event::Expose(e) if e.window == window_panel.window => {
                        window_panel.draw(&conn, gc, popup.font())?;
                        true
                    }
                    Event::EnterNotify(e) if e.event == window_panel.window => {
                        window_panel.hovered = true;
                        panel_hide_at = None;
                        true
                    }
                    Event::LeaveNotify(e) if e.event == window_panel.window => {
                        window_panel.leave(&conn, gc, popup.font())?;
                        panel_hide_at = Some(Instant::now() + Duration::from_millis(220));
                        true
                    }
                    Event::MotionNotify(e) if e.event == window_panel.window => {
                        window_panel.motion(&conn, gc, popup.font(), e.event_x, e.event_y)?;
                        panel_hide_at = None;
                        true
                    }
                    Event::ButtonPress(e) if e.event == window_panel.window => {
                        if e.detail == 4 || e.detail == 5 {
                            window_panel.scroll(&conn, gc, popup.font(), e.detail == 4)?;
                        } else if e.detail == 1 {
                            if let Some(key) = window_panel.key.clone() {
                                if let Some(action) = window_panel.click(e.event_x, e.event_y) {
                                    match action {
                                        windows::Action::Activate(id) => {
                                            if let Err(error) =
                                                apps.activate_window(&conn, root, &key, id, e.time)
                                            {
                                                eprintln!("Activate window: {error}");
                                            }
                                            window_panel.hide(&conn)?;
                                            panel_hide_at = None;
                                        }
                                        windows::Action::Close(id) => {
                                            if let Err(error) =
                                                apps.close_window(&conn, root, &key, id, e.time)
                                            {
                                                eprintln!("Close window: {error}");
                                            }
                                        }
                                        windows::Action::CloseAll => {
                                            if let Err(error) =
                                                apps.close_group(&conn, root, &key, e.time)
                                            {
                                                eprintln!("Close windows: {error}");
                                            }
                                            window_panel.hide(&conn)?;
                                            panel_hide_at = None;
                                        }
                                    }
                                    refresh_at = Some(Instant::now() + Duration::from_millis(25));
                                    dirty = true;
                                }
                            }
                        } else if e.detail == 3 {
                            window_panel.hide(&conn)?;
                            panel_hide_at = None;
                        }
                        true
                    }
                    _ => false,
                };
                if handled {
                    continue;
                }
            }
            if let Event::Expose(e) = &event {
                if e.window == notice.window() {
                    notice.redraw(&conn, gc)?;
                    continue;
                }
                if e.window == tooltip.window {
                    tooltip.redraw(&conn, gc)?;
                    continue;
                }
            }
            let drop_was_active = dnd.active();
            match dnd.handle(&conn, &event) {
                Ok(Some(paths)) => {
                    let success = match apps.import_files(&paths) {
                        Ok(added) => {
                            eprintln!("Desktop drop: {added} new pinned application(s); source files preserved");
                            refresh_at = Some(Instant::now());
                            true
                        }
                        Err(error) => {
                            eprintln!("Desktop drop rejected: {error}");
                            false
                        }
                    };
                    dnd.complete(&conn, success)?;
                }
                Ok(None) => {}
                Err(error) => {
                    eprintln!("Desktop drop: {error}");
                    dnd.complete(&conn, false)?;
                }
            }
            if drop_was_active != dnd.active() {
                dirty = true;
                if !dnd.active() {
                    pointer = None;
                    hide_at = if auto {
                        Some(Instant::now() + Duration::from_millis(preferences.hide_delay as u64))
                    } else {
                        None
                    };
                }
            }
            if popup.visible {
                let mut action = None;
                let handled = match &event {
                    Event::Expose(e) if e.window == popup.window => {
                        popup.draw(&conn, gc)?;
                        true
                    }
                    Event::MotionNotify(e) => {
                        action = popup.motion(&conn, gc, e.event_x, e.event_y)?;
                        true
                    }
                    Event::EnterNotify(_) | Event::LeaveNotify(_) => true,
                    Event::KeyPress(e) => {
                        action = popup.key(&conn, gc, e.detail)?;
                        true
                    }
                    Event::ButtonPress(e) => {
                        if e.detail == 1 {
                            action = popup.click(e.event_x, e.event_y);
                            if action.is_none() {
                                popup.close(&conn)?;
                            }
                        } else if e.detail == 4 || e.detail == 5 {
                            action = popup.wheel(e.event_x, e.event_y, e.detail == 4);
                        } else {
                            popup.close(&conn)?;
                        }
                        true
                    }
                    Event::ButtonRelease(e) => {
                        if e.detail == 1 {
                            action = popup.release();
                        }
                        true
                    }
                    _ => false,
                };
                if handled {
                    if let Some(action) = action {
                        if !matches!(
                            &action,
                            popup::Action::Page(..)
                                | popup::Action::SetColor(..)
                                | popup::Action::ToggleBorder
                                | popup::Action::Stay
                                | popup::Action::Adjust(..)
                                | popup::Action::ToggleAutoHide
                                | popup::Action::TogglePreviews
                                | popup::Action::ResetSettings
                        ) {
                            popup.close(&conn)?;
                        }
                        match action {
                            popup::Action::AddApplication => {
                                let time = match &event { Event::ButtonPress(e) => e.time, Event::KeyPress(e) => e.time, _ => 0 };
                                let items = apps.available_applications();
                                settings_ui.show_applications(&conn, popup.font(), Settings { auto_hide: auto, ..preferences }, items, time)?;
                            }
                            popup::Action::About => {
                                let time = match &event {
                                    Event::ButtonPress(e) => e.time,
                                    Event::KeyPress(e) => e.time,
                                    _ => 0,
                                };
                                settings_ui.show_about(
                                    &conn,
                                    popup.font(),
                                    Settings {
                                        auto_hide: auto,
                                        ..preferences
                                    },
                                    time,
                                )?;
                            }
                            popup::Action::Settings => {
                                let time = match &event {
                                    Event::ButtonPress(e) => e.time,
                                    Event::KeyPress(e) => e.time,
                                    _ => 0,
                                };
                                settings_ui.show_settings(
                                    &conn,
                                    popup.font(),
                                    Settings {
                                        auto_hide: auto,
                                        ..preferences
                                    },
                                    time,
                                )?;
                            }
                            popup::Action::Page(page) => {
                                if let Some(original) = color_original.take() {
                                    preferences = original;
                                    settings_changed = true;
                                }
                                popup.open_page(
                                    &conn,
                                    gc,
                                    page,
                                    Settings {
                                        auto_hide: auto,
                                        ..preferences
                                    },
                                )?;
                            }
                            popup::Action::Stay => {}
                            popup::Action::ToggleBorder => {
                                let next = Settings {
                                    border_enabled: !preferences.border_enabled,
                                    ..preferences
                                };
                                match next.save() {
                                    Ok(()) => {
                                        preferences = next;
                                        settings_changed = true;
                                    }
                                    Err(error) => eprintln!("Settings: {error}"),
                                }
                                popup.show_settings(
                                    &conn,
                                    gc,
                                    Settings {
                                        auto_hide: auto,
                                        ..preferences
                                    },
                                )?;
                            }
                            popup::Action::SetColor(target, color, commit) => {
                                if !commit && color_original.is_none() {
                                    color_original = Some(preferences);
                                }
                                let mut next = preferences;
                                next.set_color(target, color);
                                if commit {
                                    match next.save() {
                                        Ok(()) => {
                                            preferences = next;
                                            settings_changed = true;
                                            color_original = None;
                                        }
                                        Err(error) => {
                                            if let Some(original) = color_original.take() {
                                                preferences = original;
                                                settings_changed = true;
                                            }
                                            eprintln!("Save color: {error}");
                                        }
                                    }
                                } else {
                                    preferences = next;
                                    settings_changed = true;
                                }
                                popup.show_settings(
                                    &conn,
                                    gc,
                                    Settings {
                                        auto_hide: auto,
                                        ..preferences
                                    },
                                )?;
                            }
                            popup::Action::Adjust(setting, delta) => {
                                let mut next = preferences;
                                next.adjust(setting, delta);
                                match next.save() {
                                    Ok(()) => {
                                        preferences = next;
                                        settings_changed = true;
                                    }
                                    Err(e) => eprintln!("Settings: {e}"),
                                }
                                popup.show_settings(
                                    &conn,
                                    gc,
                                    Settings {
                                        auto_hide: auto,
                                        ..preferences
                                    },
                                )?;
                            }
                            popup::Action::ToggleAutoHide => {
                                let next = Settings {
                                    auto_hide: !auto,
                                    ..preferences
                                };
                                match next.save() {
                                    Ok(()) => {
                                        preferences = next;
                                        auto = next.auto_hide;
                                        settings_changed = true;
                                    }
                                    Err(e) => eprintln!("Settings: {e}"),
                                }
                                popup.show_settings(
                                    &conn,
                                    gc,
                                    Settings {
                                        auto_hide: auto,
                                        ..preferences
                                    },
                                )?;
                            }
                            popup::Action::TogglePreviews => {
                                let next = Settings {
                                    previews: !preferences.previews,
                                    ..preferences
                                };
                                match next.save() {
                                    Ok(()) => {
                                        preferences = next;
                                        settings_changed = true;
                                    }
                                    Err(e) => eprintln!("Settings: {e}"),
                                }
                                popup.show_settings(
                                    &conn,
                                    gc,
                                    Settings {
                                        auto_hide: auto,
                                        ..preferences
                                    },
                                )?;
                            }
                            popup::Action::ResetSettings => {
                                let next = Settings::default();
                                match next.save() {
                                    Ok(()) => {
                                        preferences = next;
                                        auto = next.auto_hide;
                                        settings_changed = true;
                                    }
                                    Err(e) => eprintln!("Settings: {e}"),
                                }
                                popup.show_settings(
                                    &conn,
                                    gc,
                                    Settings {
                                        auto_hide: auto,
                                        ..preferences
                                    },
                                )?;
                            }
                            popup::Action::CloseWindow(key, id) => {
                                apps.close_window(&conn, root, &key, id, 0)?;
                            }
                            popup::Action::CloseAll(key) => {
                                apps.close_group(&conn, root, &key, 0)?;
                            }
                            popup::Action::Close => {}
                            popup::Action::Quit => return Ok(()),
                            popup::Action::Launch(key) => {
                                if let Some(item) = apps.items.iter().find(|i| i.key == key) {
                                    if let Some(launcher) = item.launcher.as_ref() {
                                        launches.start(item, launcher, Instant::now());
                                        dirty = true;
                                    }
                                }
                            }
                            popup::Action::Pin(key) => match apps.toggle_pin(&key) {
                                Ok(()) => {
                                    refresh_at = Some(Instant::now());
                                }
                                Err(e) => eprintln!("Pin: {e}"),
                            },
                        }
                    }
                    if !popup.visible {
                        if let Some(original) = color_original.take() {
                            preferences = original;
                            settings_changed = true;
                        }
                        pointer = conn
                            .query_pointer(window)?
                            .reply()
                            .ok()
                            .filter(|p| {
                                p.win_y >= 0
                                    && p.win_y < H as i16
                                    && layout.hit(p.win_x as f32).is_some()
                            })
                            .map(|p| p.win_x as f32);
                        hide_at = if auto && pointer.is_none() {
                            Some(
                                Instant::now()
                                    + Duration::from_millis(preferences.hide_delay as u64),
                            )
                        } else {
                            None
                        };
                        dirty = true;
                    }
                    continue;
                }
            }
            // Ignore queued input from a popup that was closed earlier in this batch.
            let foreign_input = match &event {
                Event::ButtonPress(e) => e.event != window,
                Event::ButtonRelease(e) => e.event != window,
                Event::MotionNotify(e) => e.event != window,
                Event::EnterNotify(e) => e.event != window && e.event != edge,
                Event::LeaveNotify(e) => e.event != window && e.event != edge,
                _ => false,
            };
            if foreign_input {
                continue;
            }
            match event {
                Event::KeyPress(e)
                    if popup.is_escape(e.detail) && drag.as_ref().is_some_and(|d| d.active) =>
                {
                    finish_drag(&conn, &mut drag)?;
                    refresh_at = Some(Instant::now());
                    pointer = None;
                    hide_at = if auto {
                        Some(Instant::now() + Duration::from_millis(preferences.hide_delay as u64))
                    } else {
                        None
                    };
                    dirty = true;
                }
                Event::ButtonPress(e)
                    if e.detail == 3 && drag.as_ref().is_some_and(|d| d.active) =>
                {
                    finish_drag(&conn, &mut drag)?;
                    refresh_at = Some(Instant::now());
                    pointer = None;
                    hide_at = if auto {
                        Some(Instant::now() + Duration::from_millis(preferences.hide_delay as u64))
                    } else {
                        None
                    };
                    dirty = true;
                }
                Event::EnterNotify(_) | Event::LeaveNotify(_)
                    if drag.as_ref().is_some_and(|d| d.active) => {}
                Event::Expose(_) => dirty = true,
                Event::ConfigureNotify(e)
                    if ![
                        window,
                        edge,
                        popup.window,
                        tooltip.window,
                        window_panel.window,
                    ]
                    .contains(&e.window) =>
                {
                    overlap_at.get_or_insert_with(|| Instant::now() + Duration::from_millis(25));
                }
                Event::MapNotify(_) | Event::UnmapNotify(_) => {
                    overlap_at.get_or_insert_with(|| Instant::now() + Duration::from_millis(25));
                }
                Event::PropertyNotify(e) if apps.atoms.watched(e.atom) => {
                    refresh_at.get_or_insert_with(|| Instant::now() + Duration::from_millis(25));
                }
                Event::DestroyNotify(_) => {
                    refresh_at.get_or_insert_with(|| Instant::now() + Duration::from_millis(25));
                }
                Event::ButtonPress(e) if e.detail == 4 || e.detail == 5 => {
                    offset = if e.detail == 4 {
                        offset.saturating_sub(1)
                    } else {
                        (offset + 1).min(apps.items.len().saturating_sub(capacity))
                    };
                    scroll_changed = true;
                }
                Event::EnterNotify(e) => {
                    if e.event == edge {
                        edge_hovered = true;
                    }
                    hide_at = None;
                    target_vis = 1.;
                    if e.event == window {
                        pointer = Some(e.event_x as f32);
                    }
                    dirty = true;
                }
                Event::LeaveNotify(e) => {
                    if e.event == edge {
                        edge_hovered = false;
                    }
                    pointer = None;
                    if auto {
                        hide_at = Some(
                            Instant::now() + Duration::from_millis(preferences.hide_delay as u64),
                        );
                    }
                }
                Event::MotionNotify(e) => {
                    pointer = Some(e.event_x as f32);
                    hide_at = None;
                    target_vis = 1.;
                    if let Some(d) = drag.as_mut() {
                        d.root_x = e.root_x;
                        let distance = (e.root_x as f32 - d.start_x as f32)
                            .hypot(e.root_y as f32 - d.start_y as f32);
                        if d.eligible && !d.active && distance >= 6. {
                            let grabbed = conn
                                .grab_pointer(
                                    false,
                                    window,
                                    EventMask::POINTER_MOTION
                                        | EventMask::BUTTON_RELEASE
                                        | EventMask::BUTTON_PRESS,
                                    GrabMode::ASYNC,
                                    GrabMode::ASYNC,
                                    0u32,
                                    0u32,
                                    e.time,
                                )?
                                .reply()?;
                            if grabbed.status == GrabStatus::SUCCESS {
                                let keyboard = conn
                                    .grab_keyboard(
                                        false,
                                        window,
                                        e.time,
                                        GrabMode::ASYNC,
                                        GrabMode::ASYNC,
                                    )?
                                    .reply()?;
                                if keyboard.status == GrabStatus::SUCCESS {
                                    d.active = true;
                                } else {
                                    conn.ungrab_pointer(0u32)?;
                                    d.eligible = false;
                                    eprintln!("Drag: keyboard is busy");
                                }
                            } else {
                                d.eligible = false;
                                eprintln!("Drag: pointer is busy");
                            }
                        }
                        if d.active {
                            let local = (e.root_x as f32 - x as f32)
                                .clamp(layout.center(0), layout.center(layout.count - 1));
                            let slot = ((local - layout.center(0)) / layout.step).round() as usize;
                            let target = (offset + slot).min(d.order.len() - 1);
                            if let Some(from) = d.order.iter().position(|key| key == &d.key) {
                                if from != target {
                                    let key = d.order.remove(from);
                                    d.order.insert(target, key);
                                    apps.preview_pin_order(&d.order);
                                    scroll_changed = true;
                                }
                            }
                            dirty = true;
                        }
                    }
                }
                Event::ButtonPress(e) if e.detail == 3 => {
                    finish_drag(&conn, &mut drag)?;
                    let index = layout.hit(e.event_x as f32);
                    let item = index.and_then(|i| apps.items.get(offset + i));
                    let anchor = (
                        e.root_x,
                        y + (BASELINE - layout.base * (1. + layout.magnification) - 10.) as i16,
                    );
                    match popup.show(
                        &conn,
                        gc,
                        item,
                        anchor,
                        (screen.width_in_pixels, screen.height_in_pixels),
                        e.time,
                    ) {
                        Ok(()) => {
                            pointer = index.map(|i| layout.center(i));
                            hide_at = None;
                            target_vis = 1.;
                        }
                        Err(e) => {
                            popup.close(&conn)?;
                            eprintln!("Menu: {e}");
                        }
                    }
                    dirty = true;
                }
                Event::ButtonPress(e) if e.detail == 1 => {
                    if let Some(i) = layout.hit(e.event_x as f32) {
                        let item = &apps.items[offset + i];
                        drag = Some(Drag {
                            key: item.key.clone(),
                            start_x: e.root_x,
                            start_y: e.root_y,
                            root_x: e.root_x,
                            eligible: item.pinned,
                            active: false,
                            order: apps.pinned_keys(),
                        });
                        hide_at = None;
                    }
                }
                Event::ButtonRelease(e) if e.detail == 1 => {
                    if let Some(d) = finish_drag(&conn, &mut drag)? {
                        let lx = e.root_x as f32 - x as f32;
                        let ly = e.root_y as f32 - y as f32;
                        let inside = lx >= layout.panel_x()
                            && lx <= layout.panel_x() + layout.panel_width()
                            && ly >= 0.
                            && ly < H as f32;
                        if d.active {
                            if inside {
                                if let Err(error) = apps.save_pin_order(d.order) {
                                    eprintln!("Save order: {error}");
                                }
                            }
                            refresh_at = Some(Instant::now());
                        } else if inside
                            && (e.root_x as f32 - d.start_x as f32)
                                .hypot(e.root_y as f32 - d.start_y as f32)
                                < 6.
                        {
                            if let Some(index) = apps.items.iter().position(|i| i.key == d.key) {
                                match apps.click(&conn, root, index, e.time) {
                                    Ok(Some(launcher)) => {
                                        launches.start(
                                            &apps.items[index],
                                            &launcher,
                                            Instant::now(),
                                        );
                                    }
                                    Ok(None) => {}
                                    Err(error) => eprintln!("Window action: {error}"),
                                }
                            }
                        }
                        pointer = if inside { Some(lx) } else { None };
                        hide_at = if auto && !inside {
                            Some(
                                Instant::now()
                                    + Duration::from_millis(preferences.hide_delay as u64),
                            )
                        } else {
                            None
                        };
                        dirty = true;
                    }
                }
                Event::Error(e) => return Err(format!("X11 error: {e:?}").into()),
                _ => {}
            }
        }
        let now = Instant::now();
        if dnd.expire(&conn, now)? {
            dirty = true;
            hide_at = if auto {
                Some(now + Duration::from_millis(preferences.hide_delay as u64))
            } else {
                None
            };
        }
        if popup.visible {
            hide_at = None;
            target_vis = 1.;
        }
        if dnd.active() {
            hide_at = None;
            target_vis = 1.;
            pointer = None;
        }
        if refresh_at.is_some_and(|t| now >= t) || scroll_changed || settings_changed {
            if settings_changed {
                window_panel.set_previews(preferences.previews);
                capacity = (screen.width_in_pixels.saturating_sub(130) as usize
                    / preferences.step() as usize)
                    .max(1);
                if auto {
                    conn.map_window(edge)?;
                } else {
                    conn.unmap_window(edge)?;
                    hide_at = None;
                    target_vis = 1.;
                }
                settings_changed = false;
                overlap_at = Some(now);
                tooltip.hide(&conn)?;
                hover_key = None;
                tooltip_at = None;
            }
            if refresh_at.is_some_and(|t| now >= t) {
                apps.refresh(&conn, root, window);
                overlap_at = Some(now);
                if let Some(d) = drag.as_ref().filter(|d| d.active) {
                    apps.preview_pin_order(&d.order);
                }
                refresh_at = None;
            }
            offset = offset.min(apps.items.len().saturating_sub(capacity));
            let new_layout = Layout::configured(apps.items.len().min(capacity), preferences);
            let keys: Vec<_> = apps
                .items
                .iter()
                .skip(offset)
                .take(new_layout.count)
                .map(|i| i.key.clone())
                .collect();
            if new_layout.width != w {
                overlap_at = Some(now);
                tooltip.hide(&conn)?;
                hover_key = None;
                tooltip_at = None;
                w = new_layout.width;
                x = (screen.width_in_pixels.saturating_sub(w) / 2) as i16;
                conn.configure_window(
                    window,
                    &ConfigureWindowAux::new().x(x as i32).width(w as u32),
                )?
                .check()?;
                conn.configure_window(
                    edge,
                    &ConfigureWindowAux::new()
                        .x(x as i32 + new_layout.panel_x() as i32)
                        .width(new_layout.panel_width() as u32),
                )?
                .check()?;
                pointer = None;
            }
            sync_visuals(&keys, &visual_keys, &mut scales, &mut centers, new_layout);
            visual_keys = keys;
            layout = new_layout;
            if popup.visible {
                pointer = popup
                    .key
                    .as_ref()
                    .and_then(|key| {
                        apps.items
                            .iter()
                            .skip(offset)
                            .take(layout.count)
                            .position(|i| &i.key == key)
                    })
                    .map(|i| layout.center(i));
            }
            scroll_changed = false;
            dirty = true;
        }
        let dragged = drag.as_ref().filter(|d| d.active).and_then(|d| {
            apps.items
                .iter()
                .skip(offset)
                .take(layout.count)
                .position(|i| i.key == d.key)
        });
        if drag.is_some() {
            hide_at = None;
            target_vis = 1.;
        }
        if let Some(i) = dragged {
            pointer = Some(layout.center(i));
        }
        let dragged_x = drag.as_ref().filter(|d| d.active).map(|d| {
            (d.root_x as f32 - x as f32).clamp(
                layout.panel_x() + layout.base / 2.,
                layout.panel_x() + layout.panel_width() - layout.base / 2.,
            )
        });
        if duration.is_some_and(|d| now - start >= d) {
            return Ok(());
        }
        if overlap_at.is_some_and(|t| now >= t) {
            let dock = visibility::Rect {
                x: x as i32 + layout.panel_x() as i32,
                y: y as i32 + layout.panel_top() as i32,
                width: layout.panel_width() as u32,
                height: layout.panel_height() as u32,
            };
            blocked = visibility::blocked(&conn, root, &apps.atoms, dock);
            overlap_at = None;
        }
        if window_panel.visible {
            let owner = window_panel.key.as_ref().and_then(|key| {
                apps.items
                    .iter()
                    .skip(offset)
                    .take(layout.count)
                    .position(|i| &i.key == key && !i.windows.is_empty())
            });
            let dock_item = pointer
                .and_then(|p| layout.hit(p))
                .and_then(|i| apps.items.get(offset + i));
            let same_icon = dock_item.is_some_and(|i| window_panel.key.as_ref() == Some(&i.key));
            let other_icon = dock_item.is_some() && !same_icon;
            if owner.is_none() || popup.visible || drag.is_some() || dnd.active() || other_icon {
                window_panel.hide(&conn)?;
                panel_hide_at = None;
            } else if window_panel.hovered || same_icon {
                panel_hide_at = None;
            } else {
                let deadline =
                    *panel_hide_at.get_or_insert_with(|| now + Duration::from_millis(220));
                if now >= deadline {
                    window_panel.hide(&conn)?;
                    panel_hide_at = None;
                }
            }
        } else {
            panel_hide_at = None;
        }
        let magnify_pointer = if window_panel.visible {
            window_panel
                .key
                .as_ref()
                .and_then(|key| {
                    apps.items
                        .iter()
                        .skip(offset)
                        .take(layout.count)
                        .position(|i| &i.key == key)
                })
                .map(|i| layout.center(i))
        } else {
            pointer
        };
        dirty |= launches.tick(&apps, now);
        notice.expire(&conn, now)?;
        while let Some(message) = launches.take_notice() {
            tooltip.hide(&conn)?;
            window_panel.hide(&conn)?;
            notice.show(
                &conn,
                gc,
                popup.font(),
                &message,
                (
                    x as i32 + w as i32 / 2,
                    y as i32 + layout.panel_top() as i32 - 12,
                ),
                (screen.width_in_pixels, screen.height_in_pixels),
                now,
            )?;
        }
        let keep_visible = !auto
            || !blocked
            || pointer.is_some()
            || edge_hovered
            || popup.visible
            || notice.visible()
            || settings_ui.visible
            || window_panel.visible
            || drag.is_some()
            || dnd.active();
        if keep_visible {
            hide_at = None;
            target_vis = 1.;
        } else if target_vis > 0. {
            let deadline = *hide_at
                .get_or_insert_with(|| now + Duration::from_millis(preferences.hide_delay as u64));
            if now >= deadline {
                hide_at = None;
                target_vis = 0.;
                pointer = None;
            }
        } else {
            hide_at = None;
        }
        slide.retarget(target_vis, vis, now, preferences.animation_ms);
        let moving = vis != target_vis
            || (vis > 0.001
                && apps
                    .items
                    .iter()
                    .skip(offset)
                    .take(layout.count)
                    .enumerate()
                    .any(|(i, item)| dragged != Some(i) && launches.animating(&item.key)))
            || (0..layout.count).any(|i| {
                (scales[i] - layout.influence(magnify_pointer, i)).abs() > 0.0001
                    || (centers[i]
                        - if dragged == Some(i) {
                            dragged_x.unwrap_or(layout.center(i))
                        } else {
                            layout.center(i)
                        })
                    .abs()
                        > 0.05
            });
        if now >= next_frame && (moving || dirty) {
            let elapsed = (now - last).as_secs_f32();
            let dt = if elapsed > 0.1 {
                1. / 60.
            } else {
                elapsed.min(0.05)
            };
            let factor = 1. - (-dt / 0.035).exp();
            vis = slide.value(now);
            for (i, s) in scales.iter_mut().enumerate() {
                let t = layout.influence(magnify_pointer, i);
                *s += (t - *s) * factor;
                if (*s - t).abs() < 0.0001 {
                    *s = t;
                }
            }
            for (i, center) in centers.iter_mut().enumerate() {
                if dragged == Some(i) {
                    *center = dragged_x.unwrap_or(layout.center(i));
                } else {
                    *center += (layout.center(i) - *center) * factor;
                    if (*center - layout.center(i)).abs() < 0.05 {
                        *center = layout.center(i);
                    }
                }
            }
            let bounces: Vec<_> = apps
                .items
                .iter()
                .skip(offset)
                .take(layout.count)
                .map(|item| launches.offset(&item.key, now))
                .collect();
            let feedback: Vec<_> = apps.items.iter().skip(offset).take(layout.count).map(|item| {
                let mut badge = apps.notification_ids.get(&item.key).map_or(badges::Badge::default(), |ids| launcher_badges.for_ids(ids));
                badge.urgent |= apps.attention.contains(&item.key);
                badge
            }).collect();
            let mut pm = draw(
                layout,
                &scales,
                &centers,
                dragged,
                vis,
                &apps.items[offset..offset + layout.count],
                Decorations {
                    active: apps.active,
                    font: Some(popup.font()),
                    bounces: &bounces,
                    badges: &feedback,
                },
            );
            if dnd.accepting() {
                let dy = (1. - vis) * H as f32;
                rect(
                    &mut pm,
                    layout.panel_x() + 12.,
                    layout.panel_top() + 1. + dy,
                    layout.panel_width() - 24.,
                    3.,
                    1.5,
                    (116, 192, 255, 255),
                );
            }
            popup::upload(&conn, window, gc, &pm)?;
            let mut regions = Vec::new();
            if vis > 0.001 {
                let dy = ((1. - vis) * H as f32) as i16;
                regions.push(Rectangle {
                    x: layout.panel_x() as i16,
                    y: layout.panel_top() as i16 + dy,
                    width: layout.panel_width() as u16,
                    height: layout.panel_height() as u16,
                });
                for (i, s) in scales.iter().enumerate() {
                    let size = layout.base * (1. + layout.magnification * s);
                    let extra = if preferences.notification_badges && feedback[i].count > 0 { preferences.notification_size as f32 } else { 0. };
                    regions.push(Rectangle {
                        x: (centers[i] - size / 2. - 4.).floor() as i16,
                        y: (BASELINE
                            - size
                            - extra
                            - if dragged == Some(i) {
                                0.
                            } else {
                                bounce_offset(size, bounces[i])
                            })
                        .floor() as i16
                            + dy,
                        width: size.ceil() as u16 + 8,
                        height: (size+extra).ceil() as u16,
                    });
                }
            }
            shape::rectangles(
                &conn,
                shape::SO::SET,
                shape::SK::INPUT,
                ClipOrdering::UNSORTED,
                window,
                0,
                0,
                &regions,
            )?;
            conn.flush()?;
            dirty = false;
            last = now;
            next_frame = now + FRAME;
        }
        let candidate = if vis >= 0.98
            && target_vis > 0.
            && !popup.visible
            && !notice.visible()
            && drag.is_none()
            && !dnd.active()
        {
            pointer.and_then(|p| layout.hit(p))
        } else {
            None
        };
        let candidate_key = candidate
            .and_then(|i| apps.items.get(offset + i))
            .map(|item| item.key.clone());
        if candidate_key != hover_key {
            tooltip.hide(&conn)?;
            hover_key = candidate_key;
            tooltip_at = hover_key.as_ref().map(|_| now + Duration::from_millis(350));
        }
        if let Some(i) = candidate {
            if tooltip.visible || tooltip_at.is_some_and(|at| now >= at) {
                let item = &apps.items[offset + i];
                let size = layout.base * (1. + layout.magnification * scales[i]);
                let anchor = (
                    x as i32 + centers[i].round() as i32,
                    y as i32 + (BASELINE - size - 8.).round() as i32,
                );
                if item.windows.is_empty() {
                    tooltip.show(
                        &conn,
                        gc,
                        popup.font(),
                        &item.name,
                        anchor,
                        (screen.width_in_pixels, screen.height_in_pixels),
                    )?;
                } else {
                    tooltip.hide(&conn)?;
                    window_panel.present(
                        &conn,
                        gc,
                        popup.font(),
                        item,
                        apps.window_entries(&item.key),
                        (
                            anchor.0,
                            anchor.1,
                            screen.width_in_pixels,
                            screen.height_in_pixels,
                        ),
                    )?;
                }
                tooltip_at = None;
            }
        }
        if window_panel.visible {
            if let Some(i) = window_panel.key.as_ref().and_then(|key| {
                apps.items
                    .iter()
                    .skip(offset)
                    .take(layout.count)
                    .position(|i| &i.key == key)
            }) {
                let item = &apps.items[offset + i];
                let size = layout.base * (1. + layout.magnification * scales[i]);
                window_panel.present(
                    &conn,
                    gc,
                    popup.font(),
                    item,
                    apps.window_entries(&item.key),
                    (
                        x as i32 + centers[i].round() as i32,
                        y as i32 + (BASELINE - size - 8.).round() as i32,
                        screen.width_in_pixels,
                        screen.height_in_pixels,
                    ),
                )?;
            }
        }
        window_panel.update_previews(&conn, gc, popup.font(), now)?;
        // Block on X11 when idle; no perpetual frame loop or pointer polling.
        let mut deadline = duration.map(|d| start + d);
        for d in [
            hide_at,
            refresh_at,
            overlap_at,
            tooltip_at,
            panel_hide_at,
            window_panel.preview_deadline(),
            dnd.deadline(),
            if moving || dirty {
                Some(next_frame)
            } else {
                None
            },
            launches.deadline(),
            notice.deadline(),
        ]
        .into_iter()
        .flatten()
        {
            deadline = Some(deadline.map_or(d, |old| old.min(d)));
        }
        let timeout = deadline.map_or(-1, |d| {
            d.saturating_duration_since(Instant::now())
                .as_millis()
                .saturating_add(1)
                .min(i32::MAX as u128) as i32
        });
        // Property replies can queue events internally while refreshing the model.
        // Drain those before blocking on the socket; they need not leave it readable.
        if let Some(event) = conn.poll_for_event()? {
            pending = Some(event);
            continue;
        }
        let mut fds = [
            rustix::event::PollFd::new(conn.stream(), rustix::event::PollFlags::IN),
            rustix::event::PollFd::new(launcher_badges.wake(), rustix::event::PollFlags::IN),
        ];
        match rustix::event::poll(&mut fds, timeout) {
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focus_and_layout() {
        let l = Layout::new(5);
        assert_eq!(l.influence(Some(l.center(2)), 2), 1.);
        assert!(l.influence(Some(l.center(2)), 1) < 0.02);
        assert_eq!(l.hit(l.center(4)), Some(4));
        assert_eq!(l.hit(0.), None);
        assert!(Layout::new(6).panel_width() > l.panel_width());
    }
    #[test]
    fn fixed_background() {
        let l = Layout::new(5);
        let items: Vec<_> = (0..5)
            .map(|i| Item {
                key: i.to_string(),
                icon: None,
                windows: vec![],
                name: i.to_string(),
                pinned: false,
                launcher: None,
            })
            .collect();
        let idle = draw(
            l,
            &[0.; 5],
            &(0..5).map(|i| l.center(i)).collect::<Vec<_>>(),
            None,
            1.,
            &items,
            Decorations::default(),
        );
        let zoom = draw(
            l,
            &[1.; 5],
            &(0..5).map(|i| l.center(i)).collect::<Vec<_>>(),
            None,
            1.,
            &items,
            Decorations::default(),
        );
        assert_eq!(idle.width(), zoom.width());
        assert_eq!(
            &idle.data()[4 * (110 * idle.width() as usize + l.panel_x() as usize + 1)..][..4],
            &zoom.data()[4 * (110 * zoom.width() as usize + l.panel_x() as usize + 1)..][..4]
        );
        assert!(draw(
            l,
            &[1.; 5],
            &(0..5).map(|i| l.center(i)).collect::<Vec<_>>(),
            None,
            0.,
            &items,
            Decorations::default()
        )
        .data()
        .iter()
        .all(|&b| b == 0));
    }
}
