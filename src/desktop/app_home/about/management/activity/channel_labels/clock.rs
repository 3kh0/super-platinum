//! Presentation sampling only. Existing runtime ticks own cadence; no timer.
use super::*;
use std::time::Duration;

#[derive(Clone, Default)]
pub(super) struct Clock {
    generation: u64,
    next: Option<Instant>,
    followup: bool,
    face: BTreeMap<String, (bool, bool)>,
}
impl Clock {
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }
    pub(super) fn started(&mut self, now: Instant) {
        if !self.followup {
            self.next.get_or_insert(now + Duration::from_secs(1));
        }
    }
    pub(super) fn advance(&mut self, now: Instant, pending: bool) {
        if self.next.is_some_and(|next| now >= next) {
            self.generation = self.generation.wrapping_add(1);
            self.next = (!pending).then_some(now + Duration::from_secs(1));
            self.followup = pending;
        }
    }
    pub(super) fn finished(&mut self, generation: u64, now: Instant) {
        if self.generation == generation {
            self.followup = false;
            self.next = Some(now + Duration::from_secs(1));
        }
    }
    pub(super) fn update_face(&mut self, policies: &BTreeMap<String, Policy>, now: i64) -> bool {
        let next = policies
            .iter()
            .filter_map(|(id, p)| {
                p.at_millis(now)
                    .ok()
                    .map(|p| (id.clone(), (p.archive_icon, p.archived_suffix)))
            })
            .collect();
        if self.face == next {
            return false;
        }
        self.face = next;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_generation_retires_held_work_without_spawning_more_ticks() {
        let now = Instant::now();
        let mut c = Clock::default();
        c.started(now);
        let original = c.generation();
        c.advance(now + Duration::from_millis(999), true);
        assert_eq!(c.generation(), original);
        c.advance(now + Duration::from_secs(1), true);
        assert_ne!(c.generation(), original);
        let latest = c.generation();
        c.advance(now + Duration::from_secs(1), true);
        assert_eq!(c.generation(), latest);
    }
    #[test]
    fn slow_followup_is_not_starved_by_repeated_sampling() {
        let now = Instant::now();
        let mut c = Clock::default();
        c.started(now);
        c.advance(now + Duration::from_secs(1), true);
        c.finished(0, now + Duration::from_secs(2));
        let generation = c.generation();
        c.started(now + Duration::from_secs(2));
        c.advance(now + Duration::from_secs(30), true);
        assert_eq!(c.generation(), generation);
        c.finished(generation, now + Duration::from_secs(30));
        c.advance(now + Duration::from_secs(31), true);
        assert_ne!(c.generation(), generation);
    }
    #[test]
    fn exact_expiry_changes_face_once_and_backward_clock_restores_it() {
        let p = Policy {
            public: true,
            private: false,
            archived: true,
            frozen: true,
            archive_icon: false,
            archived_suffix: false,
            in_grace: true,
            grace_eligible: true,
            captured_now_ms: 9,
            grace_end_ms: Some(10),
        };
        let rows = [("C".into(), p)].into_iter().collect();
        let mut c = Clock::default();
        assert!(c.update_face(&rows, 9));
        assert!(!c.update_face(&rows, 9));
        assert!(c.update_face(&rows, 10));
        assert!(!c.update_face(&rows, 11));
        assert!(c.update_face(&rows, 9));
    }
}
