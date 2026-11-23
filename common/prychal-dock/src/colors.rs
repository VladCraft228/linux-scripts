use tiny_skia::Pixmap;
pub fn hsv(h: f32, s: f32, v: f32) -> u32 {
    let h = h.rem_euclid(1.) * 6.;
    let s = s.clamp(0., 1.);
    let v = v.clamp(0., 1.);
    let f = h.fract();
    let (p, q, t) = (v * (1. - s), v * (1. - s * f), v * (1. - s * (1. - f)));
    let (r, g, b) = match h.floor() as u8 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    (((r * 255.).round() as u32) << 16)
        | (((g * 255.).round() as u32) << 8)
        | (b * 255.).round() as u32
}
pub fn from_rgb(rgb: u32) -> (f32, f32, f32) {
    let (r, g, b) = (
        ((rgb >> 16) & 255) as f32 / 255.,
        ((rgb >> 8) & 255) as f32 / 255.,
        (rgb & 255) as f32 / 255.,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0. {
        0.
    } else if max == r {
        ((g - b) / d).rem_euclid(6.) / 6.
    } else if max == g {
        ((b - r) / d + 2.) / 6.
    } else {
        ((r - g) / d + 4.) / 6.
    };
    (h, if max == 0. { 0. } else { d / max }, max)
}
pub fn wheel(radius: u32) -> Pixmap {
    let side = radius * 2;
    let mut image = Pixmap::new(side, side).unwrap();
    for y in 0..side {
        for x in 0..side {
            let dx = x as f32 + 0.5 - radius as f32;
            let dy = y as f32 + 0.5 - radius as f32;
            let distance = dx.hypot(dy);
            let coverage = (radius as f32 + 0.5 - distance).clamp(0., 1.);
            if coverage == 0. {
                continue;
            }
            let color = hsv(
                dy.atan2(dx) / std::f32::consts::TAU,
                (distance / radius as f32).min(1.),
                1.,
            );
            let alpha = (coverage * 255.).round() as u32;
            let index = 4 * (y * side + x) as usize;
            image.data_mut()[index..index + 4].copy_from_slice(&[
                (((color >> 16) & 255) * alpha / 255) as u8,
                (((color >> 8) & 255) * alpha / 255) as u8,
                ((color & 255) * alpha / 255) as u8,
                alpha as u8,
            ]);
        }
    }
    image
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rgb_round_trip() {
        for color in [
            0, 0xffffff, 0x272d39, 0x74c0ff, 0xff0000, 0x00ff00, 0x0000ff,
        ] {
            let (h, s, v) = from_rgb(color);
            assert_eq!(hsv(h, s, v), color);
        }
    }
}
