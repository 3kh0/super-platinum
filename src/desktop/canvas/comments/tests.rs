use super::*;
fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Fixture".into(),
        permalink: None,
    });
    shell.overlay = Some(Overlay::Canvas);
    shell
}
fn ticket(shell: &ShellState) -> Ticket {
    Ticket {
        owner: Owner::capture(shell).unwrap(),
        serial: 1,
        query: Query::Stream(Filter::Recent),
        cursor: None,
        direction: Direction::First,
    }
}
fn model(ticket: &Ticket) -> State {
    State {
        owner: Some(ticket.owner.clone()),
        serial: ticket.serial,
        query: Some(ticket.query.clone()),
        ..Default::default()
    }
}
#[test]
fn stale_account_epoch_cannot_adopt_comments() {
    let mut shell = shell();
    let ticket = ticket(&shell);
    let model = model(&ticket);
    assert!(model.accepts(&shell, &ticket));
    shell.core.account_epoch += 1;
    assert!(!model.accepts(&shell, &ticket));
}
#[test]
fn close_reopen_same_canvas_is_a_new_visit() {
    let mut shell = shell();
    let ticket = ticket(&shell);
    let model = model(&ticket);
    shell.shortcut_generation += 1;
    assert!(!model.accepts(&shell, &ticket));
}
#[test]
fn changed_filter_or_retired_request_rejects_old_page() {
    let shell = shell();
    let ticket = ticket(&shell);
    let mut model = model(&ticket);
    model.query = Some(Query::Stream(Filter::Unread));
    assert!(!model.accepts(&shell, &ticket));
    model.query = Some(ticket.query.clone());
    model.next_serial();
    assert!(!model.accepts(&shell, &ticket));
}
#[test]
fn deleted_canvas_and_other_viewer_refuse_read_adoption() {
    let mut shell = shell();
    let ticket = ticket(&shell);
    let model = model(&ticket);
    let user = shell.self_account.user_id.clone();
    shell.self_account.user_id = "UOTHER".into();
    assert!(!model.accepts(&shell, &ticket));
    shell.self_account.user_id = user;
    shell
        .core
        .workspaces
        .get_mut(&ticket.owner.scope.team)
        .unwrap()
        .deleted_files
        .insert("Ffixture".into());
    assert!(!model.accepts(&shell, &ticket));
}
#[test]
fn hidden_canvas_does_not_receive_late_page() {
    let mut shell = shell();
    let ticket = ticket(&shell);
    let model = model(&ticket);
    shell.overlay = None;
    assert!(!model.accepts(&shell, &ticket));
}
#[test]
fn cooldown_expiry_only_enables_explicit_action() {
    let mut model = State::default();
    model.retry_at = Some(Instant::now() + std::time::Duration::from_secs(10));
    assert!(model.cooling());
    assert!(model.query.is_none());
    model.retry_at = Some(Instant::now() - std::time::Duration::from_secs(1));
    assert!(!model.cooling());
    assert!(model.query.is_none());
}
