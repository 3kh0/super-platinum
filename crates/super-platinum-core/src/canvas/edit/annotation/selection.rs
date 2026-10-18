//! Read-only existing-control routing. No request or write permit is created.
use super::{AnnotationParagraph, Error};

impl AnnotationParagraph {
    /// First nonhidden positive visible-text intersection in the supported flat paragraph.
    /// Native routing must independently agree before an existing thread opens.
    pub fn existing_control(&self, start: usize, end: usize) -> Result<Option<&str>, Error> {
        if start >= end {
            return Err(Error::Unsupported);
        }
        let mut offset = 0usize;
        let mut start_valid = start == 0;
        let mut end_valid = false;
        let mut selected = None;
        for run in &self.runs {
            let run_start = offset;
            for scalar in run.text.chars() {
                offset = offset
                    .checked_add(scalar.len_utf16())
                    .ok_or(Error::Unsupported)?;
                start_valid |= offset == start;
                end_valid |= offset == end;
            }
            if selected.is_none() && run_start < end && start < offset {
                selected = run.control.as_deref().filter(|id| {
                    !self
                        .archived
                        .iter()
                        .chain(&self.hidden)
                        .any(|old| old == id)
                });
            }
        }
        if !start_valid || !end_valid || end > offset {
            return Err(Error::Unsupported);
        }
        Ok(selected)
    }
    /// Validate original native-node routing against canonical control identity
    /// and UTF-16 boundaries. Native evidence is not a write permit; absence of
    /// a native hit can admit a new range only when no canonical text overlaps.
    pub fn existing_native_control<'a>(
        &'a self,
        start: usize,
        end: usize,
        native: Option<&str>,
    ) -> Result<Option<&'a str>, Error> {
        let positive = self.existing_control(start, end)?;
        // Hidden but nonarchived stubs are openable only at the retained exact
        // character boundaries. Native containment remains independently checked.
        let exact = if positive.is_none() {
            let mut spans: Vec<(&str, usize, usize)> = Vec::new();
            let mut at = 0;
            for run in &self.runs {
                let next = at + run.text.encode_utf16().count();
                if let Some(id) = run.control.as_deref() {
                    if let Some(last) = spans.last_mut().filter(|s| s.0 == id && s.2 == at) {
                        last.2 = next;
                    } else {
                        spans.push((id, at, next));
                    }
                }
                at = next;
            }
            spans
                .into_iter()
                .find(|(id, a, z)| {
                    *a == start && *z == end && !self.archived.iter().any(|old| old == id)
                })
                .map(|s| s.0)
        } else {
            None
        };
        let positive = positive.or(exact);
        // Visible, nonempty flat annotation spans use Selection.containsNode in
        // Slack's active-thread branch. The separate hidden-removal predicate is
        // not routing authority. A touching edge alone cannot name a thread.
        if native != positive {
            return Err(Error::Changed);
        }
        Ok(positive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::edit::{
        annotation::AnnotationRun,
        text::{TextDraft, TextStyle},
    };

    fn sample() -> AnnotationParagraph {
        AnnotationParagraph {
            draft: TextDraft::default(),
            archived: Vec::new(),
            hidden: Vec::new(),
            runs: [
                ("ab", None),
                ("\u{1f600}", Some("old-a")),
                ("x", Some("old-a")),
                ("gap", None),
                ("last", Some("old-b")),
            ]
            .into_iter()
            .map(|(text, control)| AnnotationRun {
                text: text.into(),
                style: TextStyle::default(),
                control: control.map(str::to_owned),
            })
            .collect(),
        }
    }

    #[test]
    fn containing_partial_and_multiple_visible_intersections_reuse_first() {
        let value = sample();
        for (start, end) in [(2, 4), (1, 4), (4, 6), (0, 12)] {
            assert_eq!(value.existing_control(start, end).unwrap(), Some("old-a"));
        }
        assert_eq!(value.existing_control(8, 12).unwrap(), Some("old-b"));
        assert_eq!(value.existing_control(0, 2).unwrap(), None);
        assert_eq!(value.existing_control(5, 8).unwrap(), None);
    }

    #[test]
    fn native_half_surrogate_empty_reversed_and_out_of_bounds_refuse() {
        let value = sample();
        for (start, end) in [(3, 4), (2, 3), (2, 2), (5, 2), (0, 13)] {
            assert!(value.existing_control(start, end).is_err());
        }
    }
    #[test]
    fn native_text_edges_do_not_reuse_a_disjoint_control() {
        let value = sample();
        for (start, end) in [(5, 8), (1, 2)] {
            assert_eq!(value.existing_control(start, end).unwrap(), None);
            assert_eq!(
                value.existing_native_control(start, end, None).unwrap(),
                None
            );
            for control in ["old-a", "old-b"] {
                assert!(
                    value
                        .existing_native_control(start, end, Some(control))
                        .is_err()
                );
            }
        }
    }
    #[test]
    fn native_routing_refuses_foreign_disjoint_missing_or_reordered_control() {
        let value = sample();
        assert!(value.existing_native_control(2, 4, None).is_err());
        assert!(
            value
                .existing_native_control(5, 8, Some("foreign"))
                .is_err()
        );
        assert!(value.existing_native_control(0, 1, Some("old-a")).is_err());
        assert!(value.existing_native_control(1, 12, Some("old-b")).is_err());
        assert!(value.existing_native_control(3, 4, Some("old-a")).is_err());
        assert_eq!(
            value.existing_native_control(1, 12, Some("old-a")).unwrap(),
            Some("old-a")
        );
    }
    #[test]
    fn archived_controls_never_win_active_lookup_before_a_visible_thread() {
        let mut value = sample();
        value.archived.push("old-a".into());
        assert_eq!(value.existing_native_control(2, 4, None).unwrap(), None);
        assert!(value.existing_native_control(2, 4, Some("old-a")).is_err());
        assert_eq!(
            value.existing_native_control(0, 12, Some("old-b")).unwrap(),
            Some("old-b")
        );
    }
}
