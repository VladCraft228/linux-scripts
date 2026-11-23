use crate::Result;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Copy)]
pub enum Setting {
    IconSize,
    Spacing,
    Magnification,
    Opacity,
    HideDelay,
    Animation,
    BorderWidth,
    BorderOpacity,
    CornerRadius,
    IndicatorStyle,
    NotificationSize,
    AttentionSize,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColorTarget {
    Background,
    Border,
    Active,
    Running,
    Notification,
    Attention,
}
impl ColorTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Background => "Background color",
            Self::Border => "Border color",
            Self::Active => "Active indicator color",
            Self::Running => "Running indicator color",
            Self::Notification => "Notification counter color",
            Self::Attention => "Attention dot color",
        }
    }
}
pub fn rgba(rgb: u32, alpha: u8) -> (u8, u8, u8, u8) {
    ((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, alpha)
}
#[derive(Clone, Copy)]
pub struct Settings {
    pub icon_size: u16,
    pub spacing: u16,
    pub magnification: u16,
    pub opacity: u16,
    pub auto_hide: bool,
    pub previews: bool,
    pub hide_delay: u16,
    pub animation_ms: u16,
    pub background: u32,
    pub border_color: u32,
    pub border_enabled: bool,
    pub border_width: u16,
    pub border_opacity: u16,
    pub corner_radius: u16,
    pub indicator_style: u16,
    pub active_color: u32,
    pub running_color: u32,
    pub notification_badges: bool,
    pub attention_dots: bool,
    pub notification_color: u32,
    pub attention_color: u32,
    pub notification_size: u16,
    pub attention_size: u16,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            icon_size: 48,
            spacing: 10,
            magnification: 55,
            opacity: 85,
            auto_hide: true,
            previews: true,
            hide_delay: 650,
            animation_ms: 120,
            background: 0x272d39,
            border_color: 0xffffff,
            border_enabled: true,
            border_width: 1,
            border_opacity: 25,
            corner_radius: 20,
            indicator_style: 0,
            active_color: 0x74c0ff,
            running_color: 0xc6cedc,
            notification_badges: true,
            attention_dots: true,
            notification_color: 0xda4056,
            attention_color: 0xffa44d,
            notification_size: 18,
            attention_size: 7,
        }
    }
}
impl Settings {
    pub fn step(self) -> f32 {
        (self.icon_size + self.spacing) as f32
    }
    fn path() -> Result<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|p| p.join("linux-scripts-dock-rust-prototype/settings.json"))
            .ok_or_else(|| "HOME/XDG_CONFIG_HOME is missing".into())
    }
    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        let data = match fs::read(&path) {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        if data.len() > 64 * 1024 {
            return Err("settings.json is larger than 64 KiB; preserved".into());
        }
        let value: serde_json::Value = serde_json::from_slice(&data)
            .map_err(|e| format!("Cannot read {}: {e}; preserved", path.display()))?;
        if !value.is_object() {
            return Err("settings.json must be an object; preserved".into());
        }
        let d = Self::default();
        let integer = |key: &str, default: u16, min: u16, max: u16| -> Result<u16> {
            match value.get(key) {
                None => Ok(default),
                Some(v) => Ok(v
                    .as_u64()
                    .ok_or_else(|| format!("Invalid setting {key}; settings.json preserved"))?
                    .clamp(min as u64, max as u64) as u16),
            }
        };
        let color = |key: &str, default: u32| -> Result<u32> {
            match value.get(key) {
                None => Ok(default),
                Some(v) => Ok(v
                    .as_u64()
                    .ok_or_else(|| format!("Invalid color {key}; settings.json preserved"))?
                    .min(0xffffff) as u32),
            }
        };
        Ok(Self {
            background: color("backgroundColor", d.background)?,
            border_color: color("borderColor", d.border_color)?,
            active_color: color("activeColor", d.active_color)?,
            running_color: color("runningColor", d.running_color)?,
            notification_color: color("notificationColor", d.notification_color)?,
            attention_color: color("attentionColor", d.attention_color)?,
            notification_size: integer("notificationSize", d.notification_size, 14, 24)?,
            attention_size: integer("attentionSize", d.attention_size, 6, 14)?,
            notification_badges: match value.get("notificationBadges") {
                None => d.notification_badges,
                Some(v) => v.as_bool().ok_or("Invalid notificationBadges; settings.json preserved")?,
            },
            attention_dots: match value.get("attentionDots") {
                None => d.attention_dots,
                Some(v) => v.as_bool().ok_or("Invalid attentionDots; settings.json preserved")?,
            },
            border_enabled: match value.get("borderEnabled") {
                None => d.border_enabled,
                Some(v) => v
                    .as_bool()
                    .ok_or("Invalid borderEnabled; settings.json preserved")?,
            },
            border_width: integer("borderWidth", d.border_width, 1, 8)?,
            border_opacity: integer("borderOpacity", d.border_opacity, 0, 100)?,
            corner_radius: integer("cornerRadius", d.corner_radius, 0, 40)?,
            indicator_style: integer("indicatorStyle", d.indicator_style, 0, 4)?,
            icon_size: integer("iconSize", d.icon_size, 24, 64)?,
            spacing: integer("spacing", d.spacing, 0, 32)?,
            magnification: integer("magnification", d.magnification, 0, 100)?,
            opacity: integer("opacity", d.opacity, 0, 100)?,
            hide_delay: integer("hideDelay", d.hide_delay, 0, 3000)?,
            animation_ms: integer("animationDuration", d.animation_ms, 60, 500)?,
            previews: match value.get("previews") {
                None => d.previews,
                Some(v) => v
                    .as_bool()
                    .ok_or("Invalid previews; settings.json preserved")?,
            },
            auto_hide: match value.get("autoHide") {
                None => d.auto_hide,
                Some(v) => v
                    .as_bool()
                    .ok_or("Invalid autoHide; settings.json preserved")?,
            },
        })
    }
    pub fn adjust(&mut self, setting: Setting, delta: i32) {
        if matches!(setting, Setting::IndicatorStyle) {
            self.indicator_style = (self.indicator_style as i32 + delta).rem_euclid(5) as u16;
            return;
        }
        let (value, min, max) = match setting {
            Setting::IconSize => (&mut self.icon_size, 24, 64),
            Setting::Spacing => (&mut self.spacing, 0, 32),
            Setting::Magnification => (&mut self.magnification, 0, 100),
            Setting::Opacity => (&mut self.opacity, 0, 100),
            Setting::HideDelay => (&mut self.hide_delay, 0, 3000),
            Setting::Animation => (&mut self.animation_ms, 60, 500),
            Setting::BorderWidth => (&mut self.border_width, 1, 8),
            Setting::BorderOpacity => (&mut self.border_opacity, 0, 100),
            Setting::CornerRadius => (&mut self.corner_radius, 0, 40),
            Setting::IndicatorStyle => unreachable!(),
            Setting::NotificationSize => (&mut self.notification_size, 14, 24),
            Setting::AttentionSize => (&mut self.attention_size, 6, 14),
        };
        *value = (*value as i32).saturating_add(delta).clamp(min, max) as u16;
    }
    pub fn color(self, target: ColorTarget) -> u32 {
        match target {
            ColorTarget::Background => self.background,
            ColorTarget::Border => self.border_color,
            ColorTarget::Active => self.active_color,
            ColorTarget::Running => self.running_color,
            ColorTarget::Notification => self.notification_color,
            ColorTarget::Attention => self.attention_color,
        }
    }
    pub fn set_color(&mut self, target: ColorTarget, color: u32) {
        let value = match target {
            ColorTarget::Background => &mut self.background,
            ColorTarget::Border => &mut self.border_color,
            ColorTarget::Active => &mut self.active_color,
            ColorTarget::Running => &mut self.running_color,
            ColorTarget::Notification => &mut self.notification_color,
            ColorTarget::Attention => &mut self.attention_color,
        };
        *value = color & 0xffffff;
    }
    pub fn indicator_name(self) -> &'static str {
        ["Pill", "Dot", "Line", "Square", "Hidden"][self.indicator_style.min(4) as usize]
    }
    pub fn save(self) -> Result<()> {
        let path = Self::path()?;
        let parent = path.parent().ok_or("Invalid settings path")?;
        fs::create_dir_all(parent)?;
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp = parent.join(format!(".settings-{}-{nonce}.tmp", std::process::id()));
        let value = serde_json::json!({"iconSize":self.icon_size,"spacing":self.spacing,"magnification":self.magnification,"opacity":self.opacity,"autoHide":self.auto_hide,"previews":self.previews,"hideDelay":self.hide_delay,"animationDuration":self.animation_ms,"backgroundColor":self.background,"borderColor":self.border_color,"borderEnabled":self.border_enabled,"borderWidth":self.border_width,"borderOpacity":self.border_opacity,"cornerRadius":self.corner_radius,"indicatorStyle":self.indicator_style,"activeColor":self.active_color,"runningColor":self.running_color,"notificationBadges":self.notification_badges,"attentionDots":self.attention_dots,"notificationColor":self.notification_color,"attentionColor":self.attention_color,"notificationSize":self.notification_size,"attentionSize":self.attention_size});
        let result = (|| -> Result<()> {
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp)?;
            f.write_all(&serde_json::to_vec_pretty(&value)?)?;
            f.write_all(b"\n")?;
            f.sync_all()?;
            fs::rename(&temp, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adjustment_bounds_preserve_unrelated_settings() {
        let mut s = Settings::default();
        s.adjust(Setting::IconSize, 1000);
        assert_eq!(s.icon_size, 64);
        s.adjust(Setting::Spacing, -1000);
        assert_eq!(s.spacing, 0);
        s.adjust(Setting::Opacity, -1000);
        assert_eq!(s.opacity, 0);
        s.adjust(Setting::Magnification, 1000);
        assert_eq!(s.magnification, 100);
        assert!(134. - s.icon_size as f32 * (1. + s.magnification as f32 / 100.) >= 0.);
        assert!(s.auto_hide);
    }
}
