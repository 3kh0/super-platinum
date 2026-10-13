//! Mixed cached channel fixture for the real native scroll/selection path.
pub(crate) fn fixture(shell: &mut crate::state::ShellState) {
    let mut bag = crate::thread_vm::fixture_bag(10_000);
    for (index, message) in bag.messages.iter_mut().enumerate() {
        message.ts = Some(format!("{}.000000", 1_719_800_400 + index * 60));
    }
    let unread = bag.messages[4_999].ts.clone().unwrap();
    bag.last_read = Some(unread.clone());
    bag.pending
        .push(bag.messages.last().unwrap().ts.clone().unwrap());
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .insert("C2".into(), bag);
    shell.core.active_channel = Some("C2".into());
    shell.core.pending_scroll_to = None;
    shell.unread_anchor = Some(("C2".into(), unread));
    shell.main_view = crate::state::MainView::Home;
    shell.thread_root = None;
    shell.stick_to_bottom = true;
    shell.refresh_from_core();
}
