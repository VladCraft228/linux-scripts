use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const WINDOW_SECONDS: u64 = 600;
const MAX_FILE_BYTES: u64 = 64 * 1024;
const SAVE_INTERVAL: u64 = 30;
pub const DISCHARGE_WINDOWS: [u64; 3] = [60, 300, WINDOW_SECONDS];

pub struct DischargeStats {
    pub average: f64,
    pub minimum: f64,
    pub maximum: f64,
    pub samples: u32,
}

pub struct History {
    path: Option<PathBuf>,
    samples: BTreeMap<u64, Option<f64>>,
    saved_at: u64,
    dirty: bool,
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

impl History {
    pub fn load() -> Self {
        let path = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
            .map(|path| path.join("vatra/battery-power.tsv"));
        let time = now();
        let samples = path.as_ref().map(|path| Self::read(path, time)).unwrap_or_default();
        Self { path, samples, saved_at: time, dirty: false }
    }

    fn read(path: &Path, time: u64) -> BTreeMap<u64, Option<f64>> {
        let Ok(file) = fs::File::open(path) else { return BTreeMap::new() };
        let mut contents = String::new();
        if file.take(MAX_FILE_BYTES).read_to_string(&mut contents).is_err() {
            return BTreeMap::new();
        }
        contents.lines().filter_map(|line| {
            let (timestamp, value) = line.split_once('\t')?;
            let timestamp = timestamp.parse::<u64>().ok()?;
            if !(time.saturating_sub(WINDOW_SECONDS)..=time).contains(&timestamp) {
                return None;
            }
            let value = if value == "-" {
                None
            } else {
                Some(value.parse::<f64>().ok().filter(|value| value.is_finite() && value.abs() <= 1e4)?)
            };
            Some((timestamp, value))
        }).collect()
    }

    pub fn points(&self) -> impl Iterator<Item = (u64, Option<f64>)> + '_ {
        self.samples.iter().map(|(&time, &value)| (time, value))
    }

    pub fn discharge_stats(&self, now: u64, seconds: u64) -> Option<DischargeStats> {
        let mut readings = self.samples.range(now.saturating_sub(seconds)..=now)
            .filter_map(|(_, watts)| {
                // Discharge samples are negated at collection, even at zero watts.
                watts.filter(|watts| watts.is_sign_negative()).map(f64::abs)
            });
        let first = readings.next()?;
        let (sum, count, minimum, maximum) = readings.fold(
            (first, 1_u32, first, first),
            |(sum, count, min, max), watts| (sum + watts, count + 1, min.min(watts), max.max(watts)),
        );
        Some(DischargeStats {
            average: sum / f64::from(count),
            minimum,
            maximum,
            samples: count,
        })
    }

    pub fn record(&mut self, time: u64, watts: Option<f64>) {
        self.samples.retain(|timestamp, _| (time.saturating_sub(WINDOW_SECONDS)..=time).contains(timestamp));
        self.samples.insert(time, watts.filter(|value| value.is_finite() && value.abs() <= 1e4));
        self.dirty = true;
        if time < self.saved_at || time - self.saved_at >= SAVE_INTERVAL {
            self.saved_at = time;
            if let Err(error) = self.save(time) {
                eprintln!("Cannot save Vatra history: {error}");
            }
        }
    }

    pub fn save(&mut self, time: u64) -> io::Result<()> {
        let Some(path) = self.path.as_ref().filter(|_| self.dirty) else { return Ok(()) };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut samples = Self::read(path, time);
        samples.extend(self.points().filter(|(timestamp, _)| {
            (time.saturating_sub(WINDOW_SECONDS)..=time).contains(timestamp)
        }));
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let temporary = path.with_extension(format!("tmp.{}.{nonce}", std::process::id()));
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary)?;
        let result = (|| {
            for (timestamp, watts) in samples {
                match watts {
                    Some(watts) => writeln!(file, "{timestamp}\t{watts:.4}")?,
                    None => writeln!(file, "{timestamp}\t-")?,
                }
            }
            file.flush()?;
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        } else {
            self.dirty = false;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discharge_windows_exclude_charging_missing_and_future_samples() {
        let history = History {
            path: None,
            samples: BTreeMap::from([
                (399, Some(-100.0)),
                (400, Some(-8.0)),
                (700, Some(-14.0)),
                (940, Some(-6.0)),
                (980, None),
                (990, Some(25.0)),
                (1000, Some(-10.0)),
                (1001, Some(-100.0)),
            ]),
            saved_at: 1000,
            dirty: false,
        };
        let actual = DISCHARGE_WINDOWS.map(|seconds| {
            let stats = history.discharge_stats(1000, seconds).unwrap();
            (stats.average, stats.minimum, stats.maximum, stats.samples)
        });
        assert_eq!(actual, [(8.0, 6.0, 10.0, 2), (10.0, 6.0, 14.0, 3), (9.5, 6.0, 14.0, 4)]);
        assert!(history.discharge_stats(999, 19).is_none());
    }

    #[test]
    fn history_round_trip_discards_expired_points_and_keeps_gaps() {
        let path = std::env::temp_dir().join(format!("vatra-history-test-{}", std::process::id()));
        let mut history = History {
            path: Some(path.clone()), samples: BTreeMap::new(), saved_at: 100, dirty: false,
        };
        history.record(100, Some(-7.0));
        history.record(710, Some(-5.25));
        history.record(712, None);
        history.save(712).unwrap();
        assert_eq!(History::read(&path, 712), BTreeMap::from([(710, Some(-5.25)), (712, None)]));
        fs::remove_file(path).unwrap();
    }
}
