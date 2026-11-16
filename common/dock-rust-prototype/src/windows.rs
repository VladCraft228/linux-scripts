//! Hover window chooser with bounded compositor previews and app-icon fallback.
use crate::{
    apps::{Item, WindowInfo},
    popup, previews, rect, Result,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};
use x11rb::{connection::Connection, protocol::xproto::*, wrapper::ConnectionExt as _};
const STEP: i32 = 220;
const TOP: i32 = 32;
const HEIGHT: u32 = 206;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hit {
    Window(usize),
    Close(usize),
    CloseAll,
}
pub enum Action {
    Activate(u32),
    Close(u32),
    CloseAll,
}
pub struct Windows {
    pub window: u32,
    pub visible: bool,
    pub hovered: bool,
    pub key: Option<String>,
    name: String,
    entries: Vec<WindowInfo>,
    icon: Option<Arc<Pixmap>>,
    offset: usize,
    columns: usize,
    width: u32,
    hover: Option<Hit>,
    position: Option<(i32, i32)>,
    previews: previews::Previews,
    previews_enabled: bool,
    next_preview: Option<Instant>,
}
impl Windows {
    pub fn new<C: Connection>(c: &C, root: u32, visual: u32, colormap: u32) -> Result<Self> {
        let window = c.generate_id()?;
        c.create_window(
            32,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new()
                .background_pixel(0)
                .border_pixel(0)
                .colormap(colormap)
                .override_redirect(1)
                .event_mask(
                    EventMask::EXPOSURE
                        | EventMask::ENTER_WINDOW
                        | EventMask::LEAVE_WINDOW
                        | EventMask::POINTER_MOTION
                        | EventMask::BUTTON_PRESS,
                ),
        )?
        .check()?;
        let atom =
            |s: &str| -> Result<u32> { Ok(c.intern_atom(false, s.as_bytes())?.reply()?.atom) };
        c.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"Prychal Dock Window Chooser",
        )?;
        c.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"prychal-dock-windows\0prychal-dock-windows\0",
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
            &[atom("_NET_WM_WINDOW_TYPE_POPUP_MENU")?],
        )?;
        Ok(Self {
            window,
            visible: false,
            hovered: false,
            key: None,
            name: String::new(),
            entries: vec![],
            icon: None,
            offset: 0,
            columns: 0,
            width: 1,
            hover: None,
            position: None,
            previews: previews::Previews::new(c, root),
            previews_enabled: true,
            next_preview: None,
        })
    }
    pub fn present<C: Connection>(
        &mut self,
        c: &C,
        gc: u32,
        font: &fontdue::Font,
        item: &Item,
        entries: Vec<WindowInfo>,
        placement: (i32, i32, u16, u16),
    ) -> Result<()> {
        if entries.is_empty() {
            return self.hide(c);
        }
        let same_key = self.key.as_ref() == Some(&item.key);
        let columns = entries
            .len()
            .min(((placement.2.saturating_sub(24) as usize) / STEP as usize).clamp(1, 6));
        let width = (columns as u32 * STEP as u32 + 16)
            .min(placement.2.saturating_sub(16).max(1) as u32);
        let same_icon = match (&self.icon, &item.icon) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        let changed = !same_key
            || self.entries != entries
            || self.name != item.name
            || self.columns != columns
            || self.width != width
            || !same_icon;
        if !same_key {
            self.offset = 0;
            self.hover = None;
        }
        self.offset = self.offset.min(entries.len().saturating_sub(columns));
        self.key = Some(item.key.clone());
        self.name = item.name.clone();
        self.icon = item.icon.clone();
        self.entries = entries;
        self.columns = columns;
        self.width = width;
        let height = self.height();
        let position = (
            (placement.0 - width as i32 / 2)
                .clamp(0, placement.2.saturating_sub(width as u16) as i32),
            (placement.1 - height as i32)
                .clamp(0, placement.3.saturating_sub(height as u16) as i32),
        );
        if self.position != Some(position) || changed {
            c.configure_window(
                self.window,
                &ConfigureWindowAux::new()
                    .x(position.0)
                    .y(position.1)
                    .width(width)
                    .height(height)
                    .stack_mode(StackMode::ABOVE),
            )?
            .check()?;
            self.position = Some(position);
        }
        if !self.visible {
            c.map_window(self.window)?;
            if self.previews_enabled && self.previews.available {
                self.next_preview = Some(Instant::now());
            }
        }
        if changed || !self.visible {
            self.draw(c, gc, font)?;
        }
        self.visible = true;
        c.flush()?;
        Ok(())
    }
    pub fn hide<C: Connection>(&mut self, c: &C) -> Result<()> {
        if self.visible {
            c.unmap_window(self.window)?;
            c.flush()?;
        }
        self.visible = false;
        self.next_preview = None;
        self.previews.mark_stale();
        self.hovered = false;
        self.hover = None;
        Ok(())
    }
    pub fn set_previews(&mut self, enabled: bool) {
        self.previews_enabled = enabled;
        self.next_preview = if enabled && self.visible && self.previews.available {
            Some(Instant::now())
        } else {
            None
        };
    }
    pub fn preview_deadline(&self) -> Option<Instant> {
        if self.visible && self.previews_enabled {
            self.next_preview
        } else {
            None
        }
    }
    pub fn update_previews<C: Connection>(
        &mut self,
        c: &C,
        gc: u32,
        font: &fontdue::Font,
        now: Instant,
    ) -> Result<()> {
        if self.preview_deadline().is_none_or(|at| now < at) {
            return Ok(());
        }
        let ids: Vec<_> = self
            .entries
            .iter()
            .skip(self.offset)
            .take(self.columns)
            .map(|w| w.id)
            .collect();
        if self.previews.refresh(c, &ids) {
            self.draw(c, gc, font)?;
        }
        self.next_preview = Some(Instant::now() + Duration::from_millis(250));
        Ok(())
    }
    fn top(&self) -> i32 {
        TOP + if self.entries.len() > 1 { 10 } else { 0 }
    }
    fn height(&self) -> u32 {
        HEIGHT + (self.top() - TOP) as u32
    }
    fn close_all_bounds(&self) -> (i32, i32, i32, i32) {
        (self.width as i32 - 104, 7, 92, 25)
    }
    fn left(&self) -> i32 {
        (self.width as i32 - self.columns as i32 * STEP) / 2
    }
    fn hit(&self, x: i16, y: i16) -> Option<Hit> {
        let (x, y) = (x as i32, y as i32);
        let (button_x,button_y,button_w,button_h) = self.close_all_bounds();
        if self.entries.len() > 1 && (button_x..button_x+button_w).contains(&x) && (button_y..button_y+button_h).contains(&y) {
            return Some(Hit::CloseAll);
        }
        let top = self.top();
        if !(top..top + 154).contains(&y) {
            return None;
        }
        let local = x - self.left();
        if local < 0 || local >= self.columns as i32 * STEP {
            return None;
        }
        let column = (local / STEP) as usize;
        let within = local % STEP;
        if !(4..STEP - 4).contains(&within) {
            return None;
        }
        let index = self.offset + column;
        if index >= self.entries.len() {
            return None;
        }
        if (within as f32 - 200.5).hypot(y as f32 - top as f32 - 15.5) <= 10.5 {
            Some(Hit::Close(index))
        } else {
            Some(Hit::Window(index))
        }
    }
    pub fn motion<C: Connection>(
        &mut self,
        c: &C,
        gc: u32,
        font: &fontdue::Font,
        x: i16,
        y: i16,
    ) -> Result<()> {
        self.hovered = true;
        let hit = self.hit(x, y);
        if self.hover != hit {
            self.hover = hit;
            self.draw(c, gc, font)?;
        }
        Ok(())
    }
    pub fn leave<C: Connection>(&mut self, c: &C, gc: u32, font: &fontdue::Font) -> Result<()> {
        self.hovered = false;
        if self.hover.take().is_some() {
            self.draw(c, gc, font)?;
        }
        Ok(())
    }
    pub fn click(&self, x: i16, y: i16) -> Option<Action> {
        match self.hit(x, y)? {
            Hit::Window(i) => self.entries.get(i).map(|w| Action::Activate(w.id)),
            Hit::Close(i) => self.entries.get(i).map(|w| Action::Close(w.id)),
            Hit::CloseAll => Some(Action::CloseAll),
        }
    }
    pub fn scroll<C: Connection>(
        &mut self,
        c: &C,
        gc: u32,
        font: &fontdue::Font,
        up: bool,
    ) -> Result<()> {
        let next = if up {
            self.offset.saturating_sub(1)
        } else {
            (self.offset + 1).min(self.entries.len().saturating_sub(self.columns))
        };
        if next != self.offset {
            self.offset = next;
            self.hover = None;
            self.draw(c, gc, font)?;
        }
        Ok(())
    }
    pub fn draw<C: Connection>(&self, c: &C, gc: u32, font: &fontdue::Font) -> Result<()> {
        let mut image =
            Pixmap::new(self.width, self.height()).ok_or("Window chooser allocation failed")?;
        rect(
            &mut image,
            0.,
            0.,
            self.width as f32,
            self.height() as f32,
            12.,
            (28, 33, 43, 249),
        );
        let reserved = if self.entries.len() > 1 { 132. } else { 28. };
        let header = shorten(font, &self.name, 13., (self.width as f32 - reserved).max(60.));
        popup::text(&mut image, font, &header, 14., 24., 13., [225, 232, 244]);
        if self.entries.len() > 1 {
            let (bx,by,bw,bh) = self.close_all_bounds();
            rect(
                &mut image,
                bx as f32,
                by as f32,
                bw as f32,
                bh as f32,
                6.,
                if self.hover == Some(Hit::CloseAll) {
                    (183, 70, 78, 255)
                } else {
                    (67, 49, 59, 255)
                },
            );
            let text_width: f32 = "Close all".chars().map(|ch| font.metrics(ch,12.).advance_width).sum();
            let baseline = font.horizontal_line_metrics(12.).map_or(by as f32 + 17., |m| by as f32 + bh as f32/2. + (m.ascent+m.descent)/2.);
            popup::text(
                &mut image,
                font,
                "Close all",
                bx as f32 + (bw as f32-text_width)/2.,
                baseline,
                12.,
                [255, 223, 226],
            );
        }
        for slot in 0..self.columns {
            let index = self.offset + slot;
            let Some(entry) = self.entries.get(index) else {
                continue;
            };
            let x = self.left() as f32 + slot as f32 * STEP as f32 + 4.;
            let y = self.top() as f32;
            let over = self.hover == Some(Hit::Window(index));
            rect(
                &mut image,
                x,
                y,
                212.,
                154.,
                10.,
                if entry.active {
                    (103, 177, 246, 255)
                } else {
                    (68, 77, 94, 255)
                },
            );
            rect(
                &mut image,
                x + 1.5,
                y + 1.5,
                209.,
                151.,
                9.,
                if over {
                    (58, 81, 117, 255)
                } else {
                    (39, 46, 61, 255)
                },
            );
            let preview = if self.previews_enabled {
                self.previews.frame(entry.id)
            } else {
                None
            };
            if let Some(frame) = preview {
                rect(
                    &mut image,
                    x + 8.,
                    y + 8.,
                    196.,
                    104.,
                    5.,
                    (18, 22, 30, 255),
                );
                image.draw_pixmap(
                    (x + 8. + (196. - frame.image.width() as f32) / 2.).round() as i32,
                    (y + 8. + (104. - frame.image.height() as f32) / 2.).round() as i32,
                    frame.image.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            } else if let Some(icon) = &self.icon {
                let tr = Transform::from_translate(x + 82., y + 32.)
                    .pre_scale(48. / icon.width() as f32, 48. / icon.height() as f32);
                let paint = PixmapPaint {
                    quality: FilterQuality::Bilinear,
                    ..Default::default()
                };
                image.draw_pixmap(0, 0, icon.as_ref().as_ref(), &paint, tr, None);
            } else {
                rect(
                    &mut image,
                    x + 82.,
                    y + 32.,
                    48.,
                    48.,
                    10.,
                    (79, 111, 161, 255),
                );
            }
            let lines = title_lines(font, &entry.title, 192.);
            for (line, label) in lines.iter().enumerate() {
                let width: f32 = label
                    .chars()
                    .map(|ch| font.metrics(ch, 12.).advance_width)
                    .sum();
                popup::text(
                    &mut image,
                    font,
                    label,
                    x + (212. - width) / 2.,
                    y + 127. + line as f32 * 17.,
                    12.,
                    [239, 243, 249],
                );
            }
            let closing = self.hover == Some(Hit::Close(index));
            rect(
                &mut image,
                x + 186.,
                y + 5.,
                21.,
                21.,
                10.5,
                if closing {
                    (210, 77, 84, 255)
                } else {
                    (73, 81, 99, 220)
                },
            );
            let (cx,cy) = (x+196.5,y+15.5);
            let mut cross = tiny_skia::PathBuilder::new();
            cross.move_to(cx-3.5,cy-3.5); cross.line_to(cx+3.5,cy+3.5);
            cross.move_to(cx+3.5,cy-3.5); cross.line_to(cx-3.5,cy+3.5);
            if let Some(path) = cross.finish() {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color_rgba8(255,255,255,255);
                image.stroke_path(&path,&paint,&tiny_skia::Stroke { width:1.7, line_cap:tiny_skia::LineCap::Round, ..Default::default() },Transform::identity(),None);
            }
        }
        let footer = if self.entries.len() > self.columns {
            format!(
                "{}–{} / {}   ·   scroll to see more",
                self.offset + 1,
                (self.offset + self.columns).min(self.entries.len()),
                self.entries.len()
            )
        } else {
            format!("{} {}", self.entries.len(), if self.entries.len() == 1 { "window" } else { "windows" })
        };
        popup::text(&mut image, font, &footer, 12., self.height() as f32-7., 10., [164, 179, 199]);
        popup::upload(c, self.window, gc, &image)?;
        c.flush()?;
        Ok(())
    }
}
fn shorten(font: &fontdue::Font, text: &str, size: f32, max: f32) -> String {
    let mut out = String::new();
    let mut width = 0.;
    let ellipsis = font.metrics('…', size).advance_width;
    for ch in text.chars() {
        let w = font.metrics(ch, size).advance_width;
        if width + w + ellipsis > max {
            out.push('…');
            return out;
        }
        out.push(ch);
        width += w;
    }
    out
}
fn title_lines(font: &fontdue::Font, text: &str, max: f32) -> Vec<String> {
    let mut lines = vec![String::new()];
    let mut width = 0.;
    for ch in text.chars() {
        let advance = font.metrics(ch, 12.).advance_width;
        if ch == '\n' || width + advance > max {
            if lines.len() == 2 {
                let last = lines.last_mut().unwrap();
                *last = shorten(font, &format!("{last}{ch}…"), 12., max);
                break;
            }
            lines.push(String::new());
            width = 0.;
            if ch == '\n' {
                continue;
            }
        }
        lines.last_mut().unwrap().push(ch);
        width += advance;
    }
    lines
}
