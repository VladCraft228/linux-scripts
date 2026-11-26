use crate::appearance::{Appearance, ACCENTS, BOLT_OPTIONS, PERCENT_OPTIONS};
use crate::history::{self, History, DISCHARGE_WINDOWS, WINDOW_SECONDS};
use crate::presets::{self, CpuRange, Presets, EPP_OPTIONS};
use crate::profiles::{Diagnostics, PowerSource, Profile};
use crate::telemetry::Snapshot;
use crate::tray::Indicator;
use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;
use std::process::Command;
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke, Transform};

const GREEN: [u8; 3] = [112, 222, 181];
const TEXT: [u8; 3] = [231, 239, 246];
const MUTED: [u8; 3] = [161, 181, 199];
pub const SIDEBAR_WIDTH: f32 = 160.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page { Overview, Modes, Advanced, Diagnostics, Appearance, About }

impl Page {
    const ALL: [Self; 6] = [Self::Overview, Self::Modes, Self::Advanced, Self::Diagnostics, Self::Appearance, Self::About];

    pub fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Modes => "Power modes",
            Self::Advanced => "Advanced",
            Self::Diagnostics => "Diagnostics",
            Self::Appearance => "Appearance",
            Self::About => "About",
        }
    }
}

fn sidebar_y(index: usize) -> f32 { 88.0 + 43.0 * index as f32 }

pub fn sidebar_hit(x: f64, y: f64) -> Option<Page> {
    Page::ALL.into_iter().enumerate()
        .find(|(index, _)| hit(x, y, 12.0, sidebar_y(*index), 136.0))
        .map(|(_, page)| page)
}

pub fn accent_hit(x: f64, y: f64) -> Option<usize> {
    (0..ACCENTS.len()).find(|&index| hit(x, y, accent_x(index), 145.0, 118.0))
}

fn accent_x(index: usize) -> f32 { 44.0 + index as f32 * 128.0 }

const WHEEL_SIZE: u32 = 174;
const WHEEL_LEFT: f32 = 44.0;
const WHEEL_TOP: f32 = 210.0;
const STYLE_OPTION_WIDTH: f32 = 132.0;
const STYLE_X: f32 = 250.0;
const PERCENT_TOP: f32 = 259.0;
const BOLT_TOP: f32 = 343.0;
const INDICATOR_TOP: f32 = 507.0;
const INDICATOR_X: f32 = 44.0;
const INDICATOR_WIDTH: f32 = 128.0;

fn indicator_x(index: usize) -> f32 {
    INDICATOR_X + index as f32 * (INDICATOR_WIDTH + 8.0)
}

fn style_x(index: usize) -> f32 { STYLE_X + index as f32 * (STYLE_OPTION_WIDTH + 8.0) }

pub enum AppearanceEdit {
    Accent([u8; 3]),
    Percent(usize),
    Bolt(usize),
    Indicator(Indicator),
}

pub fn appearance_hit(
    x: f64, y: f64, xfce_panel_available: bool, selected: Indicator,
) -> Option<AppearanceEdit> {
    for (index, indicator) in Indicator::ALL.into_iter().enumerate() {
        if (indicator != Indicator::XfcePanel || xfce_panel_available)
            && hit(x, y, indicator_x(index), INDICATOR_TOP, INDICATOR_WIDTH)
        {
            return Some(AppearanceEdit::Indicator(indicator));
        }
    }
    if let Some(index) = accent_hit(x, y) {
        return Some(AppearanceEdit::Accent(ACCENTS[index].0));
    }
    if let Some(color) = wheel_color(x as f32 - WHEEL_LEFT, y as f32 - WHEEL_TOP) {
        return Some(AppearanceEdit::Accent(color));
    }
    if selected == Indicator::Off { return None; }
    for index in 0..PERCENT_OPTIONS.len() {
        if hit(x, y, style_x(index), PERCENT_TOP, STYLE_OPTION_WIDTH) {
            return Some(AppearanceEdit::Percent(index));
        }
    }
    for index in 0..BOLT_OPTIONS.len() {
        if hit(x, y, style_x(index), BOLT_TOP, STYLE_OPTION_WIDTH) {
            return Some(AppearanceEdit::Bolt(index));
        }
    }
    None
}

fn wheel_color(x: f32, y: f32) -> Option<[u8; 3]> {
    let radius = WHEEL_SIZE as f32 / 2.0;
    let (dx, dy) = ((x - radius) / radius, (y - radius) / radius);
    let saturation = dx.hypot(dy);
    if saturation > 1.0 { return None; }
    let hue = (dy.atan2(dx) / std::f32::consts::TAU).rem_euclid(1.0) * 6.0;
    let component = |offset: f32| {
        let wave = (hue + offset).rem_euclid(6.0_f32);
        let channel = (wave.min(4.0 - wave)).clamp(0.0, 1.0);
        (255.0 * (1.0 - saturation * channel)).round() as u8
    };
    Some([component(5.0_f32), component(3.0_f32), component(1.0_f32)])
}

fn wheel_marker(color: [u8; 3]) -> (f32, f32) {
    let [r, g, b] = color.map(f32::from);
    let max = r.max(g).max(b);
    let delta = max - r.min(g).min(b);
    let radius = WHEEL_SIZE as f32 / 2.0;
    if delta == 0.0 { return (radius, radius); }
    let hue = if r == max { (g - b) / delta }
        else if g == max { 2.0 + (b - r) / delta }
        else { 4.0 + (r - g) / delta };
    let angle = hue * std::f32::consts::TAU / 6.0;
    let distance = radius * delta / max;
    (radius + distance * angle.cos(), radius + distance * angle.sin())
}

fn make_wheel() -> Option<Pixmap> {
    let mut wheel = Pixmap::new(WHEEL_SIZE, WHEEL_SIZE)?;
    for (index, pixel) in wheel.data_mut().chunks_exact_mut(4).enumerate() {
        let x = (index % WHEEL_SIZE as usize) as f32 + 0.5;
        let y = (index / WHEEL_SIZE as usize) as f32 + 0.5;
        if let Some([r, g, b]) = wheel_color(x, y) {
            pixel.copy_from_slice(&[r, g, b, 255]);
        }
    }
    Some(wheel)
}

pub struct Review {
    pub args: Vec<String>,
    title: String,
    rows: Vec<[String; 5]>,
    pub raising: bool,
}

impl Review {
    pub fn has_changes(&self) -> bool {
        self.rows.iter().any(|[_, current_epp, next_epp, current_cap, next_cap]| {
            current_epp != next_epp || current_cap != next_cap
        })
    }

    pub fn load(title: &str, arguments: &[&str], helper: &str) -> Result<Self, String> {
        if !std::path::Path::new(helper).is_file() {
            return Err("Install the Vatra TLP helper to preview changes.".into());
        }
        let output = Command::new(helper).arg("--preview").args(arguments).output()
            .map_err(|err| format!("Cannot open power preview: {err}"))?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(if message.is_empty() { "Power preview unavailable.".into() } else { message });
        }
        let body = String::from_utf8(output.stdout).map_err(|_| "Invalid power preview.")?;
        let mut lines = body.lines();
        let invalid = "Invalid power preview. Update the Vatra TLP helper.";
        let token = lines.next().filter(|token| token.len() == 64
            && token.bytes().all(|byte| byte.is_ascii_hexdigit())).ok_or(invalid)?;
        let raising = match lines.next() {
            Some("raise") => true,
            Some("normal") => false,
            _ => return Err(invalid.into()),
        };
        let rows: Vec<[String; 5]> = lines.map(|line| {
            let fields: Vec<_> = line.split('\t').map(str::to_owned).collect();
            fields.try_into().map_err(|_| invalid)
        }).collect::<Result<_, _>>()?;
        if rows.is_empty() || rows.len() > 2 { return Err(invalid.into()); }
        let mut args: Vec<String> = arguments.iter().map(|arg| (*arg).to_owned()).collect();
        args.push(format!("--expect={token}"));
        Ok(Self { args, title: title.into(), rows, raising })
    }
}

pub struct Dashboard {
    font: Font,
    wheel: Option<Pixmap>,
    glyphs: HashMap<(char, u16), (Metrics, Vec<u8>)>,
}

impl Dashboard {
    pub fn new() -> Result<Self, String> {
        let output = Command::new("fc-match")
            .args(["-f", "%{file}", "sans-serif"])
            .output()
            .map_err(|err| format!("Cannot run fc-match: {err}"))?;
        let path = std::str::from_utf8(&output.stdout).map_err(|err| err.to_string())?;
        if !output.status.success() || path.is_empty() {
            return Err("No usable system font found".into());
        }
        let bytes = std::fs::read(path).map_err(|err| format!("Cannot read font {path}: {err}"))?;
        let font = Font::from_bytes(bytes, FontSettings::default()).map_err(str::to_owned)?;
        Ok(Self { font, wheel: make_wheel(), glyphs: HashMap::new() })
    }

    pub fn draw(
        &mut self,
        pixels: &mut Pixmap,
        scale: f32,
        snapshot: &Snapshot,
        history: &History,
        profiles: Option<(usize, &Diagnostics)>,
        active_mode: Option<Profile>,
        indicator: Indicator,
        xfce_panel_available: bool,
        indicator_status: &str,
        advanced: bool,
        preview_source: Option<PowerSource>,
        presets: &Presets,
        cpu_ranges: Result<&[CpuRange; 2], &str>,
        apply_message: &str,
        applying: bool,
        review: Option<&Review>,
        page: Page,
        appearance: Appearance,
        diagnostics: Option<&Diagnostics>,
        helper_available: bool,
        cursor: Option<(f64, f64)>,
        range_minutes: u64,
        battery_fill: Option<f64>,
        animation_phase: f32,
    ) {
        pixels.fill(Color::from_rgba8(13, 18, 25, 255));
        let width = pixels.width() as f32 / scale - SIDEBAR_WIDTH;
        let height = pixels.height() as f32 / scale;
        let accent = appearance.accent();
        let mut frame = Frame { pixels, dashboard: self, scale, cursor, offset: 0.0, accent };
        frame.rect(0.0, 0.0, SIDEBAR_WIDTH, height, rgb([19, 25, 34]));
        frame.text(24.0, 49.0, 29, "Vatra", TEXT);
        for (index, item) in Page::ALL.into_iter().enumerate() {
            frame.button(12.0, sidebar_y(index), 136.0, item.title(), item == page);
        }
        frame.offset = SIDEBAR_WIDTH;
        frame.cursor = cursor.map(|(x, y)| (x - f64::from(SIDEBAR_WIDTH), y));
        frame.text(26.0, 51.0, 28, page.title(), TEXT);
        let panel = rgb([24, 33, 44]);
        if page == Page::About {
            frame.rounded(24.0, 82.0, width - 48.0, 240.0, 16.0, panel);
            frame.text(44.0, 135.0, 24, &format!("Vatra {}", env!("CARGO_PKG_VERSION")), TEXT);
            frame.text(44.0, 171.0, 17, "A battery and power dashboard for Linux", MUTED);
            frame.text(44.0, 211.0, 16, "Created by Vladyslav Havryliuk (VladCraft228)", TEXT);
            frame.text(44.0, 246.0, 15, "Linux Scripts · MIT License", MUTED);
            frame.button(44.0, 264.0, 132.0, "Open GitHub", true);
            return;
        }
        if page == Page::Appearance {
            frame.rounded(24.0, 82.0, width - 48.0, 368.0, 16.0, panel);
            frame.text(44.0, 122.0, 20, "Accent color", TEXT);
            for (index, (color, name)) in ACCENTS.into_iter().enumerate() {
                let x = accent_x(index);
                frame.button(x, 145.0, 118.0, name, color == accent);
                frame.rounded(x + 92.0, 155.0, 15.0, 15.0, 7.5, rgb(color));
            }
            frame.color_wheel();
            let (marker_x, marker_y) = wheel_marker(accent);
            frame.rounded(WHEEL_LEFT + marker_x - 6.0, WHEEL_TOP + marker_y - 6.0,
                12.0, 12.0, 6.0, rgb([13, 18, 25]));
            frame.rounded(WHEEL_LEFT + marker_x - 4.0, WHEEL_TOP + marker_y - 4.0,
                8.0, 8.0, 4.0, rgb(accent));
            frame.rounded(44.0, 400.0, 25.0, 19.0, 5.0, rgb(accent));
            frame.text(78.0, 415.0, 14, &format!("#{}", appearance.hex().to_uppercase()), TEXT);
            if indicator != Indicator::Off {
                frame.text(STYLE_X, 242.0, 16, "Battery percentage", TEXT);
                for (index, label) in PERCENT_OPTIONS.into_iter().enumerate() {
                    frame.button(style_x(index), PERCENT_TOP, STYLE_OPTION_WIDTH, label, index == appearance.percent);
                }
                frame.text(STYLE_X, 326.0, 16, "Charging bolt", TEXT);
                for (index, label) in BOLT_OPTIONS.into_iter().enumerate() {
                    frame.button(style_x(index), BOLT_TOP, STYLE_OPTION_WIDTH, label, index == appearance.bolt);
                }
            }
            frame.rounded(24.0, 466.0, width - 48.0, 151.0, 16.0, panel);
            frame.text(44.0, 493.0, 18, "Battery indicator", TEXT);
            for (index, option) in Indicator::ALL.into_iter().enumerate() {
                let x = indicator_x(index);
                if option == Indicator::XfcePanel && !xfce_panel_available {
                    frame.text(x + 12.0, INDICATOR_TOP + 22.0, 14, "Vatra Panel", MUTED);
                } else {
                    frame.button(x, INDICATOR_TOP, INDICATOR_WIDTH, option.label(), option == indicator);
                }
            }
            frame.text_clipped(44.0, 566.0, 16, indicator_status, TEXT, width - 88.0);
            let details = if !apply_message.is_empty() {
                apply_message
            } else if indicator == Indicator::XfcePanel {
                "Manage Vatra Panel in XFCE Panel Preferences."
            } else if xfce_panel_available {
                "Vatra Panel shows the battery percentage as full-size text."
            } else {
                "Vatra Panel is available with the optional XFCE plugin."
            };
            frame.text_clipped(44.0, 594.0, 14, details, MUTED, width - 88.0);
            return;
        }
        if page == Page::Diagnostics {
            frame.rounded(24.0, 82.0, width - 48.0, 176.0, 16.0, panel);
            frame.text(44.0, 118.0, 20, "Processor", TEXT);
            frame.rows(44.0, 150.0, &[
                format!("{} · {} policies", snapshot.driver, snapshot.policy_count),
                format!("Governor: {} · EPP: {}", snapshot.governor, snapshot.epp),
                format!("CPU limits: {} – {}", snapshot.minimum, snapshot.maximum),
                format!("Platform profile: {}", snapshot.platform_profile),
            ]);
            if let Some(state) = diagnostics {
                frame.rounded(24.0, 274.0, width - 48.0, 156.0, 16.0, panel);
                frame.text(44.0, 310.0, 20, "TLP configuration", TEXT);
                frame.rows(44.0, 338.0, &[state.tlp.clone(), state.managers.clone(),
                    state.effective_cpu[0].clone(), state.effective_cpu[1].clone()]);
            }
            if let Some(battery) = &snapshot.battery {
                frame.rounded(24.0, 446.0, width - 48.0, 156.0, 16.0, panel);
                frame.text(44.0, 481.0, 20, &format!("Battery · {}", battery.name), TEXT);
                frame.rows(44.0, 510.0, &[
                    format!("{} · {} · {} cycles", reading(battery.voltage, " V", 2),
                        reading(battery.current, " A", 2), reading(battery.cycles, "", 0)),
                    format!("Capacity: {} / {} {} · Temperature: {}", reading(battery.full, "", 1),
                        reading(battery.design, "", 1), battery.capacity_unit, reading(battery.temperature, " °C", 1)),
                    format!("Charge thresholds: {} start / {} stop",
                        reading(battery.charge_start, "%", 0), reading(battery.charge_end, "%", 0)),
                ]);
            }
            return;
        }
        if let Some(review) = review {
            frame.rounded(24.0, 82.0, width - 48.0, 350.0, 16.0, panel);
            frame.text(44.0, 135.0, 25, &format!("Review {}", review.title), TEXT);
            frame.text(44.0, 165.0, 14, "Current settings and proposed changes", MUTED);
            for (index, [source, old_epp, new_epp, old_limit, new_limit]) in review.rows.iter().enumerate() {
                let y = 199.0 + index as f32 * 63.0;
                frame.text(44.0, y, 17, if source == "AC" { "Plugged in" } else { "On battery" }, accent);
                frame.text(44.0, y + 27.0, 15, &format!("CPU limit  {old_limit} to {new_limit}"), TEXT);
                let x = width * 0.49;
                frame.text(x, y + 27.0, 13, &format!("EPP  {old_epp} to {new_epp}"), TEXT);
            }
            frame.text(44.0, 338.0, 14, "Minimum CPU frequency and governor are preserved.", MUTED);
            if review.raising {
                frame.text(44.0, 368.0, 14, "A higher CPU limit can increase power use and heat.", accent);
            }
            frame.button(44.0, OVERLAY_ACTION_TOP, 132.0, "Cancel", false);
            frame.button(190.0, OVERLAY_ACTION_TOP, 152.0,
                if review.raising { "Raise and apply" } else { "Apply changes" }, true);
            return;
        }
        if page == Page::Overview && indicator == Indicator::Off {
            frame.button(width - HEADER_INSET, 24.0, 132.0, "Set up indicator", false);
            if !apply_message.is_empty() {
                frame.text(185.0, 46.0, 13, apply_message, MUTED);
            }
        }
        if let Some((selected, diagnostics)) = profiles {
            if advanced { frame.rounded(24.0, 82.0, width - 48.0, 535.0, 16.0, panel); }
            if !advanced {
                frame.text(26.0, 109.0, 15, "Choose a mode to review its CPU limits and EPP.", MUTED);
                for (index, mode) in Profile::QUICK.into_iter().enumerate() {
                    let (x, y, w) = mode_card(width, index);
                    let active = active_mode == Some(mode);
                    let hovered = !applying && frame.hovered(x, y, w, MODE_HEIGHT);
                    let surface = if active { [49, 42, 35] } else if hovered { [36, 49, 64] } else { [24, 33, 44] };
                    let border = if active { accent } else if hovered { [107, 142, 169] } else { [49, 63, 79] };
                    frame.rounded(x, y, w, MODE_HEIGHT, 15.0, rgb(border));
                    frame.rounded(x + 1.0, y + 1.0, w - 2.0, MODE_HEIGHT - 2.0, 14.0, rgb(surface));
                    frame.rounded(x + 17.0, y + 15.0, 36.0, 36.0, 10.0, rgb([63, 49, 37]));
                    frame.mode_icon(mode, x + 26.0, y + 24.0);
                    frame.text(x + 65.0, y + 39.0, 20, mode.name(), TEXT);
                    if active {
                        frame.text(x + w - 62.0, y + 37.0, 12, "Active", accent);
                    }
                    let description = match mode {
                        Profile::Automatic => "Dynamic CPU range · follows AC/BAT",
                        Profile::Balanced => "Balanced EPP · 80% of speed range",
                        Profile::Performance => "Performance EPP · full speed range",
                        Profile::PowerSaver if snapshot.epp_options.contains("power") => "Power EPP · 60% of speed range",
                        Profile::PowerSaver => "Balanced power EPP · 60% of speed range",
                        Profile::Custom => unreachable!(),
                    };
                    let edited = PowerSource::ALL.into_iter().any(|source|
                        presets.cap(mode, source).is_some() || presets.has_override(mode, source));
                    frame.text_clipped(x + 18.0, y + 79.0, 15,
                        if edited { "Uses your saved AC and battery settings" } else { description }, MUTED, w - 36.0);
                }
                let source = match diagnostics.source {
                    Some(PowerSource::Ac) => "Plugged in",
                    Some(PowerSource::Battery) => "On battery",
                    None => "Power source unconfirmed",
                };
                frame.rounded(24.0, 388.0, width - 48.0, 104.0, 15.0, panel);
                frame.text(43.0, 420.0, 18, source, TEXT);
                if !apply_message.is_empty() {
                    frame.text(43.0, 451.0, 14, apply_message, MUTED);
                } else if !helper_available {
                    frame.text(43.0, 451.0, 14, "Install the Vatra TLP helper to change modes.", MUTED);
                }
                if applying {
                    let progress = (width - 148.0).max(0.0) * animation_phase;
                    frame.rounded(43.0, 470.0, width - 86.0, 4.0, 2.0, rgb([51, 64, 80]));
                    frame.rounded(43.0 + progress, 470.0, 62.0, 4.0, 2.0, rgb(accent));
                }
                return;
            }
            frame.text(40.0, 115.0, 20, "CPU preferences", TEXT);
            let source = diagnostics.preview_source(preview_source);
            for (index, power) in PowerSource::ALL.into_iter().enumerate() {
                let label = if diagnostics.source == Some(power) {
                    format!("{} active", power.label())
                } else {
                    power.label().into()
                };
                frame.button(source_x(width, index), SOURCE_TOP, SOURCE_WIDTH, &label, source == Some(power));
            }
            frame.button(width - REFRESH_INSET, SOURCE_TOP, REFRESH_WIDTH, "Refresh", false);
            frame.text(40.0, 139.0, 14, "Choose a power source and a mode to edit or review.", MUTED);
            for (index, profile) in Profile::ALL.into_iter().enumerate() {
                frame.button(profile_x(index), 158.0, PROFILE_WIDTH, profile.name(), selected == index);
            }
            let selected = Profile::ALL[selected];
            let automatic_current = selected == Profile::Automatic
                && diagnostics.automatic_is_current(snapshot, presets, cpu_ranges.ok());
            let saved_cap = source.and_then(|source| presets.cap(selected, source));
            let requested = source.and_then(|source| presets.requested_epp(selected, source, snapshot))
                .or_else(|| saved_cap.map(|_| "keep"));
            let range = source.and_then(|source| presets::cpu_range(cpu_ranges.ok(), source));
            let selected_cap = range.map(|range| saved_cap.unwrap_or_else(|| range.default_cap(selected)));
            if let Some((range, cap)) = range.zip(selected_cap).filter(|_| selected.editable()) {
                frame.text(40.0, CAP_TOP + 22.0, 14, "Maximum CPU speed", TEXT);
                frame.button(194.0, CAP_TOP, 32.0, "-", false);
                frame.text(236.0, CAP_TOP + 22.0, 14,
                    &if saved_cap.is_none() && selected.id() == "custom" { "Keep current".into() } else { presets::limit_label(u64::from(cap)) }, accent);
                frame.button(342.0, CAP_TOP, 32.0, "+", false);
                let fraction = f64::from(cap.saturating_sub(range.minimum).min(range.ceiling - range.minimum))
                    / f64::from(range.ceiling - range.minimum);
                frame.rounded(CAP_TRACK_START as f32, CAP_TOP + 14.0,
                    (CAP_TRACK_END - CAP_TRACK_START) as f32, 6.0, 3.0, rgb([62, 79, 98]));
                let knob = CAP_TRACK_START + (CAP_TRACK_END - CAP_TRACK_START) * fraction;
                frame.rounded(knob as f32 - 5.0, CAP_TOP + 10.0, 10.0, 14.0, 5.0, rgb(accent));
                if saved_cap.is_some() {
                    frame.button(558.0, CAP_TOP, 132.0, "Reset limit", false);
                }
            } else if !selected.editable() {
                frame.text(40.0, CAP_TOP + 22.0, 14, "Automatic uses the default CPU ceiling for each source.", MUTED);
            }
            if selected.editable() && source.is_some() {
                for (index, (value, label)) in EPP_OPTIONS.into_iter().enumerate() {
                    let label = if value == "balance_performance" { "Balance perf." } else { label };
                    if snapshot.epp_options.contains(value) {
                        frame.button(epp_x(index), EDIT_TOP, EPP_WIDTH, label, requested == Some(value));
                    } else {
                        frame.text(epp_x(index) + 12.0, EDIT_TOP + 22.0, 14, label, MUTED);
                    }
                }
                if source.is_some_and(|source| presets.has_override(selected, source)) {
                    frame.button(RESET_X, EDIT_TOP, RESET_WIDTH, "Reset", false);
                }
            }
            if let Some(source) = source {
                let active = diagnostics.source == Some(source);
                let [current_epp, max] = if active {
                    [snapshot.epp.clone(), snapshot.maximum_khz
                        .map_or_else(|| snapshot.maximum.clone(), presets::limit_label)]
                } else {
                    diagnostics.configured_cpu(source)
                };
                let supported = requested.filter(|value| snapshot.epp_options.contains(*value));
                let after = match supported { Some(value) => value, None => current_epp.as_str() };
                let proposed_limit = if selected.id() == "custom" && saved_cap.is_none() {
                    max.clone()
                } else {
                    selected_cap.map_or_else(|| "Unavailable".into(), |khz| presets::limit_label(u64::from(khz)))
                };
                frame.rounded(40.0, 308.0, width - 80.0, 155.0, 12.0, rgb([19, 26, 37]));
                let middle = width * 0.40;
                let end = width * 0.67;
                frame.text(56.0, 340.0, 17, "Preview", TEXT);
                frame.text(middle, 340.0, 13, if active { "Current" } else { "Saved in TLP" }, MUTED);
                frame.text(end, 340.0, 13, if automatic_current { "Automatic" } else { "After apply" },
                    if automatic_current { MUTED } else { accent });
                frame.text(56.0, 381.0, 15, "Energy preference", TEXT);
                frame.text(middle, 381.0, 13, &current_epp, TEXT);
                frame.text(end, 381.0, 13, after, if automatic_current { TEXT } else { accent });
                frame.text(56.0, 422.0, 15, "Maximum CPU speed", TEXT);
                frame.text(middle, 422.0, 13, &max, TEXT);
                frame.text(end, 422.0, 13, &proposed_limit, if automatic_current { TEXT } else { accent });
            }
            let changes_cap = saved_cap.is_some() || selected.id() != "custom";
            let preflight = if changes_cap && cpu_ranges.is_err() {
                cpu_ranges.err().unwrap_or_default().to_owned()
            } else if automatic_current {
                "Automatic is already active. No changes needed.".into()
            } else if !selected.editable() {
                "Review Automatic to see the limits for both power sources.".into()
            } else if source != diagnostics.source {
                "Preset edits are saved. Switch power source to apply here.".into()
            } else {
                requested.map_or_else(|| "Choose an energy preference or a CPU limit.".into(), |value| {
                    if changes_cap && (value == "keep" || value == snapshot.epp.as_str()) {
                        "Review the selected CPU limit before applying.".into()
                    } else {
                        diagnostics.preflight(value, source, snapshot).replace(" → ", " to ")
                    }
                })
            };
            frame.text_clipped(40.0, 492.0, 14, &preflight, accent, width - 80.0);
            let eligible = if selected.editable() {
                (!changes_cap || range.is_some()) && diagnostics.apply_target(requested, source, snapshot, changes_cap).is_some()
            } else {
                !automatic_current && diagnostics.quick_target(selected, snapshot, presets).is_ok() && range.is_some()
            };
            if helper_available && eligible && !applying {
                frame.button(40.0, APPLY_TOP, APPLY_WIDTH, "Review changes", false);
            }
            if !apply_message.is_empty() {
                frame.text_clipped(205.0, APPLY_TOP + 22.0, 13, apply_message, MUTED, width - 245.0);
            } else if !helper_available {
                frame.text(40.0, APPLY_TOP + 22.0, 13, "Apply requires the Vatra TLP helper.", MUTED);
            }
            return;
        }
        let card_width = (width - 64.0) * 0.56;
        let right = card_width + 40.0;
        frame.rounded(24.0, 82.0, card_width, 212.0, 16.0, panel);
        frame.rounded(right, 82.0, width - right - 24.0, 212.0, 16.0, panel);

        frame.text(44.0, 114.0, 17, "Battery", MUTED);
        if let Some(battery) = &snapshot.battery {
            let charging = battery.state.as_deref() == Some("Charging");
            let tone = if charging { GREEN } else if battery.percent.is_some_and(|n| n <= 20.0) {
                [255, 131, 121]
            } else { accent };
            frame.text(44.0, 184.0, 58, &reading(battery.percent, "%", 0), TEXT);
            frame.battery(232.0, 143.0, battery_fill, tone);
            let state = match battery.state.as_deref() {
                Some("Discharging") => "On battery",
                Some(state) => state,
                None => "State unavailable",
            };
            frame.text(46.0, 216.0, 18, state, tone);
            if !charging && battery.hours_remaining.is_some() {
                frame.text(46.0, 252.0, 17, &format!("{} remaining (estimated)", remaining(battery.hours_remaining)), TEXT);
            }
            let mut detail = format!("Battery health: {}", reading(battery.health, "%", 0));
            if let Some(stop) = battery.charge_end {
                detail.push_str(&format!("  ·  Charge limit: {stop:.0}%"));
            }
            frame.text(46.0, 275.0, 14, &detail, MUTED);
        } else {
            frame.text(44.0, 167.0, 24, "No battery detected", TEXT);
        }

        let x = right + 20.0;
        frame.text(x, 114.0, 17, "Battery power", MUTED);
        if let Some(battery) = &snapshot.battery {
            frame.text(x, 181.0, 43, &reading(battery.power, " W", 1), TEXT);
            let direction = match battery.state.as_deref() {
                Some("Charging") => "Power into battery",
                Some("Discharging") => "Power drawn from battery",
                _ => "Battery reading",
            };
            frame.text(x, 214.0, 15, direction, MUTED);
            frame.text(x, 241.0, 14, if battery.derived_power { "Calculated from voltage × current" }
                else if battery.power.is_some() { "Reported by the battery driver" }
                else { "Power reading unavailable" }, MUTED);
        }
        frame.text(x, 275.0, 14, &format!("CPU frequency limit: {}", snapshot.maximum), MUTED);

        frame.rounded(24.0, 310.0, width - 48.0, 68.0, 16.0, panel);
        frame.text(44.0, 337.0, 13, "Power mode", MUTED);
        let mode_label = active_mode.map(Profile::name).unwrap_or("No matching preset");
        frame.text(44.0, 360.0, 18, mode_label, if active_mode.is_some() { accent } else { TEXT });
        frame.button(width - MODE_BUTTON_INSET, MODE_BUTTON_TOP, 132.0, "Change mode", false);

        let chart_bottom = (height - 24.0 - DISCHARGE_STATS_HEIGHT).min(CHART_TOP + 330.0);
        frame.rounded(24.0, CHART_TOP, width - 48.0, chart_bottom - CHART_TOP, 16.0, panel);
        frame.text(44.0, CHART_TOP + 32.0, 19, "Battery power history", TEXT);
        frame.text(44.0, CHART_TOP + 54.0, 13, "Positive: charging  /  Negative: discharging", MUTED);
        let slider_x = width - 207.0;
        frame.text(slider_x - 66.0, CHART_TOP + 33.0, 13, &format!("{range_minutes} min"), TEXT);
        frame.rounded(slider_x, SLIDER_TOP, SLIDER_WIDTH, 6.0, 3.0, rgb([47, 62, 79]));
        let thumb = SLIDER_WIDTH * (range_minutes - 1) as f32 / 9.0;
        frame.rounded(slider_x, SLIDER_TOP, thumb, 6.0, 3.0, rgb(accent));
        frame.rounded(slider_x + thumb - 7.0, SLIDER_TOP - 4.0, 14.0, 14.0, 7.0, rgb(accent));
        let plot_width = width - 128.0;
        let top = CHART_TOP + 73.0;
        let plot_height = chart_bottom - top - 29.0;
        let left = 78.0;
        let end = history::now();
        let seconds = (range_minutes * 60).min(WINDOW_SECONDS);
        let start = end.saturating_sub(seconds);
        let points = || history.points().filter(move |(time, _)| (start..=end).contains(time));
        let (low, high) = points().filter_map(|(_, watts)| watts)
            .fold((0.0_f64, 0.0_f64), |(low, high), watts| (low.min(watts), high.max(watts)));
        let low = low.floor() as f32;
        let high = if low == 0.0 { high.ceil().max(1.0) } else { high.ceil() } as f32;
        let value_y = |watts: f32| top + plot_height * (high - watts) / (high - low);
        let reference = if low < 0.0 && high > 0.0 { 0.0 } else { (low + high) / 2.0 };
        let axis = rgb([69, 86, 105]);
        for value in [high, reference, low] {
            let y = value_y(value);
            frame.rect(left, y, plot_width, 1.0, axis);
            let label = if value.fract() == 0.0 { format!("{value:.0} W") } else { format!("{value:.1} W") };
            frame.text(35.0, y + 4.0, 10, &label, MUTED);
        }
        let mut path = PathBuilder::new();
        let mut previous = None;
        let mut available = false;
        for (time, value) in points() {
            let Some(watts) = value else { previous = None; continue };
            available = true;
            let x = left + plot_width * (time - start) as f32 / seconds as f32;
            let y = value_y(watts as f32);
            if previous.is_some_and(|last| time - last <= 6) {
                path.line_to(x + SIDEBAR_WIDTH, y);
            } else {
                path.move_to(x + SIDEBAR_WIDTH, y);
                frame.rect(x - 1.0, y - 1.0, 2.0, 2.0, rgb(accent));
            }
            previous = Some(time);
        }
        if let Some(path) = path.finish() {
            let mut paint = Paint::default();
            paint.set_color(rgb(accent));
            let stroke = Stroke { width: 2.0, ..Stroke::default() };
            frame.pixels.stroke_path(&path, &paint, &stroke, Transform::from_scale(scale, scale), None);
        }
        if !available {
            frame.text(left + 16.0, top + plot_height / 2.0, 13, "Waiting for charge or discharge readings", MUTED);
        }
        let oldest = format!("{range_minutes} min ago");
        for (offset, label) in [(0.0, oldest.as_str()), (1.0, "Now")] {
            let label_offset = if offset == 1.0 { 24.0 } else { 0.0 };
            frame.text(left + plot_width * offset - label_offset, chart_bottom - 10.0, 11, label, MUTED);
        }

        let stats_top = chart_bottom + 12.0;
        frame.rounded(24.0, stats_top, width - 48.0, DISCHARGE_STATS_HEIGHT - 12.0, 16.0, panel);
        frame.text(44.0, stats_top + 23.0, 14, "Battery discharge · W", TEXT);
        let column_width = (width - 88.0) / 3.0;
        for (index, seconds) in DISCHARGE_WINDOWS.into_iter().enumerate() {
            let x = 44.0 + column_width * index as f32;
            let stats = history.discharge_stats(end, seconds);
            let (average, minimum, maximum, samples) = stats
                .map_or((None, None, None, 0), |stats| {
                    (Some(stats.average), Some(stats.minimum), Some(stats.maximum), stats.samples)
                });
            let label = format!("{} min · {samples} samples", seconds / 60);
            frame.text(x, stats_top + 46.0, 13, &label, MUTED);
            frame.text(x, stats_top + 69.0, 17, &format!("Avg {}", reading(average, "", 1)), accent);
            frame.text(x, stats_top + 91.0, 13, &format!(
                "Min {}  ·  Max {}", reading(minimum, "", 1), reading(maximum, "", 1),
            ), TEXT);
        }
    }
}

const OVERLAY_ACTION_TOP: f32 = 384.0;

pub fn github_hit(x: f64, y: f64) -> bool { hit(x, y, 44.0, 264.0, 132.0) }

pub fn overlay_hit(x: f64, y: f64) -> Option<bool> {
    if hit(x, y, 44.0, OVERLAY_ACTION_TOP, 132.0) {
        Some(false)
    } else if hit(x, y, 190.0, OVERLAY_ACTION_TOP, 152.0) {
        Some(true)
    } else {
        None
    }
}

const MODE_HEIGHT: f32 = 108.0;
const CHART_TOP: f32 = 394.0;
const DISCHARGE_STATS_HEIGHT: f32 = 120.0;
const SLIDER_TOP: f32 = CHART_TOP + 25.0;
const SLIDER_WIDTH: f32 = 156.0;
const MODE_BUTTON_TOP: f32 = 327.0;
const MODE_BUTTON_INSET: f32 = 176.0;

fn mode_card(width: f32, index: usize) -> (f32, f32, f32) {
    let card = (width - 64.0) / 2.0;
    (24.0 + (card + 16.0) * (index % 2) as f32, 133.0 + 124.0 * (index / 2) as f32, card)
}

pub fn quick_mode_hit(width: f64, x: f64, y: f64) -> Option<Profile> {
    Profile::QUICK.into_iter().enumerate().find(|(index, _)| {
        let (left, top, card) = mode_card(width as f32, *index);
        (f64::from(left)..f64::from(left + card)).contains(&x)
            && (f64::from(top)..f64::from(top + MODE_HEIGHT)).contains(&y)
    }).map(|(_, mode)| mode)
}

pub fn slider_hit(width: f64, x: f64, y: f64) -> bool {
    (width - 217.0..=width - 41.0).contains(&x) && (f64::from(SLIDER_TOP - 15.0)..=f64::from(SLIDER_TOP + 20.0)).contains(&y)
}

pub fn slider_minutes(width: f64, x: f64) -> u64 {
    1 + (((x - (width - 207.0)) / f64::from(SLIDER_WIDTH)).clamp(0.0, 1.0) * 9.0).round() as u64
}

const HEADER_INSET: f32 = 156.0;
const PROFILE_WIDTH: f32 = 124.0;
const BUTTON_HEIGHT: f32 = 34.0;
const APPLY_TOP: f32 = 512.0;
const APPLY_WIDTH: f32 = 148.0;

pub fn hits_apply(x: f64, y: f64) -> bool {
    (40.0..f64::from(40.0 + APPLY_WIDTH)).contains(&x)
        && (f64::from(APPLY_TOP)..f64::from(APPLY_TOP + BUTTON_HEIGHT)).contains(&y)
}

const SOURCE_TOP: f32 = 94.0;
const SOURCE_WIDTH: f32 = 108.0;
const REFRESH_INSET: f32 = 126.0;
const REFRESH_WIDTH: f32 = 86.0;

fn source_x(width: f32, index: usize) -> f32 {
    width - 358.0 + 116.0 * index as f32
}

pub fn source_hit(width: f64, x: f64, y: f64) -> Option<PowerSource> {
    PowerSource::ALL.into_iter().enumerate()
        .find(|(index, _)| hit(x, y, source_x(width as f32, *index), SOURCE_TOP, SOURCE_WIDTH))
        .map(|(_, source)| source)
}

pub fn hits_refresh(width: f64, x: f64, y: f64) -> bool {
    hit(x, y, width as f32 - REFRESH_INSET, SOURCE_TOP, REFRESH_WIDTH)
}

fn hit(x: f64, y: f64, left: f32, top: f32, width: f32) -> bool {
    (f64::from(left)..f64::from(left + width)).contains(&x)
        && (f64::from(top)..f64::from(top + BUTTON_HEIGHT)).contains(&y)
}

fn profile_x(index: usize) -> f32 {
    40.0 + index as f32 * (PROFILE_WIDTH + 8.0)
}

const CAP_TOP: f32 = 253.0;
const CAP_TRACK_START: f64 = 396.0;
const CAP_TRACK_END: f64 = 542.0;

pub enum CapEdit { Lower, Raise, Set(f64), Reset }

pub fn cap_edit_hit(x: f64, y: f64) -> Option<CapEdit> {
    if hit(x, y, 194.0, CAP_TOP, 32.0) {
        Some(CapEdit::Lower)
    } else if hit(x, y, 342.0, CAP_TOP, 32.0) {
        Some(CapEdit::Raise)
    } else if hit(x, y, 558.0, CAP_TOP, 132.0) {
        Some(CapEdit::Reset)
    } else if hit(x, y, CAP_TRACK_START as f32 - 5.0, CAP_TOP, (CAP_TRACK_END - CAP_TRACK_START) as f32 + 10.0) {
        Some(CapEdit::Set(((x - CAP_TRACK_START) / (CAP_TRACK_END - CAP_TRACK_START)).clamp(0.0, 1.0)))
    } else {
        None
    }
}

const EDIT_TOP: f32 = 202.0;
const EPP_WIDTH: f32 = 139.0;
const RESET_X: f32 = 628.0;
const RESET_WIDTH: f32 = 72.0;

fn epp_x(index: usize) -> f32 {
    40.0 + index as f32 * (EPP_WIDTH + 8.0)
}

pub fn epp_edit_hit(x: f64, y: f64) -> Option<Option<usize>> {
    if hit(x, y, RESET_X, EDIT_TOP, RESET_WIDTH) {
        return Some(None);
    }
    (0..EPP_OPTIONS.len()).find(|&index| hit(x, y, epp_x(index), EDIT_TOP, EPP_WIDTH)).map(Some)
}

pub fn tray_hit(width: f64, x: f64, y: f64) -> bool {
    hit(x, y, width as f32 - HEADER_INSET, 24.0, 132.0)
}

pub fn navigation_hit(width: f64, x: f64, y: f64, in_profiles: bool) -> bool {
    let (inset, top) = if in_profiles { (HEADER_INSET, 24.0) } else { (MODE_BUTTON_INSET, MODE_BUTTON_TOP) };
    hit(x, y, width as f32 - inset, top, 132.0)
}

pub fn click_target(width: f64, x: f64, y: f64, in_profiles: bool) -> Option<Option<usize>> {
    if !in_profiles && navigation_hit(width, x, y, false) {
        return Some(Some(0));
    }
    if in_profiles {
        return (0..Profile::ALL.len())
            .find(|&index| hit(x, y, profile_x(index), 158.0, PROFILE_WIDTH))
            .map(Some);
    }
    None
}

fn reading(value: Option<f64>, unit: &str, digits: usize) -> String {
    value.map(|n| format!("{n:.digits$}{unit}")).unwrap_or_else(|| "—".into())
}

fn remaining(hours: Option<f64>) -> String {
    hours.map(|value| {
        let minutes = (value * 60.0).round() as u64;
        format!("{}h {}m", minutes / 60, minutes % 60)
    }).unwrap_or_else(|| "Unavailable".into())
}

struct Frame<'a> {
    pixels: &'a mut Pixmap,
    dashboard: &'a mut Dashboard,
    scale: f32,
    cursor: Option<(f64, f64)>,
    offset: f32,
    accent: [u8; 3],
}

fn rgb([r, g, b]: [u8; 3]) -> Color {
    Color::from_rgba8(r, g, b, 255)
}

impl Frame<'_> {
    fn color_wheel(&mut self) {
        let Some(wheel) = &self.dashboard.wheel else { return };
        self.pixels.draw_pixmap(WHEEL_LEFT as i32 + self.offset as i32, WHEEL_TOP as i32,
            wheel.as_ref(), &PixmapPaint::default(), Transform::from_scale(self.scale, self.scale), None);
    }

    fn hovered(&self, x: f32, y: f32, width: f32, height: f32) -> bool {
        self.cursor.is_some_and(|(px, py)| {
            (f64::from(x)..f64::from(x + width)).contains(&px)
                && (f64::from(y)..f64::from(y + height)).contains(&py)
        })
    }

    fn rounded(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
        if w <= 0.0 || h <= 0.0 { return; }
        let r = r.min(w / 2.0).min(h / 2.0);
        let x = x + self.offset;
        let (right, bottom) = (x + w, y + h);
        let mut shape = PathBuilder::new();
        shape.move_to(x + r, y);
        shape.line_to(right - r, y);
        shape.quad_to(right, y, right, y + r);
        shape.line_to(right, bottom - r);
        shape.quad_to(right, bottom, right - r, bottom);
        shape.line_to(x + r, bottom);
        shape.quad_to(x, bottom, x, bottom - r);
        shape.line_to(x, y + r);
        shape.quad_to(x, y, x + r, y);
        shape.close();
        if let Some(path) = shape.finish() {
            let mut paint = Paint::default();
            paint.set_color(color);
            paint.anti_alias = true;
            self.pixels.fill_path(&path, &paint, tiny_skia::FillRule::Winding,
                Transform::from_scale(self.scale, self.scale), None);
        }
    }

    fn battery(&mut self, x: f32, y: f32, level: Option<f64>, tone: [u8; 3]) {
        self.rounded(x + 67.0, y + 12.0, 6.0, 12.0, 2.0, rgb(MUTED));
        self.rounded(x, y, 66.0, 36.0, 8.0, rgb(MUTED));
        self.rounded(x + 2.0, y + 2.0, 62.0, 32.0, 6.0, rgb([24, 33, 44]));
        if let Some(level) = level {
            self.rounded(x + 5.0, y + 5.0, 56.0 * level as f32 / 100.0, 26.0, 4.0, rgb(tone));
        }
    }

    fn mode_icon(&mut self, mode: Profile, x: f32, y: f32) {
        let x = x + self.offset;
        let points: &[(f32, f32)] = match mode {
            Profile::Automatic => &[(2., 6.), (14., 6.), (11., 3.), (14., 6.), (11., 9.),
                (14., 6.), (14., 14.), (2., 14.), (5., 11.), (2., 14.), (5., 17.)],
            Profile::Balanced => &[(1., 13.), (5., 13.), (8., 5.), (12., 15.), (16., 9.), (19., 9.)],
            Profile::Performance => &[(11., 0.), (3., 11.), (9., 11.), (7., 19.), (17., 7.), (11., 7.), (11., 0.)],
            Profile::PowerSaver => &[(2., 17.), (4., 8.), (15., 3.), (13., 13.), (7., 16.), (3., 16.), (10., 8.)],
            Profile::Custom => return,
        };
        let mut icon = PathBuilder::new();
        for (index, &(px, py)) in points.iter().enumerate() {
            if index == 0 { icon.move_to(x + px, y + py); }
            else { icon.line_to(x + px, y + py); }
        }
        if let Some(path) = icon.finish() {
            let mut paint = Paint::default();
            paint.set_color(rgb(self.accent));
            paint.anti_alias = true;
            self.pixels.stroke_path(&path, &paint, &Stroke { width: 1.7, ..Stroke::default() },
                Transform::from_scale(self.scale, self.scale), None);
        }
    }

    fn button(&mut self, x: f32, y: f32, width: f32, text: &str, selected: bool) {
        let background = if selected { rgb(self.accent.map(|channel| channel / 4)) }
            else if self.hovered(x, y, width, BUTTON_HEIGHT) { rgb([56, 75, 93]) }
            else { rgb([37, 55, 73]) };
        self.rounded(x, y, width, BUTTON_HEIGHT, 8.0, background);
        let color = if selected { self.accent } else { TEXT };
        self.text_clipped(x + 12.0, y + 22.0, 14, text, color, width - 22.0);
    }

    fn rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        if let Some(rect) = Rect::from_xywh(x + self.offset, y, width, height) {
            let mut paint = Paint::default();
            paint.set_color(color);
            self.pixels.fill_rect(rect, &paint, Transform::from_scale(self.scale, self.scale), None);
        }
    }

    fn rows(&mut self, x: f32, start_y: f32, rows: &[String]) {
        for (index, row) in rows.iter().enumerate() {
            self.text(x, start_y + index as f32 * 23.0, 14, row, TEXT);
        }
    }

    fn text_clipped(&mut self, x: f32, y: f32, size: u16, text: &str, color: [u8; 3], width: f32) {
        let size_px = (size as f32 * self.scale).round();
        let font = &self.dashboard.font;
        let advance = |ch| font.metrics(ch, size_px).advance_width;
        let allowed = width * self.scale;
        if text.chars().map(advance).sum::<f32>() <= allowed {
            self.text(x, y, size, text, color);
            return;
        }
        let mut remaining = allowed - 3.0 * advance('.');
        let mut clipped = String::new();
        for ch in text.chars() {
            remaining -= advance(ch);
            if remaining < 0.0 { break; }
            clipped.push(ch);
        }
        clipped.push_str("...");
        self.text(x, y, size, &clipped, color);
    }

    fn text(&mut self, x: f32, y: f32, size: u16, text: &str, color: [u8; 3]) {
        let size = (size as f32 * self.scale).round() as u16;
        let mut x = (x + self.offset) * self.scale;
        let baseline = (y * self.scale).round() as i32;
        let (width, height) = (self.pixels.width() as usize, self.pixels.height() as usize);
        let canvas = self.pixels.data_mut();
        let font = &self.dashboard.font;
        for ch in text.chars() {
            let ch = if font.has_glyph(ch) { ch } else {
                match ch { '·' => '.', '–' | '—' | '−' => '-', '→' => '>', '×' => 'x', _ => '?' }
            };
            let (metrics, glyph) = self.dashboard.glyphs.entry((ch, size))
                .or_insert_with(|| font.rasterize(ch, size as f32));
            let left = x.round() as i32 + metrics.xmin;
            let top = baseline - metrics.height as i32 - metrics.ymin;
            for (row, coverage) in glyph.chunks(metrics.width.max(1)).enumerate() {
                let y = top + row as i32;
                if y < 0 || y as usize >= height { continue; }
                for (col, alpha) in coverage.iter().copied().enumerate() {
                    let x = left + col as i32;
                    if alpha == 0 || x < 0 || x as usize >= width { continue; }
                    let offset = (y as usize * width + x as usize) * 4;
                    let alpha = u16::from(alpha);
                    for (target, source) in canvas[offset..offset + 3].iter_mut().zip(color) {
                        *target = ((u16::from(source) * alpha + u16::from(*target) * (255 - alpha)) / 255) as u8;
                    }
                }
            }
            x += metrics.advance_width;
        }
    }
}
