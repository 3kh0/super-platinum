//! Display metadata is borrowed from an original Canvas-owned observation.
use super::super::attribution::{self as observed, Interaction, Navigation, Presentation};
use super::*;
use crate::media::MediaUri;
use super_platinum_core::canvas::comments::attribution::Name;

// ShellState crosses worker threads during bootstrap. Renderer signals must not.
// Each handle names one exact entry in a bounded main-thread slot; no handle
// can resolve a successor merely because its member ID happens to be equal.
use std::{cell::RefCell, sync::Arc};
#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotKind {
    Hover,
    Profile,
}
impl SlotKind {
    fn index(self) -> usize {
        match self {
            Self::Hover => 0,
            Self::Profile => 1,
        }
    }
}
struct Slots<T> {
    entries: [Option<(Arc<()>, T)>; 2],
}
impl<T> Default for Slots<T> {
    fn default() -> Self {
        Self {
            entries: [None, None],
        }
    }
}
impl<T> Slots<T> {
    fn replace(&mut self, kind: SlotKind, value: T) -> Arc<()> {
        let identity = Arc::new(());
        self.entries[kind.index()] = Some((identity.clone(), value));
        identity
    }
    fn get(&self, kind: SlotKind, identity: &Arc<()>) -> Option<&T> {
        self.entries[kind.index()]
            .as_ref()
            .filter(|(original, _)| Arc::ptr_eq(original, identity))
            .map(|(_, value)| value)
    }
    fn remove(&mut self, kind: SlotKind, identity: &Arc<()>) {
        if self.get(kind, identity).is_some() {
            self.entries[kind.index()] = None;
        }
    }
}
#[derive(Clone)]
struct Local {
    interaction: Interaction,
    model: Signal<State>,
    navigation: Option<Navigation>,
}
thread_local! {
    static LOCAL: RefCell<Slots<Local>> = RefCell::new(Slots::default());
}
#[derive(Clone)]
pub(in crate::canvas) struct ProfileToken {
    identity: Arc<()>,
    user: String,
    kind: SlotKind,
}
impl ProfileToken {
    fn install(kind: SlotKind, local: Local) -> Self {
        let user = local.interaction.id().to_owned();
        let identity = LOCAL.with(|slots| slots.borrow_mut().replace(kind, local));
        Self {
            identity,
            user,
            kind,
        }
    }
    fn local(&self) -> Option<Local> {
        // Clone before returning so no RefCell borrow spans a navigation or
        // signal write (which can clear these same slots synchronously).
        LOCAL.with(|slots| slots.borrow().get(self.kind, &self.identity).cloned())
    }
    fn clear(&self) {
        LOCAL.with(|slots| slots.borrow_mut().remove(self.kind, &self.identity));
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.kind == SlotKind::Profile
            && shell.profile_user.as_deref() == Some(self.user.as_str())
            && self.local().is_some_and(|local| {
                local.model.try_read().is_ok_and(|model| {
                    local
                        .navigation
                        .as_ref()
                        .is_some_and(|n| n.current(shell, &model))
                })
            })
    }
}
#[derive(Clone)]
pub(crate) struct HoverToken(ProfileToken);
impl HoverToken {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.0.kind == SlotKind::Hover
            && shell
                .canvas_checklist
                .comment_profile_hover
                .as_ref()
                .is_some_and(|v| Arc::ptr_eq(&v.identity, &self.0.identity))
            && shell
                .profile_hover
                .as_ref()
                .is_some_and(|v| v.user_id == self.0.user)
            && self.0.local().is_some_and(|local| {
                local
                    .model
                    .try_read()
                    .is_ok_and(|model| local.interaction.current(shell, &model))
            })
    }
    pub(crate) fn open(&self, state: Signal<ShellState>) {
        let current = self.current(&state.read());
        if current && let Some(local) = self.0.local() {
            open(state, local.model, &local.interaction);
        }
    }
}
pub(crate) fn profile_hover_token(
    shell: &ShellState,
    hover: &crate::state::ProfileHoverVm,
) -> Option<HoverToken> {
    shell
        .canvas_checklist
        .comment_profile_hover
        .as_ref()
        .filter(|v| v.user == hover.user_id)
        .cloned()
        .map(HoverToken)
}
pub(crate) fn clear_profile_hover_token(shell: &mut ShellState) {
    if let Some(token) = shell.canvas_checklist.comment_profile_hover.take() {
        token.clear();
    }
}
pub(crate) fn clear_profile_token(shell: &mut ShellState) {
    if let Some(token) = shell.canvas_checklist.comment_profile.take() {
        token.clear();
    }
}
pub(crate) fn profile_active(shell: &ShellState) -> bool {
    shell.overlay == Some(crate::state::Overlay::Canvas)
        && shell.canvas_checklist.comment_profile.is_some()
}
pub(crate) fn profile_overlay(mut state: Signal<ShellState>, shell: &ShellState) -> Element {
    if !shell
        .canvas_checklist
        .comment_profile
        .as_ref()
        .is_some_and(|v| v.current(shell))
    {
        return rsx! {};
    }
    rsx! {
        div {id:"canvas-comment-profile",class:"canvas-comment-profile",tabindex:-1,
            "data-keyboard-dialog":"true","data-escape-local":"true",
            onmounted:move |_| {dioxus::document::eval(include_str!("attribution_focus.js"));},
            onkeydown:move |event| {if event.key()==Key::Escape {event.prevent_default();event.stop_propagation();state.write().close_profile();}},
            {crate::profile::profile_pane(state,shell)}
        }
    }
}
fn open(mut state: Signal<ShellState>, model: Signal<State>, interaction: &Interaction) {
    let user = {
        let mut shell = state.write();
        let navigation = {
            let Ok(value) = model.try_read() else {
                return;
            };
            let Some(navigation) = interaction.navigation(&shell, &value) else {
                return;
            };
            navigation
        };
        let user = interaction.id().to_owned();
        crate::bootstrap::show_profile(&mut shell, user.clone());
        shell.canvas_checklist.comment_profile = Some(ProfileToken::install(
            SlotKind::Profile,
            Local {
                interaction: interaction.clone(),
                model,
                navigation: Some(navigation),
            },
        ));
        user
    };
    spawn(crate::bootstrap::refresh_open_profile(state, user));
}
fn hover(
    mut state: Signal<ShellState>,
    model: Signal<State>,
    interaction: &Interaction,
    event: MouseEvent,
) {
    let current = model
        .try_read()
        .is_ok_and(|value| interaction.current(&state.read(), &value));
    if !current {
        return;
    }
    let point = event.data().client_coordinates();
    crate::profile::show_profile_hover(state, interaction.id().to_owned(), point.x, point.y);
    state.write().canvas_checklist.comment_profile_hover = Some(ProfileToken::install(
        SlotKind::Hover,
        Local {
            interaction: interaction.clone(),
            model,
            navigation: None,
        },
    ));
}
pub(super) fn header(
    state: Signal<ShellState>,
    model: Signal<State>,
    raw: &Message,
    body: &crate::model::MessageVm,
) -> Element {
    let presentation = observed::presentation(&state.read(), &model.read(), raw);
    let timestamp = body.timestamp.clone();
    match presentation {
        Presentation::Hidden => {
            rsx! {header {"data-canvas-attribution":"hidden",time {"{timestamp}"}}}
        }
        Presentation::Pending => {
            rsx! {header {"data-canvas-attribution":"pending",span {role:"status","Loading editor..."}time {"{timestamp}"}}}
        }
        Presentation::Unavailable { notice, retryable } => {
            rsx! {header {"data-canvas-attribution":"unavailable",
                span {role:"status","{notice}"}
                if retryable {button {class:"canvas-attribution-retry",onclick:move |_|observed::retry(state,model),"Retry editor"}}
                time {"{timestamp}"}
            }}
        }
        Presentation::Ready {
            id,
            name,
            avatar,
            initials,
            interaction,
        } => {
            let (label, kind) = match name {
                Name::Known(label) => (label, "known"),
                Name::Unknown => ("Unknown member".into(), "unknown"),
                Name::Nonexistent => ("Member unavailable".into(), "nonexistent"),
            };
            let ready_avatar = avatar.filter(|v| state.read().media.is_ready(v));
            let epoch = state.read().media_epoch;
            rsx! {header {class:"canvas-attribution","data-canvas-attribution":kind,"data-editor-id":id.clone(),
                if let Some(interaction)=interaction {
                    button {class:"avatar canvas-attribution-avatar",tabindex:-1,"aria-hidden":"true",r#type:"button",
                        onmouseenter:{let i=interaction.clone();move |e|hover(state,model,&i,e)},
                        onmouseleave:move |_|crate::profile::schedule_profile_hover_close(state),
                        onclick:{let i=interaction.clone();move |_|open(state,model,&i)},
                        if let Some(avatar)=ready_avatar {img {src:avatar.webview_uri_at(epoch),alt:""}} else {"{initials}"}
                    }
                    button {class:"canvas-attribution-name",r#type:"button","data-message-sender":id.clone(),
                        ondoubleclick:|e|e.stop_propagation(),
                        onmouseenter:{let i=interaction.clone();move |e|hover(state,model,&i,e)},
                        onmouseleave:move |_|crate::profile::schedule_profile_hover_close(state),
                        onclick:move |_|open(state,model,&interaction),
                        "{label}"
                    }
                } else {span {class:"canvas-attribution-name","{label}"}}
                time {"{timestamp}"}
            }}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_handles_remain_send_without_renderer_signals() {
        fn assert_send<T: Send>() {}
        assert_send::<ProfileToken>();
        assert_send::<HoverToken>();
        assert_send::<ShellState>();
    }

    #[test]
    fn same_user_successor_cannot_be_resolved_or_cleared_by_old_handle() {
        let mut slots = Slots::default();
        let old = slots.replace(SlotKind::Hover, "UATTR0001");
        let successor = slots.replace(SlotKind::Hover, "UATTR0001");
        assert!(slots.get(SlotKind::Hover, &old).is_none());
        assert!(slots.get(SlotKind::Profile, &successor).is_none());
        slots.remove(SlotKind::Hover, &old);
        assert_eq!(slots.get(SlotKind::Hover, &successor), Some(&"UATTR0001"));
        slots.remove(SlotKind::Hover, &successor);
        assert!(slots.get(SlotKind::Hover, &successor).is_none());
    }

    #[test]
    fn hover_and_profile_slots_are_independent_and_bounded() {
        let mut slots = Slots::default();
        let hover = slots.replace(SlotKind::Hover, "hover");
        let profile = slots.replace(SlotKind::Profile, "profile");
        assert_eq!(slots.entries.iter().flatten().count(), 2);
        slots.remove(SlotKind::Profile, &hover);
        assert_eq!(slots.get(SlotKind::Profile, &profile), Some(&"profile"));
        slots.remove(SlotKind::Profile, &profile);
        assert_eq!(slots.get(SlotKind::Hover, &hover), Some(&"hover"));
        assert_eq!(slots.entries.iter().flatten().count(), 1);
    }
}
