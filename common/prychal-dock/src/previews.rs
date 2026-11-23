//! Read existing compositor pixmaps, downscale on the server, fetch only thumbnails.
//! Never redirect windows or retain full-size backing pixmaps between captures.
use crate::Result;
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};
use tiny_skia::Pixmap;
use x11rb::{
    connection::Connection,
    protocol::{composite, render, xproto::*},
};
pub const WIDTH: u16 = 196;
pub const HEIGHT: u16 = 104;
const CACHE_LIMIT: usize = 24;
pub struct Frame {
    pub image: Pixmap,
    pub stale: bool,
    used: Instant,
}
pub struct Previews {
    pub available: bool,
    root: u32,
    format: u32,
    visuals: HashMap<u32, u32>,
    frames: HashMap<u32, Frame>,
    warned: HashSet<u32>,
}
#[derive(Default)]
struct Resources {
    named: Option<u32>,
    source: Option<u32>,
    small: Option<u32>,
    destination: Option<u32>,
}
impl Resources {
    fn release<C: Connection>(&self, c: &C) {
        for picture in [self.source, self.destination].into_iter().flatten() {
            if let Ok(cookie) = render::free_picture(c, picture) {
                let _ = cookie.check();
            }
        }
        for pixmap in [self.named, self.small].into_iter().flatten() {
            if let Ok(cookie) = c.free_pixmap(pixmap) {
                let _ = cookie.check();
            }
        }
    }
}
impl Previews {
    pub fn new<C: Connection>(c: &C, root: u32) -> Self {
        let composite = composite::query_version(c, 0, 2)
            .ok()
            .and_then(|r| r.reply().ok())
            .is_some_and(|v| v.major_version > 0 || v.minor_version >= 2);
        let render_available = render::query_version(c, 0, 11)
            .ok()
            .and_then(|r| r.reply().ok())
            .is_some_and(|v| v.major_version > 0 || v.minor_version >= 10);
        let formats = render::query_pict_formats(c)
            .ok()
            .and_then(|r| r.reply().ok());
        let mut visuals = HashMap::new();
        let mut format = 0;
        if let Some(reply) = formats {
            for screen in &reply.screens {
                for depth in &screen.depths {
                    for visual in &depth.visuals {
                        visuals.insert(visual.visual, visual.format);
                    }
                }
            }
            format = reply
                .formats
                .iter()
                .find(|f| {
                    f.depth == 32
                        && f.type_ == render::PictType::DIRECT
                        && f.direct.red_mask == 255
                        && f.direct.red_shift == 16
                        && f.direct.green_mask == 255
                        && f.direct.green_shift == 8
                        && f.direct.blue_mask == 255
                        && f.direct.blue_shift == 0
                        && f.direct.alpha_mask == 255
                        && f.direct.alpha_shift == 24
                })
                .map(|f| f.id)
                .unwrap_or(0);
        }
        let pixels = c
            .setup()
            .pixmap_formats
            .iter()
            .any(|f| f.depth == 32 && f.bits_per_pixel == 32 && f.scanline_pad == 32);
        let available = composite && render_available && format != 0 && pixels;
        eprintln!(
            "Window previews: {}",
            if available {
                "XComposite + XRender available"
            } else {
                "unavailable; using application icons"
            }
        );
        Self {
            available,
            root,
            format,
            visuals,
            frames: HashMap::new(),
            warned: HashSet::new(),
        }
    }
    pub fn frame(&self, window: u32) -> Option<&Frame> {
        self.frames.get(&window)
    }
    pub fn mark_stale(&mut self) {
        for frame in self.frames.values_mut() {
            frame.stale = true;
        }
    }
    pub fn refresh<C: Connection>(&mut self, c: &C, windows: &[u32]) -> bool {
        if !self.available {
            return false;
        }
        let now = Instant::now();
        let mut changed = false;
        for &window in windows {
            match self.capture(c, window) {
                Ok(image) => {
                    let differs = self.frames.get(&window).is_none_or(|old| {
                        old.stale
                            || old.image.width() != image.width()
                            || old.image.height() != image.height()
                            || old.image.data() != image.data()
                    });
                    if differs {
                        self.frames.insert(
                            window,
                            Frame {
                                image,
                                stale: false,
                                used: now,
                            },
                        );
                        changed = true;
                    } else if let Some(frame) = self.frames.get_mut(&window) {
                        frame.used = now;
                    }
                    self.warned.remove(&window);
                }
                Err(error) => {
                    if let Some(frame) = self.frames.get_mut(&window) {
                        changed |= !frame.stale;
                        frame.stale = true;
                        frame.used = now;
                    } else if !self.warned.contains(&window) {
                        eprintln!("Preview window {window}: {error}; application icon fallback");
                        if self.warned.len() >= 64 {
                            self.warned.clear();
                        }
                        self.warned.insert(window);
                    }
                }
            }
        }
        while self.frames.len() > CACHE_LIMIT {
            let oldest = self
                .frames
                .iter()
                .filter(|(id, _)| !windows.contains(id))
                .min_by_key(|(_, f)| f.used)
                .map(|(&id, _)| id);
            if let Some(id) = oldest {
                self.frames.remove(&id);
            } else {
                break;
            }
        }
        changed
    }
    fn capture<C: Connection>(&self, c: &C, window: u32) -> Result<Pixmap> {
        let attributes = c.get_window_attributes(window)?.reply()?;
        if attributes.map_state != MapState::VIEWABLE {
            return Err("window is minimized or unmapped".into());
        }
        // XFWM normally redirects the reparenting frame, not the application child.
        let mut frame = window;
        for _ in 0..16 {
            let tree = c.query_tree(frame)?.reply()?;
            if tree.parent == self.root || tree.parent == 0 {
                break;
            }
            frame = tree.parent;
        }
        let mut last = None;
        for source in [frame, window] {
            match self.capture_source(c, source) {
                Ok(image) => return Ok(image),
                Err(e) => last = Some(e),
            }
            if frame == window {
                break;
            }
        }
        Err(last.unwrap_or_else(|| "no compositor pixmap".into()))
    }
    fn capture_source<C: Connection>(&self, c: &C, window: u32) -> Result<Pixmap> {
        let mut resources = Resources::default();
        let result = (|| -> Result<Pixmap> {
            let attributes = c.get_window_attributes(window)?.reply()?;
            let source_format = *self
                .visuals
                .get(&attributes.visual)
                .ok_or("unsupported source visual")?;
            let named = c.generate_id()?;
            composite::name_window_pixmap(c, window, named)?.check()?;
            resources.named = Some(named);
            let geometry = c.get_geometry(named)?.reply()?;
            let (width, height) =
                fit(geometry.width, geometry.height).ok_or("empty window geometry")?;
            let source = c.generate_id()?;
            render::create_picture(
                c,
                source,
                named,
                source_format,
                &render::CreatePictureAux::new().repeat(render::Repeat::PAD),
            )?
            .check()?;
            resources.source = Some(source);
            let small = c.generate_id()?;
            c.create_pixmap(32, small, self.root, width, height)?
                .check()?;
            resources.small = Some(small);
            let destination = c.generate_id()?;
            render::create_picture(
                c,
                destination,
                small,
                self.format,
                &render::CreatePictureAux::new(),
            )?
            .check()?;
            resources.destination = Some(destination);
            let fixed =
                |value: f64| -> i32 { (value * 65536.).round().clamp(1., i32::MAX as f64) as i32 };
            let transform = render::Transform {
                matrix11: fixed(geometry.width as f64 / width as f64),
                matrix12: 0,
                matrix13: 0,
                matrix21: 0,
                matrix22: fixed(geometry.height as f64 / height as f64),
                matrix23: 0,
                matrix31: 0,
                matrix32: 0,
                matrix33: 65536,
            };
            render::set_picture_transform(c, source, transform)?.check()?;
            // The default nearest filter remains usable if bilinear is unavailable.
            if let Ok(cookie) = render::set_picture_filter(c, source, b"bilinear", &[]) {
                let _ = cookie.check();
            }
            render::composite(
                c,
                render::PictOp::SRC,
                source,
                0u32,
                destination,
                0,
                0,
                0,
                0,
                0,
                0,
                width,
                height,
            )?
            .check()?;
            let reply = c
                .get_image(ImageFormat::Z_PIXMAP, small, 0, 0, width, height, u32::MAX)?
                .reply()?;
            if reply.depth != 32 {
                return Err("unexpected thumbnail depth".into());
            }
            decode(
                width,
                height,
                &reply.data,
                c.setup().image_byte_order == ImageOrder::LSB_FIRST,
            )
        })();
        resources.release(c);
        result
    }
}
fn fit(width: u16, height: u16) -> Option<(u16, u16)> {
    if width == 0 || height == 0 {
        return None;
    }
    let scale = (WIDTH as f64 / width as f64)
        .min(HEIGHT as f64 / height as f64)
        .min(1.);
    Some((
        (width as f64 * scale).round().clamp(1., WIDTH as f64) as u16,
        (height as f64 * scale).round().clamp(1., HEIGHT as f64) as u16,
    ))
}
fn decode(width: u16, height: u16, bytes: &[u8], little: bool) -> Result<Pixmap> {
    let needed = width as usize * height as usize * 4;
    if bytes.len() < needed {
        return Err("incomplete thumbnail pixels".into());
    }
    let mut image =
        Pixmap::new(width as u32, height as u32).ok_or("thumbnail allocation failed")?;
    let mut visible = false;
    for (source, destination) in bytes[..needed]
        .chunks_exact(4)
        .zip(image.data_mut().chunks_exact_mut(4))
    {
        let b: [u8; 4] = source.try_into()?;
        let pixel = if little {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        };
        let a = (pixel >> 24) as u8;
        visible |= a != 0;
        destination.copy_from_slice(&[
            ((pixel >> 16) as u8).min(a),
            ((pixel >> 8) as u8).min(a),
            (pixel as u8).min(a),
            a,
        ]);
    }
    if !visible {
        return Err("compositor returned an empty transparent frame".into());
    }
    Ok(image)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aspect_fit() {
        assert_eq!(fit(1920, 1080), Some((185, 104)));
        assert_eq!(fit(800, 1600), Some((52, 104)));
        assert_eq!(fit(0, 10), None);
    }
    #[test]
    fn pixels_and_empty() {
        assert_eq!(
            &decode(1, 1, &[0x33, 0x22, 0x11, 0xff], true)
                .unwrap()
                .data()[..4],
            &[0x11, 0x22, 0x33, 0xff]
        );
        assert_eq!(
            &decode(1, 1, &[0xff, 0x11, 0x22, 0x33], false)
                .unwrap()
                .data()[..4],
            &[0x11, 0x22, 0x33, 0xff]
        );
        assert!(decode(1, 1, &[0, 0, 0, 0], true).is_err());
        assert!(decode(1, 1, &[1, 2], true).is_err());
    }
}
