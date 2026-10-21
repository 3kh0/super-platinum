//! Exact Activity file destination: actual open/read/parser path, never a seeded View.
use super::*;
use crate::app_home::about::management::activity::fixture::file as source;
use std::sync::{Mutex, OnceLock};

#[derive(Default)]
struct Ledger {
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
fn refused() -> Error {
    Error::Offline("Closed Activity List reader unavailable".into())
}
fn navigation_case(case: &str) -> bool {
    matches!(
        case,
        "file"
            | "file-retirement"
            | "file-emoji"
            | "file-emoji-retirement"
            | "file-emoji-cold"
            | "file-encoded"
    )
}
fn requested() -> bool {
    source::requested()
        && navigation_case(&crate::app_home::about::management::activity::fixture::case())
}
fn responses() -> Vec<Value> {
    vec![
        json!({"ok":true,"file":source::file()}),
        json!({"ok":true,"list_id":source::LIST,"records":[{
            "id":"R_ACTIVITY_DESTINATION","list_id":source::LIST,"position":0,
            "fields":[{"key":"NAME","text":source::ROW}],
            "activity_navigation_opaque":{"keep":[false,0,null]}
        }]}),
        json!({"ok":true,"prefs":{"allow_lists":"full"}}),
        json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
        json!({"ok":true,"assignments":{}}),
    ]
}
fn parse(raw: &[Value]) -> Result<Loaded, Error> {
    if raw.len() != 5 || raw[0]["ok"] != true {
        return Err(refused());
    }
    let document = Document::parse(raw[0]["file"].clone()).map_err(|_| refused())?;
    if document.id != source::LIST {
        return Err(refused());
    }
    Ok(Loaded {
        document,
        records: api::parse_list_records(raw[1].clone(), source::LIST)?,
        allow_lists: api::parse_list_allow_lists(&raw[2])?,
        personal_task_lists: Some(api::parse_list_personal_task_lists(&raw[3])?),
        list_experiments: Some(api::parse_list_experiments(&raw[4])?),
    })
}
fn requests(s: &ShellState) -> Result<Vec<Value>, Error> {
    if !requested() || !source::enabled(s) {
        return Err(refused());
    }
    let w = &s.core.session.as_ref().ok_or_else(refused)?.workspaces["TWORKFLOW"];
    let c = &s.core.client;
    [
        api::file_info(c, w, source::LIST),
        api::lists_records_list(c, w, source::LIST),
        api::list_allow_lists(c, w),
        api::list_personal_task_preferences(c, w),
        api::list_experiments(c, w),
    ]
    .into_iter()
    .map(|r| {
        let RequestBody::Form(fields) = r.body else {
            return Err(refused());
        };
        Ok(json!({"method":r.method,"url":r.url,
                "fields":fields.into_iter().filter(|(k,_)|k!="token").collect::<Vec<_>>()}))
    })
    .collect()
}
pub(in crate::slack_lists) fn refresh(mut state: Signal<ShellState>) -> bool {
    if !requested()
        || state
            .peek()
            .slack_lists
            .view
            .as_ref()
            .is_none_or(|v| v.id != source::LIST)
    {
        return false;
    }
    let Some(attempt) = begin_read(&mut state.write()) else {
        return true;
    };
    let request = {
        let s = state.peek();
        if source::enabled(&s) && attempt.current(&s) {
            requests(&s)
        } else {
            Err(refused())
        }
    };
    let Ok(request) = request else {
        finish_read(&mut state.write(), &attempt, Err(refused()));
        return true;
    };
    let raw = responses();
    let serial = {
        let mut l = LEDGER.get_or_init(Default::default).lock().unwrap();
        let serial = l.requests.len() + 1;
        l.requests.push(
            json!({"serial":serial,"id":attempt.id,"generation":attempt.generation,
            "read":attempt.read,"revision":attempt.revision,"requests":request}),
        );
        l.responses.push(json!({"serial":serial,"responses":raw}));
        serial
    };
    dioxus::core::spawn_forever(async move {
        let current = {
            let s = state.peek();
            source::enabled(&s) && attempt.current(&s)
        };
        let loaded = if current { parse(&raw) } else { Err(refused()) };
        let parsed = loaded.is_ok();
        let again = finish_read(&mut state.write(), &attempt, loaded);
        LEDGER
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .settlements
            .push(json!({"serial":serial,"current":current,"parsed":parsed,"refreshAgain":again}));
        if again {
            super::super::network::refresh(state);
        }
    });
    true
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let l = LEDGER.get_or_init(Default::default).lock().unwrap();
    let loaded=s.slack_lists.view.as_ref().map(|v|json!({
        "id":v.id,"generation":v.generation,"read":v.read,"revision":v.revision,
        "loading":v.loading,"error":v.error,"document":v.loaded.as_ref().map(|l|&l.document.raw),
        "records":v.loaded.as_ref().map(|l|l.records.records.iter().map(|r|&r.raw).collect::<Vec<_>>())
    }));
    json!({"requests":l.requests,"responses":l.responses,"settlements":l.settlements,
        "completed":l.settlements.len(),"networkCalls":0,"httpCapability":false,"loaded":loaded,
        "overlay":s.overlay==Some(Overlay::SlackList),"commentsOpen":comments::presentation(s).is_some(),
        "mutations":s.slack_lists.operations.len(),"fixtureView":s.slack_lists.fixture.is_some()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_recovery_cases_cannot_admit_the_list_destination_reader() {
        for case in ["file-recovery", "file-recovery-retirement", "", "foreign"] {
            assert!(!navigation_case(case));
        }
        assert!(navigation_case("file"));
        assert!(navigation_case("file-retirement"));
        assert!(navigation_case("file-emoji"));
        assert!(navigation_case("file-emoji-retirement"));
        assert!(navigation_case("file-emoji-cold"));
        assert!(navigation_case("file-encoded"));
    }
    #[test]
    fn original_raw_destination_passes_actual_document_and_record_parsers() {
        let raw = responses();
        let loaded = parse(&raw).unwrap();
        assert_eq!(loaded.document.id, source::LIST);
        assert_eq!(loaded.document.raw, raw[0]["file"]);
        assert_eq!(loaded.records.records.len(), 1);
        assert_eq!(loaded.records.records[0].raw, raw[1]["records"][0]);
    }
    #[test]
    fn target_id_and_record_owner_are_not_reconstructed_or_repaired() {
        for n in 0..3 {
            let mut raw = responses();
            match n {
                0 => raw[0]["file"]["id"] = json!("FFOREIGN01"),
                1 => raw[1]["list_id"] = json!("FFOREIGN01"),
                _ => raw[1]["records"][0]["list_id"] = json!("FFOREIGN01"),
            }
            assert!(parse(&raw).is_err());
        }
    }
}
