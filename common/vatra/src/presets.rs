use crate::profiles::{PowerSource, Profile};
use std::{
    collections::BTreeMap,
    env, fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const EPP_OPTIONS: [(&str, &str); 4] = [
    ("performance", "Performance"),
    ("balance_performance", "Balance performance"),
    ("balance_power", "Balance power"),
    ("power", "Power"),
];

pub struct Presets {
    path: Option<PathBuf>,
    overrides: BTreeMap<String, &'static str>,
    limits: BTreeMap<String, u32>,
}

fn setting_key(profile: Profile, source: PowerSource) -> String {
    format!("{}.{}", source.label(), profile.id())
}

impl Presets {
    pub fn load() -> Self {
        let path = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|path| path.join("vatra/profiles.conf"));
        let (overrides, limits) = path.as_deref().map(Self::read).unwrap_or_default();
        Self { path, overrides, limits }
    }

    fn read(path: &Path) -> (BTreeMap<String, &'static str>, BTreeMap<String, u32>) {
        let mut overrides = BTreeMap::new();
        let mut limits = BTreeMap::new();
        let Ok(content) = fs::read_to_string(path) else { return (overrides, limits) };
        let values: BTreeMap<_, _> = content.lines().filter_map(|line| line.split_once('=')).collect();
        for source in PowerSource::ALL {
            for profile in Profile::ALL.into_iter().filter(|profile| profile.editable()) {
                let key = setting_key(profile, source);
                if let Some((epp, _)) = EPP_OPTIONS.into_iter()
                    .find(|(epp, _)| values.get(key.as_str()) == Some(epp))
                {
                    overrides.insert(key.clone(), epp);
                }
                if let Some(cap) = values.get(format!("{key}.max_khz").as_str())
                    .and_then(|value| value.parse::<u32>().ok())
                    .filter(|cap| (1..100_000_000).contains(cap))
                {
                    limits.insert(key, cap);
                }
            }
        }
        (overrides, limits)
    }

    pub fn has_override(&self, profile: Profile, source: PowerSource) -> bool {
        self.overrides.contains_key(&setting_key(profile, source))
    }

    pub fn edit(
        &mut self, profile: Profile, source: PowerSource, value: Option<&'static str>,
    ) -> io::Result<()> {
        if !profile.editable() || value.is_some_and(|value| !EPP_OPTIONS.iter().any(|(allowed, _)| *allowed == value)) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Unsupported preset or EPP"));
        }
        let key = setting_key(profile, source);
        let mut overrides = self.overrides.clone();
        match value.filter(|value| Some(*value) != profile.epp()) {
            Some(value) => { overrides.insert(key, value); }
            None => { overrides.remove(&key); }
        }
        self.save(&overrides, &self.limits)?;
        self.overrides = overrides;
        Ok(())
    }

    pub fn cap(&self, profile: Profile, source: PowerSource) -> Option<u32> {
        self.limits.get(&setting_key(profile, source)).copied()
    }

    pub fn edit_cap(&mut self, profile: Profile, source: PowerSource, value: Option<u32>) -> io::Result<()> {
        if !profile.editable() || value.is_some_and(|cap| !(1..100_000_000).contains(&cap)) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Unsupported preset or CPU limit"));
        }
        let mut limits = self.limits.clone();
        let key = setting_key(profile, source);
        match value {
            Some(value) => { limits.insert(key, value); }
            None => { limits.remove(&key); }
        }
        self.save(&self.overrides, &limits)?;
        self.limits = limits;
        Ok(())
    }

    pub fn requested_epp(&self, profile: Profile, source: PowerSource, snapshot: &crate::telemetry::Snapshot) -> Option<&'static str> {
        self.overrides.get(&setting_key(profile, source)).copied().or_else(|| {
            profile.quick_epp(&snapshot.epp_options).map(|(ac, battery)| if source == PowerSource::Ac { ac } else { battery })
        })
    }

    fn save(&self, overrides: &BTreeMap<String, &'static str>, limits: &BTreeMap<String, u32>) -> io::Result<()> {
        let path = self.path.as_deref().ok_or_else(|| io::Error::new(
            io::ErrorKind::NotFound, "No user configuration directory",
        ))?;
        let directory = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
        fs::create_dir_all(directory)?;
        let text: String = overrides.iter().map(|(key, value)| format!("{key}={value}\n"))
            .chain(limits.iter().map(|(key, value)| format!("{key}.max_khz={value}\n"))).collect();
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let temporary = directory.join(format!(".profiles-{}-{stamp}", std::process::id()));
        use std::io::Write;
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        let result = file.write_all(text.as_bytes()).and_then(|_| fs::rename(&temporary, path));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[derive(Clone, Copy)]
pub struct CpuRange {
    pub minimum: u32,
    pub ceiling: u32,
    current: u32,
    defaults: [u32; 4],
}

impl CpuRange {
    pub fn default_cap(self, profile: Profile) -> u32 {
        match profile {
            Profile::Automatic => self.defaults[0],
            Profile::Balanced => self.defaults[1],
            Profile::Performance => self.defaults[2],
            Profile::PowerSaver => self.defaults[3],
            Profile::Custom => self.current.clamp(self.minimum.saturating_add(1), self.ceiling),
        }
    }
}

pub fn limit_label(khz: u64) -> String {
    let mhz = format!("{}.{:03}", khz / 1000, khz % 1000);
    format!("{} MHz", mhz.trim_end_matches('0').trim_end_matches('.'))
}

pub fn read_ranges(helper: &str, active: PowerSource) -> Result<[CpuRange; 2], String> {
    let output = std::process::Command::new(helper).args(["--limits", active.label()]).output()
        .map_err(|error| format!("Could not read CPU limits: {error}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(error.trim().to_owned());
    }
    let invalid = "Invalid CPU range data. Update the Vatra TLP helper.";
    let text = std::str::from_utf8(&output.stdout).map_err(|_| invalid)?;
    let mut ranges = [None; 2];
    for line in text.lines() {
        let mut fields = line.split('\t');
        let index = match fields.next() {
            Some("AC") => 0,
            Some("BAT") => 1,
            _ => return Err(invalid.into()),
        };
        let values = fields.map(str::parse::<u32>).collect::<Result<Vec<_>, _>>().map_err(|_| invalid)?;
        let [minimum, ceiling, current, automatic, balanced, performance, saver] = values.as_slice()
            else { return Err(invalid.into()) };
        let defaults = [*automatic, *balanced, *performance, *saver];
        if ranges[index].is_some() || minimum >= ceiling
            || defaults.iter().any(|cap| cap <= minimum || cap > ceiling)
        {
            return Err(invalid.into());
        }
        ranges[index] = Some(CpuRange { minimum: *minimum, ceiling: *ceiling, current: *current, defaults });
    }
    match ranges {
        [Some(ac), Some(battery)] => Ok([ac, battery]),
        _ => Err(invalid.into()),
    }
}

pub fn cpu_range(ranges: Option<&[CpuRange; 2]>, source: PowerSource) -> Option<CpuRange> {
    ranges.map(|ranges| ranges[if source == PowerSource::Ac { 0 } else { 1 }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_labels_keep_exact_kilohertz() {
        assert_eq!(limit_label(4_373_858), "4373.858 MHz");
        assert_eq!(limit_label(4_373_860), "4373.86 MHz");
        assert_eq!(limit_label(4_374_000), "4374 MHz");
    }

    #[test]
    fn editing_and_resetting_preserve_independent_settings() {
        let path = env::temp_dir().join(format!("vatra-presets-{}", std::process::id()));
        let mut presets = Presets { path: Some(path.clone()), overrides: BTreeMap::new(), limits: BTreeMap::new() };
        presets.edit(Profile::Balanced, PowerSource::Battery, Some("balance_power")).unwrap();
        presets.edit(Profile::Custom, PowerSource::Ac, Some("performance")).unwrap();
        presets.edit_cap(Profile::Balanced, PowerSource::Battery, Some(2_500_000)).unwrap();
        presets.edit_cap(Profile::Custom, PowerSource::Ac, Some(3_200_000)).unwrap();
        (presets.overrides, presets.limits) = Presets::read(&path);
        assert_eq!(presets.cap(Profile::Balanced, PowerSource::Battery), Some(2_500_000));
        assert_eq!(presets.cap(Profile::Balanced, PowerSource::Ac), None);
        assert_eq!(presets.cap(Profile::Custom, PowerSource::Ac), Some(3_200_000));
        assert!(!presets.has_override(Profile::Custom, PowerSource::Battery));
        presets.edit(Profile::Balanced, PowerSource::Battery, None).unwrap();
        presets.edit_cap(Profile::Custom, PowerSource::Ac, None).unwrap();
        (presets.overrides, presets.limits) = Presets::read(&path);
        fs::remove_file(path).unwrap();
        assert!(!presets.has_override(Profile::Balanced, PowerSource::Battery));
        assert!(presets.has_override(Profile::Custom, PowerSource::Ac));
        assert_eq!(presets.cap(Profile::Balanced, PowerSource::Battery), Some(2_500_000));
        assert_eq!(presets.cap(Profile::Custom, PowerSource::Ac), None);
    }
}
