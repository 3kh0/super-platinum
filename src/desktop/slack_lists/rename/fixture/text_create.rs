//! One explicit Text create through the real builder and reducers.
use super::*;
mod entity_mode;
mod rating_mode;
pub(crate) use entity_mode::EntityCase;
mod vote;
use rating_mode::rating_gate;
use super_platinum_core::slack_lists::ColumnCreateKind;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldCreateMode {
    Text,
    Number,
    NumberPrecision,
    Entity(EntityCase),
    VoteDefault,
    VoteCustom,
    RatingDefault,
    RatingStandard,
    RatingCustom,
    Checkbox,
    Date,
    Select,
    MultiSelect,
    People,
    Link,
    Channel,
}
impl FieldCreateMode {
    pub(crate) fn is_entity(self) -> bool {
        matches!(self, Self::Entity(_))
    }
    pub(crate) fn is_rating(self) -> bool {
        matches!(
            self,
            Self::RatingDefault | Self::RatingStandard | Self::RatingCustom
        )
    }
    pub(crate) fn rating_max(self) -> u8 {
        match self {
            Self::RatingDefault => 1,
            Self::RatingStandard => 5,
            Self::RatingCustom => 3,
            _ => panic!("Not Rating"),
        }
    }
    pub(crate) fn rating_value(self) -> u8 {
        match self {
            Self::RatingDefault => 1,
            Self::RatingStandard => 4,
            Self::RatingCustom => 2,
            _ => panic!("Not Rating"),
        }
    }
    pub(crate) fn rating_options(self) -> Value {
        match self {
            Self::RatingDefault => json!({"emoji":":star:","max":1}),
            Self::RatingStandard => json!({"emoji":":heart:","max":5,"emoji_team_id":"T1"}),
            Self::RatingCustom => json!({"emoji":":fixture_quality:","max":3,"emoji_team_id":"T1"}),
            _ => panic!("Not Rating"),
        }
    }

    pub(crate) fn is_select(self) -> bool {
        matches!(self, Self::Select | Self::MultiSelect)
    }
    pub(crate) fn select_format(self) -> &'static str {
        if self == Self::MultiSelect {
            "multi_select"
        } else {
            "single_select"
        }
    }
    pub(crate) fn kind(self) -> ColumnCreateKind {
        match self {
            Self::Entity(case) => case.kind(),
            Self::RatingDefault | Self::RatingStandard | Self::RatingCustom => {
                ColumnCreateKind::Rating
            }
            Self::VoteDefault | Self::VoteCustom => ColumnCreateKind::Vote,
            Self::Text => ColumnCreateKind::Text,
            Self::Number | Self::NumberPrecision => ColumnCreateKind::NumberDefault0,
            Self::Checkbox => ColumnCreateKind::Checkbox,
            Self::Date => ColumnCreateKind::DateSingleDefault,
            Self::People => ColumnCreateKind::PeopleSingleDefault,
            Self::Link => ColumnCreateKind::Link,
            Self::Channel => ColumnCreateKind::ChannelSingleDefault,
            Self::Select | Self::MultiSelect => ColumnCreateKind::Select,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Entity(case) => {
                if case.channel() {
                    "Channel \u{c9}quipe \u{6771}\u{4eac}"
                } else {
                    "Reviewer \u{c9}quipe \u{6771}\u{4eac}"
                }
            }
            Self::RatingDefault | Self::RatingStandard | Self::RatingCustom => "Quality",
            Self::VoteDefault | Self::VoteCustom => "Support Équipe 東京",
            Self::Text => "Release notes",
            Self::Number | Self::NumberPrecision => "Release count",
            Self::Checkbox => "Verified",
            Self::Date => "Target date",
            Self::People => "Reviewer Équipe 東京",
            Self::Link => "Reference Équipe 東京",
            Self::Channel => "Channel Équipe 東京",
            Self::Select | Self::MultiSelect => "Release phase",
        }
    }
    pub(crate) fn observed(self) -> (&'static str, &'static str) {
        match self {
            Self::Entity(case) => {
                if case.channel() {
                    ("C_CHANNEL_OBSERVED", "channel_observed")
                } else {
                    ("C_PEOPLE_OBSERVED", "people_observed")
                }
            }
            Self::RatingDefault | Self::RatingStandard | Self::RatingCustom => {
                ("C_RATING_OBSERVED", "rating_observed")
            }
            Self::VoteDefault | Self::VoteCustom => ("C_VOTE_OBSERVED", "vote_observed"),
            Self::Text => ("C_TEXT_OBSERVED", "text_observed"),
            Self::Number | Self::NumberPrecision => ("C_NUMBER_OBSERVED", "number_observed"),
            Self::Checkbox => ("C_CHECKBOX_OBSERVED", "checkbox_observed"),
            Self::Date => ("C_DATE_OBSERVED", "date_observed"),
            Self::People => ("C_PEOPLE_OBSERVED", "people_observed"),
            Self::Link => ("C_LINK_OBSERVED", "link_observed"),
            Self::Channel => ("C_CHANNEL_OBSERVED", "channel_observed"),
            Self::Select | Self::MultiSelect => ("C_SELECT_OBSERVED", "select_observed"),
        }
    }
    pub(crate) fn prefix(self) -> &'static str {
        match self {
            Self::Entity(case) => case.prefix(),
            Self::RatingDefault => "slack-list-rating-default-field-create",
            Self::RatingStandard => "slack-list-rating-standard-field-create",
            Self::RatingCustom => "slack-list-rating-custom-field-create",
            Self::VoteDefault => "slack-list-vote-create-default",
            Self::VoteCustom => "slack-list-vote-create-custom",
            Self::Text => "slack-list-text-field-create",
            Self::Number => "slack-list-number-field-create",
            Self::NumberPrecision => "slack-list-number-precision-field-create",
            Self::Checkbox => "slack-list-checkbox-field-create",
            Self::Date => "slack-list-date-field-create",
            Self::People => "slack-list-people-field-create",
            Self::Link => "slack-list-link-field-create",
            Self::Channel => "slack-list-channel-field-create",
            Self::Select => "slack-list-select-field-create",
            Self::MultiSelect => "slack-list-multi-select-field-create",
        }
    }
    pub(crate) fn env_prefix(self) -> &'static str {
        match self {
            Self::Entity(_) => "SUPER_PLATINUM_LIST_ENTITY_FIELD_CREATE",
            Self::RatingDefault => "SUPER_PLATINUM_LIST_RATING_FIELD_CREATE",
            Self::RatingStandard => "SUPER_PLATINUM_LIST_RATING_FIELD_CREATE",
            Self::RatingCustom => "SUPER_PLATINUM_LIST_RATING_FIELD_CREATE",
            Self::VoteDefault | Self::VoteCustom => "SUPER_PLATINUM_LIST_VOTE_FIELD_CREATE",
            Self::Text => "SUPER_PLATINUM_LIST_TEXT_FIELD_CREATE",
            Self::Number => "SUPER_PLATINUM_LIST_NUMBER_FIELD_CREATE",
            Self::NumberPrecision => "SUPER_PLATINUM_LIST_NUMBER_PRECISION_FIELD_CREATE",
            Self::Checkbox => "SUPER_PLATINUM_LIST_CHECKBOX_FIELD_CREATE",
            Self::Date => "SUPER_PLATINUM_LIST_DATE_FIELD_CREATE",
            Self::People => "SUPER_PLATINUM_LIST_PEOPLE_FIELD_CREATE",
            Self::Link => "SUPER_PLATINUM_LIST_LINK_FIELD_CREATE",
            Self::Channel => "SUPER_PLATINUM_LIST_CHANNEL_FIELD_CREATE",
            Self::Select => "SUPER_PLATINUM_LIST_SELECT_FIELD_CREATE",
            Self::MultiSelect => "SUPER_PLATINUM_LIST_MULTI_SELECT_FIELD_CREATE",
        }
    }
}
fn mode_gate_all(
    text: Option<&str>,
    number: Option<&str>,
    checkbox: Option<&str>,
    date: Option<&str>,
    select: Option<&str>,
    alias: Option<&str>,
) -> Option<FieldCreateMode> {
    mode_gate_multi(text, number, checkbox, date, select, None, alias)
}
fn mode_gate_multi(
    text: Option<&str>,
    number: Option<&str>,
    checkbox: Option<&str>,
    date: Option<&str>,
    select: Option<&str>,
    multi: Option<&str>,
    alias: Option<&str>,
) -> Option<FieldCreateMode> {
    if let Some(multi) = multi {
        if multi != "1"
            || [text, number, checkbox, date, select]
                .iter()
                .any(Option::is_some)
        {
            return None;
        }
        let mode = FieldCreateMode::MultiSelect;
        return (alias == Some(mode.prefix())
            || alias == Some("slack-list-multi-select-field-create-narrow"))
        .then_some(mode);
    }
    let mode = match (text, number, checkbox, date, select) {
        (Some("1"), None, None, None, None) => FieldCreateMode::Text,
        (None, Some("1"), None, None, None) => FieldCreateMode::Number,
        (None, None, Some("1"), None, None) => FieldCreateMode::Checkbox,
        (None, None, None, Some("1"), None) => FieldCreateMode::Date,
        (None, None, None, None, Some("1")) => FieldCreateMode::Select,
        _ => return None,
    };
    let alias = alias?;
    (alias == mode.prefix() || alias == format!("{}-narrow", mode.prefix())).then_some(mode)
}
#[cfg(test)]
fn mode_gate(
    t: Option<&str>,
    n: Option<&str>,
    c: Option<&str>,
    d: Option<&str>,
    a: Option<&str>,
) -> Option<FieldCreateMode> {
    mode_gate_all(t, n, c, d, None, a)
}
pub(crate) fn field_create_mode() -> Option<FieldCreateMode> {
    if let Some(flag) = std::env::var_os("SUPER_PLATINUM_LIST_ENTITY_FIELD_CREATE_CHECK") {
        let exclusive = std::env::vars_os().all(|(key, _)| {
            let key = key.to_string_lossy();
            !(key.starts_with("SUPER_PLATINUM_") && key.ends_with("_CHECK"))
                || key == "SUPER_PLATINUM_LIST_ENTITY_FIELD_CREATE_CHECK"
        });
        return entity_mode::gate(
            flag.to_str(),
            exclusive,
            std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
        );
    }
    if let Some(flag) = std::env::var_os("SUPER_PLATINUM_LIST_VOTE_FIELD_CREATE_CHECK") {
        return vote::environment_gate(flag.to_str());
    }
    if let Some(flag) = std::env::var_os("SUPER_PLATINUM_LIST_RATING_FIELD_CREATE_CHECK") {
        let exclusive = [
            "TEXT",
            "NUMBER",
            "NUMBER_PRECISION",
            "CHECKBOX",
            "DATE",
            "SELECT",
            "MULTI_SELECT",
            "PEOPLE",
            "LINK",
            "CHANNEL",
        ]
        .iter()
        .all(|part| {
            std::env::var_os(format!("SUPER_PLATINUM_LIST_{part}_FIELD_CREATE_CHECK")).is_none()
        });
        return rating_gate(
            flag.to_str(),
            exclusive,
            std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
        );
    }

    if let Ok(flag) = std::env::var("SUPER_PLATINUM_LIST_NUMBER_PRECISION_FIELD_CREATE_CHECK") {
        let old = [
            "TEXT",
            "NUMBER",
            "CHECKBOX",
            "DATE",
            "SELECT",
            "MULTI_SELECT",
            "PEOPLE",
            "LINK",
            "CHANNEL",
        ];
        let exclusive = old.iter().all(|part| {
            std::env::var_os(format!("SUPER_PLATINUM_LIST_{part}_FIELD_CREATE_CHECK")).is_none()
        });
        return configured_number_gate(
            &flag,
            exclusive,
            std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
        );
    }
    mode_gate_channel(
        std::env::var("SUPER_PLATINUM_LIST_TEXT_FIELD_CREATE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_LIST_NUMBER_FIELD_CREATE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_LIST_CHECKBOX_FIELD_CREATE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_LIST_DATE_FIELD_CREATE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_LIST_SELECT_FIELD_CREATE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_LIST_MULTI_SELECT_FIELD_CREATE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_LIST_PEOPLE_FIELD_CREATE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_LIST_LINK_FIELD_CREATE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_LIST_CHANNEL_FIELD_CREATE_CHECK")
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
    if mode.is_select() {
        return Err(Error::Api("fixture_select_needs_owned_options".into()));
    }
    let board = if mode.is_rating() || mode.is_vote() {
        let wanted = if mode.is_vote() {
            "V_VOTE_BOARD"
        } else {
            "V_RATING_BOARD"
        };
        let views = server["list_metadata"]["views"]
            .as_array()
            .ok_or_else(|| Error::Api("fixture_views".into()))?;
        let matches: Vec<_> = views
            .iter()
            .enumerate()
            .filter(|(_, v)| v["id"] == wanted)
            .map(|(i, _)| i)
            .collect();
        if matches.len() != 1 {
            return Err(Error::Api("fixture_unique_created_field_board".into()));
        }
        Some(matches[0])
    } else {
        None
    };
    let (id, key) = mode.observed();
    let schema = server["list_metadata"]["schema"]
        .as_array_mut()
        .ok_or_else(|| Error::Api("fixture_schema".into()))?;
    if schema.iter().any(|c| c["id"] == id || c["key"] == key) {
        return Err(Error::Api("fixture_duplicate".into()));
    }
    let mut column = json!({"id":id,"key":key,"name":mode.name(),"type":mode.kind().as_str(),"opaque":{"fixture":"retained"}});
    if let Some(options) = if let FieldCreateMode::Entity(case) = mode {
        Some(case.options())
    } else if mode.is_vote() {
        Some(mode.vote_options())
    } else if mode.is_rating() {
        Some(mode.rating_options())
    } else if mode == FieldCreateMode::NumberPrecision {
        Some(json!({"precision":3}))
    } else {
        mode.kind().options()
    } {
        column["options"] = options;
    }
    schema.insert(1, column);
    let columns = server["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .ok_or_else(|| Error::Api("fixture_columns".into()))?;
    columns.insert(1, json!({"id":id,"key":key,"visible":true}));
    if let Some(board) = board {
        server["list_metadata"]["views"][board]["columns"]
            .as_array_mut()
            .ok_or_else(|| Error::Api("rating_board_columns".into()))?
            .insert(1, json!({"id":id,"key":key,"visible":true}));
    }
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
    if let Some(options) = i.options() {
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
                    && (mode != FieldCreateMode::NumberPrecision
                        || (e.field_choice.number_precision.value() == 3
                            && i.options() == Some(json!({"precision":3}))))
                    && (!mode.is_rating() || i.options() == Some(mode.rating_options()))
                    && (!mode.is_vote() || i.options() == Some(mode.vote_options()))
                    && (match mode {
                        FieldCreateMode::Entity(case) => i.options() == Some(case.options()),
                        _ => true,
                    })
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
    let select = if mode.is_select() {
        Some(
            select::bound_options(
                &state.peek(),
                entry(&state.peek())
                    .ok_or(ListColumnRenameFailure::NotRequested(
                        Error::TransportNotConfigured,
                    ))?
                    .serial,
            )
            .map_err(|_| {
                ListColumnRenameFailure::NotRequested(Error::Api("fixture_select_options".into()))
            })?,
        )
    } else {
        None
    };
    let select_wire = select.as_ref().map(Value::to_string);
    let vote_wire = mode.is_vote().then(|| mode.vote_options().to_string());
    let entity_wire = if let FieldCreateMode::Entity(case) = mode {
        Some(case.options().to_string())
    } else {
        None
    };
    let rating_wire = mode.is_rating().then(|| mode.rating_options().to_string());
    let request = permit.request(&state.peek().core.client, &session());
    let mut expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
        ("view_id".into(), "V_FILTER".into()),
        ("name".into(), mode.name().into()),
        ("type".into(), mode.kind().as_str().into()),
    ];
    let options = match mode {
        FieldCreateMode::Entity(_) => entity_wire.as_deref(),
        FieldCreateMode::VoteDefault | FieldCreateMode::VoteCustom => vote_wire.as_deref(),
        FieldCreateMode::Text => Some("{\"format\":\"text\"}"),
        FieldCreateMode::Number => Some("{\"precision\":0}"),
        FieldCreateMode::NumberPrecision => Some("{\"precision\":3}"),
        FieldCreateMode::RatingDefault
        | FieldCreateMode::RatingStandard
        | FieldCreateMode::RatingCustom => rating_wire.as_deref(),
        FieldCreateMode::Checkbox | FieldCreateMode::Link => None,
        FieldCreateMode::People | FieldCreateMode::Channel => {
            Some("{\"default_value\":null,\"format\":\"single_entity\",\"show_member_name\":true}")
        }
        FieldCreateMode::Select | FieldCreateMode::MultiSelect => select_wire.as_deref(),
        FieldCreateMode::Date => Some("{\"date_format\":\"default\",\"format\":\"single\"}"),
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
    {
        let mut l = ledger().lock().unwrap();
        if mode.is_select() {
            select::add_field_mode(
                mode,
                l.server.as_mut().unwrap(),
                select
                    .as_ref()
                    .ok_or(ListColumnRenameFailure::Unconfirmed(Error::Api(
                        "fixture_select_options".into(),
                    )))?,
            )
        } else {
            add_field(l.server.as_mut().unwrap(), mode)
        }
        .map_err(ListColumnRenameFailure::Unconfirmed)?;
    }
    Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
        "fixture_response_lost".into(),
    )))
}
#[cfg(test)]
mod tests;

pub(crate) fn text_create_registry(shell: &ShellState) -> Result<Value, String> {
    if !field_create_check_enabled() || !enabled(shell) {
        return Err("Exact disconnected Text checker required".into());
    }
    let e = entry(shell);
    Ok(
        json!({"present":e.is_some(),"serial":e.as_ref().map(|e|e.serial),"job":e.as_ref().and_then(|e|e.job),"intent":e.as_ref().is_some_and(|e|e.intent.is_some())}),
    )
}

#[cfg(test)]
pub(crate) fn close_date_fixture(shell: &ShellState) -> bool {
    let Some(e) = entry(shell) else { return false };
    e.target == Target::NewTextField("V_FILTER".into())
        && e.stage == Stage::Editing
        && e.field_choice.kind == ColumnCreateKind::DateSingleDefault
        && super::super::close_entry(shell, e.serial)
}
#[cfg(test)]
mod date_tests;

pub(crate) mod select;

// All CHECK flags are mutually exclusive, including malformed present values.
fn mode_gate_people(
    t: Option<&str>,
    n: Option<&str>,
    c: Option<&str>,
    d: Option<&str>,
    s: Option<&str>,
    m: Option<&str>,
    p: Option<&str>,
    a: Option<&str>,
) -> Option<FieldCreateMode> {
    if let Some(p) = p {
        return (p == "1"
            && [t, n, c, d, s, m].iter().all(Option::is_none)
            && matches!(
                a,
                Some("slack-list-people-field-create" | "slack-list-people-field-create-narrow")
            ))
        .then_some(FieldCreateMode::People);
    }
    mode_gate_multi(t, n, c, d, s, m, a)
}
#[cfg(test)]
mod people_tests;

#[cfg(test)]
pub(crate) fn close_people_fixture(shell: &ShellState) -> bool {
    let Some(e) = entry(shell) else { return false };
    e.target == Target::NewTextField("V_FILTER".into())
        && e.stage == Stage::Editing
        && e.field_choice.kind == ColumnCreateKind::PeopleSingleDefault
        && super::super::close_entry(shell, e.serial)
}

fn mode_gate_link(
    t: Option<&str>,
    n: Option<&str>,
    c: Option<&str>,
    d: Option<&str>,
    s: Option<&str>,
    m: Option<&str>,
    p: Option<&str>,
    l: Option<&str>,
    a: Option<&str>,
) -> Option<FieldCreateMode> {
    if l.is_some() {
        return (l == Some("1")
            && [t, n, c, d, s, m, p].iter().all(Option::is_none)
            && matches!(
                a,
                Some("slack-list-link-field-create" | "slack-list-link-field-create-narrow")
            ))
        .then_some(FieldCreateMode::Link);
    }
    mode_gate_people(t, n, c, d, s, m, p, a)
}
#[cfg(test)]
mod link_tests;
#[cfg(test)]
pub(crate) fn close_link_fixture(shell: &ShellState) -> bool {
    let Some(e) = entry(shell) else { return false };
    e.target == Target::NewTextField("V_FILTER".into())
        && e.stage == Stage::Editing
        && e.field_choice.kind == ColumnCreateKind::Link
        && super::super::close_entry(shell, e.serial)
}

fn mode_gate_channel(
    t: Option<&str>,
    n: Option<&str>,
    c: Option<&str>,
    d: Option<&str>,
    s: Option<&str>,
    m: Option<&str>,
    p: Option<&str>,
    l: Option<&str>,
    h: Option<&str>,
    a: Option<&str>,
) -> Option<FieldCreateMode> {
    if h.is_some() {
        return (h == Some("1")
            && [t, n, c, d, s, m, p, l].iter().all(Option::is_none)
            && matches!(
                a,
                Some("slack-list-channel-field-create" | "slack-list-channel-field-create-narrow")
            ))
        .then_some(FieldCreateMode::Channel);
    }
    mode_gate_link(t, n, c, d, s, m, p, l, a)
}
#[cfg(test)]
mod channel_tests;
#[cfg(test)]
pub(crate) fn close_channel_fixture(shell: &ShellState) -> bool {
    let Some(e) = entry(shell) else { return false };
    e.target == Target::NewTextField("V_FILTER".into())
        && e.stage == Stage::Editing
        && e.field_choice.kind == ColumnCreateKind::ChannelSingleDefault
        && super::super::close_entry(shell, e.serial)
}

fn configured_number_gate(
    flag: &str,
    exclusive: bool,
    alias: Option<&str>,
) -> Option<FieldCreateMode> {
    (flag == "1"
        && exclusive
        && matches!(
            alias,
            Some(
                "slack-list-number-precision-field-create"
                    | "slack-list-number-precision-field-create-narrow"
            )
        ))
    .then_some(FieldCreateMode::NumberPrecision)
}
#[cfg(test)]
mod number_precision_tests;
