//! Bounded cursor custody with unlimited forward traversal of valid history.
//! Previous revisits eight recent pages; First page restarts after eviction.
use std::collections::VecDeque;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Direction {
    First,
    Next,
    Previous,
}
#[derive(Default)]
pub(super) struct Pages {
    previous: VecDeque<Option<String>>,
    current: Option<String>,
    anchor: Option<String>,
    power: u64,
    distance: u64,
    known: VecDeque<String>,
    empty: usize,
}
impl Pages {
    pub fn previous(&self) -> Option<Option<String>> {
        self.previous.back().cloned()
    }
    pub fn accept(
        &mut self,
        direction: Direction,
        request: Option<String>,
        next: Option<&str>,
        ids: &[String],
    ) -> Result<(), &'static str> {
        if direction == Direction::First {
            *self = Self::default();
        }
        if direction == Direction::Previous {
            self.anchor = None;
            self.power = 0;
            self.distance = 0;
            self.empty = 0;
        }
        if next.is_some_and(|next| {
            Some(next) == request.as_deref() || self.anchor.as_deref() == Some(next)
        }) {
            return Err("The comment cursor repeated. Refresh comments to restart the read.");
        }
        if direction == Direction::Next && ids.iter().all(|id| self.known.contains(id)) {
            self.empty += 1;
        } else {
            self.empty = 0;
        }
        if self.empty >= 10 {
            return Err(
                "Comments did not advance after several pages. Refresh comments to restart the read.",
            );
        }
        if direction == Direction::Next {
            self.previous.push_back(self.current.clone());
            while self.previous.len() > 8 {
                self.previous.pop_front();
            }
        } else if direction == Direction::Previous {
            self.previous.pop_back();
        }
        self.current = request;
        for id in ids {
            if !self.known.contains(id) {
                self.known.push_back(id.clone());
            }
        }
        while self.known.len() > 200 {
            self.known.pop_front();
        }
        if let Some(next) = next {
            self.distance = self.distance.saturating_add(1);
            if self.anchor.is_none() || self.distance >= self.power {
                self.anchor = Some(next.into());
                self.power = self.power.saturating_mul(2).max(1);
                self.distance = 0;
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_history_can_continue_beyond_every_retention_bound() {
        let mut p = Pages::default();
        let mut cursor = None;
        for i in 0..1000 {
            let next = format!("cursor-{i}");
            p.accept(
                if i == 0 {
                    Direction::First
                } else {
                    Direction::Next
                },
                cursor,
                Some(&next),
                &[format!("message-{i}")],
            )
            .unwrap();
            cursor = Some(next);
        }
        assert_eq!(p.previous.len(), 8);
        assert_eq!(p.known.len(), 200);
        assert_eq!(p.previous(), Some(Some("cursor-997".into())));
    }
    #[test]
    fn repeated_cycle_longer_than_previous_window_is_detected() {
        let mut p = Pages::default();
        let mut request = None;
        let mut detected = false;
        for i in 0..100 {
            let next = format!("cursor-{}", i % 13);
            if p.accept(
                if i == 0 {
                    Direction::First
                } else {
                    Direction::Next
                },
                request,
                Some(&next),
                &[format!("message-{i}")],
            )
            .is_err()
            {
                detected = true;
                break;
            }
            request = Some(next);
        }
        assert!(detected);
    }
    #[test]
    fn consecutive_no_progress_is_bounded_without_limiting_good_pages() {
        let mut p = Pages::default();
        p.accept(Direction::First, None, Some("first"), &[])
            .unwrap();
        let mut previous = Some("first".into());
        for i in 0..9 {
            let next = format!("n{i}");
            p.accept(Direction::Next, previous, Some(&next), &[])
                .unwrap();
            previous = Some(next);
        }
        assert!(
            p.accept(Direction::Next, previous, Some("last"), &[])
                .is_err()
        );
        assert!(p.previous.len() <= 8);
    }
    #[test]
    fn explicit_previous_and_first_are_not_mistaken_for_cursor_loops() {
        let mut p = Pages::default();
        p.accept(Direction::First, None, Some("a"), &["one".into()])
            .unwrap();
        p.accept(
            Direction::Next,
            Some("a".into()),
            Some("b"),
            &["two".into()],
        )
        .unwrap();
        assert_eq!(p.previous(), Some(None));
        p.accept(Direction::Previous, None, Some("a"), &["one".into()])
            .unwrap();
        p.accept(
            Direction::Next,
            Some("a".into()),
            Some("b"),
            &["two".into()],
        )
        .unwrap();
        p.accept(Direction::First, None, Some("a"), &["one".into()])
            .unwrap();
        assert!(p.previous().is_none());
    }
}
