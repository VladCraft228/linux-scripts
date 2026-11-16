use crate::{popup, rect, Result};
use tiny_skia::Pixmap;
use x11rb::{
    connection::Connection,
    protocol::{shape, xproto::*},
    wrapper::ConnectionExt as _,
};
/// Input-transparent labels: no focus changes or keyboard/pointer grabs.
pub struct Tooltip {
    pub window: u32,
    pub visible: bool,
    name: String,
    image: Option<Pixmap>,
    position: Option<(i32, i32)>,
}
impl Tooltip {
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
            b"Prychal Dock Tooltip",
        )?;
        c.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"prychal-dock-tooltip\0prychal-dock-tooltip\0",
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
            &[a("_NET_WM_WINDOW_TYPE_TOOLTIP")?],
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
            name: String::new(),
            image: None,
            position: None,
        })
    }
    pub fn show<C: Connection>(
        &mut self,
        c: &C,
        gc: u32,
        font: &fontdue::Font,
        name: &str,
        anchor: (i32, i32),
        screen: (u16, u16),
    ) -> Result<()> {
        let changed = self.image.is_none() || self.name != name;
        if changed {
            let mut label = String::new();
            let mut length = 0.;
            for ch in name.chars() {
                let advance = font.metrics(ch, 13.).advance_width;
                if length + advance > 320. {
                    label.push('…');
                    break;
                }
                length += advance;
                label.push(ch);
            }
            let length: f32 = label
                .chars()
                .map(|ch| font.metrics(ch, 13.).advance_width)
                .sum();
            let width = (length.ceil() as u32 + 24).clamp(64, 360);
            let mut image = Pixmap::new(width, 30).ok_or("Tooltip allocation failed")?;
            rect(&mut image, 0., 0., width as f32, 30., 8., (28, 33, 42, 245));
            popup::text(
                &mut image,
                font,
                &label,
                (width as f32 - length) / 2.,
                20.,
                13.,
                [241, 245, 250],
            );
            self.name = name.into();
            self.image = Some(image);
        }
        let image = self.image.as_ref().ok_or("Missing tooltip image")?;
        let position = (
            (anchor.0 - image.width() as i32 / 2)
                .clamp(0, screen.0.saturating_sub(image.width() as u16) as i32),
            (anchor.1 - image.height() as i32)
                .clamp(0, screen.1.saturating_sub(image.height() as u16) as i32),
        );
        if changed || self.position != Some(position) {
            c.configure_window(
                self.window,
                &ConfigureWindowAux::new()
                    .x(position.0)
                    .y(position.1)
                    .width(image.width())
                    .height(image.height())
                    .stack_mode(StackMode::ABOVE),
            )?
            .check()?;
            self.position = Some(position);
        }
        if !self.visible {
            c.map_window(self.window)?;
        }
        if changed || !self.visible {
            popup::upload(c, self.window, gc, image)?;
        }
        self.visible = true;
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
