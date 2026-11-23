use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, OnceLock},
};
use tiny_skia::{Pixmap, PixmapPaint, Transform};

pub struct Icons {
    pub theme: String,
    cache: HashMap<String, Option<Arc<Pixmap>>>,
}
impl Icons {
    pub fn new() -> Self {
        let theme = Command::new("xfconf-query")
            .args(["-c", "xsettings", "-p", "/Net/IconThemeName"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let config = std::env::var_os("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .or_else(|| {
                        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
                    })?;
                let text = fs::read_to_string(config.join("gtk-3.0/settings.ini")).ok()?;
                text.lines().find_map(|line| {
                    let (k, v) = line.split_once('=')?;
                    (k.trim() == "gtk-icon-theme-name")
                        .then(|| v.trim().trim_matches('"').to_owned())
                })
            })
            .unwrap_or_else(|| "hicolor".into());
        eprintln!("System icon theme: {theme}");
        Self {
            theme,
            cache: HashMap::new(),
        }
    }
    pub fn get(&mut self, name: &str) -> Option<Arc<Pixmap>> {
        if name.is_empty() {
            return None;
        }
        if let Some(icon) = self.cache.get(name) {
            return icon.clone();
        }
        if name == "prychal-dock" || name == "linux-scripts-dock-rust" {
            let icon = dock_icon();
            self.cache.insert(name.into(), icon.clone());
            return icon;
        }
        let path = if Path::new(name).is_absolute() {
            Some(PathBuf::from(name))
        } else {
            freedesktop_icons::lookup(name)
                .with_size(96)
                .with_theme(&self.theme)
                .find()
        };
        let result = path.and_then(|p| load(&p)).map(Arc::new);
        self.cache.insert(name.into(), result.clone());
        result
    }
}
/// The application brand is embedded, so development and installed builds match.
pub fn dock_icon() -> Option<Arc<Pixmap>> {
    static ICON: OnceLock<Option<Arc<Pixmap>>> = OnceLock::new();
    ICON.get_or_init(|| {
        let tree = resvg::usvg::Tree::from_data(include_bytes!("../assets/prychal-dock.svg"), &resvg::usvg::Options::default()).ok()?;
        let mut image = Pixmap::new(96, 96)?;
        let scale = (96. / tree.size().width()).min(96. / tree.size().height());
        let transform = Transform::from_translate((96.-tree.size().width()*scale)/2., (96.-tree.size().height()*scale)/2.).pre_scale(scale,scale);
        resvg::render(&tree, transform, &mut image.as_mut());
        Some(Arc::new(image))
    }).clone()
}
/// Publish straight-alpha ARGB pixels for XFWM titlebars, switchers and task lists.
pub fn set_dock_icon<C: x11rb::connection::Connection>(connection: &C, window: u32) -> crate::Result<()> {
    use x11rb::{protocol::xproto::*, wrapper::ConnectionExt as _};
    let Some(icon) = dock_icon() else { return Ok(()); };
    let mut words = Vec::with_capacity(2 + icon.data().len()/4);
    words.extend([icon.width(), icon.height()]);
    for pixel in icon.data().chunks_exact(4) {
        let alpha = u32::from(pixel[3]);
        let straight = |value:u8| if alpha == 0 { 0 } else { ((u32::from(value)*255+alpha/2)/alpha).min(255) };
        words.push((alpha<<24)|(straight(pixel[0])<<16)|(straight(pixel[1])<<8)|straight(pixel[2]));
    }
    let atom = connection.intern_atom(false,b"_NET_WM_ICON")?.reply()?.atom;
    connection.change_property32(PropMode::REPLACE,window,atom,AtomEnum::CARDINAL,&words)?.check()?;
    Ok(())
}
fn load(path: &Path) -> Option<Pixmap> {
    if fs::metadata(path).ok()?.len() > 8 * 1024 * 1024 {
        return None;
    }
    let data = fs::read(path).ok()?;
    if path.extension().is_some_and(|s| s == "svg" || s == "svgz") {
        let options = resvg::usvg::Options {
            resources_dir: path.parent().map(Path::to_path_buf),
            ..Default::default()
        };
        let tree = resvg::usvg::Tree::from_data(&data, &options).ok()?;
        let mut pm = Pixmap::new(96, 96)?;
        let scale = (96. / tree.size().width()).min(96. / tree.size().height());
        let tr = Transform::from_translate(
            (96. - tree.size().width() * scale) / 2.,
            (96. - tree.size().height() * scale) / 2.,
        )
        .pre_scale(scale, scale);
        resvg::render(&tree, tr, &mut pm.as_mut());
        Some(pm)
    } else {
        let pm = Pixmap::decode_png(&data).ok()?;
        normalize(&pm)
    }
}
fn normalize(src: &Pixmap) -> Option<Pixmap> {
    let mut pm = Pixmap::new(96, 96)?;
    let scale = (96. / src.width() as f32).min(96. / src.height() as f32);
    let transform = Transform::from_translate(
        (96. - src.width() as f32 * scale) / 2.,
        (96. - src.height() as f32 * scale) / 2.,
    )
    .pre_scale(scale, scale);
    let paint = PixmapPaint {
        quality: tiny_skia::FilterQuality::Bilinear,
        ..Default::default()
    };
    pm.draw_pixmap(0, 0, src.as_ref(), &paint, transform, None);
    Some(pm)
}
/// Bounded EWMH ARGB icon decoder. Pixel bytes are premultiplied for tiny-skia.
pub fn window_icon(data: &[u32]) -> Option<Arc<Pixmap>> {
    let mut i = 0;
    let mut best: Option<(usize, usize, u32, u32)> = None;
    while i + 2 <= data.len() {
        let (w, h) = (data[i], data[i + 1]);
        i += 2;
        let len = (w as usize).checked_mul(h as usize)?;
        let end = i.checked_add(len)?;
        if end > data.len() {
            break;
        }
        if w > 0 && h > 0 && w <= 1024 && h <= 1024 {
            let distance = (w as i64 - 96).unsigned_abs() + (h as i64 - 96).unsigned_abs();
            if best.is_none_or(|(_, _, bw, bh)| {
                distance < (bw as i64 - 96).unsigned_abs() + (bh as i64 - 96).unsigned_abs()
            }) {
                best = Some((i, end, w, h));
            }
        }
        i = end;
    }
    let (start, end, w, h) = best?;
    let mut pm = Pixmap::new(w, h)?;
    for (pixel, &argb) in pm.data_mut().chunks_exact_mut(4).zip(&data[start..end]) {
        let a = (argb >> 24) as u8;
        pixel.copy_from_slice(&[
            ((argb >> 16 & 255) * a as u32 / 255) as u8,
            ((argb >> 8 & 255) * a as u32 / 255) as u8,
            ((argb & 255) * a as u32 / 255) as u8,
            a,
        ]);
    }
    normalize(&pm).map(Arc::new)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn argb_and_bad_data() {
        assert!(window_icon(&[999, 999, 1]).is_none());
        assert!(window_icon(&[0, 0]).is_none());
        let p = window_icon(&[1, 1, 0x80ff0000]).unwrap();
        assert_eq!(&p.data()[4 * (48 * 96 + 48)..][..4], &[128, 0, 0, 128]);
    }
    #[test]
    fn png_and_svg() {
        let temp = std::env::temp_dir().join(format!("dock-svg-test-{}.svg", std::process::id()));
        fs::write(&temp,r#"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><rect width="48" height="48" fill="red"/></svg>"#).unwrap();
        let p = load(&temp).unwrap();
        fs::remove_file(temp).unwrap();
        assert_eq!(&p.data()[..4], &[255, 0, 0, 255]);
    }
}
