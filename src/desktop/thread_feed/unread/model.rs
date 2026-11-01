//! Pure visual memory. It cannot authorize a read, navigation or mutation.
type Time = (u64, u64);

fn timestamp(raw: &str) -> Option<Time> {
    let (seconds, fraction) = raw.split_once('.')?;
    if seconds.is_empty()
        || !seconds.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_empty()
        || fraction.len() > 6
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let seconds = seconds.parse::<u64>().ok()?;
    if seconds > i64::MAX as u64 {
        return None;
    }
    Some((
        seconds,
        fraction.parse::<u64>().ok()? * 10u64.pow(6 - fraction.len() as u32),
    ))
}

#[derive(Clone, Default)]
pub(crate) struct State {
    initial: Option<(String, Time)>,
    last_read: Option<Time>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cue {
    pub(crate) index: usize,
    pub(crate) ghost: bool,
}

impl State {
    pub(crate) fn observe(
        &mut self,
        root: &str,
        current: &str,
        subscribed: bool,
        replies: &[&str],
    ) -> Option<Cue> {
        let root = timestamp(root)?;
        let read = timestamp(current)?;
        // Native loaded-reply subset. The Source before-root/older-context
        // branch and its minimum-visible/window rules remain separate.
        if root.0 == 0 || read < root || replies.len() > 100 {
            return None;
        }
        let times = replies
            .iter()
            .map(|s| timestamp(s))
            .collect::<Option<Vec<_>>>()?;
        if times.iter().any(|t| *t <= root) || times.windows(2).any(|p| p[0] >= p[1]) {
            return None;
        }
        // Native backward-read observation, not a claim about the Source
        // manuallyMarkedUnread producer or refreshId delivery timing.
        if self.last_read.is_some_and(|previous| read < previous) {
            self.initial = None;
        }
        self.last_read = Some(read);
        if self.initial.is_none() && times.last().is_some_and(|latest| *latest > read) {
            self.initial = Some((current.to_owned(), read));
        }
        let (initial_raw, initial) = self.initial.as_ref()?;
        if !subscribed {
            return None;
        }
        Some(Cue {
            index: times.iter().position(|t| t > initial)?,
            // Source distinguishes equal numeric timestamps with different
            // raw spelling here. Keep that distinction for the visual cue.
            ghost: current != initial_raw,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_reply_boundary_retains_read_advance_and_reseeds_backward_or_fresh_reads() {
        let replies = ["101.000000", "102.000000", "103.000000"];
        let mut state = State::default();
        assert_eq!(
            state.observe("100.000000", "101.000000", true, &replies),
            Some(Cue {
                index: 1,
                ghost: false
            })
        );
        assert_eq!(
            state.observe("100.000000", "103.000000", true, &replies),
            Some(Cue {
                index: 1,
                ghost: true
            })
        );
        assert_eq!(
            state.observe("100.000000", "103.000000", false, &replies),
            None
        );
        assert_eq!(
            state.observe("100.000000", "103.000000", true, &replies),
            Some(Cue {
                index: 1,
                ghost: true
            })
        );
        assert_eq!(
            state.observe("100.000000", "102.000000", true, &replies),
            Some(Cue {
                index: 2,
                ghost: false
            })
        );
        let mut fresh = State::default();
        assert_eq!(
            fresh.observe("100.000000", "103.000000", true, &replies),
            None
        );
        assert_eq!(
            fresh.observe(
                "100.000000",
                "103.000000",
                true,
                &["101.000000", "102.000000", "103.000000", "104.000000"]
            ),
            Some(Cue {
                index: 3,
                ghost: false
            })
        );
        for (cursor, values) in [
            ("missing", vec!["102.000000"]),
            ("99.000000", vec!["102.000000"]),
            ("101.000000", vec!["102.x"]),
            ("101.000000", vec!["102.000000", "102.000000"]),
            ("101.000000", vec!["103.000000", "102.000000"]),
            ("101.000000", vec!["100.000000"]),
        ] {
            assert_eq!(
                State::default().observe("100.000000", cursor, true, &values),
                None
            );
        }
        assert_eq!(
            State::default().observe("100.000000", "101.000000", true, &vec!["102.000000"; 101]),
            None
        );
        let mut spelling = State::default();
        assert_eq!(
            spelling.observe("100.0", "101.0", true, &replies),
            Some(Cue {
                index: 1,
                ghost: false
            })
        );
        assert_eq!(
            spelling.observe("100.0", "101.000000", true, &replies),
            Some(Cue {
                index: 1,
                ghost: true
            })
        );
    }
}
