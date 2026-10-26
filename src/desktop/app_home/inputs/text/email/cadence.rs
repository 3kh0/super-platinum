//! Email uses the original leading/trailing debounce; Text and Number stay trailing-only.
use super::*;
pub(super) struct Window {
    pub id: u64,
    pub binding: Binding,
    mount: u64,
    route: context::Witness,
    pub leading: Option<Queued>,
    pub trailing: Option<Queued>,
}
impl Window {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.binding.current(s)
            && self.route.current(s)
            && s.app_home
                .text
                .mounts
                .get(&(self.binding.form.clone(), self.binding.key.clone()))
                .is_some_and(|m| m.id == self.mount && m.binding.current(s))
    }
}
pub(super) fn enqueue(
    s: &mut ShellState,
    b: &Binding,
    mount: u64,
    event: super_platinum_core::block_actions::form::home::TextEvent,
) -> Option<u64> {
    let q = capture(s, b, mount, event)?;
    let id = q.id;
    let index = (b.form.clone(), b.key.clone());
    let stale = s
        .app_home
        .text
        .emails
        .get(&index)?
        .window
        .as_ref()
        .is_some_and(|w| !w.current(s));
    let w = s
        .app_home
        .text
        .emails
        .get_mut(&(b.form.clone(), b.key.clone()))?;
    if stale {
        w.window = None;
    }
    if let Some(window) = &mut w.window {
        window.id = id;
        window.trailing = Some(q);
    } else {
        w.window = Some(Window {
            id,
            binding: b.clone(),
            mount,
            route: q.route.clone(),
            leading: Some(q),
            trailing: None,
        });
    }
    Some(id)
}
pub(super) fn take_leading(
    s: &mut ShellState,
    index: &(FormOwner, InputKey),
    id: u64,
) -> Option<Queued> {
    s.app_home
        .text
        .emails
        .get_mut(index)?
        .window
        .as_mut()
        .filter(|w| w.id == id)?
        .leading
        .take()
}
pub(super) fn finish(s: &mut ShellState, index: &(FormOwner, InputKey), id: u64) -> Option<Queued> {
    let w = s.app_home.text.emails.get_mut(index)?;
    if w.window.as_ref()?.id != id {
        return None;
    }
    w.window.take()?.trailing
}
pub(in crate::app_home::inputs::text) fn launch(
    mut state: Signal<ShellState>,
    index: (FormOwner, InputKey),
    id: u64,
) {
    // Drain synchronously before another native occurrence can replace trailing.
    let leading = take_leading(&mut state.write(), &index, id);
    if let Some(q) = leading {
        dioxus::core::spawn_forever(delivery::deliver_queued(state, q));
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let q = finish(&mut state.write(), &index, id);
        if let Some(q) = q {
            delivery::deliver_queued(state, q).await;
        }
    });
}
