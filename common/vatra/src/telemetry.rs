use crate::profiles::PowerSource;
use std::fs;
use std::path::{Path, PathBuf};

fn read(path: &Path, key: &str) -> Option<String> {
    let value = fs::read_to_string(path.join(key)).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn number(path: &Path, key: &str) -> Option<f64> {
    read(path, key)?.parse::<i64>().ok().map(|n| n as f64)
}

fn members(root: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    paths.sort();
    paths
}

pub struct Battery {
    pub name: String,
    pub state: Option<String>,
    pub percent: Option<f64>,
    pub power: Option<f64>,
    pub derived_power: bool,
    pub voltage: Option<f64>,
    pub current: Option<f64>,
    pub health: Option<f64>,
    pub full: Option<f64>,
    pub design: Option<f64>,
    pub capacity_unit: &'static str,
    pub cycles: Option<f64>,
    pub temperature: Option<f64>,
    pub hours_remaining: Option<f64>,
    pub charge_start: Option<f64>,
    pub charge_end: Option<f64>,
}

impl Battery {
    fn read(path: &Path) -> Self {
        let state = read(path, "status");
        let voltage = number(path, "voltage_now").filter(|n| *n > 0.0).map(|n| n / 1e6);
        let current = number(path, "current_now").map(|n| n.abs() / 1e6);
        let reported_power = number(path, "power_now").map(|n| n.abs() / 1e6);
        let power = reported_power.or_else(|| Some(voltage? * current?));
        let energy = number(path, "energy_full").zip(number(path, "energy_full_design"));
        let charge = number(path, "charge_full").zip(number(path, "charge_full_design"));
        let (capacity, capacity_unit) = if energy.is_some() { (energy, "Wh") } else { (charge, "Ah") };
        let hours_remaining = if state.as_deref() == Some("Discharging") {
            number(path, "energy_now")
                .zip(power.filter(|n| *n > 0.1))
                .map(|(energy, power)| energy / 1e6 / power)
                .or_else(|| {
                    number(path, "charge_now")
                        .zip(current.filter(|n| *n > 0.01))
                        .map(|(charge, current)| charge / 1e6 / current)
                })
                .filter(|n| *n >= 0.0)
        } else {
            None
        };

        Self {
            name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            state,
            percent: number(path, "capacity").filter(|n| (0.0..=100.0).contains(n)),
            power,
            derived_power: reported_power.is_none() && power.is_some(),
            voltage,
            current,
            health: capacity.filter(|(_, design)| *design > 0.0).map(|(full, design)| full / design * 100.0),
            full: capacity.map(|(full, _)| full / 1e6),
            design: capacity.map(|(_, design)| design / 1e6),
            capacity_unit,
            cycles: number(path, "cycle_count").filter(|n| *n >= 0.0),
            temperature: number(path, "temp").map(|n| n / 10.0),
            hours_remaining,
            charge_start: threshold(path, "charge_control_start_threshold"),
            charge_end: threshold(path, "charge_control_end_threshold"),
        }
    }

    pub fn signed_power(&self) -> Option<f64> {
        match self.state.as_deref()? {
            "Charging" => self.power,
            "Discharging" => self.power.map(|n| -n),
            _ => None,
        }
    }
}

fn threshold(path: &Path, key: &str) -> Option<f64> {
    number(path, key).filter(|value| (0.0..=100.0).contains(value))
}

fn external_power(supplies: &[PathBuf]) -> Option<PowerSource> {
    let mut offline = false;
    for supply in supplies {
        let scope = read(supply, "scope");
        if scope.as_deref() == Some("Device") {
            continue;
        }
        let Some(kind) = read(supply, "type") else { continue };
        let system_usb = kind.starts_with("USB") && scope.as_deref() == Some("System");
        if !matches!(kind.as_str(), "Mains" | "Wireless") && !system_usb {
            continue;
        }
        match read(supply, "online").as_deref() {
            Some("1") => return Some(PowerSource::Ac),
            Some("0") => offline = true,
            _ => {}
        }
    }
    offline.then_some(PowerSource::Battery)
}

pub struct Snapshot {
    pub external_power: Option<PowerSource>,
    pub battery: Option<Battery>,
    pub policy_count: usize,
    pub epp_options: std::collections::BTreeSet<String>,
    pub driver: String,
    pub governor: String,
    pub epp: String,
    pub minimum: String,
    pub maximum: String,
    pub maximum_khz: Option<u64>,
    pub platform_profile: String,
}

fn policy_values(policies: &[PathBuf], key: &str) -> Vec<Option<String>> {
    let mut values: Vec<_> = policies.iter().map(|path| read(path, key)).collect();
    values.sort();
    values.dedup();
    values
}

fn policy_value(values: &[Option<String>], frequency: bool) -> String {
    match values {
        [Some(value)] if frequency => value.parse::<u64>().ok()
            .map(|khz| format!("{:.0} MHz", khz as f64 / 1e3))
            .unwrap_or_else(|| "Unavailable".into()),
        [Some(value)] => value.clone(),
        [] | [None] => "Unavailable".into(),
        _ => "Varies by policy".into(),
    }
}

impl Snapshot {
    pub fn read(root: &Path) -> Self {
        let supplies = members(&root.join("class/power_supply"));
        let battery = supplies.iter().find(|path| {
            read(path, "type").as_deref() == Some("Battery")
                && read(path, "scope").as_deref() != Some("Device")
                && read(path, "present").as_deref() != Some("0")
        });
        let policies: Vec<_> = members(&root.join("devices/system/cpu/cpufreq"))
            .into_iter()
            .filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("policy")))
            .collect();
        let maxima = policy_values(&policies, "scaling_max_freq");
        let maximum_khz = match maxima.as_slice() {
            [Some(value)] => value.parse().ok(),
            _ => None,
        };
        Self {
            battery: battery.map(|path| Battery::read(path)),
            external_power: external_power(&supplies),
            policy_count: policies.len(),
            epp_options: crate::profiles::shared_epp(&policies),
            driver: policy_value(&policy_values(&policies, "scaling_driver"), false),
            governor: policy_value(&policy_values(&policies, "scaling_governor"), false),
            epp: policy_value(&policy_values(&policies, "energy_performance_preference"), false),
            minimum: policy_value(&policy_values(&policies, "scaling_min_freq"), true),
            maximum: policy_value(&maxima, true),
            maximum_khz,
            platform_profile: read(&root.join("firmware/acpi"), "platform_profile")
                .unwrap_or_else(|| "Unavailable".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acer_snapshot_keeps_epp_separate_from_frequency() {
        let root = std::env::temp_dir().join(format!("vatra-telemetry-{}", std::process::id()));
        let policy = root.join("devices/system/cpu/cpufreq/policy0");
        let battery = root.join("class/power_supply/BAT0");
        fs::create_dir_all(&policy).unwrap();
        fs::create_dir_all(&battery).unwrap();
        let charger = root.join("class/power_supply/AC");
        fs::create_dir_all(&charger).unwrap();
        fs::write(charger.join("type"), "Mains").unwrap();
        fs::write(charger.join("online"), "0").unwrap();
        for (key, value) in [
            ("scaling_driver", "amd-pstate-epp"),
            ("energy_performance_preference", "balance_power"),
            ("scaling_min_freq", "411658"),
            ("scaling_max_freq", "4373858"),
        ] {
            fs::write(policy.join(key), value).unwrap();
        }
        for (key, value) in [
            ("type", "Battery"),
            ("status", "Discharging"),
            ("voltage_now", "12000000"),
            ("current_now", "550000"),
            ("energy_now", "33000000"),
            ("charge_control_end_threshold", "80"),
        ] {
            fs::write(battery.join(key), value).unwrap();
        }
        let snapshot = Snapshot::read(&root);
        assert_eq!(snapshot.external_power, Some(PowerSource::Battery));
        fs::write(charger.join("online"), "1").unwrap();
        assert_eq!(Snapshot::read(&root).external_power, Some(PowerSource::Ac));
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(snapshot.epp, "balance_power");
        assert_eq!(snapshot.minimum, "412 MHz");
        assert_eq!(snapshot.maximum, "4374 MHz");
        let battery = snapshot.battery.unwrap();
        assert!(battery.derived_power);
        assert!((battery.signed_power().unwrap() + 6.6).abs() < 1e-9);
        assert!((battery.hours_remaining.unwrap() - 5.0).abs() < 1e-9);
        assert_eq!(battery.cycles, None);
        assert_eq!(battery.charge_start, None);
        assert_eq!(battery.charge_end, Some(80.0));
    }
}
