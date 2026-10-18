//! Explicit disconnected Add/Color boundary, using the shared ledger and worker.
use super::*;
mod observer;
pub(crate) use observer::*;
pub(crate) fn check_enabled() -> bool {
    gate(
        std::env::var("SUPER_PLATINUM_LIST_BOARD_CHOICE_OPTIONS_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Case {
    Add,
    Color,
}
impl Case {
    pub(crate) fn kind(self) -> super::super::board_choice::Kind {
        match self {
            Self::Add => super::super::board_choice::Kind::Add,
            Self::Color => super::super::board_choice::Kind::Color,
        }
    }
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Add => "Board option addition",
            Self::Color => "Board option color",
        }
    }
    pub(crate) fn accepts(
        self,
        action: &super_platinum_core::slack_lists::BoardChoiceAction,
    ) -> bool {
        use super_platinum_core::slack_lists::{BoardChoiceAction, SelectCreateOption};
        match (self, action) {
            (
                Self::Add,
                BoardChoiceAction::Add {
                    value,
                    label,
                    color,
                },
            ) => {
                label == "Later"
                    && color == "orange"
                    && SelectCreateOption::capture(label, value, color).is_ok()
            }
            (Self::Color, BoardChoiceAction::Color { value, color }) => {
                value == "OPT_PROGRESS" && color == "orange"
            }
            _ => false,
        }
    }
}
pub(crate) fn case_for(alias: &str) -> Option<Case> {
    match alias {
        "slack-list-board-choice-add" | "slack-list-board-choice-add-narrow" => Some(Case::Add),
        "slack-list-board-choice-color" | "slack-list-board-choice-color-narrow" => {
            Some(Case::Color)
        }
        _ => None,
    }
}
pub(crate) fn case() -> Option<Case> {
    case_for(&std::env::var("SUPER_PLATINUM_FIXTURE").ok()?)
}
fn gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1") && alias.and_then(case_for).is_some()
}
pub(crate) fn seed(shell: &ShellState, server: Value) {
    assert!(check_enabled() && enabled(shell));
    let records = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .raw
        .clone();
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        board_records: Some(records),
        ..Default::default()
    };
}
pub(super) async fn write(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
) -> Result<Value, ListColumnRenameFailure> {
    use ListColumnRenameFailure::{NotRequested, Unconfirmed};
    if !check_enabled()
        || !enabled(&state.peek())
        || !guard.current(&state.peek(), Stage::Saving)
        || registry::blocked(&guard.key.owner)
        || !permit.is_board_choice()
    {
        return Err(NotRequested(Error::TransportNotConfigured));
    }
    let intent = with_registry(|r| {
        r.entries.get(&guard.key).and_then(|e| match &e.intent {
            Some(Intent::BoardChoice(i))
                if i.view_id() == "V_LANE" && i.column_key() == "status" =>
            {
                Some(i.clone())
            }
            _ => None,
        })
    })
    .ok_or(NotRequested(Error::TransportNotConfigured))?;
    if !case().is_some_and(|mode| mode.accepts(intent.action())) {
        return Err(NotRequested(Error::Api(
            "fixture_board_option_target".into(),
        )));
    }
    let original = intent.original().raw.clone();
    {
        let l = ledger().lock().unwrap();
        if l.pending.is_some()
            || l.held_read.is_some()
            || l.requests.len() != 3
            || l.responses.len() != 3
            || l.server.as_ref() != Some(&original)
        {
            return Err(NotRequested(Error::Api(
                "fixture_board_choice_order".into(),
            )));
        }
    }
    let request = permit.request(&state.peek().core.client, &session());
    let expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
        ("key".into(), "status".into()),
        ("options".into(), intent.options().to_string()),
        ("backfill_missing_cells_with_default".into(), "true".into()),
        ("_x_reason".into(), "update-column".into()),
    ];
    if !matches!(&request.body,RequestBody::Form(fields) if fields==&expected) {
        return Err(NotRequested(Error::Api("fixture_board_choice_form".into())));
    }
    if record(request).map_err(NotRequested)? != "/api/lists.columns.update" {
        return Err(NotRequested(Error::Api("fixture_board_choice_path".into())));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        l.pending = Some(tx);
        l.responses
            .push(json!({"syntheticFailure":"response_lost"}));
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| Unconfirmed(Error::Transport("fixture_timeout".into())))?
        .map_err(|_| Unconfirmed(Error::Transport("fixture_closed".into())))?;
    let mut l = ledger().lock().unwrap();
    let schema = l.server.as_mut().unwrap()["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap();
    schema
        .iter_mut()
        .find(|c| c["id"] == intent.column_id())
        .unwrap()["options"] = intent.options().clone();
    Err(Unconfirmed(Error::Transport(
        "fixture_response_lost".into(),
    )))
}
#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::slack_lists::BoardChoiceAction;
    #[test]
    fn exact_four_aliases_and_flag_are_closed() {
        for a in [
            "slack-list-board-choice-add",
            "slack-list-board-choice-add-narrow",
            "slack-list-board-choice-color",
            "slack-list-board-choice-color-narrow",
        ] {
            assert!(gate(Some("1"), Some(a)));
            assert!(!gate(None, Some(a)));
            assert!(!gate(Some("true"), Some(a)));
        }
        assert!(!gate(Some("1"), Some("slack-list-board-choice")));
    }
    #[test]
    fn add_and_color_admit_only_the_captured_kind_palette_and_identity() {
        let add = BoardChoiceAction::Add {
            value: "OptABCDEFG1".into(),
            label: "Later".into(),
            color: "orange".into(),
        };
        let color = BoardChoiceAction::Color {
            value: "OPT_PROGRESS".into(),
            color: "orange".into(),
        };
        assert!(Case::Add.accepts(&add));
        assert!(!Case::Color.accepts(&add));
        assert!(Case::Color.accepts(&color));
        assert!(!Case::Add.accepts(&color));
        assert!(!Case::Add.accepts(&BoardChoiceAction::Add {
            value: "guessed".into(),
            label: "Later".into(),
            color: "orange".into()
        }));
    }
}
