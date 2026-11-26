use std::{collections::BTreeSet, fs, path::Path, process::Command};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Automatic,
    Performance,
    Balanced,
    PowerSaver,
    Custom,
}

impl Profile {
    pub const ALL: [Self; 5] = [Self::Automatic, Self::Performance, Self::Balanced, Self::PowerSaver, Self::Custom];

    pub fn name(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic",
            Self::Performance => "Performance",
            Self::Balanced => "Balanced",
            Self::PowerSaver => "Power Saver",
            Self::Custom => "Custom",
        }
    }

    pub const QUICK: [Self; 4] = [Self::Automatic, Self::Balanced, Self::Performance, Self::PowerSaver];

    pub fn quick_epp(self, options: &BTreeSet<String>) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Automatic => Some(("balance_performance", "balance_power")),
            Self::PowerSaver if !options.contains("power") => Some(("balance_power", "balance_power")),
            Self::Custom => None,
            _ => self.epp().map(|value| (value, value)),
        }
    }

    pub fn editable(self) -> bool {
        !matches!(self, Self::Automatic)
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Performance => "performance",
            Self::Balanced => "balanced",
            Self::PowerSaver => "power-saver",
            Self::Custom => "custom",
        }
    }

    pub fn epp(self) -> Option<&'static str> {
        match self {
            Self::Automatic | Self::Custom => None,
            Self::Performance => Some("performance"),
            Self::Balanced => Some("balance_performance"),
            Self::PowerSaver => Some("power"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerSource {
    Ac,
    Battery,
}

impl PowerSource {
    pub const ALL: [Self; 2] = [Self::Ac, Self::Battery];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ac => "AC",
            Self::Battery => "BAT",
        }
    }
}

pub fn shared_epp(policies: &[std::path::PathBuf]) -> BTreeSet<String> {
    let mut values = policies.iter().map(|policy| {
        fs::read_to_string(policy.join("energy_performance_available_preferences"))
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    });
    let Some(first) = values.next() else { return BTreeSet::new() };
    values.fold(first, |common, available| common.intersection(&available).cloned().collect())
}

struct AppliedProfile {
    mode: Profile,
    epp: Option<String>,
    maximum_khz: Option<u64>,
}

fn applied_profile(override_file: &str, source: PowerSource) -> Option<AppliedProfile> {
    let prefix = format!("# Vatra profile {}=", source.label());
    let record = override_file.lines().find_map(|line| line.strip_prefix(&prefix))?;
    let (mode, values) = record.split_once(',')?;
    let (epp, maximum) = values.split_once(',')?;
    let mode = Profile::ALL.into_iter().find(|profile| profile.id() == mode)?;
    let epp = match epp {
        "-" => None,
        value if crate::presets::EPP_OPTIONS.iter().any(|(option, _)| *option == value) =>
            Some(value.to_owned()),
        _ => return None,
    };
    let maximum_khz = if maximum == "-" { None } else { Some(maximum.parse().ok()?) };
    Some(AppliedProfile { mode, epp, maximum_khz })
}

pub struct Diagnostics {
    pub tlp: String,
    pub managers: String,
    pub effective_cpu: [String; 2],
    ac_bat_backend: bool,
    pub source: Option<PowerSource>,
    tlp_enabled: bool,
    managers_clear: bool,
    config: Option<String>,
    applied: Option<AppliedProfile>,
}

const UNITS: [(&str, &str); 4] = [
    ("power-profiles-daemon.service", "power-profiles-daemon"),
    ("tuned.service", "tuned"),
    ("tuned-ppd.service", "tuned-ppd"),
    ("auto-cpufreq.service", "auto-cpufreq"),
];

fn unit_state(unit: &str) -> Option<String> {
    // systemctl show can return a failure status for absent units while still
    // printing LoadState=not-found. Unknown replies remain indeterminate.
    let output = Command::new("systemctl")
        .args(["--no-pager", "show", "--property=LoadState", "--property=ActiveState", unit])
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    let output = String::from_utf8(output.stdout).ok()?;
    if output.lines().any(|line| line == "LoadState=not-found") {
        return Some("not installed as a service".into());
    }
    let state = output.lines().find_map(|line| line.strip_prefix("ActiveState="))?;
    matches!(state, "active" | "inactive" | "failed" | "activating" | "deactivating" | "reloading")
        .then(|| state.to_owned())
}

fn tlp_stat(flag: &str) -> Option<String> {
    let executable = ["/usr/sbin/tlp-stat", "/usr/bin/tlp-stat"]
        .into_iter()
        .find(|path| Path::new(path).is_file())?;
    let result = Command::new(executable)
        .arg(flag)
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    if !result.status.success() {
        return None;
    }
    String::from_utf8(result.stdout).ok()
}

fn status_field<'a>(status: &'a str, name: &str) -> Option<&'a str> {
    status.split_once("+++ TLP Status")?.1.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == name).then(|| value.trim())
    })
}

fn matching_source(status: &str) -> Option<PowerSource> {
    match (status_field(status, "Mode")?, status_field(status, "Power source")?) {
        ("AC" | "ac", "AC" | "ac") => Some(PowerSource::Ac),
        ("battery" | "BAT", "battery" | "BAT") => Some(PowerSource::Battery),
        _ => None,
    }
}

fn configured_value<'a>(config: &'a str, key: &str) -> Option<(&'a str, &'a str)> {
    for line in config.lines() {
        let Some((location, assignment)) = line.split_once(": ") else { continue };
        let Some((name, raw)) = assignment.split_once('=') else { continue };
        if name != key {
            continue;
        }
        let path = location.split_whitespace().next()?;
        let file = Path::new(path).file_name()?.to_str()?;
        let value = raw.strip_prefix('"').and_then(|raw| raw.strip_suffix('"')).unwrap_or(raw);
        if !value.is_empty() {
            return Some((value, file));
        }
    }
    None
}

fn effective_cpu(config: Option<&str>, source: &str) -> String {
    let Some(config) = config else {
        return format!("{source}: TLP configuration unavailable");
    };
    let epp = ["CPU_ENERGY_PERF_POLICY_ON_", source].concat();
    let maximum = ["CPU_SCALING_MAX_FREQ_ON_", source].concat();
    let epp = configured_value(config, &epp)
        .map(|(value, file)| format!("{value} [{file}]"))
        .unwrap_or_else(|| "no explicit value".into());
    let maximum = configured_value(config, &maximum)
        .and_then(|(value, file)| value.parse::<u64>().ok().map(|khz| (khz, file)))
        .map(|(khz, file)| format!("{:.0} MHz [{file}]", khz as f64 / 1000.0))
        .unwrap_or_else(|| "no numeric value reported".into());
    format!("{source}: EPP {epp} / Max {maximum}")
}

impl Diagnostics {
    pub fn read() -> Self {
        let status = tlp_stat("-s").unwrap_or_default();
        let version = status.lines().find_map(|line| {
            line.strip_prefix("--- TLP ")?.split_whitespace().next()
        });
        let ac_bat_backend = version.is_some_and(|v| v.starts_with("1.8."));
        let source = matching_source(&status);
        let tlp_enabled = status_field(&status, "State") == Some("enabled");
        let tlp = if let Some(version) = version {
            let state = status_field(&status, "State").unwrap_or("state unavailable");
            format!("TLP {version} / {state} / {}", source.map(PowerSource::label).unwrap_or("AC/BAT mode mismatch or unavailable"))
        } else {
            "TLP status unavailable".into()
        };
        let mut detected = Vec::new();
        let mut unknown = false;
        for (unit, name) in UNITS {
            match unit_state(unit).as_deref() {
                Some("inactive" | "failed" | "not installed as a service") => {}
                Some(_) => detected.push(name),
                None => unknown = true,
            }
        }
        let managers = match (detected.is_empty(), unknown) {
            (true, false) => "No active competing systemd units detected".into(),
            (true, true) => "Competing manager status unavailable".into(),
            (false, false) => format!("Potential overlap: {}", detected.join(", ")),
            (false, true) => format!("Potential overlap: {} / partial check", detected.join(", ")),
        };
        let config = tlp_stat("-c").filter(|text| text.contains("+++ Configured Settings"));
        let effective_cpu = ["AC", "BAT"].map(|source| effective_cpu(config.as_deref(), source));
        let managers_clear = detected.is_empty() && !unknown;
        let applied = source.and_then(|source| {
            let overrides = fs::read_to_string("/etc/tlp.d/99-vatra-epp.conf").ok()?;
            applied_profile(&overrides, source)
        });
        Self { tlp, managers, effective_cpu, ac_bat_backend, source, tlp_enabled, managers_clear, config, applied }
    }

    pub fn active_mode(
        &self, snapshot: &crate::telemetry::Snapshot, presets: &crate::presets::Presets,
    ) -> Option<Profile> {
        let source = self.source?;
        if !self.tlp_enabled || snapshot.external_power.is_some_and(|power| power != source) {
            return None;
        }
        let config = self.config.as_deref()?;
        if let Some(applied) = &self.applied {
            let configured_epp = configured_value(config, &format!("CPU_ENERGY_PERF_POLICY_ON_{}", source.label()));
            let configured_max = configured_value(config, &format!("CPU_SCALING_MAX_FREQ_ON_{}", source.label()));
            let epp_matches = applied.epp.as_deref().is_none_or(|epp| {
                snapshot.epp == epp && configured_epp.is_none_or(|(value, _)| value == epp)
            });
            let cap_matches = applied.maximum_khz.is_none_or(|cap| {
                snapshot.maximum_khz == Some(cap)
                    && configured_max.is_none_or(|(value, _)| value.parse::<u64>().ok() == Some(cap))
            });
            return (epp_matches && cap_matches).then_some(applied.mode);
        }

        // Older overrides have no recorded profile. Prefer an exact match,
        // but keep an unambiguous match visible while its draft is being edited.
        let limit = |source: PowerSource| {
            let key = format!("CPU_SCALING_MAX_FREQ_ON_{}", source.label());
            configured_value(config, &key)
                .filter(|(_, file)| *file == "99-vatra-epp.conf")
                .and_then(|(value, _)| value.parse::<u64>().ok())
        };
        let current_limit = limit(source)?;
        if snapshot.maximum_khz != Some(current_limit) || PowerSource::ALL.into_iter().any(|power| limit(power).is_none()) {
            return None;
        }
        let matches = |mode| {
            let [Some(ac), Some(battery)] = PowerSource::ALL.map(|power| presets.requested_epp(mode, power, snapshot))
                else { return false };
            let current = if source == PowerSource::Ac { ac } else { battery };
            snapshot.epp == current
                && (ac == battery || !configured_value(config, "TLP_PERSISTENT_DEFAULT").is_some_and(|(value, _)| value == "1"))
                && configured_value(config, "CPU_ENERGY_PERF_POLICY_ON_AC").is_some_and(|(value, _)| value == ac)
                && configured_value(config, "CPU_ENERGY_PERF_POLICY_ON_BAT").is_some_and(|(value, _)| value == battery)
        };
        let mut candidates = Profile::QUICK.into_iter().filter(|mode| matches(*mode));
        let first = candidates.next()?;
        let Some(second) = candidates.next() else { return Some(first) };
        [first, second].into_iter().chain(candidates).find(|mode| PowerSource::ALL.into_iter().all(|power| {
            presets.cap(*mode, power).is_none_or(|cap| limit(power) == Some(u64::from(cap)))
        }))
    }

    pub fn quick_target(
        &self, mode: Profile, snapshot: &crate::telemetry::Snapshot, presets: &crate::presets::Presets,
    ) -> Result<[&'static str; 4], &'static str> {
        let source = self.source.filter(|_| self.ac_bat_backend && self.tlp_enabled)
            .ok_or("TLP 1.8 could not confirm the active power source.")?;
        if !self.managers_clear || self.config.is_none() {
            return Err("TLP configuration or competing power managers could not be verified.");
        }
        let [Some(ac), Some(battery)] = PowerSource::ALL.map(|source| presets.requested_epp(mode, source, snapshot))
            else { return Err("Unknown mode.") };
        if ![ac, battery].iter().all(|value| snapshot.epp_options.contains(*value)) {
            return Err("This mode is unavailable on one or more CPU policies.");
        }
        Ok([source.label(), ac, battery, mode.id()])
    }

    pub fn apply_target(
        &self,
        requested: Option<&'static str>,
        preview_source: Option<PowerSource>,
        snapshot: &crate::telemetry::Snapshot,
        changes_cap: bool,
    ) -> Option<(&'static str, &'static str)> {
        let requested = requested?;
        let source = self.source?;
        if preview_source != Some(source) {
            return None;
        }
        let source = source.label();
        let config = self.config.as_deref()?;
        let key = format!("CPU_ENERGY_PERF_POLICY_ON_{source}");
        let configured = configured_value(config, &key).map(|(value, _)| value);
        (self.ac_bat_backend && self.tlp_enabled && self.managers_clear
            && (snapshot.epp_options.contains(requested) || (changes_cap && requested == "keep"))
            && (changes_cap || configured != Some(requested) || snapshot.epp != requested))
            .then_some((source, requested))
    }

    pub fn preview_source(&self, selection: Option<PowerSource>) -> Option<PowerSource> {
        selection.or(self.source)
    }

    pub fn configured_cpu(&self, source: PowerSource) -> [String; 2] {
        let Some(config) = self.config.as_deref() else {
            return ["Unavailable".into(), "Unavailable".into()];
        };
        let label = source.label();
        let epp_key = format!("CPU_ENERGY_PERF_POLICY_ON_{label}");
        let max_key = format!("CPU_SCALING_MAX_FREQ_ON_{label}");
        let epp = configured_value(config, &epp_key).map_or("Unspecified", |(value, _)| value);
        let max = configured_value(config, &max_key)
            .and_then(|(value, _)| value.parse::<u64>().ok())
            .map_or_else(|| "System default".into(), |khz| format!("{:.0} MHz", khz as f64 / 1000.0));
        [epp.into(), max]
    }

    pub fn preflight(
        &self, requested: &str, preview_source: Option<PowerSource>,
        snapshot: &crate::telemetry::Snapshot,
    ) -> String {
        if let Some(source) = preview_source.filter(|source| Some(*source) != self.source) {
            let key = format!("CPU_ENERGY_PERF_POLICY_ON_{}", source.label());
            let configured = self.config.as_deref().and_then(|config| configured_value(config, &key))
                .map_or("Unavailable", |(value, _)| value);
            return format!("{} preview: {configured} → {requested} (inactive; preview only)", source.label());
        }
        let Some(source) = self.source.filter(|_| self.ac_bat_backend && self.tlp_enabled) else {
            return "Blocked: TLP 1.8 state and matching AC/BAT source required".into();
        };
        if !self.managers_clear {
            return "Blocked: competing manager status is active or unverified".into();
        }
        if !snapshot.epp_options.contains(requested) {
            return "Blocked: requested EPP is unavailable on one or more CPU policies".into();
        }
        let source = source.label();
        let key = format!("CPU_ENERGY_PERF_POLICY_ON_{source}");
        let Some(config) = self.config.as_deref() else {
            return "Blocked: effective TLP configuration is unavailable".into();
        };
        let configured = configured_value(config, &key).map(|(value, _)| value);
        if configured == Some(requested) && snapshot.epp == requested {
            return format!("{source}: {requested} is already configured and active");
        }
        if configured == Some(requested) {
            return format!("{source}: {requested} is configured, but live EPP differs");
        }
        format!("{source} EPP preview: {} -> {requested}", configured.unwrap_or("not explicitly configured"))
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_values_keep_profile_and_source_file() {
        let config = concat!(
            "/etc/tlp.d/10-acer-power.conf L0009: CPU_ENERGY_PERF_POLICY_ON_BAT=\"balance_power\"\n",
            "/etc/tlp.d/90-tlp-panel.conf L0008: CPU_SCALING_MAX_FREQ_ON_BAT=\"4373858\"\n",
        );
        assert_eq!(
            effective_cpu(Some(config), "BAT"),
            "BAT: EPP balance_power [10-acer-power.conf] / Max 4374 MHz [90-tlp-panel.conf]",
        );
        assert_eq!(effective_cpu(None, "AC"), "AC: TLP configuration unavailable");
    }

    #[test]
    fn tlp_mode_must_match_power_source() {
        let battery = "+++ TLP Status\nState = enabled\nMode = battery\nPower source = battery\n";
        assert!(matching_source(battery) == Some(PowerSource::Battery));
        let plugged_in = battery.replace("Power source = battery", "Power source = AC");
        assert_eq!(matching_source(&plugged_in), None);
        assert_eq!(matching_source("Power source = battery"), None);
    }

    #[test]
    fn inactive_source_is_preview_only() {
        let diagnostics = Diagnostics {
            tlp: String::new(),
            managers: String::new(),
            effective_cpu: [String::new(), String::new()],
            ac_bat_backend: true,
            source: Some(PowerSource::Battery),
            tlp_enabled: true,
            managers_clear: true,
            config: Some("/etc/tlp.conf L0001: CPU_ENERGY_PERF_POLICY_ON_AC=balance_performance".into()),
            applied: None,
        };
        let mut snapshot = crate::telemetry::Snapshot::read(Path::new("/dev/null"));
        snapshot.epp_options.insert("balance_power".into());
        let source = diagnostics.preview_source(Some(PowerSource::Ac));
        assert!(diagnostics.apply_target(Some("balance_power"), source, &snapshot, false).is_none());
        assert_eq!(
            diagnostics.preflight("balance_power", source, &snapshot),
            "AC preview: balance_performance → balance_power (inactive; preview only)",
        );
        assert_eq!(
            diagnostics.apply_target(Some("balance_power"), diagnostics.preview_source(None), &snapshot, false),
            Some(("BAT", "balance_power")),
        );
    }

    #[test]
    fn applied_battery_mode_is_independent_of_draft_and_ac_settings() {
        let overrides = concat!(
            "# Vatra profile AC=performance,performance,4373858\n",
            "# Vatra profile BAT=power-saver,power,2300000\n",
        );
        let mut diagnostics = Diagnostics {
            tlp: String::new(),
            managers: String::new(),
            effective_cpu: [String::new(), String::new()],
            ac_bat_backend: true,
            source: Some(PowerSource::Battery),
            tlp_enabled: true,
            managers_clear: true,
            config: Some(concat!(
                "/etc/tlp.d/99-vatra-epp.conf L0004: CPU_ENERGY_PERF_POLICY_ON_BAT=power\n",
                "/etc/tlp.d/99-vatra-epp.conf L0005: CPU_SCALING_MAX_FREQ_ON_BAT=2300000\n",
            ).into()),
            applied: applied_profile(overrides, PowerSource::Battery),
        };
        assert_eq!(applied_profile(overrides, PowerSource::Ac).unwrap().mode, Profile::Performance);
        let presets = crate::presets::Presets::load();
        let mut snapshot = crate::telemetry::Snapshot::read(Path::new("/dev/null"));
        snapshot.epp = "power".into();
        snapshot.maximum_khz = Some(2_300_000);
        assert_eq!(diagnostics.active_mode(&snapshot, &presets), Some(Profile::PowerSaver));

        snapshot.maximum_khz = Some(2_300_001);
        assert_eq!(diagnostics.active_mode(&snapshot, &presets), None);
        snapshot.maximum_khz = Some(2_300_000);
        diagnostics.applied = applied_profile("# Vatra profile BAT=custom,power,2300000", PowerSource::Battery);
        assert_eq!(diagnostics.active_mode(&snapshot, &presets), Some(Profile::Custom));
    }

    #[test]
    fn quick_saver_prefers_power_with_a_balance_power_fallback() {
        assert_eq!(Profile::Automatic.epp(), None);
        assert_eq!(Profile::PowerSaver.epp(), Some("power"));
        let mut available = BTreeSet::from(["balance_power".to_owned()]);
        assert_eq!(Profile::PowerSaver.quick_epp(&available), Some(("balance_power", "balance_power")));
        available.insert("power".into());
        assert_eq!(Profile::PowerSaver.quick_epp(&available), Some(("power", "power")));
        assert_eq!(Profile::Automatic.quick_epp(&available), Some(("balance_performance", "balance_power")));
    }
}
