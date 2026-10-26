//! Explicit Buttons retain their own immutable action/form authority. This
//! module adds only final native text custody to the existing calendar lease.
use super::*;
pub(crate) mod fixture;
#[cfg(test)]
mod tests;

pub(crate) fn needed(s: &ShellState) -> bool {
    form(s).is_some_and(|f| {
        f.fields
            .keys()
            .any(|k| dispatching(s, k) || number::supported(s, k) || email::supported(s, k))
    })
}

pub(crate) enum Lease {
    Legacy(dioxus::document::Eval),
    Mixed {
        text: bridge::TextLease,
        calendar: super::super::native::Release,
    },
    Static {
        selection: super::super::static_select::ButtonLease,
        siblings: Box<Lease>,
    },
}
impl Lease {
    pub(crate) fn current(&self, s: &ShellState, visit: actions::Key) -> bool {
        match self {
            Self::Legacy(_) => true,
            Self::Mixed { text, .. } => text.current_visit(s, visit),
            Self::Static {
                selection,
                siblings,
            } => selection.current_visit(s, visit) && siblings.current(s, visit),
        }
    }
    pub(crate) fn short(&self) -> bool {
        matches!(self, Self::Mixed { .. } | Self::Static { .. })
    }
    pub(crate) fn release(&self) {
        match self {
            Self::Legacy(eval) => {
                let _ = eval.send(());
            }
            Self::Mixed { text, calendar } => {
                text.release();
                calendar.release();
            }
            Self::Static {
                selection,
                siblings,
            } => {
                selection.release();
                siblings.release();
            }
        }
    }
}
pub(crate) async fn lock(state: Signal<ShellState>, visit: actions::Key) -> Option<Lease> {
    let static_needed = form(&state.peek()).is_some_and(|f| {
        f.fields.keys().any(|k| {
            super::super::static_select::dispatching(&state.peek(), k)
                || super::super::static_select::checkboxes::supported(&state.peek(), k)
                || super::super::static_select::radio::supported(&state.peek(), k)
        })
    });
    if static_needed {
        let selection = super::super::static_select::lock_for_button(state, visit).await?;
        let Some(siblings) = lock_snapshot(state, visit).await else {
            selection.release();
            return None;
        };
        return Some(Lease::Static {
            selection,
            siblings: Box::new(siblings),
        });
    }
    if !needed(&state.peek())
        && !super::super::native::datetime::active(&state.peek())
        && !super::super::native::date::active(&state.peek())
        && !super::super::native::time::active(&state.peek())
    {
        return super::super::native::lock(state, visit)
            .await
            .map(Lease::Legacy);
    }
    lock_snapshot(state, visit).await
}

// A failed native check can race a newer received occurrence. Retain the
// attempted serial snapshot so a later Busy probe cannot erase that evidence.
pub(super) struct RetrySnapshot {
    owner: Owner,
    route: context::Witness,
    visit: actions::Key,
    raw: serde_json::Value,
    revision: u64,
    text: serde_json::Value,
    calendar: super::super::native::custody::Watermark,
}
impl RetrySnapshot {
    pub(super) fn capture(s: &ShellState, visit: actions::Key) -> Option<Self> {
        Some(Self {
            owner: s.app_home.surface.as_ref()?.owner.clone(),
            route: context::Witness::capture(s).ok()?,
            visit,
            raw: form(s)?.raw_view().clone(),
            revision: super::super::revision(s)?,
            text: bridge::expected_visit(s, visit)?,
            calendar: super::super::native::custody::Watermark::capture(s, visit)?,
        })
    }
    pub(super) fn progress(&self, s: &ShellState) -> Option<bool> {
        if !self.owner.current(s) || !self.route.current(s) {
            return None;
        }
        let Some(now) = Self::capture(s, self.visit) else {
            return None;
        };
        if now.raw != self.raw {
            return None;
        }
        let Some(mut progress) = now.calendar.progress_from(&self.calendar) else {
            return None;
        };
        let (Some(old), Some(new)) = (
            self.text["fields"].as_array(),
            now.text["fields"].as_array(),
        ) else {
            return None;
        };
        if old.len() != new.len() {
            return None;
        }
        for (a, b) in old.iter().zip(new) {
            if a["id"] != b["id"] || a["mount"] != b["mount"] {
                return None;
            }
            if a["mount"].is_null() {
                if a["value"] != b["value"] {
                    if now.revision <= self.revision {
                        return None;
                    }
                    progress = true;
                }
            } else {
                let (Some(before), Some(after)) = (a["seq"].as_u64(), b["seq"].as_u64()) else {
                    return None;
                };
                if after < before || (after == before && a["value"] != b["value"]) {
                    return None;
                }
                progress |= after > before;
            }
        }
        Some(progress)
    }
    pub(super) fn progressed(&self, s: &ShellState) -> bool {
        self.progress(s) == Some(true)
    }
}

/// Shared short sibling custody for an owned Home control invocation.
pub(crate) async fn lock_snapshot(state: Signal<ShellState>, visit: actions::Key) -> Option<Lease> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if !snapshot_current(&state.peek(), visit) || Instant::now() >= deadline {
            return None;
        }
        let attempted = RetrySnapshot::capture(&state.peek(), visit)?;
        match bridge::lock_visit(state, visit).await {
            bridge::Lock::Refused => {
                if !attempted.progressed(&state.peek()) {
                    return None;
                }
            }
            bridge::Lock::Busy => {
                attempted.progress(&state.peek())?;
            }
            bridge::Lock::Acquired(text) => {
                if let Some(calendar) = super::super::native::lock_acknowledged(state, visit).await
                {
                    return Some(Lease::Mixed { text, calendar });
                }
                let busy = super::super::native::lease_busy(state, visit).await;
                text.release();
                let progress = attempted.progress(&state.peek())?;
                if !busy && !progress {
                    return None;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
