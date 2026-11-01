//! Short closed-window memory for passive browser intent, never result authority.
//! Current nSyU's permanent Channels path declares300000ms retention; FZxm's
//! retained-instance branch preserves its fields absent explicit overrides.
//! Native reentry deliberately reserves a fresh page1 rather than retaining
//! result/pager/selection/readiness. No persistent or Slack preference write.
use super::*;

pub(super) const WINDOW: Duration = Duration::from_secs(300);
#[derive(Default)]
pub(super) struct Retained {
    saved: Option<Saved>,
}
struct Saved {
    settings: Settings,
    expires: Instant,
}
pub(super) struct Settings {
    draft: String,
    edited: Option<String>,
    kind: Kind,
    browse_sort: Option<Sort>,
    search_sort: Option<Sort>,
}
impl Settings {
    pub(super) fn apply(self, d: &mut State) {
        d.draft = self.draft;
        d.edited = self.edited;
        d.kind = self.kind;
        d.browse_sort = self.browse_sort;
        d.search_sort = self.search_sort;
    }
}
impl Retained {
    fn remember_at(&mut self, d: &State, now: Instant) {
        // Match the existing native query bound. Oversized/invalid input must
        // not grow a retained per-workspace allocation after the dialog closes.
        self.saved = if d.draft.len() <= 4096 && !d.draft.contains('\0') {
            now.checked_add(WINDOW).map(|expires| Saved {
                expires,
                settings: Settings {
                    draft: d.draft.clone(),
                    // Independently bounded passive display text. It never
                    // replaces the query of the new visit's initial read.
                    edited: d
                        .edited
                        .as_ref()
                        .filter(|v| v.len() <= 4096 && !v.contains('\0'))
                        .cloned(),
                    kind: d.kind,
                    browse_sort: d.browse_sort,
                    search_sort: d.search_sort,
                },
            })
        } else {
            None
        };
    }
    pub(super) fn take_at(&mut self, now: Instant) -> Option<Settings> {
        self.saved
            .take()
            .filter(|s| now < s.expires)
            .map(|s| s.settings)
    }
}

pub(crate) fn remember_on_close(s: &ShellState) {
    let d = &s.channel_directory;
    let Some(entry) = d.entry.as_ref().filter(|e| e.current(s)) else {
        return;
    };
    // Independent from rate-limit/canonical state. Contention/poisoning simply
    // skips this optional memory and never blocks the renderer.
    if let Ok(mut value) = entry.family.retained.try_lock() {
        value.remember_at(d, Instant::now());
    }
}
