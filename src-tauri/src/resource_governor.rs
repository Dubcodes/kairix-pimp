use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Condvar, Mutex},
    time::{Duration, Instant},
};

pub const PRIORITY_RECOVERY: u8 = 0;
pub const PRIORITY_FOREGROUND: u8 = 1;
pub const PRIORITY_VISIBLE: u8 = 2;
pub const PRIORITY_EXPLICIT_SCAN: u8 = 3;
pub const PRIORITY_INTERACTIVE_PREVIEW: u8 = 4;
pub const PRIORITY_BACKGROUND_DERIVATIVE: u8 = 5;
pub const PRIORITY_MAINTENANCE: u8 = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Read,
    Write,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VolumeNeed {
    pub volume_id: String,
    pub access: Access,
}

impl VolumeNeed {
    pub fn read(volume_id: impl Into<String>) -> Self {
        Self {
            volume_id: volume_id.into(),
            access: Access::Read,
        }
    }

    pub fn write(volume_id: impl Into<String>) -> Self {
        Self {
            volume_id: volume_id.into(),
            access: Access::Write,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    Quiet,
    Balanced,
    Performance,
}

impl Profile {
    pub fn parse(value: &str) -> Self {
        match value {
            "quiet" => Self::Quiet,
            "performance" => Self::Performance,
            _ => Self::Balanced,
        }
    }

    fn background_limit(self, idle_boost: bool) -> usize {
        match (self, idle_boost) {
            (Self::Quiet, _) => 1,
            (Self::Balanced, false) => 1,
            (Self::Balanced, true) => 2,
            (Self::Performance, false) => 2,
            (Self::Performance, true) => 3,
        }
    }

    fn admission_cooldown(self, idle_boost: bool) -> Duration {
        match (self, idle_boost) {
            (Self::Quiet, _) => Duration::from_millis(750),
            (Self::Balanced, false) => Duration::from_millis(150),
            _ => Duration::ZERO,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Request {
    pub operation: String,
    pub priority: u8,
    pub background: bool,
    pub needs: Vec<VolumeNeed>,
}

impl Request {
    pub fn new(
        operation: impl Into<String>,
        priority: u8,
        background: bool,
        needs: Vec<VolumeNeed>,
    ) -> Self {
        let mut merged = BTreeMap::new();
        for need in needs {
            merged
                .entry(need.volume_id)
                .and_modify(|access| {
                    if need.access == Access::Write {
                        *access = Access::Write;
                    }
                })
                .or_insert(need.access);
        }
        Self {
            operation: operation.into(),
            priority: priority.min(7),
            background,
            needs: merged
                .into_iter()
                .map(|(volume_id, access)| VolumeNeed { volume_id, access })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct VolumeState {
    readers: usize,
    writers: usize,
    bytes_read: u64,
    bytes_written: u64,
    completed_operations: u64,
    io_errors: u32,
    active_since: Option<Instant>,
    active_seconds: f64,
    backoff_until: Option<Instant>,
}

#[derive(Clone, Debug)]
struct Waiter {
    ticket: u64,
    request: Request,
    enqueued_at: Instant,
    enqueued_epoch: u64,
}

struct State {
    next_ticket: u64,
    epoch: u64,
    shutdown: bool,
    profile: Profile,
    idle_boost: bool,
    background_paused: bool,
    gentle_scanning: bool,
    active_background: usize,
    next_background_admission: Instant,
    volumes: HashMap<String, VolumeState>,
    waiters: Vec<Waiter>,
    total_wait_millis: u64,
    admitted_operations: u64,
    recent_errors: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            next_ticket: 0,
            epoch: 0,
            shutdown: false,
            profile: Profile::Balanced,
            idle_boost: false,
            background_paused: false,
            gentle_scanning: true,
            active_background: 0,
            next_background_admission: Instant::now(),
            volumes: HashMap::new(),
            waiters: vec![],
            total_wait_millis: 0,
            admitted_operations: 0,
            recent_errors: vec![],
        }
    }
}

#[derive(Default)]
pub struct VolumeGovernor {
    state: Mutex<State>,
    ready: Condvar,
}

pub struct Permit<'a> {
    governor: &'a VolumeGovernor,
    request: Request,
    bytes_by_volume: HashMap<String, (u64, u64)>,
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSnapshot {
    pub volume_id: String,
    pub active_readers: usize,
    pub active_writers: usize,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub observed_bytes_per_second: Option<u64>,
    pub completed_operations: u64,
    pub io_errors: u32,
    pub backing_off: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitingSnapshot {
    pub operation: String,
    pub priority: u8,
    pub wait_millis: u64,
    pub reason: String,
    pub volume_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernorSnapshot {
    pub profile: String,
    pub idle_boost_active: bool,
    pub background_paused: bool,
    pub shutdown: bool,
    pub active_background_workers: usize,
    pub queued_workers: usize,
    pub average_wait_millis: u64,
    pub volumes: Vec<VolumeSnapshot>,
    pub waiting: Vec<WaitingSnapshot>,
    pub recent_errors: Vec<String>,
}

impl VolumeGovernor {
    pub fn configure(
        &self,
        profile: Profile,
        idle_boost: bool,
        background_paused: bool,
        gentle_scanning: bool,
    ) {
        let mut state = self.state.lock().unwrap();
        state.profile = profile;
        state.idle_boost = idle_boost;
        state.background_paused = background_paused;
        state.gentle_scanning = gentle_scanning;
        self.ready.notify_all();
    }

    pub fn acquire(
        &self,
        request: Request,
        cancelled: impl Fn() -> bool,
    ) -> Result<Permit<'_>, String> {
        if request.needs.is_empty() {
            return Ok(Permit {
                governor: self,
                request,
                bytes_by_volume: HashMap::new(),
                error: None,
            });
        }
        let mut state = self.state.lock().unwrap();
        let ticket = state.next_ticket;
        state.next_ticket = state.next_ticket.saturating_add(1);
        let enqueued_at = Instant::now();
        let enqueued_epoch = state.epoch;
        state.waiters.push(Waiter {
            ticket,
            request: request.clone(),
            enqueued_at,
            enqueued_epoch,
        });
        loop {
            if cancelled() {
                state.waiters.retain(|waiter| waiter.ticket != ticket);
                self.ready.notify_all();
                return Err("Resource wait was cancelled".into());
            }
            if state.shutdown {
                state.waiters.retain(|waiter| waiter.ticket != ticket);
                self.ready.notify_all();
                return Err("Resource governor is shutting down".into());
            }
            let now = Instant::now();
            if self.is_next_eligible(&state, ticket, now) {
                state.waiters.retain(|waiter| waiter.ticket != ticket);
                for need in &request.needs {
                    let volume = state.volumes.entry(need.volume_id.clone()).or_default();
                    if volume.readers + volume.writers == 0 {
                        volume.active_since = Some(now);
                    }
                    match need.access {
                        Access::Read => volume.readers += 1,
                        Access::Write => volume.writers += 1,
                    }
                }
                if request.background {
                    state.active_background += 1;
                    let cooldown = state.profile.admission_cooldown(state.idle_boost);
                    let gentle = if state.gentle_scanning {
                        match state.profile {
                            Profile::Quiet => Duration::from_millis(1_500),
                            Profile::Balanced => Duration::from_millis(500),
                            Profile::Performance => Duration::from_millis(250),
                        }
                    } else {
                        Duration::ZERO
                    };
                    state.next_background_admission = now + cooldown.max(gentle);
                }
                state.total_wait_millis = state
                    .total_wait_millis
                    .saturating_add(enqueued_at.elapsed().as_millis() as u64);
                state.admitted_operations = state.admitted_operations.saturating_add(1);
                return Ok(Permit {
                    governor: self,
                    request,
                    bytes_by_volume: HashMap::new(),
                    error: None,
                });
            }
            let (next, _) = self
                .ready
                .wait_timeout(state, Duration::from_millis(100))
                .unwrap();
            state = next;
        }
    }

    fn is_next_eligible(&self, state: &State, ticket: u64, now: Instant) -> bool {
        let Some(waiter) = state.waiters.iter().find(|waiter| waiter.ticket == ticket) else {
            return false;
        };
        if !resources_available(state, &waiter.request, now) {
            return false;
        }
        if waiter.request.background
            && (state.background_paused
                || state.active_background >= state.profile.background_limit(state.idle_boost)
                || now < state.next_background_admission)
        {
            return false;
        }
        let rank = waiter_rank(waiter, state.epoch);
        !state.waiters.iter().any(|candidate| {
            candidate.ticket != ticket
                && resources_overlap(&candidate.request, &waiter.request)
                && resources_available(state, &candidate.request, now)
                && (!candidate.request.background
                    || (!state.background_paused
                        && state.active_background
                            < state.profile.background_limit(state.idle_boost)
                        && now >= state.next_background_admission))
                && waiter_rank(candidate, state.epoch) < rank
        })
    }

    pub fn snapshot(&self) -> GovernorSnapshot {
        let state = self.state.lock().unwrap();
        let now = Instant::now();
        let mut volumes = state
            .volumes
            .iter()
            .map(|(volume_id, volume)| {
                let seconds = volume.active_seconds
                    + volume
                        .active_since
                        .map(|started| started.elapsed().as_secs_f64())
                        .unwrap_or_default();
                VolumeSnapshot {
                    volume_id: volume_id.clone(),
                    active_readers: volume.readers,
                    active_writers: volume.writers,
                    bytes_read: volume.bytes_read,
                    bytes_written: volume.bytes_written,
                    observed_bytes_per_second: (seconds > 0.0).then(|| {
                        ((volume.bytes_read + volume.bytes_written) as f64 / seconds) as u64
                    }),
                    completed_operations: volume.completed_operations,
                    io_errors: volume.io_errors,
                    backing_off: volume.backoff_until.is_some_and(|until| until > now),
                }
            })
            .collect::<Vec<_>>();
        volumes.sort_by(|left, right| left.volume_id.cmp(&right.volume_id));
        let mut waiting = state
            .waiters
            .iter()
            .map(|waiter| WaitingSnapshot {
                operation: waiter.request.operation.clone(),
                priority: waiter.request.priority,
                wait_millis: waiter.enqueued_at.elapsed().as_millis() as u64,
                reason: waiting_reason(&state, &waiter.request, now),
                volume_ids: waiter
                    .request
                    .needs
                    .iter()
                    .map(|need| need.volume_id.clone())
                    .collect(),
            })
            .collect::<Vec<_>>();
        waiting.sort_by_key(|waiter| waiter.priority);
        GovernorSnapshot {
            profile: match state.profile {
                Profile::Quiet => "quiet",
                Profile::Balanced => "balanced",
                Profile::Performance => "performance",
            }
            .into(),
            idle_boost_active: state.idle_boost,
            background_paused: state.background_paused,
            shutdown: state.shutdown,
            active_background_workers: state.active_background,
            queued_workers: state.waiters.len(),
            average_wait_millis: if state.admitted_operations == 0 {
                0
            } else {
                state.total_wait_millis / state.admitted_operations
            },
            volumes,
            waiting,
            recent_errors: state.recent_errors.clone(),
        }
    }

    pub fn shutdown(&self) {
        self.state.lock().unwrap().shutdown = true;
        self.ready.notify_all();
    }
}

impl Permit<'_> {
    pub fn add_bytes_for(&mut self, volume_id: &str, bytes_read: u64, bytes_written: u64) {
        let entry = self
            .bytes_by_volume
            .entry(volume_id.to_string())
            .or_default();
        entry.0 = entry.0.saturating_add(bytes_read);
        entry.1 = entry.1.saturating_add(bytes_written);
    }

    pub fn record_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut state = self.governor.state.lock().unwrap();
        let now = Instant::now();
        for need in &self.request.needs {
            let volume = state.volumes.entry(need.volume_id.clone()).or_default();
            match need.access {
                Access::Read => volume.readers = volume.readers.saturating_sub(1),
                Access::Write => volume.writers = volume.writers.saturating_sub(1),
            }
            let (bytes_read, bytes_written) = self
                .bytes_by_volume
                .get(&need.volume_id)
                .copied()
                .unwrap_or_default();
            volume.bytes_read = volume.bytes_read.saturating_add(bytes_read);
            volume.bytes_written = volume.bytes_written.saturating_add(bytes_written);
            volume.completed_operations = volume.completed_operations.saturating_add(1);
            if volume.readers + volume.writers == 0 {
                if let Some(started) = volume.active_since.take() {
                    volume.active_seconds += started.elapsed().as_secs_f64();
                }
            }
            if self.error.is_some() {
                volume.io_errors = volume.io_errors.saturating_add(1);
                let exponent = volume.io_errors.min(5);
                let base = if cfg!(test) { 5 } else { 250 };
                volume.backoff_until = Some(now + Duration::from_millis(base * (1 << exponent)));
            } else {
                volume.io_errors = 0;
                volume.backoff_until = None;
            }
        }
        if self.request.background {
            state.active_background = state.active_background.saturating_sub(1);
        }
        if let Some(error) = &self.error {
            state
                .recent_errors
                .push(format!("{}: {error}", self.request.operation));
            if state.recent_errors.len() > 20 {
                state.recent_errors.remove(0);
            }
        }
        state.epoch = state.epoch.saturating_add(1);
        self.governor.ready.notify_all();
    }
}

fn waiter_rank(waiter: &Waiter, epoch: u64) -> (u8, u64) {
    let age_promotions = ((epoch.saturating_sub(waiter.enqueued_epoch)) / 4) as u8;
    (
        waiter.request.priority.saturating_sub(age_promotions),
        waiter.ticket,
    )
}

fn resources_available(state: &State, request: &Request, now: Instant) -> bool {
    request.needs.iter().all(|need| {
        state.volumes.get(&need.volume_id).is_none_or(|volume| {
            if volume.backoff_until.is_some_and(|until| until > now) {
                return false;
            }
            match need.access {
                Access::Read => volume.readers == 0 && volume.writers == 0,
                Access::Write => volume.readers == 0 && volume.writers == 0,
            }
        })
    })
}

fn resources_overlap(left: &Request, right: &Request) -> bool {
    left.needs.iter().any(|left_need| {
        right
            .needs
            .iter()
            .any(|right_need| left_need.volume_id == right_need.volume_id)
    })
}

fn waiting_reason(state: &State, request: &Request, now: Instant) -> String {
    if request.needs.iter().any(|need| {
        state
            .volumes
            .get(&need.volume_id)
            .is_some_and(|volume| volume.backoff_until.is_some_and(|until| until > now))
    }) {
        return "Backing off after an I/O error".into();
    }
    if request.needs.iter().any(|need| {
        state
            .volumes
            .get(&need.volume_id)
            .is_some_and(|volume| volume.readers + volume.writers > 0)
    }) {
        return "Waiting for volume access".into();
    }
    if request.background && state.background_paused {
        return "Background processing is paused".into();
    }
    if request.background
        && state.active_background >= state.profile.background_limit(state.idle_boost)
    {
        return "Waiting for background capacity".into();
    }
    "Waiting behind higher-priority work".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    };

    fn read_request(name: &str, priority: u8, volume: &str) -> Request {
        Request::new(name, priority, false, vec![VolumeNeed::read(volume)])
    }

    #[test]
    fn one_full_content_reader_is_admitted_per_volume() {
        let governor = Arc::new(VolumeGovernor::default());
        let first = governor
            .acquire(read_request("first", 3, "volume-a"), || false)
            .unwrap();
        let second_governor = governor.clone();
        let (sender, receiver) = mpsc::channel();
        let second = std::thread::spawn(move || {
            let _permit = second_governor
                .acquire(read_request("second", 3, "volume-a"), || false)
                .unwrap();
            sender.send(()).unwrap();
        });
        assert!(receiver.recv_timeout(Duration::from_millis(30)).is_err());
        drop(first);
        receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        second.join().unwrap();
    }

    #[test]
    fn different_volumes_admit_independently() {
        let governor = VolumeGovernor::default();
        let _first = governor
            .acquire(read_request("first", 3, "volume-a"), || false)
            .unwrap();
        let _second = governor
            .acquire(read_request("second", 3, "volume-b"), || false)
            .unwrap();
        let snapshot = governor.snapshot();
        assert_eq!(snapshot.volumes.len(), 2);
        assert!(snapshot
            .volumes
            .iter()
            .all(|volume| volume.active_readers == 1));
    }

    #[test]
    fn reused_drive_letter_with_a_new_volume_identity_does_not_reuse_the_old_lock() {
        let governor = VolumeGovernor::default();
        let _old_card = governor
            .acquire(read_request("old E drive", 3, "volume-guid:old"), || false)
            .unwrap();
        let _replacement_card = governor
            .acquire(read_request("new E drive", 3, "volume-guid:new"), || false)
            .unwrap();
        assert_eq!(governor.snapshot().volumes.len(), 2);
    }

    #[test]
    fn stable_volume_identity_groups_multiple_logical_paths() {
        let request = Request::new(
            "grouped",
            3,
            false,
            vec![VolumeNeed::read("stable-id"), VolumeNeed::read("stable-id")],
        );
        assert_eq!(request.needs, vec![VolumeNeed::read("stable-id")]);
    }

    #[test]
    fn source_and_destination_are_acquired_atomically_without_deadlock() {
        let governor = Arc::new(VolumeGovernor::default());
        let held = governor
            .acquire(read_request("held", 3, "destination"), || false)
            .unwrap();
        let worker_governor = governor.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _permit = worker_governor
                .acquire(
                    Request::new(
                        "copy",
                        PRIORITY_FOREGROUND,
                        false,
                        vec![VolumeNeed::read("source"), VolumeNeed::write("destination")],
                    ),
                    || false,
                )
                .unwrap();
            sender.send(()).unwrap();
        });
        assert!(governor
            .acquire(read_request("unrelated", 3, "source"), || false)
            .is_ok());
        assert!(receiver.recv_timeout(Duration::from_millis(30)).is_err());
        drop(held);
        receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn foreground_waiter_precedes_background_and_aging_is_bounded() {
        let governor = Arc::new(VolumeGovernor::default());
        let active = governor
            .acquire(read_request("active", 2, "volume-a"), || false)
            .unwrap();
        let (sender, receiver) = mpsc::channel();
        let background_governor = governor.clone();
        let background_sender = sender.clone();
        let background = std::thread::spawn(move || {
            let _permit = background_governor
                .acquire(
                    Request::new(
                        "background",
                        PRIORITY_MAINTENANCE,
                        true,
                        vec![VolumeNeed::read("volume-a")],
                    ),
                    || false,
                )
                .unwrap();
            background_sender.send("background").unwrap();
        });
        while governor.snapshot().queued_workers < 1 {
            std::thread::yield_now();
        }
        let foreground_governor = governor.clone();
        let foreground = std::thread::spawn(move || {
            let _permit = foreground_governor
                .acquire(
                    read_request("foreground", PRIORITY_FOREGROUND, "volume-a"),
                    || false,
                )
                .unwrap();
            sender.send("foreground").unwrap();
        });
        while governor.snapshot().queued_workers < 2 {
            std::thread::yield_now();
        }
        drop(active);
        assert_eq!(receiver.recv().unwrap(), "foreground");
        assert_eq!(receiver.recv().unwrap(), "background");
        foreground.join().unwrap();
        background.join().unwrap();
    }

    #[test]
    fn cancellation_and_shutdown_release_waiters() {
        let governor = Arc::new(VolumeGovernor::default());
        let _active = governor
            .acquire(read_request("active", 2, "volume-a"), || false)
            .unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_governor = governor.clone();
        let worker_cancelled = cancelled.clone();
        let worker = std::thread::spawn(move || {
            worker_governor
                .acquire(read_request("waiting", 3, "volume-a"), || {
                    worker_cancelled.load(Ordering::Relaxed)
                })
                .map(|permit| drop(permit))
        });
        while governor.snapshot().queued_workers == 0 {
            std::thread::yield_now();
        }
        cancelled.store(true, Ordering::Relaxed);
        assert!(worker.join().unwrap().is_err());
        governor.shutdown();
        assert!(governor
            .acquire(read_request("after-shutdown", 1, "volume-b"), || false)
            .is_err());
    }

    #[test]
    fn io_errors_back_off_and_metrics_remain_local() {
        let governor = VolumeGovernor::default();
        {
            let mut permit = governor
                .acquire(read_request("failing", 3, "volume-a"), || false)
                .unwrap();
            permit.add_bytes_for("volume-a", 4096, 0);
            permit.record_error("synthetic read failure");
        }
        let snapshot = governor.snapshot();
        assert!(snapshot.volumes[0].backing_off);
        assert_eq!(snapshot.volumes[0].bytes_read, 4096);
        assert_eq!(snapshot.volumes[0].io_errors, 1);
        assert_eq!(snapshot.recent_errors.len(), 1);
    }

    #[test]
    fn profiles_and_idle_transitions_change_real_background_admission() {
        let governor = Arc::new(VolumeGovernor::default());
        governor.configure(Profile::Balanced, false, false, false);
        let first = governor
            .acquire(
                Request::new(
                    "first background",
                    PRIORITY_BACKGROUND_DERIVATIVE,
                    true,
                    vec![VolumeNeed::read("volume-a")],
                ),
                || false,
            )
            .unwrap();
        let second_governor = governor.clone();
        let (sender, receiver) = mpsc::channel();
        let second = std::thread::spawn(move || {
            let _permit = second_governor
                .acquire(
                    Request::new(
                        "second background",
                        PRIORITY_BACKGROUND_DERIVATIVE,
                        true,
                        vec![VolumeNeed::read("volume-b")],
                    ),
                    || false,
                )
                .unwrap();
            sender.send(()).unwrap();
        });
        assert!(receiver.recv_timeout(Duration::from_millis(30)).is_err());
        governor.configure(Profile::Balanced, true, false, false);
        receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        drop(first);
        second.join().unwrap();

        governor.configure(Profile::Performance, true, true, false);
        let cancelled = AtomicBool::new(false);
        let paused_governor = governor.clone();
        let paused = std::thread::spawn(move || {
            paused_governor
                .acquire(
                    Request::new(
                        "paused background",
                        PRIORITY_MAINTENANCE,
                        true,
                        vec![VolumeNeed::read("volume-c")],
                    ),
                    || cancelled.load(Ordering::Relaxed),
                )
                .map(drop)
        });
        while governor.snapshot().queued_workers == 0 {
            std::thread::yield_now();
        }
        governor.shutdown();
        assert!(paused.join().unwrap().is_err());
    }

    #[test]
    fn bounded_aging_eventually_promotes_a_waiting_background_request() {
        let waiter = Waiter {
            ticket: 7,
            request: Request::new(
                "old maintenance",
                PRIORITY_MAINTENANCE,
                true,
                vec![VolumeNeed::read("archive")],
            ),
            enqueued_at: Instant::now(),
            enqueued_epoch: 0,
        };

        assert_eq!(waiter_rank(&waiter, 0).0, PRIORITY_MAINTENANCE);
        assert_eq!(waiter_rank(&waiter, 24).0, 0);
    }

    #[test]
    fn paused_aged_background_work_cannot_block_foreground_work() {
        let governor = Arc::new(VolumeGovernor::default());
        let active = governor
            .acquire(read_request("active", PRIORITY_FOREGROUND, "archive"), || false)
            .unwrap();
        governor.configure(Profile::Balanced, false, true, false);
        let background_governor = governor.clone();
        let background = std::thread::spawn(move || {
            background_governor
                .acquire(
                    Request::new(
                        "paused background",
                        PRIORITY_MAINTENANCE,
                        true,
                        vec![VolumeNeed::read("archive")],
                    ),
                    || false,
                )
                .map(drop)
        });
        while governor.snapshot().queued_workers == 0 {
            std::thread::yield_now();
        }
        {
            let mut state = governor.state.lock().unwrap();
            state.epoch = 24;
        }
        let foreground_governor = governor.clone();
        let (sender, receiver) = mpsc::channel();
        let foreground = std::thread::spawn(move || {
            let _permit = foreground_governor
                .acquire(
                    read_request("foreground", PRIORITY_FOREGROUND, "archive"),
                    || false,
                )
                .unwrap();
            sender.send(()).unwrap();
        });
        drop(active);
        receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        foreground.join().unwrap();
        governor.shutdown();
        assert!(background.join().unwrap().is_err());
    }
}
