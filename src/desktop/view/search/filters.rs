//! Shared handlers retain their original typed/legacy branch and draft origin.
use super::*;
use crate::state::{search_filter_types as files, search_message_scopes as messages};
#[derive(Clone)]
pub(super) enum Source {
    Generic(Option<messages::fallback::Source>),
    Files(Option<files::Source>),
    Messages(Option<messages::Source>),
}
impl Source {
    pub(super) fn capture(s: &ShellState) -> Self {
        if s.search_pager.kind == SearchKind::Files {
            Self::Files(files::Source::capture(s))
        } else if messages::typed_panel(s) {
            Self::Messages(messages::Source::capture(s))
        } else {
            Self::Generic(messages::fallback::Source::capture(s))
        }
    }
    pub(super) fn allows(&self, s: &ShellState) -> bool {
        match self {
            Self::Files(Some(source)) => files::allows(s, Some(source)),
            Self::Messages(Some(source)) => source.current(s),
            Self::Generic(source) => {
                s.overlay == Some(crate::state::Overlay::Search)
                    && s.search_pager.kind == SearchKind::Messages
                    && !s.search_history_open
                    && s.search_filters.is_some()
                    && files::allows(s, None)
                    && messages::generic_allowed(s)
                    && match source {
                        Some(source) => source.current(s),
                        None => !messages::fallback::has_draft(s),
                    }
            }
            _ => false,
        }
    }
    pub(super) fn key(&self) -> Option<&str> {
        match self {
            Self::Files(Some(s)) => Some(s.key()),
            Self::Messages(Some(s)) => Some(s.key()),
            Self::Generic(Some(s)) => Some(s.key()),
            _ => None,
        }
    }
}
#[derive(Clone)]
pub(super) enum Toggle {
    Messages(messages::Toggle),
    Other(files::Toggle),
}
impl Toggle {
    pub(super) fn capture(s: &ShellState) -> Self {
        if messages::wants(s) {
            Self::Messages(messages::Toggle::capture(s))
        } else {
            Self::Other(files::Toggle::capture(s))
        }
    }
}
pub(super) fn toggle(s: &mut ShellState, source: Toggle) {
    match source {
        Toggle::Messages(source) => messages::toggle(s, source),
        Toggle::Other(source) => {
            if s.search_pager.kind == SearchKind::Files || messages::generic_allowed(s) {
                let before = (s.search_filters.is_some(), s.search_history_open);
                files::toggle(s, source);
                if s.search_pager.kind == SearchKind::Messages
                    && before != (s.search_filters.is_some(), s.search_history_open)
                {
                    messages::fallback::after_toggle(s);
                }
            }
        }
    }
}
pub(super) fn submit(mut state: Signal<ShellState>, source: Source) {
    let allowed = {
        let s = state.peek();
        source.allows(&s)
    };
    if !allowed {
        return;
    }
    match source {
        Source::Files(Some(source)) => super::file_types::submit(state, Some(source), false),
        Source::Messages(Some(source)) => {
            let submission = messages::apply(&mut state.write(), &source);
            if let Some(submission) = submission {
                #[cfg(test)]
                let _io = try_consume_context::<
                    std::rc::Rc<crate::bootstrap::search_message_scope_io::Io>,
                >()
                .expect("Messages scope UI tests require scoped offline IO");
                spawn(async move {
                    if !submission.current(&state.peek()) {
                        return;
                    }
                    if submission.unchanged() {
                        if crate::bootstrap::search_message_pages::refresh_current(state).await {
                            return;
                        }
                        if !submission.current(&state.peek()) {
                            return;
                        }
                    }
                    crate::bootstrap::search(state).await;
                });
            }
        }
        Source::Generic(Some(source)) => {
            let submission = messages::fallback::apply(&mut state.write(), &source);
            submit_fallback(state, submission);
        }
        Source::Generic(None) => super::file_types::submit(state, None, true),
        _ => {}
    }
}
pub(super) fn clear(mut state: Signal<ShellState>, source: Source) {
    let allowed = {
        let s = state.peek();
        source.allows(&s)
    };
    if !allowed {
        return;
    }
    match source {
        Source::Files(Some(source)) => super::file_types::clear(state, Some(source), false),
        Source::Messages(Some(source)) => messages::clear(&mut state.write(), &source, None, true),
        Source::Generic(Some(source)) => {
            let submission = messages::fallback::clear(&mut state.write(), &source);
            submit_fallback(state, submission);
        }
        Source::Generic(None) => super::file_types::clear(state, None, true),
        _ => {}
    }
}

fn submit_fallback(state: Signal<ShellState>, submission: Option<messages::Submission>) {
    if let Some(submission) = submission {
        #[cfg(test)]
        let _io =
            try_consume_context::<std::rc::Rc<crate::bootstrap::search_message_scope_io::Io>>()
                .expect("Cold Messages filter UI tests require scoped offline IO");
        spawn(async move {
            if !submission.current(&state.peek()) {
                return;
            }
            if submission.unchanged() {
                if crate::bootstrap::search_message_pages::refresh_current(state).await {
                    return;
                }
                if !submission.current(&state.peek()) {
                    return;
                }
            }
            crate::bootstrap::search(state).await;
        });
    }
}
