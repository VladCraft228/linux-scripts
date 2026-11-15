//! Application policy. No Qt types, raw pointers or shell-expanded Exec strings.
use gio::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn read_desktop(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("Desktop entry exceeds 1 MiB".into());
    }
    Ok(bytes)
}
pub fn token(s: &[u8]) -> String {
    format!("{:x}", Sha256::digest(s))
}
fn yes() -> bool {
    true
}
// QSettings INI values can become QString after restarting the process, even
// when originally written as integers. Accept both representations at the boundary.
fn deserialize_preferences<'de, D>(deserializer: D) -> Result<BTreeMap<String, i32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = BTreeMap::<String, serde_json::Value>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|(key, value)| {
            let integer = match value {
                serde_json::Value::String(text) => text.trim().parse::<i64>().ok(),
                serde_json::Value::Number(number) => number.as_i64().or_else(|| {
                    number
                        .as_f64()
                        .filter(|n| n.is_finite() && n.fract() == 0.0)
                        .map(|n| n.clamp(i32::MIN as f64, i32::MAX as f64) as i64)
                }),
                _ => None,
            };
            integer.map(|n| (key, n.clamp(i32::MIN as i64, i32::MAX as i64) as i32))
        })
        .collect())
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub pins: Vec<String>,
    #[serde(default)]
    pub imports: BTreeMap<String, String>,
    #[serde(default = "yes")]
    pub auto_hide: bool,
    #[serde(default, deserialize_with = "deserialize_preferences")]
    pub preferences: BTreeMap<String, i32>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            pins: vec![],
            imports: BTreeMap::new(),
            auto_hide: true,
            preferences: BTreeMap::new(),
        }
    }
}
pub const PREFS: &[(&str, i32, i32, i32)] = &[
    ("iconSize", 48, 24, 80),
    ("iconSpacing", 10, 0, 32),
    ("magnification", 55, 0, 100),
    ("opacity", 85, 0, 100),
    ("bottomOffset", 96, 0, 160),
    ("hideDelay", 700, 0, 3000),
    ("animationDuration", 120, 60, 500),
    ("borderEnabled", 1, 0, 1),
    ("borderColor", 0xffffff, 0, 0xffffff),
    ("backgroundColor", 0x2a303c, 0, 0xffffff),
    ("borderWidth", 1, 1, 8),
    ("borderOpacity", 38, 0, 100),
    ("indicatorStyle", 0, 0, 4),
    ("indicatorActiveColor", 0x78b3ff, 0, 0xffffff),
    ("indicatorRunningColor", 0x8d9aae, 0, 0xffffff),
    ("cornerRadius", 22, 0, 60),
];
impl Config {
    pub fn reset_preferences(&mut self) {
        self.preferences.clear();
        self.auto_hide = true;
        self.normalize();
    }
    pub fn normalize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.pins.retain(|id| seen.insert(id.clone()));
        for &(name, initial, min, max) in PREFS {
            let v = self.preferences.entry(name.into()).or_insert(initial);
            *v = (*v).clamp(min, max);
        }
    }
    pub fn preference(&mut self, name: &str, value: i32) {
        if let Some(&(_, _, min, max)) = PREFS.iter().find(|p| p.0 == name) {
            self.preferences.insert(name.into(), value.clamp(min, max));
        }
    }
}
#[derive(Clone, Default, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub window_id: u32,
    pub window_title: String,
    pub icon_url: String,
    pub window_active: bool,
    pub class_names: Vec<String>,
    pub desktop_id: String,
    pub icon_key: String,
}
#[derive(Clone, Default, Debug)]
pub struct Desktop {
    pub id: String,
    pub file: PathBuf,
    pub name: String,
    pub icon: String,
    pub wm_class: String,
    pub executable: String,
    pub fingerprint: String,
    pub icon_names: Vec<String>,
    pub show_in_chooser: bool,
}
impl Desktop {
    fn describe(app: &gio::DesktopAppInfo, id: String) -> Self {
        let file = app.filename().unwrap_or_default();
        let icon = app
            .icon()
            .map(|icon| {
                if let Some(themed) = icon.downcast_ref::<gio::ThemedIcon>() {
                    themed
                        .names()
                        .first()
                        .map(ToString::to_string)
                        .unwrap_or_default()
                } else if let Some(file) = icon.downcast_ref::<gio::FileIcon>() {
                    file.file()
                        .path()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                } else {
                    String::new()
                }
            })
            .unwrap_or_default();
        let icon_names = app
            .icon()
            .and_then(|icon| icon.downcast::<gio::ThemedIcon>().ok())
            .map(|icon| icon.names().iter().map(ToString::to_string).collect())
            .unwrap_or_default();
        Self {
            id,
            file: file.clone(),
            name: app.display_name().into(),
            icon,
            icon_names,
            show_in_chooser: app.should_show(),
            wm_class: app.startup_wm_class().unwrap_or_default().to_lowercase(),
            executable: app
                .executable()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase(),
            fingerprint: read_desktop(&file)
                .ok()
                .filter(|b| b.len() <= 1024 * 1024)
                .map(|b| token(&b))
                .unwrap_or_default(),
        }
    }
}
#[derive(Clone, Default, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub app_key: String,
    pub app_name: String,
    pub icon_url: String,
    pub app_active: bool,
    pub running_count: usize,
    pub pinned: bool,
    pub launching: bool,
    pub desktop_id: String,
    pub windows: Vec<u32>,
    pub titles: Vec<String>,
    pub active: Option<usize>,
    pub icon: String,
    pub icon_names: Vec<String>,
}
struct Pending {
    windows: Vec<u32>,
    deadline: Instant,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Activate(u32),
    Minimize(u32),
    Close(u32),
}
pub struct Model {
    pub config: Config,
    pub catalog: BTreeMap<String, Desktop>,
    pub groups: Vec<Group>,
    tasks: Vec<Task>,
    last_active: HashMap<String, u32>,
    pending: HashMap<String, Pending>,
    pub import_dir: PathBuf,
    pub data_dirs: Vec<PathBuf>,
    pub timeout: Duration,
}
impl Model {
    pub fn new(mut config: Config, import_dir: PathBuf, data_dirs: Vec<PathBuf>) -> Self {
        config.normalize();
        let mut catalog = BTreeMap::new();
        for app in gio::AppInfo::all() {
            if let Ok(app) = app.downcast::<gio::DesktopAppInfo>() {
                if !app.is_hidden() {
                    if let Some(id) = app.id() {
                        catalog.insert(id.to_string(), Desktop::describe(&app, id.to_string()));
                    }
                }
            }
        }
        for (id, path) in &config.imports {
            if let Some(app) = gio::DesktopAppInfo::from_filename(path) {
                if !app.is_hidden() {
                    catalog.insert(id.clone(), Desktop::describe(&app, id.clone()));
                }
            }
        }
        let mut model = Self {
            config,
            catalog,
            groups: vec![],
            tasks: vec![],
            last_active: HashMap::new(),
            pending: HashMap::new(),
            import_dir,
            data_dirs,
            timeout: Duration::from_secs(12),
        };
        model.rebuild();
        model
    }
    pub fn reload_catalog(&mut self) {
        let fresh = Self::new(
            self.config.clone(),
            self.import_dir.clone(),
            self.data_dirs.clone(),
        );
        self.catalog = fresh.catalog;
        self.rebuild();
    }
    pub fn available_applications(&self) -> serde_json::Value {
        let mut entries: Vec<_> = self
            .catalog
            .values()
            .filter(|e| {
                e.show_in_chooser
                    && e.id != "linux-scripts-dock.desktop"
                    && !e.file.as_os_str().is_empty()
            })
            .collect();
        entries.sort_by_key(|e| (e.name.to_lowercase(), e.id.clone()));
        serde_json::json!(entries.iter().map(|e|serde_json::json!({"id":e.id,"name":e.name,"pinned":self.config.pins.contains(&e.id)})).collect::<Vec<_>>())
    }
    pub fn pin_application(&mut self, id: &str) -> Result<(), String> {
        let file = self
            .catalog
            .get(id)
            .ok_or("Application is no longer available")?
            .file
            .clone();
        let url = url::Url::from_file_path(file).map_err(|_| "Invalid application desktop path")?;
        self.pin_files(&[url.to_string()], -1)
    }
    pub fn catalog_icon(&self, id: &str) -> serde_json::Value {
        self.catalog
            .get(id)
            .map(|e| serde_json::json!({"name":e.name,"icon":e.icon,"iconNames":e.icon_names}))
            .unwrap_or(serde_json::Value::Null)
    }
    pub fn match_app(&self, hint: &str, classes: &[String]) -> String {
        if !hint.is_empty() {
            let id = if hint.ends_with(".desktop") {
                hint.to_owned()
            } else {
                format!("{hint}.desktop")
            };
            if self.catalog.contains_key(&id) {
                return id;
            }
            if let Some(entry) = self.catalog.values().find(|e| e.file == Path::new(hint)) {
                return entry.id.clone();
            }
        }
        for explicit in [true, false] {
            let found: Vec<_> = self
                .catalog
                .values()
                .filter(|e| {
                    if explicit {
                        !e.wm_class.is_empty() && classes.contains(&e.wm_class)
                    } else {
                        classes.contains(&e.id.trim_end_matches(".desktop").to_lowercase())
                            || (!e.executable.is_empty() && classes.contains(&e.executable))
                    }
                })
                .collect();
            if found.len() == 1 {
                return found[0].id.clone();
            }
            if found.len() > 1 {
                return String::new();
            }
        }
        String::new()
    }
    pub fn update(&mut self, tasks: Vec<Task>) {
        self.tasks = tasks;
        self.rebuild();
    }
    pub fn rebuild(&mut self) {
        let mut groups: Vec<Group> = self
            .config
            .pins
            .iter()
            .map(|id| {
                let e = self.catalog.get(id);
                Group {
                    app_key: token(format!("desktop:{id}").as_bytes()),
                    desktop_id: id.clone(),
                    app_name: e.map(|e| e.name.clone()).unwrap_or(id.clone()),
                    pinned: true,
                    ..Default::default()
                }
            })
            .collect();
        for task in &self.tasks {
            let id = self.match_app(&task.desktop_id, &task.class_names);
            let identity = if !id.is_empty() {
                format!("desktop:{id}")
            } else if let Some(class) = task.class_names.last() {
                format!("class:{class}")
            } else {
                format!("window:{}", task.window_id)
            };
            let key = token(identity.as_bytes());
            let i = if let Some(i) = groups.iter().position(|g| g.app_key == key) {
                i
            } else {
                groups.push(Group {
                    app_key: key,
                    desktop_id: id.clone(),
                    app_name: self
                        .catalog
                        .get(&id)
                        .map(|e| e.name.clone())
                        .unwrap_or_else(|| {
                            task.class_names
                                .last()
                                .cloned()
                                .unwrap_or("Application".into())
                        }),
                    ..Default::default()
                });
                groups.len() - 1
            };
            let g = &mut groups[i];
            if g.icon_url.is_empty() {
                g.icon_url = task.icon_url.clone();
            }
            if task.window_active {
                g.active = Some(g.windows.len());
                self.last_active.insert(g.app_key.clone(), task.window_id);
            }
            g.windows.push(task.window_id);
            g.titles.push(task.window_title.clone());
        }
        for g in &mut groups {
            if let Some(e) = self.catalog.get(&g.desktop_id) {
                g.icon = e.icon.clone();
                g.icon_names = e.icon_names.clone();
            }
            if !g.desktop_id.is_empty() {
                g.icon_url = format!("image://application/{}", g.app_key);
            }
            g.app_active = g.active.is_some();
            g.running_count = g.windows.len();
        }
        self.pending.retain(|key, p| {
            groups
                .iter()
                .find(|g| &g.app_key == key)
                .is_some_and(|g| !g.windows.iter().any(|w| !p.windows.contains(w)))
                && Instant::now() < p.deadline
        });
        for g in &mut groups {
            g.launching = self.pending.contains_key(&g.app_key);
        }
        self.groups = groups;
    }
    pub fn next_timeout(&self) -> Option<u64> {
        self.pending
            .values()
            .map(|p| {
                p.deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .max(1) as u64
            })
            .min()
    }
    pub fn launch(&mut self, key: &str) -> Result<(), String> {
        if self.pending.contains_key(key) {
            return Ok(());
        }
        let group = self
            .groups
            .iter()
            .find(|g| g.app_key == key)
            .cloned()
            .ok_or("Unknown application")?;
        let app=self.catalog.get(&group.desktop_id).and_then(|e|gio::DesktopAppInfo::from_filename(&e.file)).ok_or("The application's desktop entry is unavailable. Restart the dock after installing applications.")?;
        let context = gio::AppLaunchContext::new();
        context.unsetenv("QT_QPA_PLATFORM");
        context.unsetenv("QT_QUICK_BACKEND");
        self.pending.insert(
            key.into(),
            Pending {
                windows: group.windows,
                deadline: Instant::now() + self.timeout,
            },
        );
        if let Err(error) = app.launch(&[], Some(&context)) {
            self.pending.remove(key);
            self.rebuild();
            return Err(error.to_string());
        }
        self.rebuild();
        Ok(())
    }
    pub fn activate(&mut self, key: &str) -> Result<Vec<Action>, String> {
        let Some(g) = self.groups.iter().find(|g| g.app_key == key) else {
            return Ok(vec![]);
        };
        if g.windows.is_empty() {
            self.launch(key)?;
            return Ok(vec![]);
        }
        if let Some(i) = g.active {
            return Ok(vec![Action::Minimize(g.windows[i])]);
        }
        let last = self
            .last_active
            .get(key)
            .filter(|w| g.windows.contains(w))
            .copied()
            .unwrap_or(g.windows[0]);
        Ok(vec![Action::Activate(last)])
    }
    pub fn perform(&mut self, action: &str, key: &str, wid: u32) -> Result<Vec<Action>, String> {
        if action == "autohide" {
            self.config.auto_hide = !self.config.auto_hide;
            return Ok(vec![]);
        }
        let Some(g) = self.groups.iter().find(|g| g.app_key == key).cloned() else {
            return Ok(vec![]);
        };
        match action {
            "launch" => self.launch(key)?,
            "close" if g.windows.contains(&wid) => return Ok(vec![Action::Close(wid)]),
            "window" if g.windows.contains(&wid) => return Ok(vec![Action::Activate(wid)]),
            "closeAll" => return Ok(g.windows.iter().map(|w| Action::Close(*w)).collect()),
            "pin" if !g.desktop_id.is_empty() => {
                if g.pinned {
                    self.config.pins.retain(|id| id != &g.desktop_id);
                } else {
                    self.config.pins.push(g.desktop_id);
                }
                self.rebuild();
            }
            _ => (),
        }
        Ok(vec![])
    }
    pub fn reorder(&mut self, key: &str, target: &str) -> bool {
        let from = self
            .groups
            .iter()
            .find(|g| g.app_key == key && g.pinned)
            .and_then(|g| self.config.pins.iter().position(|id| id == &g.desktop_id));
        let to = self
            .groups
            .iter()
            .find(|g| g.app_key == target && g.pinned)
            .and_then(|g| self.config.pins.iter().position(|id| id == &g.desktop_id));
        if let (Some(from), Some(to)) = (from, to) {
            let id = self.config.pins.remove(from);
            self.config.pins.insert(to, id);
            self.rebuild();
            true
        } else {
            false
        }
    }
    pub fn previews(&self, key: &str) -> serde_json::Value {
        self.groups.iter().find(|g|g.app_key==key).map(|g|serde_json::json!(g.windows.iter().enumerate().map(|(i,w)|serde_json::json!({"wid":w,"title":g.titles[i],"iconUrl":g.icon_url,"active":g.active==Some(i)})).collect::<Vec<_>>())).unwrap_or(serde_json::json!([]))
    }
    pub fn menu(&self, key: &str) -> serde_json::Value {
        let mut result = vec![];
        let mut add = |label: String, action: &str, enabled: bool, wid: u32| {
            result.push(
                serde_json::json!({"label":label,"action":action,"enabled":enabled,"wid":wid}),
            )
        };
        if let Some(g) = self.groups.iter().find(|g| g.app_key == key) {
            add(
                if g.pinned {
                    "Unpin application"
                } else {
                    "Pin application"
                }
                .into(),
                "pin",
                !g.desktop_id.is_empty(),
                0,
            );
            add(
                "Launch application".into(),
                "launch",
                self.catalog.contains_key(&g.desktop_id),
                0,
            );
            for (w, t) in g.windows.iter().zip(&g.titles) {
                add(t.clone(), "window", true, *w);
            }
            if g.windows.len() == 1 {
                add("Close window".into(), "close", true, g.windows[0]);
            } else if g.windows.len() > 1 {
                if let Some(i) = g.active {
                    add("Close active window".into(), "close", true, g.windows[i]);
                }
                add(
                    format!("Close all windows ({})", g.windows.len()),
                    "closeAll",
                    true,
                    0,
                );
            }
        }
        add(
            if self.config.auto_hide {
                "Disable intelligent hide"
            } else {
                "Enable intelligent hide"
            }
            .into(),
            "autohide",
            true,
            0,
        );
        add("Add application…".into(), "addApplication", true, 0);
        add("Settings…".into(), "settings", true, 0);
        add("About Dock…".into(), "about", true, 0);
        add("Quit dock".into(), "quit", true, 0);
        serde_json::json!(result)
    }
    pub fn local_files(urls: &[String]) -> Result<Vec<PathBuf>, String> {
        if urls.is_empty() || urls.len() > 32 {
            return Err("Drop 1–32 readable local .desktop files.".into());
        }
        let hostname = gethostname::gethostname().to_string_lossy().to_lowercase();
        urls.iter()
            .map(|s| {
                let mut url = url::Url::parse(s).map_err(|e| e.to_string())?;
                let host = url.host_str().unwrap_or("").to_lowercase();
                if url.scheme() != "file"
                    || (!host.is_empty() && host != "localhost" && host != hostname)
                {
                    return Err(format!("Not a local file URL: {s}"));
                }
                url.set_host(None).map_err(|_| "Invalid local host")?;
                let path = url
                    .to_file_path()
                    .map_err(|_| format!("Invalid local file URL: {s}"))?;
                let meta = fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                if !meta.is_file()
                    || meta.len() > 1024 * 1024
                    || !path
                        .extension()
                        .is_some_and(|s| s.eq_ignore_ascii_case("desktop"))
                {
                    return Err(format!("Not a bounded desktop entry: {}", path.display()));
                }
                fs::File::open(&path).map_err(|e| e.to_string())?;
                fs::canonicalize(path).map_err(|e| e.to_string())
            })
            .collect()
    }
    pub fn pin_files(&mut self, urls: &[String], position: i32) -> Result<(), String> {
        let paths = Self::local_files(urls)?;
        let mut plans = vec![];
        let mut config = self.config.clone();
        let mut pos = if position < 0 {
            config.pins.len()
        } else {
            (position as usize).min(config.pins.len())
        };
        for path in paths {
            let bytes = read_desktop(&path)?;
            if bytes.len() > 1024 * 1024 {
                return Err("Desktop entry grew beyond 1 MiB".into());
            }
            let app = gio::DesktopAppInfo::from_filename(&path)
                .ok_or_else(|| format!("Invalid or unavailable application: {}", path.display()))?;
            if app.is_hidden() || app.name().is_empty() {
                return Err("Hidden or unnamed desktop entry".into());
            }
            let hash = token(&bytes);
            let mut entry = Desktop::describe(&app, String::new());
            if let Some(known) = self
                .catalog
                .values()
                .find(|e| fs::canonicalize(&e.file).ok().as_ref() == Some(&path))
            {
                entry.id = known.id.clone();
            }
            if entry.id.is_empty() {
                let mut same: Vec<_> = self
                    .catalog
                    .values()
                    .filter(|e| e.fingerprint == hash)
                    .collect();
                same.sort_by_key(|e| !self.config.pins.contains(&e.id));
                if let Some(e) = same.first() {
                    entry = (*e).clone();
                }
            }
            if entry.id.is_empty() {
                for data in &self.data_dirs {
                    if let Ok(root) = fs::canonicalize(data.join("applications")) {
                        if let Ok(relative) = path.strip_prefix(root) {
                            let id = relative.to_string_lossy().replace('/', "-");
                            if let Some(registered) = gio::DesktopAppInfo::new(&id) {
                                if registered
                                    .filename()
                                    .and_then(|p| fs::canonicalize(p).ok())
                                    .as_ref()
                                    == Some(&path)
                                {
                                    entry.id = id;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            let managed = entry.id.is_empty();
            if managed {
                entry.id = format!("linux-scripts-import-{hash}.desktop");
                entry.file = self.import_dir.join(&entry.id);
                entry.fingerprint = hash;
                config
                    .imports
                    .insert(entry.id.clone(), entry.file.to_string_lossy().into_owned());
            }
            if !config.pins.contains(&entry.id) {
                config.pins.insert(pos, entry.id.clone());
                pos += 1;
            }
            if !plans
                .iter()
                .any(|(e, _, _): &(Desktop, Vec<u8>, bool)| e.id == entry.id)
            {
                plans.push((entry, bytes, managed));
            }
        }
        let mut created = vec![];
        for (entry, bytes, managed) in &plans {
            if *managed {
                let existed = entry.file.exists();
                let write = (|| -> Result<(), String> {
                    fs::create_dir_all(&self.import_dir).map_err(|e| e.to_string())?;
                    let tmp = entry
                        .file
                        .with_extension(format!("tmp-{}", std::process::id()));
                    use std::io::Write;
                    let mut f = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&tmp)
                        .map_err(|e| e.to_string())?;
                    let result = (|| {
                        f.write_all(bytes)?;
                        f.sync_all()?;
                        fs::rename(&tmp, &entry.file)
                    })()
                    .map_err(|e: std::io::Error| e.to_string());
                    let _ = fs::remove_file(tmp);
                    result
                })();
                if let Err(e) = write {
                    for path in created {
                        let _ = fs::remove_file(path);
                    }
                    return Err(e);
                }
                if !existed {
                    created.push(entry.file.clone());
                }
            }
        }
        for (entry, _, _) in plans {
            self.catalog.insert(entry.id.clone(), entry);
        }
        self.config = config;
        self.rebuild();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model() -> Model {
        let mut m = Model::new(
            Config::default(),
            std::env::temp_dir().join("dock-rust-tests"),
            vec![],
        );
        m.catalog.clear();
        m
    }
    fn entry(id: &str, class: &str) -> Desktop {
        Desktop {
            id: id.into(),
            name: id.into(),
            wm_class: class.into(),
            ..Default::default()
        }
    }
    #[test]
    fn grouping_and_click_policy() {
        let mut m = model();
        m.catalog
            .insert("app.desktop".into(), entry("app.desktop", "app"));
        m.config.pins.push("app.desktop".into());
        m.update(vec![Task {
            window_id: 7,
            class_names: vec!["app".into()],
            window_active: true,
            ..Default::default()
        }]);
        assert_eq!(m.groups.len(), 1);
        assert!(m.groups[0].pinned);
        let key = m.groups[0].app_key.clone();
        assert_eq!(m.activate(&key).unwrap(), vec![Action::Minimize(7)]);
        assert!(m.perform("close", &key, 9).unwrap().is_empty());
        m.update(vec![]);
        assert_eq!(m.groups.len(), 1);
        assert_eq!(m.groups[0].running_count, 0);
    }
    #[test]
    fn ambiguous_matches_are_not_guessed() {
        let mut m = model();
        for id in ["a.desktop", "b.desktop"] {
            m.catalog.insert(id.into(), entry(id, "same"));
        }
        assert_eq!(m.match_app("", &["same".into()]), "");
        assert_eq!(m.match_app("a", &[]), "a.desktop");
    }
    #[test]
    fn settings_clamp_and_deduplicate() {
        let mut c = Config {
            pins: vec!["a".into(), "a".into()],
            ..Default::default()
        };
        c.normalize();
        c.preference("animationDuration", 10000);
        assert_eq!(c.pins.len(), 1);
        assert_eq!(c.preferences["animationDuration"], 500);
        c.preference("bad", 0);
        assert!(!c.preferences.contains_key("bad"));
    }
    #[test]
    fn rejected_urls_do_not_change_pins() {
        let mut m = model();
        assert!(m
            .pin_files(&["https://example.org/app.desktop".into()], 0)
            .is_err());
        assert!(m.config.pins.is_empty());
    }
    #[test]
    fn launch_completion_requires_new_window() {
        let mut m = model();
        m.config.pins.push("app.desktop".into());
        m.rebuild();
        let key = m.groups[0].app_key.clone();
        m.pending.insert(
            key.clone(),
            Pending {
                windows: vec![7],
                deadline: Instant::now() + Duration::from_secs(1),
            },
        );
        m.update(vec![Task {
            window_id: 7,
            desktop_id: "app".into(),
            ..Default::default()
        }]); // Need explicit catalog identity.
        m.catalog
            .insert("app.desktop".into(), entry("app.desktop", "app"));
        m.pending.insert(
            key.clone(),
            Pending {
                windows: vec![7],
                deadline: Instant::now() + Duration::from_secs(1),
            },
        );
        m.update(vec![Task {
            window_id: 7,
            desktop_id: "app".into(),
            ..Default::default()
        }]);
        assert!(m.groups[0].launching);
        m.update(vec![Task {
            window_id: 8,
            desktop_id: "app".into(),
            ..Default::default()
        }]);
        assert!(!m.groups[0].launching);
    }
    #[test]
    fn pinned_order_and_explicit_window_close() {
        let mut m = model();
        for id in ["a.desktop", "b.desktop"] {
            m.catalog.insert(id.into(), entry(id, id));
            m.config.pins.push(id.into());
        }
        m.update(vec![
            Task {
                window_id: 10,
                desktop_id: "a".into(),
                ..Default::default()
            },
            Task {
                window_id: 11,
                desktop_id: "a".into(),
                ..Default::default()
            },
        ]);
        let a = m.groups[0].app_key.clone();
        let b = m.groups[1].app_key.clone();
        assert!(m.reorder(&a, &b));
        assert_eq!(m.config.pins[0], "b.desktop");
        assert_eq!(
            m.perform("closeAll", &a, 0).unwrap(),
            vec![Action::Close(10), Action::Close(11)]
        );
        assert_eq!(
            m.perform("window", &a, 11).unwrap(),
            vec![Action::Activate(11)]
        );
        assert_eq!(m.previews(&a).as_array().unwrap().len(), 2);
    }
    #[test]
    fn timeout_clears_pending_launch_without_polling() {
        let mut m = model();
        m.config.pins.push("a.desktop".into());
        m.rebuild();
        let key = m.groups[0].app_key.clone();
        m.pending.insert(
            key.clone(),
            Pending {
                windows: vec![],
                deadline: Instant::now() - Duration::from_millis(1),
            },
        );
        m.rebuild();
        assert!(!m.groups[0].launching);
        assert!(m.next_timeout().is_none());
    }
    #[test]
    fn imported_launcher_survives_source_removal_and_deduplicates() {
        let root = std::env::temp_dir().join(format!("dock-import-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("External launcher.desktop");
        let bytes=b"[Desktop Entry]\nType=Application\nName=Dock Rust Import Test\nExec=/usr/bin/true\nStartupWMClass=dock-rust-import-test\n";
        fs::write(&path, bytes).unwrap();
        let mut m = model();
        m.import_dir = root.join("copies");
        let url = url::Url::from_file_path(&path).unwrap().to_string();
        m.pin_files(&[url.clone(), url.clone()], 0).unwrap();
        assert_eq!(m.config.pins.len(), 1);
        let count = m.config.pins.len();
        let invalid = root.join("invalid.desktop");
        fs::write(&invalid, b"invalid").unwrap();
        assert!(m
            .pin_files(
                &[
                    url.clone(),
                    url::Url::from_file_path(&invalid).unwrap().to_string()
                ],
                0
            )
            .is_err());
        assert_eq!(m.config.pins.len(), count);
        fs::remove_file(path).unwrap();
        let mut restored = Model::new(m.config.clone(), m.import_dir.clone(), vec![]);
        let key = restored.groups[0].app_key.clone();
        assert_eq!(restored.groups[0].app_name, "Dock Rust Import Test");
        restored.launch(&key).unwrap();
        assert!(restored.groups[0].launching);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn ini_numeric_strings_survive_restart() {
        let input = serde_json::json!({
            "pins": ["my-game.desktop"],
            "imports": {"my-game.desktop": "/tmp/my-game.desktop"},
            "preferences": {
                "iconSize": "64", "animationDuration": "150", "hideDelay": " 250 ",
                "bottomOffset": 8, "magnification": 55.0, "opacity": "85"
            }
        });
        let mut config: Config = serde_json::from_value(input).unwrap();
        config.normalize();
        assert_eq!(config.preferences["animationDuration"], 150);
        assert_eq!(config.preferences["iconSize"], 64);
        assert_eq!(config.preferences["hideDelay"], 250);
        assert_eq!(config.preferences["bottomOffset"], 8);
        assert_eq!(config.preferences["magnification"], 55);
        assert_eq!(config.pins, vec!["my-game.desktop"]);
        assert_eq!(config.imports["my-game.desktop"], "/tmp/my-game.desktop");
        let restored: Config =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(restored, config);
    }
    #[test]
    fn malformed_preference_does_not_discard_other_settings() {
        let mut config: Config = serde_json::from_value(serde_json::json!({
            "preferences": {"iconSize": "64", "opacity": "invalid", "hideDelay": "-10", "animationDuration": "999999"}
        })).unwrap();
        config.normalize();
        assert_eq!(config.preferences["iconSize"], 64);
        assert_eq!(config.preferences["opacity"], 85);
        assert_eq!(config.preferences["hideDelay"], 0);
        assert_eq!(config.preferences["animationDuration"], 500);
    }
    #[test]
    fn transparent_background_border_and_radius_persist_independently() {
        let mut c = Config::default();
        c.normalize();
        c.preference("opacity", 0);
        c.preference("borderEnabled", 1);
        c.preference("cornerRadius", 0);
        let mut loaded: Config = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        loaded.normalize();
        assert_eq!(loaded.preferences["opacity"], 0);
        assert_eq!(loaded.preferences["borderEnabled"], 1);
        assert_eq!(loaded.preferences["cornerRadius"], 0);
        loaded.preference("borderEnabled", 0);
        loaded.preference("cornerRadius", 1000);
        assert_eq!(loaded.preferences["cornerRadius"], 60);
        assert_eq!(loaded.preferences["opacity"], 0);
    }
    #[test]
    fn border_color_roundtrip_and_bounds() {
        let mut c = Config::default();
        c.normalize();
        c.preference("borderColor", 0xff667a);
        let loaded: Config = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(loaded.preferences["borderColor"], 0xff667a);
        c.preference("borderColor", -1);
        assert_eq!(c.preferences["borderColor"], 0);
    }
    #[test]
    fn background_color_is_independent_and_persistent() {
        let mut c = Config::default();
        c.normalize();
        assert_eq!(c.preferences["backgroundColor"], 0x2a303c);
        c.preference("backgroundColor", 0x123456);
        c.preference("opacity", 0);
        let restored: Config = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(restored.preferences["backgroundColor"], 0x123456);
        assert_eq!(restored.preferences["borderColor"], 0xffffff);
        assert_eq!(restored.preferences["opacity"], 0);
    }
    #[test]
    fn reset_only_appearance_and_behavior_keeps_launchers() {
        let mut c = Config::default();
        c.normalize();
        c.pins.push("game.desktop".into());
        c.imports
            .insert("game.desktop".into(), "/tmp/game.desktop".into());
        c.preference("indicatorStyle", 3);
        c.preference("indicatorActiveColor", 0xff0000);
        c.auto_hide = false;
        c.reset_preferences();
        assert_eq!(c.preferences["indicatorStyle"], 0);
        assert_eq!(c.preferences["indicatorActiveColor"], 0x78b3ff);
        assert!(c.auto_hide);
        assert_eq!(c.pins, vec!["game.desktop"]);
        assert_eq!(c.imports.len(), 1);
    }
    #[test]
    fn border_opacity_is_independent_and_about_is_always_available() {
        let mut m = model();
        m.config.preference("borderOpacity", 0);
        assert_eq!(m.config.preferences["borderOpacity"], 0);
        assert_eq!(m.config.preferences["opacity"], 85);
        m.config.preference("borderOpacity", 500);
        assert_eq!(m.config.preferences["borderOpacity"], 100);
        assert!(m
            .menu("")
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["action"] == "about"));
        m.config.reset_preferences();
        assert_eq!(m.config.preferences["borderOpacity"], 38);
    }
    #[test]
    fn icon_spacing_persists_clamps_and_resets() {
        let mut c = Config::default();
        c.normalize();
        assert_eq!(c.preferences["iconSpacing"], 10);
        c.preference("iconSpacing", 24);
        let restored: Config = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(restored.preferences["iconSpacing"], 24);
        c.preference("iconSpacing", -1);
        assert_eq!(c.preferences["iconSpacing"], 0);
        c.preference("iconSpacing", 999);
        assert_eq!(c.preferences["iconSpacing"], 32);
        c.reset_preferences();
        assert_eq!(c.preferences["iconSpacing"], 10);
    }
    #[test]
    fn chooser_filters_sorts_and_pins_without_execution_or_duplicates() {
        let root = std::env::temp_dir().join(format!("dock-chooser-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("app.desktop");
        let marker = root.join("launched");
        fs::write(
            &path,
            format!(
                "[Desktop Entry]\nType=Application\nName=Chooser App\nExec=/usr/bin/touch {}\n",
                marker.display()
            ),
        )
        .unwrap();
        let mut m = model();
        let mut e = entry("test.desktop", "test");
        e.file = path.clone();
        e.name = "Chooser App".into();
        e.show_in_chooser = true;
        m.catalog.insert(e.id.clone(), e);
        m.catalog
            .insert("hidden.desktop".into(), entry("hidden.desktop", "hidden"));
        assert_eq!(m.available_applications().as_array().unwrap().len(), 1);
        m.pin_application("test.desktop").unwrap();
        m.pin_application("test.desktop").unwrap();
        assert_eq!(m.config.pins, vec!["test.desktop"]);
        assert!(!marker.exists());
        assert!(path.exists());
        assert!(m.available_applications()[0]["pinned"].as_bool().unwrap());
        assert!(m.pin_application("unknown.desktop").is_err());
        let _ = fs::remove_dir_all(root);
    }
}
