//! Immutable oR09/zbK1 observations. The desktop owner supplies cache/currentness.
use crate::{
    config::WorkspaceSession,
    slack::{PreparedRequest, SlackClient},
};
use serde_json::Value;

mod model;
mod offline;
pub use model::decode_model;
pub use offline::{Entity, Features, Offline};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Response,
    Model,
    Offline,
}

/// Closed NHDy request: the only added field is the captured wire reason.
pub fn model_request(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        workspace,
        "search.autocomplete.model",
        vec![("_x_reason", "AutocompleteModelCache".into())],
    )
}

/// The owner must use its captured workspace, not a result item's team.
pub fn offline_request(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        workspace,
        "search.autocomplete.offlineFeatures",
        vec![("_x_reason", "search-offline-features".into())],
    )
}

fn response(value: &Value) -> Result<(), Error> {
    if value.is_object() && value["ok"] == true && !value.get("error").is_some_and(|v| !v.is_null())
    {
        Ok(())
    } else {
        Err(Error::Response)
    }
}

#[cfg(test)]
mod tests;
