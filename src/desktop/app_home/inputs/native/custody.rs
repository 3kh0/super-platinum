//! Received calendar progress is a retry hint, never DOM lease authority.
use super::*;
mod progress;

pub(crate) struct Watermark {
    owner: FormOwner,
    rows: Vec<(InputKey, u64, u64, u64, String)>,
}
impl Watermark {
    pub(crate) fn capture(s: &ShellState, visit: actions::Key) -> Option<Self> {
        if !snapshot_current(s, visit) {
            return None;
        }
        let owner = owner(s)?;
        let form = s.app_home.forms.get(&owner)?;
        if !ready(s, &owner, form) {
            return None;
        }
        let mut rows = Vec::new();
        for (key, field) in &form.fields {
            if !calendar(&field.kind) {
                continue;
            }
            let e = s
                .app_home
                .native
                .entries
                .get(&(owner.clone(), key.clone()))?;
            // Reject malformed received buffers independently of presentation.
            if matches!(field.kind, InputKind::DateTime) {
                if !datetime::settled(s, &owner, key, e) || e.value.len() > 8192 {
                    return None;
                }
            } else if matches!(field.kind, InputKind::Time { .. }) {
                if !time::settled(s, &owner, key, e) || e.value.len() > 4096 {
                    return None;
                }
            } else if matches!(field.kind, InputKind::Date) {
                if !date::settled(s, &owner, key, e) || e.value.len() > 8192 {
                    return None;
                }
            } else {
                typed(&field.kind, &e.value).ok()?;
            }
            rows.push((key.clone(), e.id, e.mount, e.seq, e.value.clone()));
        }
        Some(Self { owner, rows })
    }
    pub(crate) fn progress_from(&self, old: &Self) -> Option<bool> {
        if self.owner != old.owner {
            return None;
        }
        progress::compare(&old.rows, &self.rows)
    }
}
