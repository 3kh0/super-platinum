//! One-shot metadata scripts for the two exact recovery cases.
use super::{CANVAS, LIST, Mode};
use std::collections::{BTreeMap, VecDeque};

#[derive(Default)]
pub(super) struct Plan(BTreeMap<&'static str, VecDeque<Mode>>);
impl Plan {
    pub(super) fn new(case: &str) -> Self {
        let rows = match case {
            "file-recovery" => vec![
                (CANVAS, VecDeque::from([Mode::Offline, Mode::Held])),
                (LIST, VecDeque::from([Mode::Malformed, Mode::Held])),
            ],
            "file-recovery-retirement" => {
                vec![(CANVAS, VecDeque::from([Mode::Held]))]
            }
            _ => vec![],
        };
        Self(rows.into_iter().collect())
    }
    pub(super) fn next(&mut self, id: &str) -> Mode {
        self.0
            .get_mut(id)
            .and_then(VecDeque::pop_front)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_metadata_is_per_id_and_consumes_only_original_attempts() {
        let mut p = Plan::new("file-recovery");
        assert!(matches!(p.next(CANVAS), Mode::Offline));
        assert!(matches!(p.next(LIST), Mode::Malformed));
        assert!(matches!(p.next(CANVAS), Mode::Held));
        assert!(matches!(p.next(LIST), Mode::Held));
        assert!(matches!(p.next(CANVAS), Mode::Success));
        assert!(matches!(p.next(LIST), Mode::Success));
    }
    #[test]
    fn retirement_holds_only_first_canvas_and_old_cases_stay_successful() {
        let mut p = Plan::new("file-recovery-retirement");
        assert!(matches!(p.next(CANVAS), Mode::Held));
        assert!(matches!(p.next(CANVAS), Mode::Success));
        assert!(matches!(p.next(LIST), Mode::Success));
        for case in ["file", "file-retirement", "foreign", ""] {
            let mut p = Plan::new(case);
            assert!(matches!(p.next(CANVAS), Mode::Success));
            assert!(matches!(p.next(LIST), Mode::Success));
        }
    }
}
