mod appearance;
mod dashboard;
mod history;
mod instance;
mod telemetry;
mod tray;
mod profiles;
mod presets;

use appearance::Appearance;
use dashboard::{Dashboard, Page, SIDEBAR_WIDTH};
use history::History;
use profiles::{Diagnostics, PowerSource, Profile};
use presets::{CpuRange, Presets, EPP_OPTIONS};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::{num::NonZeroU32, path::Path, rc::Rc, time::{Duration, Instant}};
use telemetry::Snapshot;
use tiny_skia::Pixmap;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, WindowEvent},
    keyboard::{Key, NamedKey},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    platform::{wayland::WindowAttributesExtWayland, x11::WindowAttributesExtX11},
    window::{CursorIcon, Icon, UserAttentionType, Window, WindowId},
};

const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);
fn tlp_helper() -> Option<&'static str> {
    ["/usr/libexec/vatra-tlp-helper", "/usr/local/libexec/vatra-tlp-helper"]
        .into_iter().find(|path| Path::new(path).is_file())
}

struct App {
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    window: Option<Rc<Window>>,
    canvas: Option<Pixmap>,
    dashboard: Dashboard,
    snapshot: Snapshot,
    history: History,
    next_sample: Instant,
    error: Option<String>,
    profile: Option<usize>,
    advanced: bool,
    preview_source: Option<PowerSource>,
    presets: Presets,
    cpu_ranges: Result<[CpuRange; 2], String>,
    diagnostics: Option<Diagnostics>,
    power_refresh_attempts: u8,
    cursor: Option<(f64, f64)>,
    range_minutes: u64,
    dragging_range: bool,
    battery_transition: Option<(f64, f64, Instant)>,
    next_frame: Instant,
    review: Option<dashboard::Review>,
    auxiliary: Option<Page>,
    appearance: Appearance,
    apply_result: Option<Receiver<String>>,
    apply_message: String,
    proxy: EventLoopProxy<AppEvent>,
    tray: Option<tray::Tray>,
    tray_generation: u64,
    tray_deadline: Option<Instant>,
    start_in_tray: bool,
}

impl App {
    fn sample(&mut self) {
        let snapshot = Snapshot::read(Path::new("/sys"));
        if snapshot.external_power != self.snapshot.external_power {
            self.power_refresh_attempts = 3;
            self.review = None;
            if self.apply_result.is_none() {
                self.apply_message.clear();
            }
        }
        let old_level = self.snapshot.battery.as_ref().and_then(|battery| battery.percent);
        let new_level = snapshot.battery.as_ref().and_then(|battery| battery.percent);
        if old_level != new_level {
            self.battery_transition = old_level.zip(new_level)
                .map(|(from, to)| (from, to, Instant::now()));
        }
        self.snapshot = snapshot;
        if self.power_refresh_attempts > 0 && self.apply_result.is_none() {
            let diagnostics = self.read_diagnostics();
            self.power_refresh_attempts -= 1;
            if diagnostics.source.is_some() {
                self.power_refresh_attempts = 0;
            }
            self.diagnostics = Some(diagnostics);
            if self.advanced { self.refresh_cpu_ranges(); }
        }
        self.history.record(history::now(), self.snapshot.battery.as_ref().and_then(|battery| battery.signed_power()));
        self.update_tray();
        self.next_sample = Instant::now() + SAMPLE_INTERVAL;
    }

    fn open(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn std::error::Error>> {
        let icon = Icon::from_rgba(include_bytes!("../assets/vatra.rgba").to_vec(), 64, 64)?;
        let attributes = Window::default_attributes()
            .with_title("Vatra")
            .with_window_icon(Some(icon))
            .with_inner_size(LogicalSize::new(1060, 760))
            .with_min_inner_size(LogicalSize::new(920, 710));
        let attributes = WindowAttributesExtX11::with_name(attributes, "vatra", "vatra");
        let attributes = WindowAttributesExtWayland::with_name(attributes, "vatra", "vatra");
        let window = Rc::new(event_loop.create_window(attributes)?);
        let context = softbuffer::Context::new(Rc::clone(&window))?;
        self.surface = Some(softbuffer::Surface::new(&context, Rc::clone(&window))?);
        window.request_redraw();
        self.window = Some(window);
        Ok(())
    }

    fn render(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else { return Ok(()) };
        let size = window.inner_size();
        let (Some(width), Some(height)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return Ok(());
        };
        if !self.canvas.as_ref().is_some_and(|p| p.width() == size.width && p.height() == size.height) {
            surface.resize(width, height)?;
            self.canvas = Some(Pixmap::new(size.width, size.height).ok_or("Invalid canvas size")?);
        }
        let Some(canvas) = &mut self.canvas else { return Ok(()) };
        let battery_fill = self.battery_transition.map(|(from, to, start)| {
            let fraction = (start.elapsed().as_secs_f64() / 0.32).min(1.0);
            from + (to - from) * (1.0 - (1.0 - fraction).powi(3))
        }).or_else(|| self.snapshot.battery.as_ref().and_then(|battery| battery.percent));
        let animation_phase = 1.0 - (self.next_sample.saturating_duration_since(Instant::now())
            .as_secs_f32() / SAMPLE_INTERVAL.as_secs_f32()).clamp(0.0, 1.0);
        let active_mode = active_mode(self.diagnostics.as_ref(), &self.snapshot, &self.presets);
        self.dashboard.draw(
            canvas, window.scale_factor() as f32, &self.snapshot, &self.history,
            self.profile.zip(self.diagnostics.as_ref()),
            active_mode,
            self.tray.is_some(),
            self.advanced,
            self.preview_source,
            &self.presets,
            self.cpu_ranges.as_ref().map_err(String::as_str),
            &self.apply_message,
            self.apply_result.is_some(),
            self.review.as_ref(),
            page(self.auxiliary, self.profile, self.advanced),
            self.appearance,
            self.diagnostics.as_ref(),
            tlp_helper().is_some(),
            self.cursor,
            self.range_minutes,
            battery_fill,
            animation_phase,
        );
        let mut buffer = surface.buffer_mut()?;
        for (dst, rgba) in buffer.iter_mut().zip(canvas.data().chunks_exact(4)) {
            *dst = u32::from_be_bytes([0, rgba[0], rgba[1], rgba[2]]);
        }
        window.pre_present_notify();
        buffer.present()?;
        Ok(())
    }

    fn read_diagnostics(&self) -> Diagnostics {
        let mut diagnostics = Diagnostics::read();
        if self.snapshot.external_power.zip(diagnostics.source)
            .is_some_and(|(observed, reported)| observed != reported)
        {
            diagnostics.source = None;
            diagnostics.tlp = "TLP has not confirmed the current power source.".into();
        }
        diagnostics
    }

    fn select(&mut self, profile: Option<usize>) {
        if profile.is_some() && self.profile.is_none() {
            self.diagnostics = Some(self.read_diagnostics());
            self.preview_source = None;
        }
        self.auxiliary = None;
        self.profile = profile;
        self.dragging_range = false;
        if profile.is_none() { self.advanced = false; }
        self.review = None;
        if self.apply_result.is_none() {
            self.apply_message.clear();
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn toggle_advanced(&mut self) {
        if self.profile.is_some() && self.apply_result.is_none() {
            self.advanced = !self.advanced;
            self.select(Some(0));
            if self.advanced { self.refresh_cpu_ranges(); }
        }
    }

    fn choose_quick_mode(&mut self, mode: Profile) {
        if self.apply_result.is_some() {
            return;
        }
        self.diagnostics = Some(self.read_diagnostics());
        let diagnostics = self.diagnostics.as_ref().expect("diagnostics refreshed");
        match diagnostics.quick_target(mode, &self.snapshot, &self.presets) {
            Ok(args) => {
                let caps: Vec<_> = PowerSource::ALL.into_iter().filter_map(|source| {
                    self.presets.cap(mode, source).map(|khz| format!("--cap-{}={khz}", source.label()))
                }).collect();
                let args: Vec<_> = args.into_iter().chain(caps.iter().map(String::as_str)).collect();
                self.preview_apply(mode.name(), &args);
            }
            Err(error) => self.apply_message = error.into(),
        }
        if let Some(window) = &self.window { window.request_redraw(); }
    }

    fn select_source(&mut self, source: PowerSource) {
        self.preview_source = Some(source);
        self.select(self.profile);
    }

    fn refresh_profiles(&mut self) {
        if self.profile.is_some() && self.apply_result.is_none() {
            self.diagnostics = Some(self.read_diagnostics());
            self.preview_source = None;
            if self.advanced { self.refresh_cpu_ranges(); }
            self.select(self.profile);
        }
    }

    fn edit_preset(&mut self, option: Option<usize>) {
        if !self.advanced || self.apply_result.is_some() {
            return;
        }
        let Some((index, diagnostics)) = self.profile.zip(self.diagnostics.as_ref()) else { return };
        let profile = Profile::ALL[index];
        let Some(source) = diagnostics.preview_source(self.preview_source) else { return };
        if !profile.editable() {
            return;
        }
        let requested = option.map(|index| EPP_OPTIONS[index].0);
        if requested.is_some_and(|value| !self.snapshot.epp_options.contains(value))
            || (requested.is_none() && !self.presets.has_override(profile, source))
        {
            return;
        }
        self.review = None;
        self.apply_message = match self.presets.edit(profile, source, requested) {
            Ok(()) => format!("{} {} preset saved in Vatra.", source.label(), profile.name()),
            Err(error) => format!("Cannot save preset: {error}"),
        };
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn refresh_cpu_ranges(&mut self) {
        self.cpu_ranges = tlp_helper().ok_or_else(|| "Install the Vatra TLP helper to edit CPU limits.".to_owned())
            .and_then(|helper| {
                let active = self.diagnostics.as_ref().and_then(|diagnostics| diagnostics.source)
                    .ok_or("TLP could not confirm the current power source.")?;
                presets::read_ranges(helper, active)
            });
    }

    fn edit_cap(&mut self, edit: dashboard::CapEdit) {
        if !self.advanced || self.apply_result.is_some() { return; }
        let Some((index, diagnostics)) = self.profile.zip(self.diagnostics.as_ref()) else { return };
        let profile = Profile::ALL[index];
        let Some(source) = diagnostics.preview_source(self.preview_source) else { return };
        if !profile.editable() { return; }
        let Some(range) = presets::cpu_range(self.cpu_ranges.as_ref().ok(), source) else { return };
        let current = self.presets.cap(profile, source).unwrap_or_else(|| range.default_cap(profile));
        let step = 50_000;
        let floor = range.minimum.saturating_add(step).min(range.ceiling);
        let requested = match edit {
            dashboard::CapEdit::Lower => Some(current.saturating_sub(step).clamp(floor, range.ceiling)),
            dashboard::CapEdit::Raise => Some(current.saturating_add(step).clamp(floor, range.ceiling)),
            dashboard::CapEdit::Set(fraction) => {
                let steps = (f64::from(range.ceiling - floor) * fraction / f64::from(step)).round() as u32;
                Some(if fraction >= 1.0 { range.ceiling } else { (floor + steps * step).min(range.ceiling) })
            }
            dashboard::CapEdit::Reset => None,
        };
        if matches!(edit, dashboard::CapEdit::Lower) && requested.is_some_and(|cap| cap >= current)
            || matches!(edit, dashboard::CapEdit::Raise) && requested.is_some_and(|cap| cap <= current)
        {
            return;
        }
        self.apply_message = match self.presets.edit_cap(profile, source, requested) {
            Ok(()) => format!("{} {} preset saved in Vatra.", source.label(), profile.name()),
            Err(error) => format!("Cannot save CPU limit: {error}"),
        };
        if let Some(window) = &self.window { window.request_redraw(); }
    }

    fn review_advanced(&mut self) {
        if !self.advanced || self.apply_result.is_some() || tlp_helper().is_none() { return; }
        let Some((index, diagnostics)) = self.profile.zip(self.diagnostics.as_ref()) else { return };
        let profile = Profile::ALL[index];
        if !profile.editable() {
            self.choose_quick_mode(profile);
            return;
        }
        let Some(target) = diagnostics.preview_source(self.preview_source) else { return };
        let cap = self.presets.cap(profile, target);
        let changes_cap = cap.is_some() || profile.id() != "custom";
        if changes_cap && self.cpu_ranges.is_err() { return; }
        let requested = self.presets.requested_epp(profile, target, &self.snapshot)
            .or_else(|| cap.map(|_| "keep"));
        let Some((source, epp)) = diagnostics.apply_target(requested, Some(target), &self.snapshot, changes_cap) else { return };
        let flag = format!("--profile={}", profile.id());
        let cap_flag = cap.map(|khz| format!("--cap-{source}={khz}"));
        let args: Vec<_> = [source, epp, flag.as_str()].into_iter()
            .chain(cap_flag.as_deref()).collect();
        self.preview_apply(&format!("{} ({source})", profile.name()), &args);
    }

    fn preview_apply(&mut self, title: &str, args: &[&str]) {
        self.review = None;
        self.apply_message = match tlp_helper().ok_or_else(|| "Install the Vatra TLP helper to preview changes.".to_owned())
            .and_then(|helper| dashboard::Review::load(title, args, helper)) {
            Ok(review) => {
                self.review = Some(review);
                String::new()
            }
            Err(error) => error,
        };
        if let Some(window) = &self.window { window.request_redraw(); }
    }

    fn finish_review(&mut self, apply: bool) {
        if let Some(review) = self.review.take() {
            if apply { self.start_apply(review.args); }
        }
        if let Some(window) = &self.window { window.request_redraw(); }
    }

    fn open_page(&mut self, page: Page) {
        match page {
            Page::Overview => self.select(None),
            Page::Modes | Page::Advanced => {
                self.select(Some(0));
                self.advanced = page == Page::Advanced;
                if self.advanced {
                    self.refresh_cpu_ranges();
                    if let Some(mode) = active_mode(self.diagnostics.as_ref(), &self.snapshot, &self.presets) {
                        self.profile = Profile::ALL.iter().position(|profile| *profile == mode);
                    }
                }
            }
            Page::Diagnostics | Page::Appearance | Page::About => {
                self.select(None);
                self.auxiliary = Some(page);
                if page == Page::Diagnostics {
                    self.diagnostics = Some(self.read_diagnostics());
                }
            }
        }
        if let Some(window) = &self.window { window.request_redraw(); }
    }

    fn edit_appearance(&mut self, edit: dashboard::AppearanceEdit) {
        let mut next = self.appearance;
        match edit {
            dashboard::AppearanceEdit::Accent(color) => next = next.with_accent(color),
            dashboard::AppearanceEdit::Percent(position) => next.percent = position,
            dashboard::AppearanceEdit::Bolt(style) => next.bolt = style,
        }
        self.apply_message = match next.save() {
            Ok(()) => {
                self.appearance = next;
                String::new()
            }
            Err(error) => format!("Could not save appearance: {error}"),
        };
        self.update_tray();
        if let Some(window) = &self.window { window.request_redraw(); }
    }

    fn start_apply(&mut self, args: Vec<String>) {
        let Some(helper) = tlp_helper() else { return };
        self.apply_message = "Waiting for authorization and TLP verification...".into();
        let (sender, receiver) = mpsc::channel();
        self.apply_result = Some(receiver);
        std::thread::spawn(move || {
            let output = std::process::Command::new("/usr/bin/pkexec")
                .args(["--disable-internal-agent", helper]).args(args).output();
            let message = match output {
                Ok(result) if result.status.success() =>
                    String::from_utf8_lossy(&result.stdout).trim().to_owned(),
                Ok(result) if result.status.code() == Some(126) => "Authorization cancelled.".into(),
                Ok(result) if result.status.code() == Some(127) => "Authorization unavailable or denied.".into(),
                Ok(result) if !result.stderr.is_empty() =>
                    String::from_utf8_lossy(&result.stderr).trim().to_owned(),
                Ok(result) => format!("Apply failed ({})", result.status),
                Err(err) => format!("Cannot start pkexec: {err}"),
            };
            let _ = sender.send(message);
        });
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn enable_tray(&mut self) {
        self.apply_message = match self.start_tray().and_then(|_| tray::set_enabled(true)) {
            Ok(()) => String::new(),
            Err(error) => format!("Could not enable Vatra tray: {error}"),
        };
        if let Some(window) = &self.window { window.request_redraw(); }
    }

    fn start_tray(&mut self) -> std::io::Result<()> {
        if self.tray.is_some() { return Ok(()); }
        self.tray_generation += 1;
        let generation = self.tray_generation;
        let proxy = self.proxy.clone();
        self.tray = Some(tray::Tray::spawn(move |action| {
            let _ = proxy.send_event(AppEvent::Tray(generation, action));
        })?);
        self.tray_deadline = Some(Instant::now() + Duration::from_secs(5));
        self.update_tray();
        Ok(())
    }

    fn update_tray(&mut self) {
        if let Some(tray) = &mut self.tray {
            let mode = active_mode(self.diagnostics.as_ref(), &self.snapshot, &self.presets);
            tray.update(&self.snapshot, mode, self.apply_result.is_none()
                && tlp_helper().is_some(), &self.apply_message, self.appearance);
        }
    }

    fn show_window(&mut self, event_loop: &ActiveEventLoop) {
        self.start_in_tray = false;
        if self.window.is_none() {
            if let Err(error) = self.open(event_loop) {
                self.stop_on_error(event_loop, error);
                return;
            }
        }
        if let Some(window) = &self.window {
            window.set_minimized(false);
            window.focus_window();
            if !window.has_focus() {
                window.request_user_attention(Some(UserAttentionType::Informational));
            }
        }
    }

    fn remove_tray(&mut self) {
        self.tray_generation += 1;
        self.tray = None;
        self.tray_deadline = None;
    }

    fn stop_on_error(&mut self, event_loop: &ActiveEventLoop, error: Box<dyn std::error::Error>) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }
}

fn page(auxiliary: Option<Page>, profile: Option<usize>, advanced: bool) -> Page {
    auxiliary.unwrap_or(match (profile, advanced) {
        (Some(_), true) => Page::Advanced,
        (Some(_), false) => Page::Modes,
        _ => Page::Overview,
    })
}

fn active_mode(diagnostics: Option<&Diagnostics>, snapshot: &Snapshot, presets: &Presets) -> Option<Profile> {
    diagnostics.and_then(|state| state.active_mode(snapshot, presets))
}

enum AppEvent {
    Instance(instance::Request),
    Tray(u64, tray::Action),
}

impl ApplicationHandler<AppEvent> for App {
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Instance(instance::Request::Activate) => self.show_window(event_loop),
            AppEvent::Instance(instance::Request::EnableTray) => {
                self.enable_tray();
                self.show_window(event_loop);
            }
            AppEvent::Instance(instance::Request::Background) => {}
            AppEvent::Tray(generation, action) if generation == self.tray_generation => {
                if let Some(tray) = &mut self.tray { tray.invalidate(); }
                match action {
                    tray::Action::Open => {
                        self.select(None);
                        self.show_window(event_loop);
                    }
                    tray::Action::Embedded(embedded) => {
                        if let Some(tray) = &mut self.tray { tray.embedded = embedded; }
                        if embedded {
                            self.tray_deadline = None;
                        } else if self.tray_deadline.is_none() {
                            self.tray_deadline = Some(Instant::now() + Duration::from_secs(5));
                            self.show_window(event_loop);
                        }
                    }
                    tray::Action::Mode(mode) => {
                        self.advanced = false;
                        self.select(Some(0));
                        self.show_window(event_loop);
                        self.choose_quick_mode(mode);
                    }
                    tray::Action::Autostart(value) => {
                        if let Err(error) = tray::set_autostart(value) {
                            self.apply_message = format!("Could not change autostart: {error}");
                        }
                    }
                    tray::Action::Disable => {
                        if let Err(error) = tray::set_enabled(false) {
                            self.apply_message = format!("Could not disable tray: {error}");
                        } else {
                            self.remove_tray();
                            self.show_window(event_loop);
                        }
                    }
                    tray::Action::Quit => event_loop.exit(),
                    tray::Action::Exited => {
                        self.remove_tray();
                        self.apply_message = "Tray unavailable. Check the X11 session and GTK tray dependencies.".into();
                        self.show_window(event_loop);
                    }
                }
                self.update_tray();
            }
            AppEvent::Tray(..) => {}
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() && !self.start_in_tray {
            self.show_window(event_loop);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if !self.window.as_ref().is_some_and(|window| window.id() == id) { return; }
        match event {
            WindowEvent::CloseRequested => {
                if self.tray.as_ref().is_some_and(|tray| tray.embedded) {
                    self.surface = None;
                    self.canvas = None;
                    self.window = None;
                    self.cursor = None;
                    self.dragging_range = false;
                } else {
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && !event.repeat => {
                if self.review.is_some() {
                    match event.logical_key.as_ref() {
                        Key::Named(NamedKey::Escape) => self.finish_review(false),
                        Key::Named(NamedKey::Enter) if self.review.as_ref().is_some_and(|review| !review.raising) => self.finish_review(true),
                        _ => {}
                    }
                    return;
                }
                if self.auxiliary.is_some() {
                    if event.logical_key.as_ref() == Key::Named(NamedKey::Escape) {
                        self.open_page(Page::Overview);
                    }
                    return;
                }
                match event.logical_key.as_ref() {
                    Key::Character("p" | "P") => self.select(if self.profile.is_none() { Some(0) } else { None }),
                    Key::Named(NamedKey::Escape) if self.advanced => self.toggle_advanced(),
                    Key::Named(NamedKey::Escape) if self.profile.is_some() => self.select(None),
                    Key::Character("a" | "A") => self.toggle_advanced(),
                    Key::Character(key @ ("1" | "2" | "3" | "4")) if self.profile.is_some() && !self.advanced => {
                        let index = usize::from(key.as_bytes()[0] - b'1');
                        self.choose_quick_mode(Profile::QUICK[index]);
                    }
                    Key::Character("1") if self.profile.is_some() => self.select_source(PowerSource::Ac),
                    Key::Character("2") if self.profile.is_some() => self.select_source(PowerSource::Battery),
                    Key::Character("3") => self.edit_preset(Some(0)),
                    Key::Character("4") => self.edit_preset(Some(1)),
                    Key::Character("5") => self.edit_preset(Some(2)),
                    Key::Character("6") => self.edit_preset(Some(3)),
                    Key::Character("0") => self.edit_preset(None),
                    Key::Character("-") => self.edit_cap(dashboard::CapEdit::Lower),
                    Key::Character("+") => self.edit_cap(dashboard::CapEdit::Raise),
                    Key::Character("r" | "R") => self.refresh_profiles(),
                    Key::Named(NamedKey::Enter) => self.review_advanced(),
                    Key::Named(NamedKey::ArrowRight) if self.profile.is_none() => {
                        self.range_minutes = (self.range_minutes + 1).min(10);
                        if let Some(window) = &self.window { window.request_redraw(); }
                    }
                    Key::Named(NamedKey::ArrowLeft) if self.profile.is_none() => {
                        self.range_minutes = self.range_minutes.saturating_sub(1).max(1);
                        if let Some(window) = &self.window { window.request_redraw(); }
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        if let Some(index) = self.profile.filter(|_| self.advanced) {
                            self.select(Some((index + 1) % Profile::ALL.len()));
                        }
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        if let Some(index) = self.profile.filter(|_| self.advanced) {
                            self.select(Some((index + Profile::ALL.len() - 1) % Profile::ALL.len()));
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(window) = &self.window {
                    let scale = window.scale_factor();
                    let (x, y) = (position.x / scale, position.y / scale);
                    let width = f64::from(window.inner_size().width) / scale - f64::from(SIDEBAR_WIDTH);
                    self.cursor = Some((x, y));
                    let panel_x = x - f64::from(SIDEBAR_WIDTH);
                    if self.dragging_range {
                        self.range_minutes = dashboard::slider_minutes(width, panel_x);
                    }
                    let pointing = dashboard::sidebar_hit(x, y).is_some() || match self.auxiliary {
                        Some(Page::About) => dashboard::github_hit(panel_x, y),
                        Some(Page::Appearance) => dashboard::appearance_hit(panel_x, y).is_some(),
                        Some(_) => false,
                        None if self.review.is_some() => dashboard::overlay_hit(panel_x, y).is_some(),
                        None if self.advanced => dashboard::click_target(width, panel_x, y, true).is_some()
                            || dashboard::source_hit(width, panel_x, y).is_some()
                            || dashboard::hits_refresh(width, panel_x, y)
                            || dashboard::epp_edit_hit(panel_x, y).is_some()
                            || dashboard::cap_edit_hit(panel_x, y).is_some()
                            || dashboard::hits_apply(panel_x, y),
                        None if self.profile.is_some() => self.apply_result.is_none()
                            && dashboard::quick_mode_hit(width, panel_x, y).is_some(),
                        None => dashboard::navigation_hit(width, panel_x, y, false)
                            || (self.tray.is_none() && dashboard::tray_hit(width, panel_x, y))
                            || self.dragging_range || dashboard::slider_hit(width, panel_x, y),
                    };
                    window.set_cursor(if pointing { CursorIcon::Pointer } else { CursorIcon::Default });
                    window.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.dragging_range = false;
                if let Some(window) = &self.window { window.request_redraw(); }
            }
            WindowEvent::Focused(false) |
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                self.dragging_range = false;
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                if let (Some((x, y)), Some(window)) = (self.cursor, &self.window) {
                    let width = f64::from(window.inner_size().width) / window.scale_factor()
                        - f64::from(SIDEBAR_WIDTH);
                    if let Some(page) = dashboard::sidebar_hit(x, y) {
                        self.open_page(page);
                        return;
                    }
                    let x = x - f64::from(SIDEBAR_WIDTH);
                    match self.auxiliary {
                        Some(Page::About) => {
                            if dashboard::github_hit(x, y) {
                                if let Ok(mut browser) = std::process::Command::new("xdg-open")
                                    .arg("https://github.com/VladCraft228/linux-scripts").spawn()
                                {
                                    std::thread::spawn(move || { let _ = browser.wait(); });
                                }
                            }
                        }
                        Some(Page::Appearance) => {
                            if let Some(edit) = dashboard::appearance_hit(x, y) { self.edit_appearance(edit); }
                        }
                        Some(_) => {}
                        None if self.review.is_some() => {
                            if let Some(apply) = dashboard::overlay_hit(x, y) { self.finish_review(apply); }
                        }
                        None if self.advanced => {
                            if dashboard::hits_refresh(width, x, y) {
                                self.refresh_profiles();
                            } else if let Some(source) = dashboard::source_hit(width, x, y) {
                                self.select_source(source);
                            } else if let Some(edit) = dashboard::epp_edit_hit(x, y) {
                                self.edit_preset(edit);
                            } else if let Some(edit) = dashboard::cap_edit_hit(x, y) {
                                self.edit_cap(edit);
                            } else if dashboard::hits_apply(x, y) {
                                self.review_advanced();
                            } else if let Some(target) = dashboard::click_target(width, x, y, true) {
                                self.select(target);
                            }
                        }
                        None if self.profile.is_some() => {
                            if let Some(mode) = dashboard::quick_mode_hit(width, x, y) {
                                self.choose_quick_mode(mode);
                            }
                        }
                        None => {
                            if self.tray.is_none() && dashboard::tray_hit(width, x, y) {
                                self.enable_tray();
                            } else if dashboard::slider_hit(width, x, y) {
                                self.dragging_range = true;
                                self.range_minutes = dashboard::slider_minutes(width, x);
                                window.request_redraw();
                            } else if dashboard::navigation_hit(width, x, y, false) {
                                self.open_page(Page::Modes);
                            }
                        }
                    }
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.render() {
                    self.stop_on_error(event_loop, error);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if Instant::now() >= self.next_sample {
            if let Some(result) = self.apply_result.as_ref().map(Receiver::try_recv) {
                let message = match result {
                    Ok(message) => Some(message),
                    Err(TryRecvError::Disconnected) => Some("Apply worker stopped unexpectedly.".into()),
                    Err(TryRecvError::Empty) => None,
                };
                if let Some(message) = message {
                    eprintln!("{message}");
                    self.apply_message = message;
                    self.apply_result = None;
                    self.diagnostics = Some(self.read_diagnostics());
                    if self.advanced { self.refresh_cpu_ranges(); }
                }
            }
            self.sample();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        let now = Instant::now();
        if self.tray_deadline.is_some_and(|deadline| now >= deadline) {
            self.remove_tray();
            self.apply_message = "Add Notification Area to the XFCE panel to use the tray.".into();
            self.show_window(event_loop);
            if let Some(window) = &self.window { window.request_redraw(); }
        }
        if self.battery_transition.is_some_and(|(_, _, start)| start.elapsed() >= Duration::from_millis(320)) {
            self.battery_transition = None;
            if self.profile.is_none() {
                if let Some(window) = &self.window { window.request_redraw(); }
            }
        }
        let animate_overview = self.window.is_some() && self.profile.is_none() && self.battery_transition.is_some();
        let animate_apply = self.window.is_some() && self.profile.is_some() && !self.advanced && self.apply_result.is_some();
        let next = if animate_overview || animate_apply {
            if now >= self.next_frame {
                if let Some(window) = &self.window { window.request_redraw(); }
                self.next_frame = now + Duration::from_millis(33);
            }
            self.next_sample.min(self.next_frame)
        } else {
            self.next_sample
        };
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            self.tray_deadline.map_or(next, |deadline| next.min(deadline))
        ));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = match std::env::args().nth(1).as_deref() {
        Some("--enable-tray") => instance::Request::EnableTray,
        Some("--background") => instance::Request::Background,
        _ => instance::Request::Activate,
    };
    let Some(mut instance) = instance::Instance::acquire(request)? else { return Ok(()) };
    let event_loop = EventLoop::<AppEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let instance_proxy = proxy.clone();
    instance.listen(move |request| instance_proxy.send_event(AppEvent::Instance(request)).is_ok())?;
    let mut app = App {
        surface: None,
        window: None,
        canvas: None,
        dashboard: Dashboard::new()?,
        snapshot: Snapshot::read(Path::new("/sys")),
        history: History::load(),
        next_sample: Instant::now(),
        error: None,
        profile: None,
        advanced: false,
        preview_source: None,
        presets: Presets::load(),
        cpu_ranges: Err("CPU limits haven't been checked.".into()),
        diagnostics: None,
        power_refresh_attempts: 1,
        cursor: None,
        range_minutes: 10,
        dragging_range: false,
        battery_transition: None,
        next_frame: Instant::now(),
        review: None,
        auxiliary: None,
        appearance: Appearance::load(),
        apply_result: None,
        apply_message: String::new(),
        proxy,
        tray: None,
        tray_generation: 0,
        tray_deadline: None,
        start_in_tray: false,
    };
    if tray::enabled() || matches!(request, instance::Request::EnableTray) {
        match app.start_tray() {
            Ok(()) => {
                app.start_in_tray = matches!(request, instance::Request::Background);
                if matches!(request, instance::Request::EnableTray) {
                    if let Err(error) = tray::set_enabled(true) { eprintln!("Could not save tray preference: {error}"); }
                }
            }
            Err(error) => eprintln!("Vatra tray unavailable: {error}"),
        }
    }
    app.sample();
    let result = event_loop.run_app(&mut app);
    if let Err(error) = app.history.save(history::now()) {
        eprintln!("Cannot save Vatra history: {error}");
    }
    result?;
    match app.error {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}
