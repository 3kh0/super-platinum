//! Exact closed info/get transport. Responses, delivery and retirement are independent.
use crate::app_home::about::management::fixture::first_publish as endpoint;
use crate::{
    scheduled::Context,
    state::{ChannelTab, ShellState},
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use super_platinum_core::{
    channel_workflows::Read,
    slack::{Error, PreparedRequest, client::RequestBody},
};
use tokio::sync::oneshot;
pub(crate) mod raw;
pub(crate) mod thread;

pub(crate) fn is_case(case: &str) -> bool {
    matches!(
        case,
        "first-details-success"
            | "first-details-errors"
            | "first-details-close-held"
            | "first-details-supersession"
            | "first-details-account-retirement"
            | "first-details-rate-limit"
    )
}
pub(crate) fn requested() -> bool {
    endpoint::requested() && is_case(&endpoint::case())
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    requested() && endpoint::capture(s).is_ok()
}
fn refused() -> Error {
    Error::Api("closed_workflow_details_fixture".into())
}
#[derive(Clone, Default)]
pub(crate) struct Model(Arc<Mutex<Data>>);
#[derive(Default)]
struct Data {
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Vec<(usize, oneshot::Sender<()>)>,
}
impl Model {
    pub(crate) fn snapshot(&self) -> Value {
        let m = self.0.lock().unwrap();
        json!({"requests":m.requests,"responses":m.responses,"settlements":m.settlements,"releases":m.releases,"held":m.held.iter().map(|(n,tx)|json!({"serial":n,"receiverRetired":tx.is_closed()})).collect::<Vec<_>>(),"networkCalls":0})
    }
    pub(crate) fn release(&self, serial: usize) -> Result<Value, String> {
        let mut m = self.0.lock().unwrap();
        let i = m
            .held
            .iter()
            .position(|(n, _)| *n == serial)
            .ok_or("Original details sender missing")?;
        let (_, tx) = m.held.remove(i);
        let retired = tx.is_closed();
        let result =
            json!({"serial":serial,"receiverRetired":retired,"delivered":tx.send(()).is_ok()});
        m.releases.push(result.clone());
        Ok(result)
    }
}
pub(crate) fn capture(s: &ShellState) -> Result<Model, Error> {
    if !enabled(s) {
        return Err(refused());
    }
    endpoint::tab_model(s)
        .map(|m| m.details())
        .ok_or_else(refused)
}
struct Settlement {
    model: Model,
    serial: usize,
    returned: bool,
}
impl Drop for Settlement {
    fn drop(&mut self) {
        self.model
            .0
            .lock()
            .unwrap()
            .settlements
            .push(json!({"serial":self.serial,"returned":self.returned}));
    }
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let s = state.peek();
        let model = capture(&s)?;
        if s.channel_tab != ChannelTab::Workflows
            || s.core.active_channel.as_deref() != Some("CADDDEST01")
        {
            return Err(refused());
        }
        let selected = super::snapshot(&s);
        if selected["current"] != true || selected["status"] != "loading" {
            return Err(refused());
        }
        let trigger = selected["selection"]["triggerId"]
            .as_str()
            .filter(|v| matches!(*v, "TR_REVIEW" | "FtFolder01"))
            .ok_or_else(refused)?;
        let ctx = Context::capture(&s).ok_or_else(refused)?;
        let method = request
            .url
            .strip_prefix("https://example.test/api/")
            .and_then(|v| v.split('?').next())
            .ok_or_else(refused)?;
        let read = match method {
            "workflows.triggers.info" => Read::trigger_info(trigger)?,
            "functions.workflows.get" => Read::workflow(raw::workflow_id(trigger))?,
            _ => return Err(refused()),
        };
        endpoint::tab_request(request, &read.request(&ctx.client, &ctx.session))?;
        let RequestBody::Form(fields) = &request.body else {
            return Err(refused());
        };
        let mut m = model.0.lock().unwrap();
        let serial = m.requests.len() + 1;
        let case = endpoint::case();
        let hold = match case.as_str() {
            "first-details-close-held" => matches!(serial, 1 | 3),
            "first-details-supersession" => matches!(serial, 1 | 5),
            "first-details-account-retirement" => serial == 1,
            _ => false,
        };
        let result = if case == "first-details-rate-limit" && serial == 1 {
            Err(Error::RateLimited {
                retry_after_secs: Some(12),
            })
        } else if case == "first-details-errors" && serial == 4 {
            Err(Error::Offline("original workflow get unavailable".into()))
        } else if case == "first-details-errors" && serial <= 2 {
            Ok(
                json!({"ok":false,"error":if serial==1{"access_denied"}else{"trigger_not_found"},"opaque_receipt":format!("original-details-error-{serial}")}),
            )
        } else {
            Ok(if method == "workflows.triggers.info" {
                raw::trigger(trigger)
            } else {
                raw::workflow(trigger)
            })
        };
        m.requests.push(json!({"serial":serial,"method":method,"fields":fields.iter().filter(|(k,_)|!matches!(k.as_str(),"token"|"enterprise_token")).collect::<Vec<_>>(),"retrySafe":request.retry_safe(),"owner":selected["selection"],"attempt":selected["serial"]}));
        m.responses.push(json!({"serial":serial,"result":match &result{Ok(v)=>json!({"raw":v}),Err(Error::RateLimited{retry_after_secs})=>json!({"error":"rate-limited","retryAfterSeconds":retry_after_secs}),Err(_)=>json!({"error":"offline"})}}));
        let receiver = if hold {
            let (tx, rx) = oneshot::channel();
            m.held.push((serial, tx));
            Some(rx)
        } else {
            None
        };
        drop(m);
        Ok((model, serial, receiver, result))
    })();
    let (model, serial, receiver, result) = match prepared {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    let mut settlement = Settlement {
        model,
        serial,
        returned: false,
    };
    if let Some(rx) = receiver {
        if tokio::time::timeout(std::time::Duration::from_secs(45), rx)
            .await
            .ok()
            .and_then(Result::ok)
            .is_none()
        {
            return Some(Err(refused()));
        }
    }
    settlement.returned = true;
    Some(result)
}
