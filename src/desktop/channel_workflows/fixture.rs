//! Closed original-owner reader. Product projection is observed in the DOM,
//! independently of these request/response/receiver receipts.
use crate::app_home::about::management::fixture::first_publish as endpoint;
use crate::{
    scheduled::Context,
    state::{ChannelTab, ShellState},
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use super_platinum_core::{
    channel_workflows::Read,
    slack::{Error, PreparedRequest, client::RequestBody},
};
use tokio::sync::oneshot;
mod about;
pub(crate) mod raw;
pub(crate) use about::about_response;

pub(crate) fn is_case(case: &str) -> bool {
    super::details::fixture::is_case(case)
        || matches!(
            case,
            "first-tab-success"
                | "first-tab-empty"
                | "first-tab-failure"
                | "first-tab-retirement"
                | "first-tab-rate-limit"
        )
}
pub(crate) fn requested() -> bool {
    endpoint::requested() && is_case(&endpoint::case())
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    requested() && endpoint::capture(s).is_ok()
}
fn refused() -> Error {
    Error::Api("channel_workflows_closed_fixture".into())
}
#[derive(Clone)]
pub(crate) struct Model(Arc<Mutex<Data>>);
#[derive(Default)]
struct Data {
    details: super::details::fixture::Model,
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    about: Vec<Value>,
    held: Vec<(usize, oneshot::Sender<()>)>,
}
impl Model {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(Data::default())))
    }
    pub(crate) fn snapshot(&self) -> Value {
        let m = self.0.lock().unwrap();
        let mut value = json!({"requests":m.requests,"responses":m.responses,"settlements":m.settlements,"releases":m.releases,
            "about":m.about,"held":m.held.iter().map(|(n,tx)|json!({"serial":n,"receiverRetired":tx.is_closed()})).collect::<Vec<_>>(),"networkCalls":0});
        if super::details::fixture::requested() {
            value["details"] = m.details.snapshot();
        }
        value
    }
    pub(crate) fn details(&self) -> super::details::fixture::Model {
        self.0.lock().unwrap().details.clone()
    }
    pub(crate) fn release(&self, serial: usize) -> Result<Value, String> {
        let mut m = self.0.lock().unwrap();
        let index = m
            .held
            .iter()
            .position(|(n, _)| *n == serial)
            .ok_or("Original Workflows sender absent")?;
        let (_, tx) = m.held.remove(index);
        let retired = tx.is_closed();
        let value =
            json!({"serial":serial,"receiverRetired":retired,"delivered":tx.send(()).is_ok()});
        m.releases.push(value.clone());
        Ok(value)
    }
}
pub(crate) fn capture(s: &ShellState) -> Result<Model, Error> {
    if !enabled(s) {
        return Err(refused());
    }
    endpoint::tab_model(s).ok_or_else(refused)
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    endpoint::tab_model(s)
        .map(|m| m.snapshot())
        .unwrap_or(Value::Null)
}
fn parts(request: &PreparedRequest) -> Result<(&str, &[(String, String)]), Error> {
    if request.method != "POST" {
        return Err(refused());
    }
    let method = request
        .url
        .strip_prefix("https://example.test/api/")
        .and_then(|s| s.split('?').next())
        .ok_or_else(refused)?;
    let RequestBody::Form(fields) = &request.body else {
        return Err(refused());
    };
    Ok((method, fields))
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
        if s.channel_tab != ChannelTab::Workflows {
            return Err(refused());
        }
        let channel = s
            .core
            .active_channel
            .as_deref()
            .filter(|v| matches!(*v, raw::DEST | raw::PRIVATE))
            .ok_or_else(refused)?;
        let ctx = Context::capture(&s).ok_or_else(refused)?;
        let (method, fields) = parts(request)?;
        let expected = match method {
            "bookmarks.list" => Read::bookmarks(channel)?,
            "workflows.triggers.preview" if channel == raw::DEST => {
                Read::previews(&raw::IDS.iter().map(|s| s.to_string()).collect::<Vec<_>>())?
            }
            _ => return Err(refused()),
        }
        .request(&ctx.client, &ctx.session);
        endpoint::tab_request(request, &expected)?;
        let parent = endpoint::snapshot(&s);
        let accepted = parent["confirmationAdd"]["bookmarks"]
            .as_array()
            .ok_or_else(refused)?;
        if accepted.len() != 1 || accepted[0]["id"] != "BkFIRST01" {
            return Err(refused());
        }
        let mut m = model.0.lock().unwrap();
        let serial = m.requests.len() + 1;
        let preview = method == "workflows.triggers.preview";
        let first = preview && !m.requests.iter().any(|r| r["method"] == method);
        let hold = first && endpoint::case() == "first-tab-retirement";
        let result = if first && endpoint::case() == "first-tab-rate-limit" {
            Err(Error::RateLimited {
                retry_after_secs: Some(12),
            })
        } else if first && endpoint::case() == "first-tab-failure" {
            Err(Error::Offline("synthetic original preview offline".into()))
        } else {
            Ok(if preview {
                raw::previews()
            } else {
                raw::bookmarks(channel, accepted)
            })
        };
        m.requests.push(json!({"serial":serial,"method":method,"fields":fields.iter().filter(|(k,_)|!matches!(k.as_str(),"token"|"enterprise_token")).collect::<Vec<_>>(),
            "retrySafe":request.retry_safe(),"owner":{"account":s.core.active_account,"epoch":s.core.account_epoch,"team":s.core.active_team,"channel":channel,"channelGeneration":s.channel_generation,"tab":"Workflows","socket":s.core.workspaces["TWORKFLOW"].rt_generation}}));
        m.responses.push(json!({"serial":serial,"result":match &result{Ok(v)=>json!({"raw":v}),Err(Error::RateLimited{retry_after_secs})=>json!({"error":"rate-limited","retryAfterSeconds":retry_after_secs}),Err(_)=>json!({"error":"offline"})}}));
        let rx = if hold {
            let (tx, rx) = oneshot::channel();
            m.held.push((serial, tx));
            Some(rx)
        } else {
            None
        };
        drop(m);
        Ok((model, serial, rx, result))
    })();
    let (model, serial, rx, result) = match prepared {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    let mut settlement = Settlement {
        model,
        serial,
        returned: false,
    };
    if let Some(rx) = rx {
        if tokio::time::timeout(Duration::from_secs(45), rx)
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
