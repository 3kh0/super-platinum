//! One explicit bounded drafts.list page. No absence or cursor grants write authority.
//! Captured h/H5 fetchDraftsApi uses next_ts from the final last_updated_ts.
use super::*;
use crate::slack::Error;

pub const PAGE_SIZE: usize = 25;

#[derive(Clone)]
pub struct Scan {
    expected: Record,
    next: Option<String>,
    anchor: Option<String>,
    power: u64,
    distance: u64,
}
pub enum Page {
    More(Scan),
    Found(String),
    Exhausted,
}
impl Scan {
    pub fn new(record: &Record) -> Result<Self, &'static str> {
        if record.phase() != Phase::Unknown || record.server_id().is_some() {
            return Err("Only an unresolved create needs draft discovery");
        }
        Ok(Self {
            expected: record.clone(),
            next: None,
            anchor: None,
            power: 1,
            distance: 0,
        })
    }
    pub fn matches(&self, record: &Record) -> bool {
        &self.expected == record
    }
    pub fn request(
        &self,
        client: &SlackClient,
        workspace: &WorkspaceSession,
    ) -> Result<PreparedRequest, Error> {
        if workspace.team_id != self.expected.intent.owner.team
            || workspace.user_id != self.expected.intent.owner.user
        {
            return Err(Error::Api(
                "scheduled_forward_discovery_owner_changed".into(),
            ));
        }
        let mut fields = vec![
            ("is_active", "true".into()),
            ("limit", PAGE_SIZE.to_string()),
        ];
        if let Some(next) = &self.next {
            fields.push(("next_ts", next.clone()));
        }
        Ok(client.rest_form(workspace, "drafts.list", fields))
    }
    pub fn accept(mut self, response: &Value) -> Result<Page, &'static str> {
        if response.get("ok") != Some(&Value::Bool(true))
            || response.get("error").is_some_and(|v| !v.is_null())
        {
            return Err("Draft discovery did not return a positive page");
        }
        let rows = response
            .get("drafts")
            .and_then(Value::as_array)
            .ok_or("Draft page is missing")?;
        if rows.len() > PAGE_SIZE {
            return Err("Draft page exceeded the requested bound");
        }
        let more = match response.get("has_more") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(value)) => *value,
            _ => return Err("Draft continuation is malformed"),
        };
        let mut owned = rows.iter().filter(|v| {
            v.get("client_msg_id").and_then(Value::as_str)
                == Some(self.expected.intent.client_msg_id.as_str())
        });
        if let Some(row) = owned.next() {
            if owned.next().is_some() {
                return Err("Duplicate matching draft identities need review");
            }
            let envelope = json!({"ok":true,"draft":row});
            if !request::complete_for(&self.expected, &envelope) {
                return Err("Matching draft identity is incomplete");
            }
            // The caller must read drafts.info again. A differing complete draft
            // is useful conflict evidence, never permission to overwrite it.
            return Ok(Page::Found(
                row["id"].as_str().ok_or("Missing draft ID")?.into(),
            ));
        }
        if !more || rows.is_empty() {
            return Ok(Page::Exhausted);
        }
        let next = rows
            .last()
            .and_then(|v| v.get("last_updated_ts"))
            .and_then(cursor)
            .ok_or("Draft continuation is malformed")?;
        if self.next.as_ref() == Some(&next) || self.anchor.as_ref() == Some(&next) {
            return Err("Draft continuation repeated without progress");
        }
        // Brent's cycle witness retains one cursor, not an ever-growing history.
        if self.anchor.is_none() || self.distance == self.power {
            self.anchor = Some(next.clone());
            self.distance = 0;
            self.power = self
                .power
                .checked_mul(2)
                .ok_or("Draft continuation counter exhausted")?;
        }
        self.distance = self
            .distance
            .checked_add(1)
            .ok_or("Draft continuation counter exhausted")?;
        self.next = Some(next);
        Ok(Page::More(self))
    }
}
fn cursor(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(v) => v.clone(),
        Value::Number(v) => v.to_string(),
        _ => return None,
    };
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, "0"));
    (text.len() <= 64
        && !whole.is_empty()
        && whole.bytes().all(|x| x.is_ascii_digit())
        && !fraction.is_empty()
        && fraction.bytes().all(|x| x.is_ascii_digit())
        && text.bytes().any(|x| matches!(x, b'1'..=b'9')))
    .then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::client::RequestBody;
    fn record() -> Record {
        let intent = Intent::new(
            Owner {
                team: "T1".into(),
                user: "U1".into(),
            },
            Source {
                channel: "C1".into(),
                timestamp: "100.000001".into(),
                thread: None,
                author: Some("U1".into()),
                permalink: "https://fixture.slack.com/archives/C1/p100000001".into(),
                private: false,
            },
            Destination::Conversation("C2".into()),
            "Destination".into(),
            vec![],
            2000,
            false,
            1000,
        )
        .unwrap();
        Record {
            id: intent.client_msg_id.clone(),
            intent,
            revision: 1,
            phase: Phase::Unknown,
            action: Action::Schedule,
            attempt: Some(uuid::Uuid::new_v4().to_string()),
            response_recorded: false,
            retry_at: 0,
            updated_at: 1001,
            reviewed: None,
            observed: None,
            outcome: String::new(),
        }
    }
    fn workspace() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        }
    }
    fn page(cursor: &str) -> Value {
        json!({"ok":true,"has_more":true,"drafts":[{"client_msg_id":"other","last_updated_ts":cursor}]})
    }
    fn more(scan: Scan, value: &Value) -> Scan {
        match scan.accept(value).unwrap() {
            Page::More(s) => s,
            _ => panic!("continuation expected"),
        }
    }
    #[test]
    fn exact_cursor_requests_and_unbounded_valid_manual_progress() {
        let r = record();
        let mut scan = Scan::new(&r).unwrap();
        let client = SlackClient::default();
        let workspace = workspace();
        for n in (1..=240).rev() {
            let request = scan.request(&client, &workspace).unwrap();
            let RequestBody::Form(fields) = request.body else {
                panic!("form")
            };
            assert!(fields.contains(&("limit".into(), "25".into())));
            assert!(fields.contains(&("is_active".into(), "true".into())));
            if n == 240 {
                assert!(!fields.iter().any(|(k, _)| k == "next_ts"));
            } else {
                assert!(fields.contains(&("next_ts".into(), format!("{}.000001", n + 1))));
            }
            scan = more(scan, &page(&format!("{n}.000001")));
        }
        assert!(scan.matches(&r));
        let mut other = workspace;
        other.user_id = "U2".into();
        assert!(scan.request(&client, &other).is_err());
    }
    #[test]
    fn cycles_malformed_and_empty_pages_never_imply_rejection() {
        let r = record();
        let scan = Scan::new(&r).unwrap();
        assert!(matches!(
            scan.clone()
                .accept(&json!({"ok":true,"drafts":[],"has_more":true}))
                .unwrap(),
            Page::Exhausted
        ));
        for bad in [
            json!({"ok":false,"drafts":[],"has_more":false}),
            json!({"ok":true,"drafts":[],"has_more":"true"}),
            page("NaN"),
            page("0"),
            json!({"ok":true,"has_more":false,"drafts":vec![json!({});26]}),
        ] {
            assert!(scan.clone().accept(&bad).is_err());
        }
        let s = more(scan.clone(), &page("10"));
        assert!(s.clone().accept(&page("10")).is_err());
        let s = more(s, &page("9"));
        assert!(s.accept(&page("10")).is_err());
        assert_eq!(r.phase(), Phase::Unknown);
    }
    #[test]
    fn complete_changed_candidate_is_discovery_not_write_authority() {
        let r = record();
        let raw = json!({"id":"Dr1","client_msg_id":r.intent.client_msg_id,"blocks":r.intent.blocks,"destinations":r.intent.destination.wire(),"file_ids":[],"date_scheduled":3000,"last_updated_ts":"1234","is_deleted":false,"is_sent":false,"is_from_composer":true});
        let response = json!({"ok":true,"has_more":false,"drafts":[raw.clone()]});
        assert!(
            matches!(Scan::new(&r).unwrap().accept(&response).unwrap(),Page::Found(id) if id=="Dr1")
        );
        assert!(
            Scan::new(&r)
                .unwrap()
                .accept(&json!({"ok":true,"has_more":false,"drafts":[raw.clone(),raw]}))
                .is_err()
        );
        let mut partial = response;
        partial["drafts"][0]
            .as_object_mut()
            .unwrap()
            .remove("blocks");
        assert!(Scan::new(&r).unwrap().accept(&partial).is_err());
        assert_eq!(r.phase(), Phase::Unknown);
    }
}
