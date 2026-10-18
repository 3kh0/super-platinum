//! Canvas links use the same owned navigation as message links. A link event
//! cannot discard an editor or outlive the Canvas that rendered its anchor.
use super::*;

#[derive(Clone)]
struct Owner {
    epoch: u64,
    team: Option<String>,
    generation: u64,
    file: String,
}
impl Owner {
    fn current(&self, shell: &ShellState) -> bool {
        shell.core.account_epoch == self.epoch
            && shell.core.active_team == self.team
            && shell.shortcut_generation == self.generation
            && surface::visible(shell, &self.file)
    }
}

#[derive(Default)]
pub(super) struct Feedback {
    attempt: u64,
    notice: Option<(Owner, String)>,
}
impl Feedback {
    fn begin(&mut self) -> u64 {
        self.attempt = self.attempt.wrapping_add(1);
        self.notice = None;
        self.attempt
    }
    fn accept(&mut self, shell: &ShellState, owner: &Owner, attempt: u64, message: &str) {
        if self.attempt == attempt && owner.current(shell) {
            self.notice = Some((owner.clone(), message.into()));
        }
    }
    pub(super) fn visible(&self, shell: &ShellState) -> Option<&str> {
        self.notice
            .as_ref()
            .filter(|(owner, _)| owner.current(shell))
            .map(|(_, message)| message.as_str())
    }
}

pub(super) fn render(
    state: Signal<ShellState>,
    url: &str,
    content: Element,
    rename: Signal<Option<RenameDraft>>,
    text: Signal<Option<text::Draft>>,
    review: Signal<Option<checklist::recovery::review::Draft>>,
    comments_open: Signal<bool>,
    mut feedback: Signal<Feedback>,
) -> Element {
    let shell = state.read();
    let generation = shell.shortcut_generation;
    let file = shell.canvas.as_ref().map(|target| target.id.clone());
    let destination = url.to_owned();
    rsx! {a {href:url,
        onclick:move |event| {
            event.prevent_default();
            let mut state = state;
            let shell = state.peek();
            let owner = Owner {epoch:shell.core.account_epoch,team:shell.core.active_team.clone(),generation,file:file.clone().unwrap_or_default()};
            if !owner.current(&shell) { return; }
            let attempt = feedback.write().begin();
            let editing = rename.peek().is_some() || text.peek().is_some()
                || review.peek().is_some();
            let allowed = if *comments_open.peek() {
                // These anchors are unmounted when comments replace the body.
                // Reject a queued event from the old body, not comment reading.
                Err("This canvas view changed. Open the link from the current view.")
            } else {
                admission(&shell, generation, file.as_deref(), editing)
            };
            drop(shell);
            let receive: crate::slack_links::FailureFeedback = Some(std::rc::Rc::new(move |shell, message| {
                let mut feedback = feedback;
                if let Ok(mut current) = feedback.try_write() {
                    current.accept(shell, &owner, attempt, message);
                }
            }));
            if let Err(message) = allowed {
                let mut shell = state.write();
                shell.show_toast(message);
                receive.as_ref().unwrap()(&shell, message);
                return;
            }
            // Mail links are supported by the Canvas export but deliberately
            // outside the Slack route parser. Keep their existing OS handler.
            if mail_link(&destination) {
                if crate::media::open_external(&destination).is_err() {
                    let mut shell = state.write();
                    shell.show_toast("Could not open this link.");
                    receive.as_ref().unwrap()(&shell, "Could not open this link.");
                }
            } else {
                crate::slack_links::open_with_feedback(state, &destination, receive);
            }
        },
        {content}
    }}
}

fn mail_link(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|value| value.scheme() == "mailto")
}

fn admission(
    shell: &ShellState,
    generation: u64,
    file: Option<&str>,
    editing: bool,
) -> Result<(), &'static str> {
    if shell.shortcut_generation != generation
        || file.is_none_or(|file| !surface::visible(shell, file))
    {
        return Err("This canvas view changed. Open the link from the current view.");
    }
    if editing || !surface::can_leave(shell) {
        return Err("Finish or cancel your canvas edit before opening this link.");
    }
    if checklist::presentation(shell).is_some_and(|view| {
        view.operation
            .as_ref()
            .is_some_and(|(_, _, stage)| stage.blocks_refresh())
    }) {
        return Err("Wait for the canvas change to finish before opening this link.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_link_failure_belongs_to_current_canvas_and_latest_click() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.canvas = Some(CanvasTarget {
            id: "Ffixture".into(),
            title: "Canvas".into(),
            permalink: None,
        });
        shell.overlay = Some(Overlay::Canvas);
        let owner = Owner {
            epoch: shell.core.account_epoch,
            team: shell.core.active_team.clone(),
            generation: shell.shortcut_generation,
            file: "Ffixture".into(),
        };
        let mut feedback = Feedback::default();
        let first = feedback.begin();
        // An unrelated global notification is never projected into this region.
        shell.show_toast("Unrelated status");
        assert!(feedback.visible(&shell).is_none());
        feedback.accept(&shell, &owner, first, "Could not open this conversation.");
        assert_eq!(
            feedback.visible(&shell),
            Some("Could not open this conversation.")
        );
        let second = feedback.begin();
        feedback.accept(&shell, &owner, first, "Old failure");
        assert!(feedback.visible(&shell).is_none());
        feedback.accept(&shell, &owner, second, "Current failure");
        assert_eq!(feedback.visible(&shell), Some("Current failure"));
        shell.shortcut_generation += 1;
        feedback.accept(&shell, &owner, second, "Replaced visit failure");
        assert!(feedback.visible(&shell).is_none());
        shell.shortcut_generation = owner.generation;
        shell.canvas.as_mut().unwrap().id = "Freplacement".into();
        feedback.accept(&shell, &owner, second, "Replaced canvas failure");
        assert!(feedback.visible(&shell).is_none());
        shell.canvas.as_mut().unwrap().id = owner.file.clone();
        shell.core.account_epoch += 1;
        assert!(feedback.visible(&shell).is_none());
    }

    #[test]
    fn canvas_link_keeps_editor_and_rejects_stale_or_replaced_view() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.canvas = Some(CanvasTarget {
            id: "Ffixture".into(),
            title: "Canvas".into(),
            permalink: None,
        });
        shell.overlay = Some(Overlay::Canvas);
        let generation = shell.shortcut_generation;
        assert!(admission(&shell, generation, Some("Ffixture"), false).is_ok());
        assert!(admission(&shell, generation, Some("Ffixture"), true).is_err());
        surface::hold_local(&mut shell, generation);
        assert!(admission(&shell, generation, Some("Ffixture"), false).is_err());
        surface::release_local(&mut shell, generation);
        assert!(admission(&shell, generation, Some("Fother"), false).is_err());
        shell.shortcut_generation += 1;
        assert!(admission(&shell, generation, Some("Ffixture"), false).is_err());
        shell.overlay = None;
        assert!(admission(&shell, shell.shortcut_generation, Some("Ffixture"), false).is_err());
    }

    #[test]
    fn only_mailto_bypasses_the_shared_slack_dispatcher() {
        assert!(mail_link("mailto:person@example.com?subject=Canvas"));
        for url in [
            "https://example.com/mailto:person",
            "slack://channel?team=T1&id=C1",
            "javascript:alert(1)",
        ] {
            assert!(!mail_link(url));
        }
    }
}
