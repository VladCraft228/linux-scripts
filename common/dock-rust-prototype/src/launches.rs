use crate::apps::{Apps, Item, Launch};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    process::Child,
    time::{Duration, Instant},
};
const TIMEOUT: Duration = Duration::from_secs(12);
const SETTLE: Duration = Duration::from_millis(130);
pub struct Notification {
    pub title: String,
    pub detail: String,
}
struct Pending {
    name: String,
    known: HashSet<u32>,
    started: Instant,
    deadline: Instant,
    token: u64,
    finish: Option<(Instant, f32)>,
}
impl Pending {
    fn offset(&self, now: Instant) -> f32 {
        if let Some((at, height)) = self.finish {
            let t =
                (now.saturating_duration_since(at).as_secs_f32() / SETTLE.as_secs_f32()).min(1.);
            height * (1. - t).powi(3)
        } else {
            bounce(now.saturating_duration_since(self.started))
        }
    }
    fn settle(&mut self, now: Instant) {
        if self.finish.is_none() {
            self.finish = Some((now, self.offset(now)));
        }
    }
}
struct Process {
    key: String,
    name: String,
    token: u64,
    child: Child,
}
pub struct Launches {
    pending: HashMap<String, Pending>,
    processes: Vec<Process>,
    notices: VecDeque<Notification>,
    token: u64,
    poll_at: Option<Instant>,
}
impl Launches {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            processes: vec![],
            notices: VecDeque::new(),
            token: 0,
            poll_at: None,
        }
    }
    pub fn report(&mut self, title: &str, detail: String) {
        eprintln!("{title}: {detail}");
        if self.notices.len() >= 8 {
            self.notices.pop_front();
        }
        self.notices.push_back(Notification {
            title: title.into(),
            detail,
        });
    }
    pub fn take_notice(&mut self) -> Option<Notification> {
        self.notices.pop_front()
    }
    pub fn start(&mut self, item: &Item, launcher: &Launch, now: Instant) -> bool {
        if self
            .pending
            .get(&item.key)
            .is_some_and(|p| p.finish.is_none())
        {
            return false;
        }
        match launcher.spawn() {
            Ok(child) => {
                self.token = self.token.wrapping_add(1);
                let token = self.token;
                self.pending.insert(
                    item.key.clone(),
                    Pending {
                        name: item.name.clone(),
                        known: item.windows.iter().copied().collect(),
                        started: now,
                        deadline: now + TIMEOUT,
                        token,
                        finish: None,
                    },
                );
                self.processes.push(Process {
                    key: item.key.clone(),
                    name: item.name.clone(),
                    token,
                    child,
                });
                self.poll_at = Some(now);
                true
            }
            Err(error) => {
                self.report(
                    "Could not launch application",
                    format!("{}: {error}", item.name),
                );
                false
            }
        }
    }
    pub fn tick(&mut self, apps: &Apps, now: Instant) -> bool {
        let previous = self.pending.len();
        let mut timeouts = Vec::new();
        // Observe a new X11 window before examining launcher-helper exit status.
        for (key, p) in &mut self.pending {
            if p.finish.is_some() {
                continue;
            }
            let appeared = apps
                .items
                .iter()
                .find(|i| &i.key == key)
                .is_some_and(|item| has_new(&p.known, &item.windows));
            if appeared {
                p.settle(now);
            } else if now >= p.deadline {
                timeouts.push(p.name.clone());
                p.settle(now);
            }
        }
        for name in timeouts {
            self.report(
                "Launch not confirmed",
                format!("No new window detected for {name} within 12 seconds."),
            );
        }
        if self.poll_at.is_some_and(|at| now >= at) {
            let mut index = 0;
            let mut failures = Vec::new();
            while index < self.processes.len() {
                let status = self.processes[index].child.try_wait();
                let (remove, failure) = match status {
                    Ok(Some(status)) => (true, (!status.success()).then(|| status.to_string())),
                    Ok(None) => (false, None),
                    Err(error) => (true, Some(error.to_string())),
                };
                if remove {
                    let process = self.processes.swap_remove(index);
                    if let Some(reason) = failure {
                        if let Some(p) = self
                            .pending
                            .get_mut(&process.key)
                            .filter(|p| p.token == process.token && p.finish.is_none())
                        {
                            p.settle(now);
                            failures.push(format!("{}: {reason}", process.name));
                        }
                    }
                } else {
                    index += 1;
                }
            }
            for detail in failures {
                self.report("Application exited during launch", detail);
            }
            self.poll_at = if self.processes.is_empty() {
                None
            } else {
                Some(
                    now + if self.pending.values().any(|p| p.finish.is_none()) {
                        Duration::from_millis(200)
                    } else {
                        Duration::from_secs(1)
                    },
                )
            };
        }
        self.pending.retain(|_, p| {
            p.finish
                .is_none_or(|(at, _)| now.saturating_duration_since(at) < SETTLE)
        });
        self.pending.len() != previous
    }
    pub fn offset(&self, key: &str, now: Instant) -> f32 {
        self.pending.get(key).map_or(0., |p| p.offset(now))
    }
    pub fn animating(&self, key: &str) -> bool {
        self.pending.contains_key(key)
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.pending
            .values()
            .map(|p| p.finish.map_or(p.deadline, |(at, _)| at + SETTLE))
            .chain(self.poll_at)
            .min()
    }
}
fn has_new(known: &HashSet<u32>, windows: &[u32]) -> bool {
    windows.iter().any(|id| !known.contains(id))
}
fn bounce(elapsed: Duration) -> f32 {
    let ms = elapsed.as_secs_f32() * 1000.;
    let cycle = (ms / 400.).floor().min(30.) as i32;
    let phase = ms % 400.;
    let amplitude = (18. * 0.84_f32.powi(cycle)).max(4.);
    if phase < 170. {
        let t = phase / 170.;
        amplitude * (1. - (1. - t).powi(2))
    } else {
        let t = (phase - 170.) / 230.;
        amplitude * (1. - t.powi(2))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_cycle_and_decreasing_amplitude() {
        assert_eq!(bounce(Duration::ZERO), 0.);
        assert!((bounce(Duration::from_millis(170)) - 18.).abs() < 0.001);
        assert!(bounce(Duration::from_millis(570)) < 18.);
        assert_eq!(bounce(Duration::from_millis(400)), 0.);
    }
    #[test]
    fn settling_returns_to_baseline_and_cannot_restart() {
        let now = Instant::now();
        let mut pending = Pending {
            name: "Example".into(),
            known: HashSet::new(),
            started: now - Duration::from_millis(170),
            deadline: now + TIMEOUT,
            token: 1,
            finish: None,
        };
        pending.settle(now);
        assert!((pending.offset(now) - 18.).abs() < 0.001);
        assert!(pending.offset(now + SETTLE / 2) < 18.);
        pending.settle(now + SETTLE / 2);
        assert_eq!(pending.offset(now + SETTLE), 0.);
    }
    #[test]
    fn existing_window_is_not_completion() {
        let known = [10, 11].into_iter().collect();
        assert!(!has_new(&known, &[10, 11]));
        assert!(!has_new(&known, &[10]));
        assert!(has_new(&known, &[10, 11, 12]));
    }
}
