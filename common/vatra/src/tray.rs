#[path = "tray/sni.rs"]
mod sni;

pub use sni::notify;

use crate::{appearance::Appearance, profiles::Profile, telemetry::Snapshot};
use std::{
    env, fs,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::Arc,
    thread,
};

pub enum Action {
    Open,
    Embedded(bool),
    Mode(Profile),
    Autostart(bool),
    Disable,
    Quit,
    Exited,
    Watcher(bool),
    SniReady(bool),
    Fallback(u64, Box<Action>),
}

type Sender = Arc<dyn Fn(Action) + Send + Sync>;


impl Action {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "open" => Self::Open,
            "embedded" => Self::Embedded(true),
            "unembedded" => Self::Embedded(false),
            "autostart-on" => Self::Autostart(true),
            "autostart-off" => Self::Autostart(false),
            "disable" => Self::Disable,
            "quit" => Self::Quit,
            value => Self::Mode(Profile::QUICK.into_iter().find(|mode| mode.id() == value)?),
        })
    }
}

struct Fallback {
    child: Child,
    input: ChildStdin,
    previous: String,
}

impl Fallback {
    pub fn spawn(mut send: impl FnMut(Action) + Send + 'static) -> io::Result<Self> {
        if env::var_os("DISPLAY").is_none() || env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland") {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "Vatra's tray requires an X11 session"));
        }
        let executable = env::current_exe()?;
        let helper = executable.parent().and_then(|bin| bin.parent())
            .map(|prefix| prefix.join("libexec/vatra-tray"))
            .filter(|path| path.is_file())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Install Vatra to use the tray"))?;
        let mut child = Command::new("/usr/bin/python3").arg("-I").arg(helper)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
        let input = child.stdin.take().expect("piped tray stdin");
        let output = child.stdout.take().expect("piped tray stdout");
        if let Err(error) = thread::Builder::new().name("vatra-tray".into()).spawn(move || {
            for line in BufReader::new(output).lines().map_while(Result::ok) {
                if let Some(action) = Action::parse(&line) { send(action); }
            }
            send(Action::Exited);
        }) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(Self { child, input, previous: String::new() })
    }

    fn update(&mut self, update: &str) {
        if update != self.previous && self.input.write_all(update.as_bytes()).is_ok() {
            self.previous = update.to_owned();
        }
    }
}

impl Drop for Fallback {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct Tray {
    sni: sni::Worker,
    fallback: Option<Fallback>,
    sender: Sender,
    watcher: bool,
    fallback_epoch: u64,
    previous: String,
    pub embedded: bool,
}

impl Tray {
    pub fn spawn(send: impl Fn(Action) + Send + Sync + 'static) -> io::Result<Self> {
        let sender: Sender = Arc::new(send);
        Ok(Self {
            sni: sni::Worker::spawn(sender.clone())?, sender, fallback: None,
            watcher: false, fallback_epoch: 0, previous: String::new(), embedded: false,
        })
    }

    fn start_fallback(&mut self) {
        if self.fallback.is_some() { return; }
        self.fallback_epoch = self.fallback_epoch.wrapping_add(1);
        let epoch = self.fallback_epoch;
        let send = self.sender.clone();
        if let Ok(mut fallback) = Fallback::spawn(move |action| send(Action::Fallback(epoch, Box::new(action)))) {
            if !self.previous.is_empty() { fallback.update(&self.previous); }
            self.fallback = Some(fallback);
        }
    }

    pub fn handle(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::Watcher(available) => {
                if !available && !self.watcher && self.fallback.is_some() {
                    return None;
                }
                self.watcher = available;
                if available {
                    self.fallback_epoch = self.fallback_epoch.wrapping_add(1);
                    self.fallback = None; // Kill and reap before registering SNI.
                    self.sni.enable();
                    // Keep a hidden window hidden during a short, normal migration.
                    self.embedded = false;
                    None
                } else {
                    self.start_fallback();
                    Some(Action::Embedded(false))
                }
            }
            Action::SniReady(true) if self.watcher => Some(Action::Embedded(true)),
            Action::SniReady(false) if self.watcher => {
                self.start_fallback();
                Some(Action::Embedded(false))
            }
            Action::SniReady(_) => None,
            Action::Fallback(epoch, action) if epoch == self.fallback_epoch => match *action {
                Action::Exited => {
                    self.fallback = None;
                    Some(Action::Embedded(false))
                }
                action => Some(action),
            },
            Action::Fallback(..) => None,
            action => Some(action),
        }
    }

    pub fn update(&mut self, snapshot: &Snapshot, mode: Option<Profile>, ready: bool, message: &str, appearance: Appearance) {
        let mut details = Vec::new();
        let mut level = "-".to_owned();
        let mut charging = false;
        if let Some(battery) = &snapshot.battery {
            if let Some(percent) = battery.percent {
                level = format!("{percent:.0}");
                details.push(format!("{level}%"));
            }
            if let Some(state) = &battery.state {
                details.push(state.clone());
                charging = state == "Charging";
            }
            if let Some(hours) = battery.hours_remaining {
                let minutes = (hours * 60.0).round() as u64;
                details.push(format!("{}h {:02}m remaining", minutes / 60, minutes % 60));
            }
        }
        let mode_id = mode.map(Profile::id).unwrap_or("-");
        let mode_label = mode.map(Profile::name).unwrap_or("No matching preset");
        let battery = if details.is_empty() { "Battery status unavailable".into() } else { details.join(" · ") };
        let message = message.replace(['\t', '\r', '\n'], " ");
        let update = format!("{level}\t{}\t{mode_id}\t{}\t{}\t{}\t{}\t{}\tVatra · {battery}\\nMode: {mode_label}{}\n",
            u8::from(charging), u8::from(ready), u8::from(autostart_enabled()),
            appearance.hex(), appearance.percent, appearance.bolt,
            if message.is_empty() { String::new() } else { format!("\\n{message}") });
        if update != self.previous {
            self.sni.update(update.clone());
            self.previous = update;
        }
        if let Some(fallback) = self.fallback.as_mut() {
            fallback.update(&self.previous);
        }
    }
}

fn config_dir() -> io::Result<PathBuf> {
    env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No user configuration directory"))
}

pub fn enabled() -> bool {
    config_dir().is_ok_and(|dir| dir.join("vatra/tray.enabled").is_file())
}

pub fn set_enabled(value: bool) -> io::Result<()> {
    set_file(config_dir()?.join("vatra/tray.enabled"), value.then_some(""))
}

pub fn autostart_enabled() -> bool {
    config_dir().is_ok_and(|dir| dir.join("autostart/vatra-tray.desktop").is_file())
}

pub fn set_autostart(value: bool) -> io::Result<()> {
    let path = config_dir()?.join("autostart/vatra-tray.desktop");
    if !value { return set_file(path, None); }
    let executable = env::current_exe()?;
    let executable = executable.to_str().ok_or(io::ErrorKind::InvalidInput)?;
    if executable.contains(['\n', '\r']) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let arg = executable.replace('\\', "\\\\\\\\").replace('"', "\\\\\"")
        .replace('$', "\\\\$").replace('`', "\\\\`").replace('%', "%%");
    let desktop = format!("[Desktop Entry]\nType=Application\nName=Vatra\nExec=\"{arg}\" --background\nTerminal=false\n");
    set_file(path, Some(&desktop))
}

fn set_file(path: PathBuf, contents: Option<&str>) -> io::Result<()> {
    if let Some(contents) = contents {
        fs::create_dir_all(path.parent().expect("settings path has a parent"))?;
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        let result = fs::write(&temporary, contents).and_then(|_| fs::rename(&temporary, &path));
        if result.is_err() { let _ = fs::remove_file(temporary); }
        return result;
    }
    fs::remove_file(path).or_else(|err| {
        if err.kind() == io::ErrorKind::NotFound { Ok(()) } else { Err(err) }
    })
}
