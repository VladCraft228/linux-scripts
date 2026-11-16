//! Application-supplied launcher badges. Never owns org.freedesktop.Notifications.
use std::{
    collections::HashMap,
    io::{Read, Write},
    os::unix::net::UnixStream,
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::{blocking::{Connection, MessageIterator}, zvariant::OwnedValue};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Badge { pub count: u64, pub urgent: bool }
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Record { count: u64, visible: bool, urgent: bool, count_serial: u64 }
#[derive(Default)]
struct State {
    records: HashMap<(String, String), Record>,
    generation: u64,
}
impl State {
    fn update(&mut self, sender: &str, uri: &str, values: &HashMap<String, OwnedValue>) -> bool {
        let Some(id) = application_id(uri) else { return false; };
        if values.len() > 32 || sender.len() > 255 { return false; }
        let key = (sender.to_string(), id);
        if !self.records.contains_key(&key) && self.records.len() >= 1024 { return false; }
        let previous = self.records.get(&key).copied().unwrap_or_default();
        let mut next = previous;
        if let Some(value) = values.get("count").and_then(integer) { next.count = value; }
        if let Some(value) = values.get("count-visible").and_then(|v| bool::try_from(v).ok()) { next.visible = value; }
        if let Some(value) = values.get("urgent").and_then(|v| bool::try_from(v).ok()) { next.urgent = value; }
        if previous == next { return false; }
        self.generation = self.generation.wrapping_add(1);
        if previous.count != next.count || previous.visible != next.visible { next.count_serial = self.generation; }
        self.records.insert(key, next);
        true
    }
    fn remove_sender(&mut self, sender: &str) -> bool {
        let previous = self.records.len();
        self.records.retain(|(owner, _), _| owner != sender);
        if previous == self.records.len() { return false; }
        self.generation = self.generation.wrapping_add(1);
        true
    }
    fn snapshot(&self) -> HashMap<String, (Badge, u64)> {
        let mut badges: HashMap<String, (Badge, u64)> = HashMap::new();
        for ((_, id), record) in &self.records {
            let entry = badges.entry(id.clone()).or_default();
            entry.0.urgent |= record.urgent;
            if record.count_serial >= entry.1 {
                entry.0.count = if record.visible { record.count } else { 0 };
                entry.1 = record.count_serial;
            }
        }
        badges
    }
}
fn integer(value: &OwnedValue) -> Option<u64> {
    i64::try_from(value).ok().map(|n| n.max(0) as u64)
        .or_else(|| u64::try_from(value).ok())
        .or_else(|| i32::try_from(value).ok().map(|n| n.max(0) as u64))
        .or_else(|| u32::try_from(value).ok().map(u64::from))
}
fn application_id(uri: &str) -> Option<String> {
    if uri.len() > 1024 { return None; }
    let id = percent_encoding::percent_decode_str(uri.strip_prefix("application://")?).decode_utf8().ok()?;
    if id.is_empty() || id.len() > 512 || id.contains('/') || id.contains('\\') || id.chars().any(char::is_control) { return None; }
    // Match the catalog's existing normalization, including IDs such as
    // org.telegram.desktop.desktop whose application ID itself ends in .desktop.
    let id = id.trim_end_matches(".desktop").to_lowercase();
    (!id.is_empty()).then_some(id)
}
pub struct Badges {
    wake: UnixStream,
    state: Arc<Mutex<State>>,
    generation: u64,
    cache: HashMap<String, (Badge, u64)>,
}
impl Badges {
    pub fn new() -> std::io::Result<Self> {
        let (wake, writer) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        writer.set_nonblocking(true)?;
        let state = Arc::new(Mutex::new(State::default()));
        let worker_state = state.clone();
        std::thread::Builder::new().name("dock-launcher-badges".into()).spawn(move || {
            let mut writer = writer;
            let mut reported_failure = false;
            loop {
                if let Err(error) = listen(&worker_state, &mut writer) {
                    if !reported_failure {
                        eprintln!("Launcher badges unavailable: {error}; X11 attention still works.");
                        reported_failure = true;
                    }
                }
                {
                    let mut state = worker_state.lock().unwrap_or_else(|e| e.into_inner());
                    if !state.records.is_empty() {
                        state.records.clear(); state.generation = state.generation.wrapping_add(1);
                        let _ = writer.write(&[1]);
                    }
                }
                // Only reconnect after an unavailable/disconnected bus, never poll updates.
                std::thread::sleep(Duration::from_secs(15));
            }
        })?;
        Ok(Self { wake, state, generation: 0, cache: HashMap::new() })
    }
    pub fn wake(&self) -> &UnixStream { &self.wake }
    pub fn drain(&mut self) -> bool {
        let mut buffer = [0; 256];
        while self.wake.read(&mut buffer).is_ok_and(|n| n != 0) {}
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation == self.generation { return false; }
        self.generation = state.generation;
        self.cache = state.snapshot();
        true
    }
    pub fn for_ids(&self, ids: &[String]) -> Badge {
        let mut result = Badge::default();
        let mut serial = 0;
        for id in ids {
            if let Some((badge, sequence)) = self.cache.get(id) {
                result.urgent |= badge.urgent;
                if *sequence >= serial { result.count = badge.count; serial = *sequence; }
            }
        }
        result
    }
}
fn listen(state: &Arc<Mutex<State>>, wake: &mut UnixStream) -> zbus::Result<()> {
    let connection = Connection::session()?;
    let mut messages = MessageIterator::from(&connection);
    for rule in [
        "type='signal',interface='com.canonical.Unity.LauncherEntry',member='Update'",
        "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'",
    ] {
        connection.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "AddMatch", &(rule,))?;
    }
    // Some apps publish their initial state only once a launcher service appears.
    // DoNotQueue avoids taking over another dock's service or replacing its owner.
    let _ = connection.request_name_with_flags("com.canonical.Unity", zbus::fdo::RequestNameFlags::DoNotQueue.into());
    eprintln!("Launcher badges: listening on the session bus");
    for message in &mut messages {
        let message = message?;
        let header = message.header();
        let changed = if header.interface().is_some_and(|i| i.as_str() == "com.canonical.Unity.LauncherEntry") && header.member().is_some_and(|m| m.as_str() == "Update") {
            if let (Some(sender), Ok((uri, properties))) = (header.sender(), message.body().deserialize::<(String, HashMap<String, OwnedValue>)>()) {
                state.lock().unwrap_or_else(|e| e.into_inner()).update(sender.as_str(), &uri, &properties)
            } else { false }
        } else if header.sender().is_some_and(|s| s.as_str() == "org.freedesktop.DBus") && header.interface().is_some_and(|i| i.as_str() == "org.freedesktop.DBus") && header.member().is_some_and(|m| m.as_str() == "NameOwnerChanged") {
            if let Ok((name, old, new)) = message.body().deserialize::<(String, String, String)>() {
                name.starts_with(':') && !old.is_empty() && new.is_empty() && state.lock().unwrap_or_else(|e| e.into_inner()).remove_sender(&old)
            } else { false }
        } else { false };
        if changed { let _ = wake.write(&[1]); }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_uri_is_bounded_and_normalized() {
        assert_eq!(application_id("application://Org.Example.App.desktop").as_deref(), Some("org.example.app"));
        assert!(application_id("application://%2ftmp%2fapp.desktop").is_none());
        assert_eq!(application_id("application://org.telegram.desktop.desktop").as_deref(), Some("org.telegram"));
    }
    #[test]
    fn partial_updates_clear_and_sender_exit_removes_state() {
        let mut state = State::default();
        let uri = "application://app.desktop";
        assert!(state.update(":1.20", uri, &HashMap::from([("count".into(), OwnedValue::from(7i64)), ("count-visible".into(), OwnedValue::from(true))])));
        assert_eq!(state.snapshot()["app"].0.count, 7);
        state.update(":1.20", uri, &HashMap::from([("urgent".into(), OwnedValue::from(true))]));
        assert_eq!(state.snapshot()["app"].0.count, 7);
        state.update(":1.20", uri, &HashMap::from([("count-visible".into(), OwnedValue::from(false))]));
        assert_eq!(state.snapshot()["app"].0.count, 0);
        assert!(state.remove_sender(":1.20"));
        assert!(state.snapshot().is_empty());
    }
}
