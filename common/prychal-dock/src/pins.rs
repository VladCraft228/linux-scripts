use crate::Result;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Pins {
    pub ids: Vec<String>,
    path: Option<PathBuf>,
}
impl Pins {
    pub fn load(defaults: Vec<String>) -> Result<Self> {
        let path = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|p| p.join("linux-scripts-dock-rust-prototype/pins.json"));
        let ids = if let Some(p) = &path {
            match fs::read(p) {
                Ok(data) => {
                    if data.len() > 1024 * 1024 {
                        return Err("pins.json is larger than 1 MiB".into());
                    }
                    let ids: Vec<String> = serde_json::from_slice(&data)
                        .map_err(|e| format!("Cannot read {}: {e}; file preserved", p.display()))?;
                    if ids.len() > 512 || ids.iter().any(|s| s.is_empty() || s.len() > 4096) {
                        return Err("Invalid pins.json; file preserved".into());
                    }
                    ids
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => defaults,
                Err(e) => return Err(e.into()),
            }
        } else {
            defaults
        };
        let mut unique = Vec::new();
        for id in ids {
            if !unique.contains(&id) {
                unique.push(id)
            }
        }
        Ok(Self { ids: unique, path })
    }
    pub fn toggle(&mut self, key: &str) -> Result<()> {
        let mut ids = self.ids.clone();
        if ids.iter().any(|s| s == key) {
            ids.retain(|s| s != key)
        } else {
            if ids.len() >= 512 {
                return Err("Maximum of 512 pinned applications".into());
            }
            ids.push(key.into())
        }
        self.save(ids)
    }
    pub fn add(&mut self, keys: &[String]) -> Result<usize> {
        let mut ids = self.ids.clone();
        for key in keys {
            if !ids.contains(key) {
                ids.push(key.clone());
            }
        }
        if ids.len() > 512 {
            return Err("Maximum of 512 pinned applications".into());
        }
        let added = ids.len() - self.ids.len();
        if added > 0 {
            self.save(ids)?;
        }
        Ok(added)
    }
    pub fn remove(&mut self, keys: &[String]) -> Result<bool> {
        let ids: Vec<_> = self.ids.iter().filter(|key| !keys.contains(key)).cloned().collect();
        if ids.len() == self.ids.len() { return Ok(false); }
        self.save(ids)?;
        Ok(true)
    }
    pub fn reorder(&mut self, ids: Vec<String>) -> Result<()> {
        let mut old = self.ids.clone();
        let mut new = ids.clone();
        old.sort();
        new.sort();
        if old != new {
            return Err("Pinned applications changed while dragging".into());
        }
        if ids == self.ids {
            return Ok(());
        }
        self.save(ids)
    }
    fn save(&mut self, ids: Vec<String>) -> Result<()> {
        let path = self
            .path
            .as_ref()
            .ok_or("HOME/XDG_CONFIG_HOME is missing; cannot save pins")?;
        let parent = path.parent().ok_or("Invalid pin configuration path")?;
        fs::create_dir_all(parent)?;
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp = parent.join(format!(".pins-{}-{nonce}.tmp", std::process::id()));
        let result = (|| -> Result<()> {
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp)?;
            f.write_all(&serde_json::to_vec_pretty(&ids)?)?;
            f.write_all(b"\n")?;
            f.sync_all()?;
            fs::rename(&temp, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result?;
        self.ids = ids;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reorder_is_saved_and_rejects_changed_membership() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("dock-pin-order-{}-{nonce}", std::process::id()));
        let path = dir.join("pins.json");
        let mut pins = Pins {
            ids: vec!["one".into(), "two".into(), "three".into()],
            path: Some(path.clone()),
        };
        pins.reorder(vec!["three".into(), "one".into(), "two".into()])
            .unwrap();
        let saved: Vec<String> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved, pins.ids);
        assert!(pins
            .reorder(vec!["one".into(), "one".into(), "two".into()])
            .is_err());
        assert_eq!(
            serde_json::from_slice::<Vec<String>>(&fs::read(path).unwrap()).unwrap(),
            saved
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
