use crate::{popup, rect, Result};
use tiny_skia::Pixmap;
use x11rb::{
    connection::Connection,
    protocol::{shape, xproto::*},
    wrapper::ConnectionExt as _,
};
/// Input-transparent launch feedback; never grabs focus or input.
pub struct Notice {
    window: u32,
    visible: bool,
    deadline: Option<std::time::Instant>,
    image: Option<Pixmap>,
}
impl Notice {
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
                .event_mask(EventMask::EXPOSURE),
        )?
        .check()?;
        let a = |name: &str| -> Result<u32> {
            Ok(c.intern_atom(false, name.as_bytes())?.reply()?.atom)
        };
        c.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"Prychal Dock Launch Message",
        )?;
        c.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"prychal-dock-notice\0prychal-dock-notice\0",
        )?;
        c.change_property32(
            PropMode::REPLACE,
            window,
            a("_NET_WM_PID")?,
            AtomEnum::CARDINAL,
            &[std::process::id()],
        )?;
        c.change_property32(
            PropMode::REPLACE,
            window,
            a("_NET_WM_WINDOW_TYPE")?,
            AtomEnum::ATOM,
            &[a("_NET_WM_WINDOW_TYPE_NOTIFICATION")?],
        )?;
        shape::rectangles(
            c,
            shape::SO::SET,
            shape::SK::INPUT,
            ClipOrdering::UNSORTED,
            window,
            0,
            0,
            &[],
        )?;
        Ok(Self {
            window,
            visible: false,
            deadline: None,
            image: None,
        })
    }
    pub fn window(&self) -> u32 {
        self.window
    }
    pub fn visible(&self) -> bool {
        self.visible
    }
    pub fn deadline(&self) -> Option<std::time::Instant> {
        self.deadline
    }
    pub fn expire<C: Connection>(&mut self, c: &C, now: std::time::Instant) -> Result<()> {
        if self.deadline.is_some_and(|at| now >= at) {
            self.hide(c)?;
            self.deadline = None;
        }
        Ok(())
    }
    pub fn show<C: Connection>(
        &mut self,
        c: &C,
        gc: u32,
        font: &fontdue::Font,
        message: &crate::launches::Notification,
        anchor: (i32, i32),
        screen: (u16, u16),
        now: std::time::Instant,
    ) -> Result<()> {
        let width = u32::from(screen.0.saturating_sub(24)).clamp(64, 520);
        let mut image = Pixmap::new(width, 106).ok_or("Notice allocation failed")?;
        rect(
            &mut image,
            0.,
            0.,
            width as f32,
            106.,
            12.,
            (55, 32, 38, 250),
        );
        popup::text(
            &mut image,
            font,
            &message.title,
            14.,
            25.,
            14.,
            [255, 211, 215],
        );
        let mut lines = vec![String::new()];
        let mut length = 0.;
        for ch in message.detail.chars() {
            let ch = if ch.is_control() { ' ' } else { ch };
            let advance = font.metrics(ch, 12.).advance_width;
            if length + advance > width as f32 - 32. {
                if lines.len() == 3 {
                    lines.last_mut().unwrap().push('…');
                    break;
                }
                lines.push(String::new());
                length = 0.;
            }
            lines.last_mut().unwrap().push(ch);
            length += advance;
        }
        for (index, line) in lines.iter().enumerate() {
            popup::text(
                &mut image,
                font,
                line,
                14.,
                47. + index as f32 * 18.,
                12.,
                [244, 232, 234],
            );
        }
        let position = (
            (anchor.0 - width as i32 / 2).clamp(0, screen.0.saturating_sub(width as u16) as i32),
            (anchor.1 - 106).clamp(0, screen.1.saturating_sub(106) as i32),
        );
        c.configure_window(
            self.window,
            &ConfigureWindowAux::new()
                .x(position.0)
                .y(position.1)
                .width(width)
                .height(106)
                .stack_mode(StackMode::ABOVE),
        )?
        .check()?;
        c.map_window(self.window)?;
        popup::upload(c, self.window, gc, &image)?;
        self.image = Some(image);
        self.visible = true;
        self.deadline = Some(now + std::time::Duration::from_secs(6));
        c.flush()?;
        Ok(())
    }
    pub fn redraw<C: Connection>(&self, c: &C, gc: u32) -> Result<()> {
        if self.visible {
            if let Some(image) = &self.image {
                popup::upload(c, self.window, gc, image)?;
                c.flush()?;
            }
        }
        Ok(())
    }
    pub fn hide<C: Connection>(&mut self, c: &C) -> Result<()> {
        if self.visible {
            c.unmap_window(self.window)?;
            c.flush()?;
            self.visible = false;
        }
        Ok(())
    }
}
