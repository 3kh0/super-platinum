use super::*;
use serde_json::json;
use super_platinum_core::{slack::models::User, slack_lists::EntityCreateFormat};

struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn shell(channel: bool) -> (ShellState, Cleanup) {
    let mut s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        if channel {
            "slack-list-channel-field-create"
        } else {
            "slack-list-people-field-create"
        },
    );
    if channel {
        crate::slack_lists::fixture::text_field_create::channel::cache::seed(&mut s).unwrap();
    } else {
        crate::slack_lists::fixture::text_field_create::people::cache::seed(&mut s).unwrap();
    }
    let key = Key::current(&s).unwrap();
    assert!(Stamp::capture(&s).is_some());
    (s, Cleanup(key))
}
fn shown(s: &ShellState) -> directory::Presentation {
    entry(s)
        .unwrap()
        .entity_draft
        .unwrap()
        .directory
        .presentation()
}
fn late() -> Results {
    Results::Users(vec![User {
        id: "U88888888".into(),
        name: Some("Late person".into()),
        ..User::default()
    }])
}
#[test]
fn creation_queries_use_real_registry_owner_and_domain_without_a_cell_editor() {
    for channel in [false, true] {
        let (s, _cleanup) = shell(channel);
        let stamp = Stamp::capture(&s).unwrap();
        let request = begin(&s, stamp, if channel { "beta" } else { "Bob" }.into()).unwrap();
        assert!(request.current(&s));
        assert!(s.slack_lists.view.as_ref().unwrap().editor.is_none());
        let p = shown(&s);
        assert!(!p.loading);
        assert_eq!(p.candidates.len(), 1);
        assert_eq!(
            p.candidates[0].id,
            if channel { "G00000002" } else { "U00000002" }
        );
        let before = p;
        finish(
            &s,
            &request,
            Ok(if channel {
                late()
            } else {
                Results::Channels(vec![])
            }),
        );
        assert!(shown(&s) == before);
        assert!(crate::slack_lists::rename::close_entry(&s, stamp.serial));
    }
}
#[test]
fn configuration_epoch_retires_requests_without_erasing_query_or_options_order() {
    let (s, _cleanup) = shell(false);
    let stamp = Stamp::capture(&s).unwrap();
    let request = begin(&s, stamp, "Alice".into()).unwrap();
    assert!(stamp.with(&s, |draft| {
        let options = draft
            .options
            .with_format(EntityCreateFormat::Multiple)
            .unwrap()
            .select_multiple(vec!["W00000012".into(), "U00000001".into()])
            .unwrap()
            .with_format(EntityCreateFormat::Single)
            .unwrap();
        draft.replace_options(options)
    }));
    let fresh = Stamp::capture(&s).unwrap();
    assert_ne!(fresh.epoch, stamp.epoch);
    assert_eq!(fresh.choice, stamp.choice);
    assert!(!request.current(&s));
    let before = shown(&s);
    assert_eq!(before.query, "Alice");
    finish(&s, &request, Ok(late()));
    finish(
        &s,
        &request,
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    assert!(shown(&s) == before);
    assert!(begin(&s, stamp, "late".into()).is_none());
    let p = presentation(&s).unwrap();
    assert_eq!(p.options.selected_ids(), ["W00000012", "U00000001"]);
    assert!(begin(&s, fresh, "Bob".into()).is_some());
    assert!(crate::slack_lists::rename::close_entry(&s, stamp.serial));
}
#[test]
fn obsolete_creation_results_cannot_change_a_new_query_type_owner_or_population() {
    for change in [
        "query",
        "type-aba",
        "cancel-reopen",
        "account",
        "selection",
        "population",
        "preparing",
    ] {
        let (mut s, _cleanup) = shell(false);
        let stamp = Stamp::capture(&s).unwrap();
        let request = begin(&s, stamp, "Alice".into()).unwrap();
        match change {
            "query" => {
                begin(&s, stamp, "Bob".into()).unwrap();
            }
            "type-aba" => {
                assert!(kind_shell(
                    &s,
                    stamp.serial,
                    stamp.visit,
                    stamp.selection,
                    stamp.choice,
                    "text"
                ));
                let choice = choice(&s).unwrap();
                assert!(kind_shell(
                    &s,
                    stamp.serial,
                    stamp.visit,
                    stamp.selection,
                    choice,
                    "user"
                ));
            }
            "cancel-reopen" => {
                assert!(crate::slack_lists::rename::close_entry(&s, stamp.serial));
                assert!(open_shell(&mut s, stamp.visit, stamp.selection));
                let e = entry(&s).unwrap();
                assert_ne!(e.serial, stamp.serial);
                assert!(kind_shell(
                    &s,
                    e.serial,
                    stamp.visit,
                    stamp.selection,
                    e.field_choice,
                    "user"
                ));
            }
            "account" => s.core.account_epoch += 1,
            "selection" => {
                s.slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation = next_id()
            }
            "population" => {
                let l = s
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap();
                let mut raw = l.records.raw.clone();
                raw["response_metadata"]["next_cursor"] = json!("changed");
                l.records = RecordsPage::parse(raw).unwrap();
            }
            "preparing" => {
                with_registry(|r| {
                    r.entries.get_mut(&request.key).unwrap().stage = Stage::Preparing
                });
            }
            _ => unreachable!(),
        }
        assert!(!request.current(&s), "{change}");
        let before = with_registry(|r| {
            r.entries[&request.key]
                .entity_draft
                .as_ref()
                .unwrap()
                .directory
                .presentation()
        });
        finish(&s, &request, Ok(late()));
        finish(
            &s,
            &request,
            Err(Error::RateLimited {
                retry_after_secs: Some(60),
            }),
        );
        let after = with_registry(|r| {
            r.entries[&request.key]
                .entity_draft
                .as_ref()
                .unwrap()
                .directory
                .presentation()
        });
        assert!(after == before, "{change}");
    }
}
#[test]
fn creation_cooldown_keeps_cache_and_success_uses_current_cache_veto() {
    for channel in [false, true] {
        let (mut s, _cleanup) = shell(channel);
        let stamp = Stamp::capture(&s).unwrap();
        let request = begin(&s, stamp, "".into()).unwrap();
        finish(
            &s,
            &request,
            Err(Error::RateLimited {
                retry_after_secs: Some(60),
            }),
        );
        assert!(begin(&s, stamp, if channel { "beta" } else { "Bob" }.into()).is_none());
        let p = shown(&s);
        assert_eq!(
            p.candidates[0].id,
            if channel { "G00000002" } else { "U00000002" }
        );
        assert!(!p.loading && p.error.unwrap().contains("wait"));
        assert!(crate::slack_lists::rename::close_entry(&s, stamp.serial));
        assert!(open_shell(&mut s, stamp.visit, stamp.selection));
        let e = entry(&s).unwrap();
        assert!(kind_shell(
            &s,
            e.serial,
            stamp.visit,
            stamp.selection,
            e.field_choice,
            if channel { "channel" } else { "user" }
        ));
        let stamp = Stamp::capture(&s).unwrap();
        let request = begin(&s, stamp, "fuzzy".into()).unwrap();
        let team = s.core.active_team.clone().unwrap();
        let w = s.core.workspaces.get_mut(&team).unwrap();
        let result = if channel {
            let remote = w.channels["C00000001"].clone();
            w.channels.get_mut("C00000001").unwrap().is_archived = true;
            Results::Channels(vec![remote])
        } else {
            let remote = w.users["U00000001"].clone();
            w.users.get_mut("U00000001").unwrap().deleted = true;
            Results::Users(vec![remote])
        };
        finish(&s, &request, Ok(result));
        assert!(shown(&s).candidates.is_empty());
        assert!(crate::slack_lists::rename::close_entry(&s, stamp.serial));
    }
}
