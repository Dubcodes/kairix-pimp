use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

use crate::{fs_safe::FileSnapshot, read_json, write_json};

pub const MINIMUM_STABLE_SECONDS: i64 = 15;
pub const REQUIRED_STABLE_OBSERVATIONS: u32 = 2;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryState {
    #[serde(default)]
    pub candidates: HashMap<String, CandidateObservation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateObservation {
    pub length: u64,
    pub modified_millis: Option<u128>,
    pub first_stable_unix_seconds: i64,
    pub last_observed_unix_seconds: i64,
    pub stable_observations: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Wait,
    Verify,
}

impl DiscoveryState {
    pub fn observe(
        &mut self,
        relative_path: &str,
        snapshot: &FileSnapshot,
        now_unix_seconds: i64,
    ) -> Decision {
        let candidate =
            self.candidates
                .entry(relative_path.to_string())
                .or_insert(CandidateObservation {
                    length: snapshot.length,
                    modified_millis: snapshot.modified_millis,
                    first_stable_unix_seconds: now_unix_seconds,
                    last_observed_unix_seconds: now_unix_seconds,
                    stable_observations: 0,
                });
        if candidate.length != snapshot.length
            || candidate.modified_millis != snapshot.modified_millis
        {
            *candidate = CandidateObservation {
                length: snapshot.length,
                modified_millis: snapshot.modified_millis,
                first_stable_unix_seconds: now_unix_seconds,
                last_observed_unix_seconds: now_unix_seconds,
                stable_observations: 0,
            };
            return Decision::Wait;
        }
        candidate.stable_observations += 1;
        candidate.last_observed_unix_seconds = now_unix_seconds;
        if candidate.stable_observations >= REQUIRED_STABLE_OBSERVATIONS
            && now_unix_seconds - candidate.first_stable_unix_seconds >= MINIMUM_STABLE_SECONDS
        {
            Decision::Verify
        } else {
            Decision::Wait
        }
    }

    pub fn verified(&mut self, relative_path: &str) {
        self.candidates.remove(relative_path);
    }
}

pub fn load(path: &Path) -> Result<DiscoveryState, String> {
    read_json(path)
}

pub fn save(path: &Path, state: &DiscoveryState) -> Result<(), String> {
    write_json(path, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_copy_must_remain_stable_across_time_and_observations() {
        let mut state = DiscoveryState::default();
        let first = FileSnapshot {
            length: 10,
            modified_millis: Some(1),
        };
        assert_eq!(state.observe("clip.mp4", &first, 0), Decision::Wait);
        let growing = FileSnapshot {
            length: 20,
            modified_millis: Some(2),
        };
        assert_eq!(state.observe("clip.mp4", &growing, 10), Decision::Wait);
        assert_eq!(state.observe("clip.mp4", &growing, 20), Decision::Wait);
        assert_eq!(state.observe("clip.mp4", &growing, 25), Decision::Verify);
    }

    #[test]
    fn later_changes_reset_stability() {
        let mut state = DiscoveryState::default();
        let stable = FileSnapshot {
            length: 10,
            modified_millis: Some(1),
        };
        state.observe("clip.mp4", &stable, 0);
        assert_eq!(state.observe("clip.mp4", &stable, 20), Decision::Verify);
        let changed = FileSnapshot {
            length: 11,
            modified_millis: Some(2),
        };
        assert_eq!(state.observe("clip.mp4", &changed, 21), Decision::Wait);
    }
}
