//! Raw name-policy fixture. No policy Observation is manufactured here.
use super::*;
#[derive(Clone, Default)]
pub(crate) struct Model(Arc<Mutex<Ledger>>);
#[derive(Default)]
struct Ledger {
    initial_boot: Option<Value>,
    policy_override: i32,
    requests: Vec<Value>,
    mode: Mode,
    held: std::collections::VecDeque<(oneshot::Sender<Result<Value, Error>>, Value)>,
    completed: usize,
    cancelled: usize,
}
#[derive(Clone, Copy, Default)]
pub(crate) enum Mode {
    #[default]
    Success,
    Failure,
    Malformed,
    Held,
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    super::enabled(s)
        && matches!(
            case().as_str(),
            "user-real" | "user-display" | "user-refresh" | "user-deactivated"
        )
}
fn model(s: &ShellState) -> Result<Model, Error> {
    if !enabled(s) {
        return Err(refused());
    }
    super::model(s)?
        .0
        .lock()
        .unwrap()
        .users
        .clone()
        .ok_or_else(refused)
}
pub(super) fn install(s: &mut ShellState) {
    use super_platinum_core::state::posting_metadata::authority::BootRead;
    let transport = s.core.transport.clone().unwrap();
    let session = s.core.session.as_ref().unwrap().workspaces["TWORKFLOW"].clone();
    let read = BootRead::begin_full(&mut s.core, &transport, &session).unwrap();
    let request = read.request(&s.core).unwrap();
    let (method, args) = fields(&request).unwrap();
    let mut raw = boot();
    raw["team"]["name"] = json!(session.name);
    assert!(read.apply(&mut s.core, raw, true).unwrap());
    assert!(s.app_home.about.surface.as_ref().unwrap().owner.current(s));
    let ledger = Ledger {
        policy_override: if case() == "user-display" { -1 } else { 1 },
        initial_boot: Some(json!({"method":method,"args":args,"applied":true})),
        ..Default::default()
    };
    s.app_home
        .about
        .management
        .activity_fixture
        .as_ref()
        .unwrap()
        .0
        .lock()
        .unwrap()
        .users = Some(Model(Arc::new(Mutex::new(ledger))));
}

fn user(id: &str, first: &str) -> Value {
    json!({"id":id,"name":format!("{}_username",first.to_lowercase()),"real_name":format!("{first} top real"),"team_id":"TWORKFLOW","updated":101,"deleted":case()=="user-deactivated" && id=="UACTADA01","is_bot":false,"is_stranger":false,"profile":{"real_name":format!("{first} profile real"),"real_name_normalized":format!("{first} profile real"),"display_name":format!("{first} display"),"display_name_normalized":format!("{first} display")}})
}
fn boot() -> Value {
    let owner = user("UWORKFLOW", "Owner");
    json!({"ok":true,"self":owner,"team":{"id":"TWORKFLOW","name":"Activity fixture"},"users":[owner,user("UACTADA01","Ada"),user("UACTZOE01","Zoe")],"workspaces":[{"id":"TWORKFLOW","prefs":{"display_real_names":true}}],"prefs":{"display_real_names":true},"channels":[],"ims":[],"groups":[],"mpims":[]})
}
pub(super) fn messages(row: &mut Value) {
    row["trigger_message"] = json!({"template":"Review {person} with {person}","variables":{"person":{"type":"USER","value":"UACTADA01"}}});
    row["step_events"][0]["outcome_message"] = json!({"template":"Completed by {person}","variables":{"person":{"type":"USER","value":"UACTZOE01"}}});
}
pub(crate) fn policy_override(s: &ShellState, value: i32) -> Result<(), String> {
    if !matches!(case().as_str(), "user-refresh" | "user-deactivated") || !matches!(value, 1 | -1) {
        return Err("Mutable policy is restricted to refresh fixture".into());
    }
    model(s)
        .map_err(|_| "User fixture unavailable")?
        .0
        .lock()
        .unwrap()
        .policy_override = value;
    Ok(())
}
pub(crate) fn mode(s: &ShellState, mode: Mode) -> Result<(), String> {
    model(s)
        .map_err(|_| "User policy fixture unavailable")?
        .0
        .lock()
        .unwrap()
        .mode = mode;
    Ok(())
}
fn fields(request: &PreparedRequest) -> Result<(String, Value), Error> {
    if request.method != "POST" {
        return Err(refused());
    }
    let url = url::Url::parse(&request.url).map_err(|_| refused())?;
    if url.scheme() != "https"
        || url.host_str() != Some("example.test")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(refused());
    }
    let method = url.path().strip_prefix("/api/").ok_or_else(refused)?;
    if !matches!(
        method,
        "client.userBoot" | "client.gantryBoot" | "users.prefs.get" | "team.prefs.get"
    ) {
        return Err(refused());
    }
    let RequestBody::Form(fields) = &request.body else {
        return Err(refused());
    };
    let mut args = serde_json::Map::new();
    for (key, value) in fields {
        if key != "token" && args.insert(key.clone(), json!(value)).is_some() {
            return Err(refused());
        }
    }
    Ok((method.into(), Value::Object(args)))
}
fn raw(method: &str, args: &Value, policy_override: i32) -> Result<Value, Error> {
    Ok(match method {
        "client.userBoot" => boot(),
        "client.gantryBoot"
            if args["include_channels"] == "false" && args["_x_reason"] == "fetch-client-data" =>
        {
            boot()
        }
        "users.prefs.get" => {
            json!({"ok":true,"prefs":{"display_real_names_override":policy_override}})
        }
        "team.prefs.get" if args.get("target_team").is_none_or(|v| v == "TWORKFLOW") => {
            json!({"ok":true,"prefs":{"display_real_names":true}})
        }
        _ => return Err(refused()),
    })
}
pub(crate) async fn request(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    let model = model(&state.peek())?;
    let (method, args) = fields(request)?;
    let response = raw(&method, &args, model.0.lock().unwrap().policy_override)?;
    let receiver = {
        let mut m = model.0.lock().unwrap();
        if m.held.iter().any(|(send, _)| !send.is_closed()) {
            return Err(refused());
        }
        m.requests.push(json!({"method":method,"args":args}));
        let mode = if method == "users.prefs.get" {
            std::mem::take(&mut m.mode)
        } else {
            Mode::Success
        };
        match mode {
            Mode::Held => {
                let (send, recv) = oneshot::channel();
                m.held.push_back((send, response));
                recv
            }
            Mode::Failure => {
                m.completed += 1;
                return Err(Error::Offline("Synthetic policy unavailable".into()));
            }
            Mode::Malformed => {
                m.completed += 1;
                return Ok(json!({"ok":true,"prefs":false}));
            }
            Mode::Success => {
                m.completed += 1;
                return Ok(response);
            }
        }
    };
    struct Finish {
        model: Model,
        completed: bool,
    }
    impl Drop for Finish {
        fn drop(&mut self) {
            let mut m = self.model.0.lock().unwrap();
            m.completed += 1;
            if !self.completed {
                m.cancelled += 1
            }
        }
    }
    let mut finish = Finish {
        model,
        completed: false,
    };
    let result = receiver.await.map_err(|_| refused())?;
    finish.completed = true;
    result
}
pub(crate) fn release(s: &ShellState) -> Result<Value, String> {
    let m = model(s).map_err(|_| "User fixture unavailable")?;
    let (send, raw) =
        m.0.lock()
            .unwrap()
            .held
            .pop_front()
            .ok_or("No held policy read")?;
    let delivered = send.send(Ok(raw)).is_ok();
    Ok(json!({"delivered":delivered,"cancelledBeforeReply":!delivered}))
}
pub(crate) fn invalidate(s: &mut ShellState) -> Result<Value, String> {
    model(s).map_err(|_| "User fixture unavailable")?;
    let before = s.app_home.workflow_names_revision;
    let generation = s
        .core
        .workspaces
        .get("TWORKFLOW")
        .ok_or("Missing workspace")?
        .rt_generation;
    let event = super_platinum_core::slack::events::RtEvent::PreferencesChanged {
        name: "display_real_names_override".into(),
        value: json!(
            model(s)
                .map_err(|_| "User fixture unavailable")?
                .0
                .lock()
                .unwrap()
                .policy_override
        ),
    };
    crate::app_home::realtime(s, "TWORKFLOW", generation, &event);
    if s.app_home.workflow_names_revision == before {
        return Err("Current About preference event did not retire policy".into());
    }
    Ok(json!({"before":before,"after":s.app_home.workflow_names_revision}))
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let searches = super::super::members::fixture::snapshot(s)["ledger"]["searches"]
        .as_u64()
        .unwrap_or(0);
    let ledger=s.app_home.about.management.activity_fixture.as_ref().and_then(|m|m.0.lock().unwrap().users.clone()).map(|m|{let m=m.0.lock().unwrap();json!({"initialBoot":m.initial_boot,"requests":m.requests,"completed":m.completed,"cancelled":m.cancelled,"held":!m.held.is_empty(),"heldCount":m.held.len(),"networkCalls":0,"searches":searches})});
    json!({"ledger":ledger,"override":s.app_home.about.management.activity_fixture.as_ref().and_then(|m|m.0.lock().unwrap().users.clone()).map(|m|m.0.lock().unwrap().policy_override),"namesRevision":s.app_home.workflow_names_revision,"users":s.core.workspaces.get("TWORKFLOW").map(|w|["UACTADA01","UACTZOE01"].into_iter().map(|id|json!({"id":id,"raw":w.users.get(id)})).collect::<Vec<_>>())})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_user_fixture_adopts_raw_boot_without_installing_name_policy() {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        super::super::install_case(&mut s, "user-real");
        let w = &s.core.workspaces["TWORKFLOW"];
        let channel = &w.channels[s.core.active_channel.as_ref().unwrap()];
        assert!(channel.is_im);
        assert_eq!(channel.user.as_deref(), Some("U_HOME_BOT"));
        assert!(w.users["U_HOME_BOT"].is_bot);
        let ada = &w.users["UACTADA01"];
        assert_eq!(ada.name.as_deref(), Some("ada_username"));
        assert_eq!(ada.real_name.as_deref(), Some("Ada top real"));
        assert_eq!(
            ada.profile.as_ref().unwrap().real_name.as_deref(),
            Some("Ada profile real")
        );
        assert_eq!(
            ada.profile.as_ref().unwrap().display_name.as_deref(),
            Some("Ada display")
        );
        assert!(s.app_home.about.surface.as_ref().unwrap().owner.current(&s));
        assert!(s.app_home.about.management.documents.is_empty());
        let m = s
            .app_home
            .about
            .management
            .activity_fixture
            .as_ref()
            .unwrap()
            .0
            .lock()
            .unwrap()
            .users
            .clone()
            .unwrap();
        let m = m.0.lock().unwrap();
        assert!(m.requests.is_empty());
        assert_eq!(m.initial_boot.as_ref().unwrap()["applied"], true);
    }
}
