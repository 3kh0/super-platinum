//! One explicit Text create through the real builder and reducers.
use super::*;
use super_platinum_core::slack_lists::ColumnCreateKind;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldCreateMode {
    Text,
    Number,
    Checkbox,
}
impl FieldCreateMode {
    pub(crate) fn kind(self) -> ColumnCreateKind {
        match self {
            Self::Text => ColumnCreateKind::Text,
            Self::Number => ColumnCreateKind::NumberDefault0,
            Self::Checkbox => ColumnCreateKind::Checkbox,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Text => "Release notes",
            Self::Number => "Release count",
            Self::Checkbox => "Verified",
        }
    }
    pub(crate) fn observed(self) -> (&'static str, &'static str) {
        match self {
            Self::Text => ("C_TEXT_OBSERVED", "text_observed"),
            Self::Number => ("C_NUMBER_OBSERVED", "number_observed"),
            Self::Checkbox => ("C_CHECKBOX_OBSERVED", "checkbox_observed"),
        }
    }
    pub(crate) fn prefix(self) -> &'static str {
        match self {
            Self::Text => "slack-list-text-field-create",
            Self::Number => "slack-list-number-field-create",
            Self::Checkbox => "slack-list-checkbox-field-create",
        }
    }
    pub(crate) fn env_prefix(self) -> &'static str {
        match self {
            Self::Text => "SUPER_PLATINUM_LIST_TEXT_FIELD_CREATE",
            Self::Number => "SUPER_PLATINUM_LIST_NUMBER_FIELD_CREATE",
            Self::Checkbox => "SUPER_PLATINUM_LIST_CHECKBOX_FIELD_CREATE",
        }
    }
}
fn mode_gate(
    text: Option<&str>,
    number: Option<&str>,
    checkbox: Option<&str>,
    alias: Option<&str>,
) -> Option<FieldCreateMode> {
    let mode = match (text, number, checkbox) {
        (Some("1"), None, None) => FieldCreateMode::Text,
        (None, Some("1"), None) => FieldCreateMode::Number,
        (None, None, Some("1")) => FieldCreateMode::Checkbox,
        _ => return None,
    };
    let alias = alias?;
    (alias == mode.prefix() || alias == format!("{}-narrow", mode.prefix())).then_some(mode)
}
pub(crate) fn field_create_mode() -> Option<FieldCreateMode> {
    mode_gate(
        std::env::var("SUPER_PLATINUM_LIST_TEXT_FIELD_CREATE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_LIST_NUMBER_FIELD_CREATE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_LIST_CHECKBOX_FIELD_CREATE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
pub(crate) fn field_create_check_enabled() -> bool {
    field_create_mode().is_some()
}
pub(crate) fn seed_text_create(shell: &ShellState, server: Value) {
    assert!(field_create_check_enabled() && enabled(shell));
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        ..Default::default()
    };
}
pub(crate) fn stale_text_create_file(shell: &ShellState) -> Result<(), String> {
    if !field_create_check_enabled() || !enabled(shell) {
        return Err("Exact disconnected Text creation checker required".into());
    }
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Uncertain
                && e.target.is_text_creation()
                && matches!(e.intent, Some(Intent::NewTextField(_)))
        })
        .ok_or("No uncertain Text creation intent")?;
    ledger()
        .lock()
        .unwrap()
        .queue_precision_file(e.original.raw)
}
/// Setup a replacement visit from the captured canonical snapshot while the
/// old check remains held. This does not claim ordinary reopen preserves Find.
pub(crate) fn seed_text_create_reopen(
    shell: &mut ShellState,
    visit: u64,
    loaded: crate::slack_lists::Loaded,
    find: crate::slack_lists::find::State,
) -> Result<(), String> {
    if !field_create_check_enabled()
        || !enabled(shell)
        || ledger().lock().unwrap().held_read.is_none()
    {
        return Err("Held disconnected Text creation check required".into());
    }
    seed_reopened(shell, visit, loaded, find)
}
fn seed_reopened(
    shell: &mut ShellState,
    visit: u64,
    loaded: crate::slack_lists::Loaded,
    find: crate::slack_lists::find::State,
) -> Result<(), String> {
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Checking
                && e.target == Target::NewTextField("V_FILTER".into())
                && e.intent.is_some()
                && e.job.is_some()
        })
        .ok_or("Missing retained Text creation check")?;
    if generation(shell) != Some(visit)
        || e.original != loaded.document
        || e.original_records.as_deref() != Some(&loaded.records)
        || e.view_selection == selection(shell)
        || find.input != "Launch item 0"
        || find.applied != find.input
        || find.revision == 0
    {
        return Err("Wrong captured snapshot/Find or replacement visit".into());
    }
    let v = shell
        .slack_lists
        .view
        .as_mut()
        .ok_or("Missing replacement List")?;
    if v.id != loaded.document.id
        || v.loaded.is_some()
        || v.loading
        || v.unavailable
        || v.read != 0
        || v.revision != 0
        || !v.layout.find.input.is_empty()
        || !v.layout.find.applied.is_empty()
    {
        return Err("Replacement List is not an unpopulated fixture visit".into());
    }
    v.title = loaded.document.title.clone();
    v.loaded = Some(loaded);
    v.stale = false;
    v.revision = next_id();
    v.layout.find = find;
    crate::slack_lists::rebuild_rows(shell);
    Ok(())
}

// Fixture data only: the production observer discovers these returned identities.
fn add_field(server: &mut Value, mode: FieldCreateMode) -> Result<(), Error> {
    let (id, key) = mode.observed();
    let schema = server["list_metadata"]["schema"]
        .as_array_mut()
        .ok_or_else(|| Error::Api("fixture_schema".into()))?;
    if schema.iter().any(|c| c["id"] == id || c["key"] == key) {
        return Err(Error::Api("fixture_duplicate".into()));
    }
    let mut column = json!({"id":id,"key":key,"name":mode.name(),"type":mode.kind().as_str(),"opaque":{"fixture":"retained"}});
    if let Some(options) = mode.kind().options() {
        column["options"] = options;
    }
    schema.insert(1, column);
    let columns = server["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .ok_or_else(|| Error::Api("fixture_columns".into()))?;
    columns.insert(1, json!({"id":id,"key":key,"visible":true}));
    Ok(())
}
pub(crate) fn text_create_custody(shell: &ShellState) -> Result<Value, String> {
    if !field_create_check_enabled() || !enabled(shell) {
        return Err("Exact disconnected Text checker required".into());
    }
    let e = entry(shell).ok_or("Missing Text intent")?;
    let Some(Intent::NewTextField(i)) = &e.intent else {
        return Err("Missing actual create intent".into());
    };
    let records = e.original_records.as_ref().ok_or("Missing captured page")?;
    let mut custody = json!({"serial":e.serial,"document":e.original.raw,"population":records.raw,"intentDocument":i.original().raw,"intentPopulation":i.original_records().raw,"name":i.desired_name(),"view":i.view_id(),"user":i.authenticated_user(),"kind":i.kind().as_str(),"choice":e.field_choice.revision});
    if let Some(options) = i.kind().options() {
        custody["options"] = options;
    }
    Ok(custody)
}
pub(super) async fn write(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
) -> Result<Value, ListColumnRenameFailure> {
    if !field_create_check_enabled()
        || !enabled(&state.peek())
        || !guard.current(&state.peek(), Stage::Saving)
        || registry::blocked(&guard.key.owner)
        || !permit.is_text_creation()
    {
        return Err(ListColumnRenameFailure::NotRequested(
            Error::TransportNotConfigured,
        ));
    }
    let mode = field_create_mode().ok_or(ListColumnRenameFailure::NotRequested(
        Error::TransportNotConfigured,
    ))?;
    let valid = with_registry(|r| {
        r.entries.get(&guard.key).is_some_and(|e| match &e.intent {
            Some(Intent::NewTextField(i)) => {
                e.target == Target::NewTextField("V_FILTER".into())
                    && i.desired_name() == mode.name()
                    && i.kind() == mode.kind()
                    && e.field_choice.kind == mode.kind()
                    && e.original_records.as_deref() == Some(i.original_records())
            }
            _ => false,
        })
    });
    if !valid {
        return Err(ListColumnRenameFailure::NotRequested(
            Error::TransportNotConfigured,
        ));
    }
    {
        let l = ledger().lock().unwrap();
        if l.pending.is_some()
            || l.held_read.is_some()
            || l.requests
                .iter()
                .any(|r| r["path"] == "/api/lists.columns.create")
        {
            return Err(ListColumnRenameFailure::NotRequested(Error::Api(
                "fixture_single_create".into(),
            )));
        }
    }
    let request = permit.request(&state.peek().core.client, &session());
    let mut expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
        ("view_id".into(), "V_FILTER".into()),
        ("name".into(), mode.name().into()),
        ("type".into(), mode.kind().as_str().into()),
    ];
    let options = match mode {
        FieldCreateMode::Text => Some("{\"format\":\"text\"}"),
        FieldCreateMode::Number => Some("{\"precision\":0}"),
        FieldCreateMode::Checkbox => None,
    };
    if let Some(options) = options {
        expected.push(("options".into(), options.into()));
    }
    expected.push(("_x_reason".into(), "create-column".into()));
    if !matches!(&request.body,RequestBody::Form(fields) if fields==&expected) {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_create_form".into(),
        )));
    }
    if record(request).map_err(ListColumnRenameFailure::NotRequested)?
        != "/api/lists.columns.create"
    {
        return Err(ListColumnRenameFailure::NotRequested(Error::Api(
            "fixture_create_path".into(),
        )));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        l.responses
            .push(json!({"syntheticFailure":"response_lost"}));
        l.pending = Some(tx);
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| {
            ListColumnRenameFailure::Unconfirmed(Error::Transport("fixture_timeout".into()))
        })?
        .map_err(|_| {
            ListColumnRenameFailure::Unconfirmed(Error::Transport("fixture_closed".into()))
        })?;
    add_field(ledger().lock().unwrap().server.as_mut().unwrap(), mode)
        .map_err(ListColumnRenameFailure::Unconfirmed)?;
    Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
        "fixture_response_lost".into(),
    )))
}
#[cfg(test)]
mod tests {
    fn mode_gate(
        text: Option<&str>,
        number: Option<&str>,
        alias: Option<&str>,
    ) -> Option<FieldCreateMode> {
        super::mode_gate(text, number, None, alias)
    }
    #[test]
    fn checkbox_gate_is_exact_and_options_are_absent() {
        for alias in [
            "slack-list-checkbox-field-create",
            "slack-list-checkbox-field-create-narrow",
        ] {
            assert_eq!(
                super::mode_gate(None, None, Some("1"), Some(alias)),
                Some(FieldCreateMode::Checkbox)
            );
            for extra in [Some(""), Some("0"), Some("1")] {
                assert!(super::mode_gate(extra, None, Some("1"), Some(alias)).is_none());
                assert!(super::mode_gate(None, extra, Some("1"), Some(alias)).is_none());
            }
        }
        assert!(
            super::mode_gate(None, None, Some("1"), Some("slack-list-text-field-create")).is_none()
        );
        let mut doc =
            json!({"list_metadata":{"schema":[{"id":"old"}],"views":[{"columns":[{"id":"old"}]}]}});
        add_field(&mut doc, FieldCreateMode::Checkbox).unwrap();
        assert!(doc["list_metadata"]["schema"][1].get("options").is_none());
        assert_eq!(doc["list_metadata"]["schema"][1]["type"], "checkbox");
    }

    use super::*;
    #[test]
    fn only_exact_opted_in_aliases_enable_text_executor() {
        for alias in [
            "slack-list-text-field-create",
            "slack-list-text-field-create-narrow",
        ] {
            assert_eq!(
                mode_gate(Some("1"), None, Some(alias)),
                Some(FieldCreateMode::Text)
            );
            for flag in [None, Some("true"), Some("0"), Some("")] {
                assert!(mode_gate(flag, None, Some(alias)).is_none());
            }
        }
        for alias in [
            None,
            Some("slack-list-field-visibility"),
            Some("slack-list-text-field-create-extra"),
        ] {
            assert!(mode_gate(Some("1"), None, alias).is_none());
        }
    }
    #[test]
    fn number_gate_is_exact_and_cannot_borrow_text_authority() {
        for alias in [
            "slack-list-number-field-create",
            "slack-list-number-field-create-narrow",
        ] {
            assert_eq!(
                mode_gate(None, Some("1"), Some(alias)),
                Some(FieldCreateMode::Number)
            );
            assert!(mode_gate(Some("1"), Some("1"), Some(alias)).is_none());
            assert!(mode_gate(Some("1"), None, Some(alias)).is_none());
            for value in [None, Some("true"), Some("0"), Some("")] {
                assert!(mode_gate(None, value, Some(alias)).is_none());
            }
        }
        assert!(mode_gate(None, Some("1"), Some("slack-list-text-field-create")).is_none());
        assert!(
            mode_gate(
                None,
                Some("1"),
                Some("slack-list-number-field-create-extra")
            )
            .is_none()
        );
    }
    #[test]
    fn number_addition_has_default_zero_precision_without_backfill() {
        let before = json!({"opaque":7,"list_metadata":{"schema":[{"id":"old","key":"old"}],"views":[{"columns":[{"id":"old","key":"old"}],"opaque":true}]}});
        let mut after = before.clone();
        add_field(&mut after, FieldCreateMode::Number).unwrap();
        assert_eq!(
            after["list_metadata"]["schema"][1],
            json!({"id":"C_NUMBER_OBSERVED","key":"number_observed","name":"Release count","type":"number","options":{"precision":0},"opaque":{"fixture":"retained"}})
        );
        after["list_metadata"]["schema"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        after["list_metadata"]["views"][0]["columns"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        assert_eq!(after, before);
    }
    #[test]
    fn addition_preserves_full_prior_metadata_and_refuses_duplicate() {
        let before = json!({"opaque":7,"list_metadata":{"schema":[{"id":"old","key":"old","type":"text","opaque":3}],"views":[{"id":"V_FILTER","columns":[{"id":"old","key":"old"}],"sorts":[{"key":"estimate","ascending":true}],"filters":[{"key":"done","operator":"is_not_empty","values":[]}],"info_column_filters":[],"opaque":false}]}});
        let mut after = before.clone();
        add_field(&mut after, FieldCreateMode::Text).unwrap();
        let mut restored = after.clone();
        restored["list_metadata"]["schema"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        restored["list_metadata"]["views"][0]["columns"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        assert_eq!(restored, before);
        assert!(add_field(&mut after, FieldCreateMode::Text).is_err());
    }
    #[test]
    fn replacement_seed_keeps_full_canonical_population_and_active_find_before_retired_read() {
        let mut shell = ShellState::fixture_variant(
            crate::media::MediaRegistry::default(),
            "slack-list-text-field-create",
        );
        let key = Key::current(&shell).unwrap();
        struct Cleanup(Key);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                with_registry(|r| {
                    r.entries.remove(&self.0);
                });
            }
        }
        let _cleanup = Cleanup(key.clone());
        let e = entry(&shell).unwrap();
        let v = shell.slack_lists.view.as_mut().unwrap();
        v.layout.find.input = "Launch item 0".into();
        v.layout.find.applied = "Launch item 0".into();
        v.layout.find.revision = 37;
        crate::slack_lists::rebuild_rows(&mut shell);
        let saved = shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .clone()
            .unwrap();
        let find = shell.slack_lists.view.as_ref().unwrap().layout.find.clone();
        let (write, intent) = prepare(&mut shell, e.serial).unwrap();
        admitted(
            &shell,
            &write,
            &intent,
            &network::Metadata {
                document: saved.document.clone(),
                allow: saved.allow_lists.clone(),
                personal: saved.personal_task_lists.clone().unwrap(),
            },
        )
        .unwrap();
        crate::slack_lists::rename::completed(
            &mut shell,
            &write,
            Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
                "lost".into(),
            ))),
        );
        let guard = Guard::capture_read(&shell, key.clone(), e.serial).unwrap();
        with_registry(|r| {
            let e = r.entries.get_mut(&key).unwrap();
            e.job = Some(guard.job);
            e.stage = Stage::Checking;
        });
        let old_visit = generation(&shell).unwrap();
        let old_selection = selection(&shell).unwrap();
        let mut changed = saved.document.raw.clone();
        add_field(&mut changed, FieldCreateMode::Text).unwrap();
        crate::slack_lists::close_shell(&mut shell);
        shell.core.account_epoch += 1;
        assert!(crate::slack_lists::open_shell(
            &mut shell,
            serde_json::from_value(changed.clone()).unwrap()
        ));
        let visit = generation(&shell).unwrap();
        assert_ne!(visit, old_visit);
        assert_ne!(selection(&shell), Some(old_selection));
        crate::slack_lists::fixture::refresh(&mut shell);
        assert!(shell.slack_lists.view.as_ref().unwrap().loaded.is_none());
        assert!(blocks_read(&shell));
        let mut wrong = saved.clone();
        wrong.document = Document::parse(changed.clone()).unwrap();
        assert!(seed_reopened(&mut shell, visit, wrong, find.clone()).is_err());
        assert!(seed_reopened(&mut shell, old_visit, saved.clone(), find.clone()).is_err());
        assert!(seed_reopened(&mut shell, visit, saved.clone(), Default::default()).is_err());
        let mut wrong_records = saved.clone();
        wrong_records.records.raw["opaque_fixture_change"] = json!(true);
        assert!(seed_reopened(&mut shell, visit, wrong_records, find.clone()).is_err());
        seed_reopened(&mut shell, visit, saved.clone(), find.clone()).unwrap();
        let same = |s: &ShellState| {
            let v = s.slack_lists.view.as_ref().unwrap();
            assert_eq!(v.loaded.as_ref().unwrap().document, saved.document);
            assert_eq!(v.loaded.as_ref().unwrap().records, saved.records);
            assert_eq!(
                (
                    &v.layout.find.input,
                    &v.layout.find.applied,
                    v.layout.find.revision
                ),
                (&find.input, &find.applied, find.revision)
            );
            assert_eq!(v.layout.selected.as_deref(), Some("V_FILTER"));
            assert_eq!(
                v.layout
                    .entries
                    .iter()
                    .filter_map(|e| match e {
                        crate::slack_lists::layout::TableEntry::Row(r) => Some(r.id.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                [3, 1, 7, 5]
                    .into_iter()
                    .map(|i| format!("R_LIST_{i:03}"))
                    .collect::<Vec<_>>()
            );
        };
        same(&shell);
        let retained = entry(&shell).unwrap();
        assert_eq!(retained.stage, Stage::Checking);
        assert_eq!(retained.job, Some(guard.job));
        assert_eq!(retained.serial, e.serial);
        assert!(!guard.current(&shell, Stage::Checking));
        assert!(!crate::slack_lists::rename::checked(
            &mut shell,
            &guard,
            Ok(network::Metadata {
                document: Document::parse(changed).unwrap(),
                allow: saved.allow_lists.clone(),
                personal: saved.personal_task_lists.clone().unwrap()
            })
        ));
        same(&shell);
        assert_eq!(entry(&shell).unwrap().stage, Stage::Uncertain);
        assert!(seed_reopened(&mut shell, visit, saved.clone(), find.clone()).is_err());
    }
}

pub(crate) fn text_create_registry(shell: &ShellState) -> Result<Value, String> {
    if !field_create_check_enabled() || !enabled(shell) {
        return Err("Exact disconnected Text checker required".into());
    }
    let e = entry(shell);
    Ok(
        json!({"present":e.is_some(),"serial":e.as_ref().map(|e|e.serial),"job":e.as_ref().and_then(|e|e.job),"intent":e.as_ref().is_some_and(|e|e.intent.is_some())}),
    )
}
