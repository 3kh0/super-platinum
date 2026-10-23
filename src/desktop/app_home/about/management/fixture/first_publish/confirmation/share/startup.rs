//! Closed startup responses before any Share surface exists. No Store injection.
use super::*;
use super_platinum_core::block_actions::{name_policy, shortcuts::grid};

pub(crate) const CASE_ENV: &str = "SUPER_PLATINUM_WORKFLOW_SHARE_STARTUP_CASE";
pub(crate) fn case() -> String {
    std::env::var(CASE_ENV).unwrap_or_default()
}
pub(crate) fn requested() -> bool {
    std::env::var(search::CHECK_ENV).as_deref() == Ok("1")
        && std::env::var(search::CASE_ENV).as_deref() == Ok("startup")
        && matches!(
            case().as_str(),
            "success" | "failure" | "missing" | "prefs-retired" | "socket-retired"
        )
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    requested() && crate::fixture::is_fixture() && capture(s).is_ok() && context(s).is_ok()
}
#[derive(Default)]
pub(super) struct Data {
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Vec<(usize, oneshot::Sender<()>)>,
}
impl Data {
    pub(super) fn snapshot(&self) -> Value {
        json!({"case":case(),"requests":self.requests,"responses":self.responses,
            "settlements":self.settlements,"releases":self.releases,
            "held":self.held.iter().map(|(n,_)|n).collect::<Vec<_>>(),
            "networkCalls":0,"storeInjection":false})
    }
}
pub(crate) fn release(model: &Model, serial: usize) -> Result<Value, String> {
    let mut m = model.0.lock().unwrap();
    let d = m.startup.as_mut().ok_or("Startup fixture absent")?;
    let i = d
        .held
        .iter()
        .position(|(n, _)| *n == serial)
        .ok_or("Original startup sender absent")?;
    let (_, tx) = d.held.remove(i);
    let v =
        json!({"serial":serial,"receiverRetired":tx.is_closed(),"delivered":tx.send(()).is_ok()});
    d.releases.push(v.clone());
    Ok(v)
}
struct Settlement {
    model: Model,
    serial: usize,
    returned: bool,
}
impl Drop for Settlement {
    fn drop(&mut self) {
        if let Some(d) = self.model.0.lock().unwrap().startup.as_mut() {
            d.settlements
                .push(json!({"serial":self.serial,"returned":self.returned}));
        }
    }
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let (method, fields) = crate::app_home::about::management::fixture::decode(request).ok()?;
    if !matches!(
        method,
        "client.gantryBoot" | "users.prefs.get" | "team.prefs.get" | "experiments.getByUser"
    ) {
        return None;
    }
    let prepared = (|| {
        let s = state.peek();
        if !enabled(&s) {
            return Err(closed());
        }
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let expected = match method {
            "client.gantryBoot" => grid::discovery(&client, &session),
            "users.prefs.get" => name_policy::request::user(&client, &session),
            "team.prefs.get" => {
                let target = fields
                    .iter()
                    .find(|(k, _)| k == "target_team")
                    .map(|(_, v)| v.as_str())
                    .ok_or_else(closed)?;
                if target != session.team_id {
                    return Err(closed());
                }
                name_policy::request::team_for(&client, &session, target).map_err(|_| closed())?
            }
            "experiments.getByUser" => api::list_experiments(&client, &session),
            _ => return Err(closed()),
        };
        super::super::super::picker::same_request(request, &expected)?;
        let parent = crate::app_home::about::management::fixture::first_publish::capture(&s)
            .map_err(|_| closed())?;
        let source =
            parent.0.lock().unwrap().seed.as_ref().ok_or_else(closed)?["boot"]["self"].clone();
        if source["id"] != "UWORKFLOW" || source["team_id"] != "TWORKFLOW" {
            return Err(closed());
        }
        let mut raw = search::ranking::startup_response(method, &source)?;
        let scenario = case();
        let offline = method == "users.prefs.get" && scenario == "failure";
        if method == "users.prefs.get" && scenario == "missing" {
            raw = json!({"ok":true,"opaque_receipt":"original-startup-missing-prefs"});
        }
        let mut m = model.0.lock().unwrap();
        let d = m.startup.as_mut().ok_or_else(closed)?;
        if d.requests.len() >= 12 {
            return Err(closed());
        }
        let serial = d.requests.len() + 1;
        let hold = method == "users.prefs.get"
            && serial == 2
            && matches!(
                scenario.as_str(),
                "success" | "prefs-retired" | "socket-retired"
            );
        d.requests.push(json!({"serial":serial,"method":method,"body":safe_body(&request.body),
            "endpoint":request.url.split('?').next(),"retrySafe":request.retry_safe(),
            "owner":owner(&s,&session),"rtGeneration":s.core.workspaces[&session.team_id].rt_generation,
            "shareMounted":active(&s)}));
        d.responses.push(if offline {
            json!({"serial":serial,"error":"offline"})
        } else {
            json!({"serial":serial,"raw":raw})
        });
        let rx = if hold {
            let (tx, rx) = oneshot::channel();
            d.held.push((serial, tx));
            Some(rx)
        } else {
            None
        };
        drop(m);
        Ok((model, serial, raw, offline, rx))
    })();
    let (model, serial, raw, offline, rx) = match prepared {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    let mut settlement = Settlement {
        model,
        serial,
        returned: false,
    };
    if let Some(rx) = rx {
        if !matches!(
            tokio::time::timeout(Duration::from_secs(45), rx).await,
            Ok(Ok(()))
        ) {
            return Some(Err(closed()));
        }
    }
    settlement.returned = true;
    Some(if offline {
        Err(Error::Offline(
            "Synthetic startup preferences unavailable".into(),
        ))
    } else {
        Ok(raw)
    })
}
