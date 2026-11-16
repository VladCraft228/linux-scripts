use crate::apps::{words, Atoms};
use std::time::{Duration, Instant};
use x11rb::{connection::Connection, protocol::xproto::*};
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
impl Rect {
    pub fn overlaps(self, other: Self) -> bool {
        self.width > 0
            && self.height > 0
            && other.width > 0
            && other.height > 0
            && (self.x as i64) < other.x as i64 + other.width as i64
            && (other.x as i64) < self.x as i64 + self.width as i64
            && (self.y as i64) < other.y as i64 + other.height as i64
            && (other.y as i64) < self.y as i64 + self.height as i64
    }
}
/// Query only after relevant window events; never poll every animation frame.
pub fn blocked<C: Connection>(c: &C, root: u32, a: &Atoms, dock: Rect) -> bool {
    if words(c, root, a.showing_desktop).first() == Some(&1) {
        return false;
    }
    let desktop = words(c, root, a.current).first().copied().unwrap_or(0);
    for window in words(c, root, a.clients) {
        if words(c, window, a.pid).first() == Some(&std::process::id()) {
            continue;
        }
        let types = words(c, window, a.types);
        if !types.is_empty() && !types.contains(&a.normal) && !types.contains(&a.dialog) {
            continue;
        }
        let state = words(c, window, a.state);
        if state.contains(&a.hidden) || state.contains(&a.skip) {
            continue;
        }
        if words(c, window, a.desktop)
            .first()
            .is_some_and(|&d| d != desktop && d != u32::MAX)
        {
            continue;
        }
        let Some(attributes) = c
            .get_window_attributes(window)
            .ok()
            .and_then(|r| r.reply().ok())
        else {
            continue;
        };
        if attributes.map_state != MapState::VIEWABLE {
            continue;
        }
        let Some(geometry) = c.get_geometry(window).ok().and_then(|r| r.reply().ok()) else {
            continue;
        };
        let Some(origin) = c
            .translate_coordinates(window, root, 0, 0)
            .ok()
            .and_then(|r| r.reply().ok())
        else {
            continue;
        };
        let extents = words(c, window, a.frame_extents);
        let edge = |i: usize| extents.get(i).copied().unwrap_or(0).min(16384);
        let (left, right, top, bottom) = (edge(0), edge(1), edge(2), edge(3));
        let rect = Rect {
            x: origin.dst_x as i32 - left as i32,
            y: origin.dst_y as i32 - top as i32,
            width: geometry.width as u32 + left + right,
            height: geometry.height as u32 + top + bottom,
        };
        if dock.overlaps(rect) {
            return true;
        }
    }
    false
}
/// Exact, symmetric full-travel duration. Magnification uses its own animation.
pub struct Slide {
    from: f32,
    to: f32,
    started: Instant,
    duration: Duration,
    milliseconds: u16,
}
impl Slide {
    pub fn new(value: f32, now: Instant, ms: u16) -> Self {
        Self {
            from: value,
            to: value,
            started: now,
            duration: Duration::ZERO,
            milliseconds: ms,
        }
    }
    pub fn retarget(&mut self, to: f32, current: f32, now: Instant, ms: u16) {
        if self.to != to || self.milliseconds != ms {
            self.from = current;
            self.to = to;
            self.started = now;
            self.milliseconds = ms;
            self.duration =
                Duration::from_secs_f32((ms as f32 / 1000. * (to - current).abs()).max(0.001));
        }
    }
    pub fn value(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.to;
        }
        let t = (now.saturating_duration_since(self.started).as_secs_f32()
            / self.duration.as_secs_f32())
        .min(1.);
        if t >= 1. {
            return self.to;
        }
        let eased = t * t * (3. - 2. * t);
        self.from + (self.to - self.from) * eased
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edge_touch_is_not_overlap() {
        let d = Rect {
            x: 100,
            y: 100,
            width: 100,
            height: 50,
        };
        assert!(!d.overlaps(Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 200
        }));
        assert!(d.overlaps(Rect {
            x: 199,
            y: 90,
            width: 10,
            height: 40
        }));
        assert!(!d.overlaps(Rect {
            x: 100,
            y: 200,
            width: 100,
            height: 50
        }));
    }
    #[test]
    fn show_hide_same_duration() {
        let now = Instant::now();
        let mut hide = Slide::new(1., now, 120);
        hide.retarget(0., 1., now, 120);
        let mid = hide.value(now + Duration::from_millis(60));
        assert!((mid - 0.5).abs() < 0.001);
        assert_eq!(hide.value(now + Duration::from_millis(120)), 0.);
        let mut show = Slide::new(0., now, 120);
        show.retarget(1., 0., now, 120);
        assert!((show.value(now + Duration::from_millis(60)) - mid).abs() < 0.001);
        assert_eq!(show.value(now + Duration::from_millis(120)), 1.);
    }
}
