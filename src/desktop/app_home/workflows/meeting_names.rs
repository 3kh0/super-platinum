//! Meeting names use actual same-owner calendar responses. A failed read never
//! becomes an observed missing event. This store is lazy and has no timer.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::block_actions::home::workflow::meeting_events as meeting;

pub(in crate::app_home) struct Store {
    origin: Origin,
    id: u64,
    status: Option<meeting::CalendarStatus>,
    events: BTreeMap<String, meeting::Event>,
}
#[derive(Clone)]
pub(super) struct Witness {
    origin: Origin,
    id: u64,
}
impl Witness {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.origin.current(s)
            && s.app_home
                .workflow_meetings
                .as_ref()
                .is_some_and(|m| m.id == self.id && m.origin.current(s))
    }
}
/// Called only after the existing Home account/realtime envelope guard. Unknown
/// calendar changes invalidate rather than pretending to be a complete event.
pub(in crate::app_home) fn invalidate(s: &mut ShellState, team: &str) {
    if s.app_home
        .workflow_meetings
        .as_ref()
        .is_some_and(|m| m.origin.owner.ctx.session.team_id == team)
    {
        s.app_home.workflow_meetings = None;
    }
}
pub(super) async fn resolve(
    mut state: Signal<ShellState>,
    query: &entity_search::Query,
    links: BTreeSet<String>,
) -> Result<(BTreeMap<String, meeting::Event>, Witness), Error> {
    let witness = {
        let mut s = state.write();
        if !query.current(&s) {
            return Err(unavailable());
        }
        if s.app_home
            .workflow_meetings
            .as_ref()
            .is_none_or(|m| !m.origin.current(&s))
        {
            s.app_home.workflow_meetings = Some(Store {
                origin: query.owner().clone(),
                id: next(),
                status: None,
                events: BTreeMap::new(),
            });
        }
        let m = s.app_home.workflow_meetings.as_ref().unwrap();
        Witness {
            origin: m.origin.clone(),
            id: m.id,
        }
    };
    if links.is_empty() {
        return Ok((BTreeMap::new(), witness));
    }
    let status = {
        state
            .peek()
            .app_home
            .workflow_meetings
            .as_ref()
            .and_then(|m| m.status.clone())
    };
    let status = match status {
        Some(status) => status,
        None => {
            let ctx = &query.owner().owner.ctx;
            let raw = query
                .read(
                    state,
                    meeting::request::user_status(&ctx.client, &ctx.session),
                )
                .await?;
            let status = meeting::CalendarStatus::decode(raw).map_err(|_| unavailable())?;
            let mut s = state.write();
            if !query.current(&s) || !witness.current(&s) {
                return Err(unavailable());
            }
            s.app_home.workflow_meetings.as_mut().unwrap().status = Some(status.clone());
            status
        }
    };
    // A real disconnected status is kept distinctly. The caller may render the
    // initialized store's passive name; no event read or server absence invented.
    if !status.connected() {
        return Ok((BTreeMap::new(), witness));
    }
    let mut events = BTreeMap::new();
    for link in links {
        let retained = {
            let s = state.peek();
            if !query.current(&s) || !witness.current(&s) {
                return Err(unavailable());
            }
            s.app_home
                .workflow_meetings
                .as_ref()
                .unwrap()
                .events
                .get(&link)
                .cloned()
        };
        let event = match retained {
            Some(event) => event,
            None => {
                {
                    let s = state.peek();
                    if s.app_home.workflow_meetings.as_ref().unwrap().events.len() >= 4096 {
                        return Err(unavailable());
                    }
                }
                let ctx = &query.owner().owner.ctx;
                let request =
                    meeting::request::event_by_link(&ctx.client, &ctx.session, &status, &link)
                        .map_err(|_| unavailable())?;
                let raw = query.read(state, request).await?;
                let event = meeting::decode_event(&link, &raw).map_err(|_| unavailable())?;
                let mut s = state.write();
                if !query.current(&s)
                    || !witness.current(&s)
                    || s.app_home
                        .workflow_meetings
                        .as_ref()
                        .unwrap()
                        .status
                        .as_ref()
                        != Some(&status)
                {
                    return Err(unavailable());
                }
                s.app_home
                    .workflow_meetings
                    .as_mut()
                    .unwrap()
                    .events
                    .insert(link.clone(), event.clone());
                event
            }
        };
        events.insert(link, event);
    }
    Ok((events, witness))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    generation: String,
    locale: String,
    time_zone: String,
    names: BTreeMap<String, String>,
}
/// The actual formatter runs in the native renderer, with one captured clock
/// and local timezone for the batch. English is this application's UI locale,
/// not a guess from the machine's regional date-format setting.
pub(super) async fn format(
    state: Signal<ShellState>,
    query: &entity_search::Query,
    events: &BTreeMap<String, meeting::Event>,
    witness: &Witness,
) -> Result<BTreeMap<String, String>, Error> {
    if !query.current(&state.peek()) || !witness.current(&state.peek()) {
        return Err(unavailable());
    }
    let inputs = events
        .iter()
        .filter(|(_, event)| !event.deleted())
        .map(|(link, event)| {
            event
                .format_input()
                .map(|input| (link.clone(), serde_json::to_value(input).unwrap()))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(|_| unavailable())?;
    let generation = query.generation.to_string();
    let request = serde_json::json!({"generation":generation,"events":inputs});
    let script = format!(
        "const formatter = {};\n{}\ndioxus.send(workflowMeetingNames({},formatter));await dioxus.recv();",
        include_str!("../workflow/meeting_event_format.js"),
        include_str!("meeting_names.js"),
        request
    );
    let mut eval = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(5), eval.recv::<Option<Reply>>()).await;
    let _ = eval.send(());
    if !query.current(&state.peek()) || !witness.current(&state.peek()) {
        return Err(unavailable());
    }
    let reply = result
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)?;
    if reply.generation != generation
        || reply.locale != "en-US"
        || reply.time_zone.is_empty()
        || reply.names.keys().ne(inputs.keys())
    {
        return Err(unavailable());
    }
    Ok(reply.names)
}
fn unavailable() -> Error {
    Error::Api("home_workflow_meeting_observation_unavailable".into())
}
