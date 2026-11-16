use crate::{
    icons::{window_icon, Icons},
    Result,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::Arc,
};
use tiny_skia::Pixmap;
use x11rb::{connection::Connection, protocol::xproto::*};

#[derive(Clone)]
pub struct Item {
    pub name: String,
    pub pinned: bool,
    pub launcher: Option<Launch>,
    pub key: String,
    pub icon: Option<Arc<Pixmap>>,
    pub windows: Vec<u32>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Launch {
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub terminal: bool,
}
impl Launch {
    pub fn spawn(&self) -> std::io::Result<Child> {
        let mut cmd = if self.terminal {
            let mut c = Command::new("xfce4-terminal");
            c.arg("--execute").args(&self.argv);
            c
        } else {
            let mut c = Command::new(&self.argv[0]);
            c.args(&self.argv[1..]);
            c
        };
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        cmd.spawn()
    }
}
const SHORTCUTS: [(&str, &str, &[&str]); 5] = [
    ("xfce4-terminal", "utilities-terminal", &["xfce4-terminal"]),
    ("thunar", "system-file-manager", &["thunar"]),
    ("firefox", "firefox", &["firefox", "firefox-esr"]),
    (
        "mousepad",
        "accessories-text-editor",
        &["org.xfce.mousepad", "mousepad"],
    ),
    ("xfce4-appfinder", "system-search", &["xfce4-appfinder"]),
];
#[derive(Default)]
struct Desktop {
    path: PathBuf,
    fingerprint: String,
    name: String,
    launcher: Option<Launch>,
    id: String,
    icon: String,
    aliases: Vec<String>,
    menu_visible: bool,
}
#[derive(Default)]
struct Catalog {
    entries: Vec<Desktop>,
    aliases: HashMap<String, Option<usize>>,
}
fn normalize(s: &str) -> String {
    s.trim().trim_end_matches(".desktop").to_lowercase()
}
fn fingerprint(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(data))
}
fn imports_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|p| p.join("linux-scripts-dock-rust-prototype/launchers"))
}
fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(p) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    {
        dirs.push(p)
    }
    dirs.extend(std::env::split_paths(
        &std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into()),
    ));
    dirs
}
fn files(base: &Path, dir: &Path, depth: usize, out: &mut Vec<(String, PathBuf)>) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if p.is_dir() && !e.file_type().is_ok_and(|t| t.is_symlink()) {
            files(base, &p, depth + 1, out)
        } else if p.extension().is_some_and(|x| x == "desktop") {
            if let Ok(rel) = p.strip_prefix(base) {
                out.push((rel.to_string_lossy().replace('/', "-"), p));
            }
        }
    }
}
fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => out.push(' '),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other)
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c)
        }
    }
    out
}
fn expand_exec(exec: &str, name: &str, icon: &str, path: &Path) -> Option<Vec<String>> {
    let mut argv = Vec::new();
    for arg in shell_words::split(&unescape(exec)).ok()? {
        if ["%f", "%F", "%u", "%U", "%d", "%D", "%n", "%N", "%v", "%m"].contains(&arg.as_str()) {
            continue;
        }
        if arg == "%i" {
            if !icon.is_empty() {
                argv.extend(["--icon".into(), icon.into()]);
            }
            continue;
        }
        let mut expanded = String::new();
        let mut chars = arg.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                expanded.push(c);
                continue;
            }
            match chars.next()? {
                '%' => expanded.push('%'),
                'c' => expanded.push_str(name),
                'k' => expanded.push_str(&path.to_string_lossy()),
                _ => return None,
            }
        }
        argv.push(expanded);
    }
    if argv.first().is_none_or(|a| a.is_empty()) {
        return None;
    }
    Some(argv)
}
#[cfg(test)]
fn parse(id: &str, text: &str) -> Option<Desktop> {
    parse_at(id, text, Path::new(id))
}
fn parse_at(id: &str, text: &str, path: &Path) -> Option<Desktop> {
    let mut section = false;
    let mut values = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line == "[Desktop Entry]";
            continue;
        }
        if section && !line.starts_with('#') {
            if let Some((k, v)) = line.split_once('=') {
                values.insert(k.trim(), v.trim());
            }
        }
    }
    if values.get("Hidden") == Some(&"true") || values.get("Type") != Some(&"Application") {
        return None;
    }
    let mut aliases = vec![normalize(id)];
    if let Some(class) = values.get("StartupWMClass") {
        aliases.push(normalize(class))
    }
    if let Some(exec) = values
        .get("Exec")
        .and_then(|e| shell_words::split(e).ok())
        .and_then(|v| v.into_iter().next())
    {
        let name = Path::new(&exec)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if ![
            "env", "sh", "bash", "python", "python3", "java", "wine", "flatpak", "steam",
        ]
        .contains(&name.as_str())
        {
            aliases.push(normalize(&name))
        }
    }
    let name = unescape(values.get("Name").copied().unwrap_or(id));
    let icon = unescape(values.get("Icon").copied().unwrap_or(""));
    let launcher = values
        .get("Exec")
        .and_then(|exec| expand_exec(exec, &name, &icon, path))
        .map(|argv| Launch {
            argv,
            cwd: values
                .get("Path")
                .filter(|s| !s.is_empty())
                .map(|s| PathBuf::from(unescape(s))),
            terminal: values.get("Terminal") == Some(&"true"),
        });
    let desktops = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let desktops: Vec<_> = desktops.split(':').collect();
    let listed = |value: &&str| value.split(';').any(|de| !de.is_empty() && desktops.iter().any(|current| current.eq_ignore_ascii_case(de)));
    let menu_visible = values.get("NoDisplay") != Some(&"true")
        && values.get("OnlyShowIn").is_none_or(listed)
        && !values.get("NotShowIn").is_some_and(listed)
        && values.get("TryExec").is_none_or(|exec| executable(&unescape(exec)));
    Some(Desktop {
        menu_visible,
        path: path.to_path_buf(),
        fingerprint: fingerprint(text.as_bytes()),
        name,
        launcher,
        id: normalize(id),
        icon,
        aliases,
    })
}
fn localized_name(path: &Path) -> Option<String> {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))?;
    let locale = locale.split('.').next()?;
    let language = locale.split('_').next()?;
    let text = fs::read_to_string(path).ok()?;
    let mut section = false;
    let mut names = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') { section = line == "[Desktop Entry]"; }
        if section { if let Some((key, value)) = line.split_once('=') { names.insert(key.trim(), value.trim()); } }
    }
    names.get(format!("Name[{locale}]").as_str()).or_else(|| names.get(format!("Name[{language}]").as_str())).map(|value| unescape(value))
}
fn executable(command: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let usable = |path: &Path| fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    if command.contains('/') { return usable(Path::new(command)); }
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| usable(&dir.join(command))))
}
impl Catalog {
    fn load() -> Self {
        let mut seen = HashSet::new();
        let mut c = Self::default();
        let mut bases: Vec<_> = data_dirs()
            .into_iter()
            .map(|d| d.join("applications"))
            .collect();
        if let Some(imports) = imports_dir() {
            bases.push(imports);
        }
        for base in bases {
            let mut list = Vec::new();
            files(&base, &base, 0, &mut list);
            for (id, path) in list {
                if !seen.insert(id.clone()) {
                    continue;
                }
                if let Ok(text) = fs::read_to_string(&path) {
                    if let Some(d) = parse_at(&id, &text, &path) {
                        c.entries.push(d)
                    }
                }
            }
        }
        for (i, d) in c.entries.iter().enumerate() {
            for alias in &d.aliases {
                c.aliases
                    .entry(alias.clone())
                    .and_modify(|e| {
                        if *e != Some(i) {
                            *e = None
                        }
                    })
                    .or_insert(Some(i));
            }
        }
        c
    }
    fn find(&self, keys: &[String]) -> Option<&Desktop> {
        for k in keys {
            if let Some(d) = self.entries.iter().find(|d| d.id == *k) {
                return Some(d);
            }
        }
        keys.iter().find_map(|k| {
            self.aliases
                .get(k)
                .copied()
                .flatten()
                .map(|i| &self.entries[i])
        })
    }
}
#[derive(Clone)]
pub struct Atoms {
    pub clients: u32,
    pub active: u32,
    pub state: u32,
    pub demands_attention: u32,
    pub icon: u32,
    pub class: u32,
    pub gtk: u32,
    pub kde: u32,
    pub types: u32,
    pub skip: u32,
    pub hidden: u32,
    pub normal: u32,
    pub dialog: u32,
    pub activate: u32,
    pub minimize: u32,
    pub desktop: u32,
    pub current: u32,
    pub pid: u32,
    pub name: u32,
    pub close: u32,
    pub showing_desktop: u32,
    pub frame_extents: u32,
}
impl Atoms {
    pub fn new<C: Connection>(c: &C) -> Result<Self> {
        let a = |s: &str| -> Result<u32> { Ok(c.intern_atom(false, s.as_bytes())?.reply()?.atom) };
        Ok(Self {
            clients: a("_NET_CLIENT_LIST")?,
            active: a("_NET_ACTIVE_WINDOW")?,
            state: a("_NET_WM_STATE")?,
            demands_attention: a("_NET_WM_STATE_DEMANDS_ATTENTION")?,
            icon: a("_NET_WM_ICON")?,
            class: AtomEnum::WM_CLASS.into(),
            gtk: a("_GTK_APPLICATION_ID")?,
            kde: a("_KDE_NET_WM_DESKTOP_FILE")?,
            types: a("_NET_WM_WINDOW_TYPE")?,
            skip: a("_NET_WM_STATE_SKIP_TASKBAR")?,
            hidden: a("_NET_WM_STATE_HIDDEN")?,
            normal: a("_NET_WM_WINDOW_TYPE_NORMAL")?,
            dialog: a("_NET_WM_WINDOW_TYPE_DIALOG")?,
            activate: a("_NET_ACTIVE_WINDOW")?,
            minimize: a("WM_CHANGE_STATE")?,
            desktop: a("_NET_WM_DESKTOP")?,
            current: a("_NET_CURRENT_DESKTOP")?,
            pid: a("_NET_WM_PID")?,
            name: a("_NET_WM_NAME")?,
            close: a("_NET_CLOSE_WINDOW")?,
            showing_desktop: a("_NET_SHOWING_DESKTOP")?,
            frame_extents: a("_NET_FRAME_EXTENTS")?,
        })
    }
    pub fn watched(&self, a: u32) -> bool {
        [
            self.clients,
            self.active,
            self.state,
            self.icon,
            self.class,
            self.gtk,
            self.kde,
            self.types,
            self.desktop,
            self.pid,
            self.name,
            AtomEnum::WM_NAME.into(),
            AtomEnum::WM_HINTS.into(),
            self.current,
            self.showing_desktop,
            self.frame_extents,
        ]
        .contains(&a)
    }
}
fn property<C: Connection>(c: &C, w: u32, a: u32, length: u32) -> Option<GetPropertyReply> {
    c.get_property(false, w, a, AtomEnum::ANY, 0, length)
        .ok()?
        .reply()
        .ok()
}
pub(crate) fn words<C: Connection>(c: &C, w: u32, a: u32) -> Vec<u32> {
    property(c, w, a, 262_144)
        .and_then(|r| r.value32().map(|v| v.collect()))
        .unwrap_or_default()
}
fn strings<C: Connection>(c: &C, w: u32, a: u32) -> Vec<String> {
    property(c, w, a, 4096)
        .map(|r| {
            r.value
                .split(|&b| b == 0)
                .filter_map(|s| std::str::from_utf8(s).ok())
                .map(normalize)
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub active: bool,
}
pub struct Apps {
    pub settings_window: Option<u32>,
    titles: HashMap<u32, String>,
    pub items: Vec<Item>,
    pub active: u32,
    pub attention: HashSet<String>,
    pub notification_ids: HashMap<String, Vec<String>>,
    pub atoms: Atoms,
    catalog: Catalog,
    pins: crate::pins::Pins,
    icons: Icons,
    watching: HashSet<u32>,
}
impl Apps {
    pub fn new<C: Connection>(c: &C) -> Result<Self> {
        let catalog = Catalog::load();
        let defaults = SHORTCUTS
            .iter()
            .map(|(cmd, _, aliases)| {
                let keys: Vec<_> = aliases.iter().map(|s| normalize(s)).collect();
                catalog
                    .find(&keys)
                    .map(|d| d.id.clone())
                    .unwrap_or_else(|| normalize(cmd))
            })
            .collect();
        let pins = crate::pins::Pins::load(defaults)?;
        Ok(Self {
            settings_window: None,
            titles: HashMap::new(),
            pins,
            items: Vec::new(),
            active: 0,
            attention: HashSet::new(),
            notification_ids: HashMap::new(),
            atoms: Atoms::new(c)?,
            catalog,
            icons: Icons::new(),
            watching: HashSet::new(),
        })
    }
    pub fn refresh<C: Connection>(&mut self, c: &C, root: u32, own: u32) {
        self.attention.clear();
        let old: Vec<String> = self.items.iter().map(|i| i.key.clone()).collect();
        let mut items = Vec::new();
        let mut pin_alias = HashMap::new();
        for (command, fallback, aliases) in SHORTCUTS {
            let keys: Vec<_> = aliases.iter().map(|s| normalize(s)).collect();
            let entry = self.catalog.find(&keys);
            let key = entry
                .map(|d| d.id.clone())
                .unwrap_or_else(|| normalize(command));
            let name = entry
                .map(|d| d.icon.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(fallback);
            let icon = self.icons.get(name).or_else(|| self.icons.get(fallback));
            for alias in keys {
                pin_alias.insert(alias, key.clone());
            }
            items.push(Item {
                key,
                icon,
                windows: Vec::new(),
                name: entry
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| command.into()),
                launcher: entry.and_then(|d| d.launcher.clone()).or_else(|| {
                    Some(Launch {
                        argv: vec![command.into()],
                        cwd: None,
                        terminal: false,
                    })
                }),
                pinned: true,
            });
        }
        let templates = items;
        let mut items = Vec::new();
        for key in &self.pins.ids {
            if let Some(item) = templates.iter().find(|i| &i.key == key) {
                items.push(item.clone());
                continue;
            }
            let entry = self.catalog.find(std::slice::from_ref(key));
            let icon = entry
                .and_then(|d| self.icons.get(&d.icon))
                .or_else(|| self.icons.get("application-x-executable"));
            items.push(Item {
                key: key.clone(),
                name: entry.map(|d| d.name.clone()).unwrap_or_else(|| key.clone()),
                icon,
                windows: vec![],
                launcher: entry.and_then(|d| d.launcher.clone()),
                pinned: true,
            });
        }
        // Explicit pins take priority over generic shortcut aliases when matching windows.
        let mut explicit = HashMap::new();
        for key in &self.pins.ids {
            if let Some(entry) = self.catalog.find(std::slice::from_ref(key)) {
                for alias in &entry.aliases {
                    explicit.entry(alias.clone()).or_insert_with(|| key.clone());
                }
            }
        }
        pin_alias.extend(explicit);
        let pins_len = items.len();
        self.active = words(c, root, self.atoms.active)
            .first()
            .copied()
            .unwrap_or(0);
        let clients = words(c, root, self.atoms.clients);
        let live: HashSet<_> = clients.iter().copied().collect();
        self.watching.retain(|w| live.contains(w));
        self.titles.retain(|w, _| live.contains(w));
        for w in clients {
            if w == own {
                continue;
            }
            if self.settings_window == Some(w) {
                // Keep our settings event mask intact. Only this explicitly registered
                // managed window may appear; overlay windows remain excluded by PID.
                self.titles.insert(w, "Prychal Dock Settings".into());
                items.push(Item {
                    key: "internal:dock-settings".into(),
                    name: "Prychal Dock Settings".into(),
                    icon: self
                        .icons
                        .get("prychal-dock")
                        .or_else(|| self.icons.get("preferences-system"))
                        .or_else(|| self.icons.get("preferences-desktop"))
                        .or_else(|| self.icons.get("application-x-executable")),
                    windows: vec![w],
                    pinned: false,
                    launcher: None,
                });
                continue;
            }
            // Our managed settings window owns its mouse/key/expose event mask.
            if words(c, w, self.atoms.pid).first() == Some(&std::process::id()) {
                continue;
            }
            if self.watching.insert(w) {
                if let Ok(cookie) = c.change_window_attributes(
                    w,
                    &ChangeWindowAttributesAux::new()
                        .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
                ) {
                    let _ = cookie.check();
                }
            }
            let types = words(c, w, self.atoms.types);
            if !types.is_empty()
                && !types.contains(&self.atoms.normal)
                && !types.contains(&self.atoms.dialog)
            {
                continue;
            }
            let window_state = words(c, w, self.atoms.state);
            if window_state.contains(&self.atoms.skip) {
                continue;
            }
            let title = property(c, w, self.atoms.name, 4096)
                .filter(|r| r.format == 8 && !r.value.is_empty())
                .map(|r| {
                    String::from_utf8_lossy(&r.value)
                        .trim_end_matches('\0')
                        .to_owned()
                })
                .or_else(|| {
                    property(c, w, AtomEnum::WM_NAME.into(), 4096)
                        .filter(|r| r.format == 8 && !r.value.is_empty())
                        .map(|r| {
                            String::from_utf8(r.value.clone())
                                .unwrap_or_else(|_| {
                                    r.value.iter().map(|&b| char::from(b)).collect()
                                })
                                .trim_end_matches('\0')
                                .to_owned()
                        })
                })
                .unwrap_or_default();
            self.titles.insert(w, title);
            let mut keys = strings(c, w, self.atoms.gtk);
            keys.extend(strings(c, w, self.atoms.kde).into_iter().map(|s| {
                normalize(
                    Path::new(&s)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .as_ref(),
                )
            }));
            let classes = strings(c, w, self.atoms.class);
            keys.extend(classes.iter().rev().cloned());
            let entry = self.catalog.find(&keys);
            let key = keys
                .iter()
                .find_map(|k| pin_alias.get(k).cloned())
                .or_else(|| entry.map(|d| d.id.clone()))
                .unwrap_or_else(|| {
                    keys.first()
                        .cloned()
                        .unwrap_or_else(|| format!("window-{w}"))
                });
            if w != self.active && (window_state.contains(&self.atoms.demands_attention)
                || words(c, w, AtomEnum::WM_HINTS.into()).first().is_some_and(|flags| flags & (1 << 8) != 0)) {
                self.attention.insert(key.clone());
            }
            if let Some(item) = items.iter_mut().find(|i| i.key == key) {
                item.windows.push(w);
                if item.icon.is_none() {
                    item.icon = window_icon(&words(c, w, self.atoms.icon));
                }
                continue;
            }
            let icon = entry
                .and_then(|d| self.icons.get(&d.icon))
                .or_else(|| keys.iter().find_map(|s| self.icons.get(s)))
                .or_else(|| window_icon(&words(c, w, self.atoms.icon)))
                .or_else(|| self.icons.get("application-x-executable"));
            items.push(Item {
                key: key.clone(),
                icon,
                windows: vec![w],
                name: entry.map(|d| d.name.clone()).unwrap_or_else(|| key.clone()),
                launcher: entry.and_then(|d| d.launcher.clone()).or_else(|| {
                    templates
                        .iter()
                        .find(|i| i.key == key)
                        .and_then(|i| i.launcher.clone())
                }),
                pinned: false,
            });
        }
        // Keep surviving running applications in the same order on focus changes.
        items[pins_len..].sort_by_key(|i| {
            old.iter()
                .position(|key| key == &i.key)
                .unwrap_or(usize::MAX)
        });
        self.notification_ids.clear();
        for item in &items {
            let mut ids = vec![item.key.clone()];
            if let Some(entry) = self.catalog.find(std::slice::from_ref(&item.key)) {
                ids.push(entry.id.clone());
                ids.extend(self.catalog.entries.iter().filter(|candidate| candidate.fingerprint == entry.fingerprint).map(|candidate| candidate.id.clone()));
            }
            ids.sort(); ids.dedup();
            self.notification_ids.insert(item.key.clone(), ids);
        }
        self.items = items;
    }
    pub fn import_files(&mut self, paths: &[PathBuf]) -> Result<usize> {
        if paths.is_empty() || paths.len() > 32 {
            return Err("Drop 1 to 32 desktop launchers".into());
        }
        let directory = imports_dir().ok_or("HOME/XDG_DATA_HOME is missing")?;
        let mut keys = Vec::new();
        let mut copies: Vec<(PathBuf, Vec<u8>)> = Vec::new();
        // Validate the entire batch before writing files or changing pins.
        for path in paths {
            let path = fs::canonicalize(path)?;
            if path.extension().is_none_or(|e| e != "desktop") || !fs::metadata(&path)?.is_file() {
                return Err(format!("Not a local .desktop file: {}", path.display()).into());
            }
            let file = fs::File::open(&path)?;
            if !file.metadata()?.is_file() {
                return Err("Desktop launcher is not a regular file".into());
            }
            let mut bytes = Vec::new();
            file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 1024 * 1024 {
                return Err(format!("Desktop launcher exceeds 1 MiB: {}", path.display()).into());
            }
            let text = std::str::from_utf8(&bytes)?;
            let id = path
                .file_name()
                .ok_or("Invalid launcher filename")?
                .to_string_lossy();
            let entry = parse_at(&id, text, &path)
                .filter(|d| d.launcher.is_some())
                .ok_or_else(|| {
                    format!(
                        "No supported Application/Exec launcher in {}",
                        path.display()
                    )
                })?;
            let existing = self.catalog.entries.iter().find(|d| {
                d.path == path
                    || d.fingerprint == entry.fingerprint
                    || (d.launcher == entry.launcher && d.icon == entry.icon)
            });
            let key = if let Some(existing) = existing {
                existing.id.clone()
            } else {
                let key = format!("import-{}", entry.fingerprint);
                let destination = directory.join(format!("{key}.desktop"));
                if !copies.iter().any(|(p, _)| p == &destination) {
                    copies.push((destination, bytes));
                }
                key
            };
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        let mut created = Vec::new();
        let write_result = (|| -> Result<usize> {
            if !copies.is_empty() {
                fs::create_dir_all(&directory)?;
            }
            for (path, bytes) in &copies {
                if path.exists() {
                    if fs::read(path)? != *bytes {
                        return Err(
                            "Imported desktop file collision; existing file preserved".into()
                        );
                    }
                    continue;
                }
                let temp = directory.join(format!(
                    ".drop-{}-{}.tmp",
                    std::process::id(),
                    fingerprint(bytes)
                ));
                let result = (|| -> Result<()> {
                    let mut f = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o600)
                        .open(&temp)?;
                    f.write_all(bytes)?;
                    f.sync_all()?;
                    fs::rename(&temp, path)?;
                    Ok(())
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&temp);
                }
                result?;
                created.push(path.clone());
            }
            self.pins.add(&keys)
        })();
        if write_result.is_err() {
            for path in created {
                let _ = fs::remove_file(path);
            }
        }
        let added = write_result?;
        self.catalog = Catalog::load();
        Ok(added)
    }
    pub fn available_applications(&mut self) -> Vec<crate::applications::Application> {
        self.catalog = Catalog::load();
        let imports = imports_dir();
        let mut result: Vec<_> = self.catalog.entries.iter()
            .filter(|d| d.menu_visible && d.launcher.is_some() && d.id != "linux-scripts-dock-rust" && d.id != "prychal-dock" && !imports.as_ref().is_some_and(|path| d.path.starts_with(path)))
            .map(|d| crate::applications::Application {
                key: d.id.clone(), name: localized_name(&d.path).unwrap_or_else(|| d.name.clone()), pinned: self.application_is_pinned(d),
            }).collect();
        result.sort_by(|a,b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.key.cmp(&b.key)));
        result
    }
    fn application_is_pinned(&self, entry: &Desktop) -> bool {
        self.pins.ids.iter().any(|key| key == &entry.id || self.catalog.find(std::slice::from_ref(key)).is_some_and(|pin| pin.fingerprint == entry.fingerprint))
    }
    pub fn pin_application(&mut self, key: &str) -> Result<bool> {
        self.catalog = Catalog::load();
        let entry = self.catalog.entries.iter().find(|d| d.id == key && d.menu_visible && d.launcher.is_some()).ok_or("Application is no longer available; reopen the picker")?;
        if self.application_is_pinned(entry) { return Ok(false); }
        Ok(self.pins.add(&[key.to_string()])? > 0)
    }
    pub fn unpin_application(&mut self, key: &str) -> Result<bool> {
        self.catalog = Catalog::load();
        let fingerprint = self.catalog.entries.iter().find(|entry| entry.id == key).map(|entry| entry.fingerprint.clone());
        let keys: Vec<_> = self.pins.ids.iter().filter(|id| {
            id.as_str() == key || fingerprint.as_ref().is_some_and(|fingerprint| {
                self.catalog.find(std::slice::from_ref(*id)).is_some_and(|entry| &entry.fingerprint == fingerprint)
            })
        }).cloned().collect();
        self.pins.remove(&keys)
    }
    pub fn pinned_keys(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|i| i.pinned)
            .map(|i| i.key.clone())
            .collect()
    }
    pub fn preview_pin_order(&mut self, order: &[String]) {
        let n = self.items.iter().take_while(|i| i.pinned).count();
        self.items[..n].sort_by_key(|i| {
            order
                .iter()
                .position(|key| key == &i.key)
                .unwrap_or(usize::MAX)
        });
    }
    pub fn save_pin_order(&mut self, order: Vec<String>) -> Result<()> {
        self.pins.reorder(order)
    }
    pub fn toggle_pin(&mut self, key: &str) -> Result<()> {
        let item = self
            .items
            .iter()
            .find(|i| i.key == key)
            .ok_or("Application no longer exists")?;
        if !item.pinned && item.launcher.is_none() {
            return Err("No desktop launcher found for this application".into());
        }
        self.pins.toggle(key)
    }
    pub fn window_entries(&self, key: &str) -> Vec<WindowInfo> {
        self.items
            .iter()
            .find(|i| i.key == key)
            .map(|item| {
                item.windows
                    .iter()
                    .map(|&id| WindowInfo {
                        id,
                        title: self
                            .titles
                            .get(&id)
                            .filter(|s| !s.is_empty())
                            .cloned()
                            .unwrap_or_else(|| item.name.clone()),
                        active: id == self.active,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn activate_window<C: Connection>(
        &self,
        c: &C,
        root: u32,
        key: &str,
        window: u32,
        time: u32,
    ) -> Result<()> {
        if !self
            .items
            .iter()
            .any(|i| i.key == key && i.windows.contains(&window))
        {
            return Ok(());
        }
        if let Some(&desktop) = words(c, window, self.atoms.desktop)
            .first()
            .filter(|&&d| d != u32::MAX)
        {
            c.send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                ClientMessageEvent::new(32, root, self.atoms.current, [desktop, time, 0, 0, 0]),
            )?
            .check()?;
        }
        c.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ClientMessageEvent::new(
                32,
                window,
                self.atoms.activate,
                [2, time, self.active, 0, 0],
            ),
        )?
        .check()?;
        c.flush()?;
        Ok(())
    }
    pub fn close_window<C: Connection>(
        &self,
        c: &C,
        root: u32,
        key: &str,
        window: u32,
        time: u32,
    ) -> Result<()> {
        if !self
            .items
            .iter()
            .any(|i| i.key == key && i.windows.contains(&window))
        {
            return Ok(());
        }
        c.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ClientMessageEvent::new(32, window, self.atoms.close, [time, 2, 0, 0, 0]),
        )?
        .check()?;
        c.flush()?;
        Ok(())
    }
    pub fn close_group<C: Connection>(&self, c: &C, root: u32, key: &str, time: u32) -> Result<()> {
        if let Some(item) = self.items.iter().find(|i| i.key == key) {
            for &window in &item.windows {
                self.close_window(c, root, key, window, time)?;
            }
        }
        Ok(())
    }
    pub fn click<C: Connection>(
        &self,
        c: &C,
        root: u32,
        index: usize,
        time: u32,
    ) -> Result<Option<Launch>> {
        let Some(item) = self.items.get(index) else {
            return Ok(None);
        };
        if item.windows.is_empty() {
            return Ok(item.launcher.clone());
        }
        let active = item.windows.iter().copied().find(|&w| w == self.active);
        if let Some(w) =
            active.filter(|&w| !words(c, w, self.atoms.state).contains(&self.atoms.hidden))
        {
            c.send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                ClientMessageEvent::new(32, w, self.atoms.minimize, [3, 0, 0, 0, 0]),
            )?
            .check()?;
        } else {
            let w = active.unwrap_or(item.windows[0]);
            self.activate_window(c, root, &item.key, w, time)?;
        }
        c.flush()?;
        Ok(None)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_launch_arguments() {
        let args = expand_exec(
            r#"/opt/editor --title "%c" %i %U %% "$HOME""#,
            "Editor Name",
            "editor-icon",
            Path::new("/apps/editor.desktop"),
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "/opt/editor",
                "--title",
                "Editor Name",
                "--icon",
                "editor-icon",
                "%",
                "$HOME"
            ]
        );
        assert!(expand_exec("app --file=%f", "App", "", Path::new("app.desktop")).is_none());
        assert!(expand_exec("app %unknown", "App", "", Path::new("app.desktop")).is_none());
    }
    #[test]
    fn desktop_groups() {
        let d=parse("org.example.Editor.desktop","[Desktop Entry]\nType=Application\nIcon=editor\nStartupWMClass=EditorWindow\nExec=/usr/bin/editor %U\n[Desktop Action New]\nIcon=wrong").unwrap();
        assert_eq!(d.icon, "editor");
        assert!(d.aliases.contains(&"editorwindow".into()));
        assert!(d.aliases.contains(&"editor".into()));
        assert!(parse("x", "[Desktop Entry]\nType=Application\nHidden=true").is_none());
    }
}
