//! Renderer attention/visit admission, separate from core cursor decisions.
//! Matching R614311, S2 3986675, B2 29965159 establish attention and bubbling.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Editor {
    #[default]
    None,
    EmptyComposer,
    NonemptyComposer,
    Other,
}

/// Do not infer composer eligibility merely from a global Escape registration.
/// Integration must prove each opt-in against the matching editor/shortcut path.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ComposerPolicy {
    pub empty: bool,
    pub nonempty: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KeyEvent {
    pub escape: bool,
    pub shift: bool,
    pub control: bool,
    pub meta: bool,
    pub alt: bool,
    pub repeat: bool,
    pub composing: bool,
    pub prevented: bool,
    pub local_scope: bool,
    pub editor: Editor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Intent {
    Escape,
    ClearUnreads,
}

pub(crate) fn intent(event: KeyEvent, composers: ComposerPolicy) -> Option<Intent> {
    if !event.escape
        || event.control
        || event.meta
        || event.alt
        || event.repeat
        || event.composing
        || event.prevented
        || event.local_scope
    {
        return None;
    }
    let editor_allowed = match event.editor {
        Editor::None => true,
        Editor::EmptyComposer => composers.empty,
        Editor::NonemptyComposer => composers.nonempty,
        Editor::Other => false,
    };
    editor_allowed.then_some(if event.shift {
        Intent::ClearUnreads
    } else {
        Intent::Escape
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Focus {
    /// Includes the current surface's primary transcript and its proven input.
    Primary,
    /// Scope must match the mounted thread pane, not merely an open thread root.
    Thread(String),
    /// Rail/sidebar may use active primary channel; never close a background thread.
    Workspace,
    Other,
}

/// Non-secret renderer coordinates. CursorOwner separately guards full session,
/// transport and realtime generation; these fields cannot replace that owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub account: String,
    pub epoch: u64,
    pub team: String,
    pub primary_scope: Option<String>,
    pub thread_scope: Option<String>,
    pub focus: Focus,
    pub focus_generation: u64,
    pub overlay_generation: u64,
    pub focused: bool,
    pub visible: bool,
    pub chat_attention: bool,
    pub error_screen: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Primary(String),
    Thread(String),
    Workspace,
}

/// One key event captures one destination. No late fallback from a stale thread
/// to a newly active channel, and no re-selection after a credential await.
#[derive(Debug)]
pub(crate) struct Captured {
    snapshot: Snapshot,
    target: Target,
}

impl Captured {
    pub(crate) fn new(snapshot: Snapshot, intent: Intent) -> Option<Self> {
        if snapshot.account.is_empty()
            || snapshot.team.is_empty()
            || !snapshot.focused
            || !snapshot.visible
            || !snapshot.chat_attention
            || snapshot.error_screen
            || snapshot.focus == Focus::Other
        {
            return None;
        }
        let target = match intent {
            Intent::ClearUnreads => Target::Workspace,
            Intent::Escape => match &snapshot.focus {
                Focus::Thread(scope) => {
                    if snapshot.thread_scope.as_ref() != Some(scope) {
                        return None;
                    }
                    Target::Thread(scope.clone())
                }
                Focus::Primary | Focus::Workspace => {
                    Target::Primary(snapshot.primary_scope.clone()?)
                }
                Focus::Other => return None,
            },
        };
        Some(Self { snapshot, target })
    }

    /// Consume immediately before admission/dispatch. A confirmation dialog
    /// deliberately changes focus: create its separate owner-bound intent only
    /// after this capture is accepted, not by weakening the focus comparison.
    pub(crate) fn consume(self, current: &Snapshot) -> Option<Target> {
        (self.snapshot == *current).then_some(self.target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        Snapshot {
            account: "fixture-account".into(),
            epoch: 7,
            team: "T1".into(),
            primary_scope: Some("channel-visit-3".into()),
            thread_scope: Some("thread-visit-8".into()),
            focus: Focus::Primary,
            focus_generation: 1,
            overlay_generation: 0,
            focused: true,
            visible: true,
            chat_attention: true,
            error_screen: false,
        }
    }

    #[test]
    fn modifiers_and_editor_consumption_never_become_plain_escape() {
        let event = KeyEvent {
            escape: true,
            ..Default::default()
        };
        assert_eq!(
            intent(event, ComposerPolicy::default()),
            Some(Intent::Escape)
        );
        assert_eq!(
            intent(
                KeyEvent {
                    shift: true,
                    ..event
                },
                ComposerPolicy::default()
            ),
            Some(Intent::ClearUnreads)
        );
        for blocked in [
            KeyEvent {
                control: true,
                ..event
            },
            KeyEvent {
                meta: true,
                ..event
            },
            KeyEvent { alt: true, ..event },
            KeyEvent {
                composing: true,
                ..event
            },
            KeyEvent {
                repeat: true,
                ..event
            },
            KeyEvent {
                prevented: true,
                ..event
            },
            KeyEvent {
                local_scope: true,
                ..event
            },
            KeyEvent {
                editor: Editor::EmptyComposer,
                ..event
            },
            KeyEvent {
                editor: Editor::NonemptyComposer,
                ..event
            },
            KeyEvent {
                editor: Editor::Other,
                ..event
            },
        ] {
            assert_eq!(intent(blocked, ComposerPolicy::default()), None);
        }
    }

    #[test]
    fn proven_canonical_composers_do_not_grant_other_editors_authority() {
        let policy = ComposerPolicy {
            empty: true,
            nonempty: true,
        };
        for editor in [Editor::EmptyComposer, Editor::NonemptyComposer] {
            assert_eq!(
                intent(
                    KeyEvent {
                        escape: true,
                        editor,
                        ..Default::default()
                    },
                    policy
                ),
                Some(Intent::Escape)
            );
            assert_eq!(
                intent(
                    KeyEvent {
                        escape: true,
                        shift: true,
                        editor,
                        ..Default::default()
                    },
                    policy
                ),
                Some(Intent::ClearUnreads)
            );
            assert_eq!(
                intent(
                    KeyEvent {
                        escape: true,
                        editor,
                        prevented: true,
                        ..Default::default()
                    },
                    policy
                ),
                None
            );
        }
        assert_eq!(
            intent(
                KeyEvent {
                    escape: true,
                    editor: Editor::Other,
                    ..Default::default()
                },
                policy
            ),
            None
        );
    }

    #[test]
    fn only_focused_thread_consumes_thread_escape() {
        let mut current = snapshot();
        assert_eq!(
            Captured::new(current.clone(), Intent::Escape)
                .unwrap()
                .consume(&current),
            Some(Target::Primary("channel-visit-3".into()))
        );
        current.focus = Focus::Thread("thread-visit-8".into());
        assert_eq!(
            Captured::new(current.clone(), Intent::Escape)
                .unwrap()
                .consume(&current),
            Some(Target::Thread("thread-visit-8".into()))
        );
        current.focus = Focus::Thread("old-thread".into());
        assert!(Captured::new(current, Intent::Escape).is_none());
    }

    #[test]
    fn stale_key_cannot_follow_navigation_focus_or_account_rotation() {
        let original = snapshot();
        let mut changes = Vec::new();
        let mut changed = original.clone();
        changed.epoch += 1;
        changes.push(changed);
        let mut changed = original.clone();
        changed.team = "T2".into();
        changes.push(changed);
        let mut changed = original.clone();
        changed.focus_generation += 1;
        changes.push(changed);
        let mut changed = original.clone();
        changed.overlay_generation += 1;
        changes.push(changed);
        let mut changed = original.clone();
        changed.primary_scope = Some("channel-visit-4".into());
        changes.push(changed);
        let mut changed = original.clone();
        changed.focused = false;
        changes.push(changed);
        for current in changes {
            assert!(
                Captured::new(original.clone(), Intent::Escape)
                    .unwrap()
                    .consume(&current)
                    .is_none()
            );
        }
    }

    #[test]
    fn bulk_is_workspace_scoped_even_without_a_loaded_channel() {
        let mut current = snapshot();
        current.primary_scope = None;
        current.focus = Focus::Workspace;
        assert!(Captured::new(current.clone(), Intent::Escape).is_none());
        assert_eq!(
            Captured::new(current.clone(), Intent::ClearUnreads)
                .unwrap()
                .consume(&current),
            Some(Target::Workspace)
        );
        current.chat_attention = false;
        assert!(Captured::new(current, Intent::ClearUnreads).is_none());
    }
}
