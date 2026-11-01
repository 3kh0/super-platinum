//! Literal native input adapter: edits are separate from the committed page.
//! Source channel-explorer queryOps have their own lifetime. The native existing
//! owner-bound300s/4096 memory is a finite passive policy, not global-store parity.
use super::*;

// Source uses ECMAScript String.trim, whose set differs from Rust str::trim.
// In particular U+0085 is not included and U+FEFF is included.
pub(super) fn empty_edit(text: &str) -> bool {
    text.chars().all(|c| {
        matches!(c,
            '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
            '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
            '\u{205f}' | '\u{3000}' | '\u{feff}'
        )
    })
}

impl Entry {
    pub(super) fn edit_input(&self, mut state: Signal<ShellState>, text: String) {
        let Ok(mut s) = state.try_write() else { return };
        if !self.current(&s) {
            return;
        }
        // A nonempty committed query positively supplies Source canClear,
        // independently of unknown initial-filter equality. Do not infer that
        // guard for an already-empty query and custom filters. Preserve the
        // native busy fence: a local edit cannot retire a held request to make
        // another dispatch appear idle.
        let clear = empty_edit(&text)
            && !s.channel_directory.draft.is_empty()
            && !s.channel_directory.busy();
        let d = &mut s.channel_directory;
        d.edited = Some(text);
        if clear {
            d.revision = d.revision.wrapping_add(1);
            d.draft.clear();
            d.kind = Kind::default();
            d.selected = None;
            d.failed = None;
            d.error = None;
            // Keep the literal local edit, including whitespace. The Source
            // auto-empty callback does not itself erase local ops. Its final
            // external reset/effect synchronization is separately qualified.
        }
        drop(s);
        if clear {
            self.start(state, read::Mode::Search);
        }
    }

    pub(super) fn submit_input(&self, mut state: Signal<ShellState>) {
        let Ok(mut s) = state.try_write() else { return };
        // Check before taking the edit or bumping revision. Otherwise the
        // revision mismatch would hide the pending ticket from busy().
        if !self.current(&s) || s.channel_directory.busy() {
            return;
        }
        let d = &mut s.channel_directory;
        if let Some(text) = d.edited.take() {
            if text != d.draft {
                d.revision = d.revision.wrapping_add(1);
                d.draft = text;
                d.selected = None;
                d.failed = None;
                d.error = None;
            }
        }
        // Existing begin reserves synchronously. Equal text never bypasses its
        // pending-peer guard, and no nested Signal write can lose the dispatch.
        drop(s);
        self.start(state, read::Mode::Search);
    }
}
