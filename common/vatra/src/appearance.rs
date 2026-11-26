use std::{env, fs, io, path::PathBuf};

pub const ACCENTS: [([u8; 3], &str); 4] = [
    ([255, 177, 107], "Ember"),
    ([130, 194, 255], "Blue"),
    ([112, 222, 181], "Mint"),
    ([205, 170, 255], "Violet"),
];

pub const PERCENT_OPTIONS: [&str; 3] = ["Tooltip only", "Inside battery", "Beside battery"];
pub const BOLT_OPTIONS: [&str; 3] = ["Theme color", "White", "Hidden"];

#[derive(Clone, Copy)]
pub struct Appearance {
    accent: [u8; 3],
    pub percent: usize,
    pub bolt: usize,
}

impl Default for Appearance {
    fn default() -> Self {
        Self { accent: ACCENTS[0].0, percent: 0, bolt: 0 }
    }
}

impl Appearance {
    pub fn load() -> Self {
        let Some(contents) = path().and_then(|path| fs::read_to_string(path).ok()) else {
            return Self::default();
        };
        let mut style = Self::default();
        if let Some(preset) = contents.trim().parse::<usize>().ok().and_then(|index| ACCENTS.get(index)) {
            style.accent = preset.0;
            return style;
        }
        for (key, value) in contents.lines().filter_map(|line| line.split_once('=')) {
            match key {
                "accent" if value.len() == 6 => {
                    if let Ok(rgb) = u32::from_str_radix(value, 16) {
                        style.accent = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
                    }
                }
                "percent" => {
                    if let Ok(index) = value.parse::<usize>() {
                        if index < PERCENT_OPTIONS.len() { style.percent = index; }
                    }
                }
                "bolt" => {
                    if let Ok(index) = value.parse::<usize>() {
                        if index < BOLT_OPTIONS.len() { style.bolt = index; }
                    }
                }
                _ => {}
            }
        }
        style
    }

    pub fn accent(self) -> [u8; 3] { self.accent }

    pub fn hex(self) -> String {
        let [r, g, b] = self.accent;
        format!("{r:02x}{g:02x}{b:02x}")
    }

    pub fn with_accent(mut self, color: [u8; 3]) -> Self {
        self.accent = color;
        self
    }

    pub fn save(self) -> io::Result<()> {
        let file = path().ok_or(io::ErrorKind::NotFound)?;
        let directory = file.parent().ok_or(io::ErrorKind::InvalidInput)?;
        fs::create_dir_all(directory)?;
        let temporary = file.with_extension(format!("tmp-{}", std::process::id()));
        let contents = format!("accent={}\npercent={}\nbolt={}\n", self.hex(), self.percent, self.bolt);
        let result = fs::write(&temporary, contents).and_then(|_| fs::rename(&temporary, &file));
        if result.is_err() { let _ = fs::remove_file(&temporary); }
        result
    }
}

fn path() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|directory| directory.join("vatra/appearance"))
}
