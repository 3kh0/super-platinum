//! Stable NDJSON control-plane types shared by desktop shells.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentRequest {
    #[serde(default)]
    pub id: u64,
    #[serde(flatten)]
    pub cmd: AgentCommand,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum AgentCommand {
    Ping,
    Help,
    State,
    OpenPalette,
    ClosePalette,
    SetQuery {
        query: String,
    },
    Type {
        text: String,
    },
    Move {
        #[serde(default = "default_move_delta")]
        delta: isize,
    },
    Submit,
    SelectEntry {
        index: usize,
    },
    SelectChannel {
        channel: String,
    },
    SelectWorkspace {
        team: String,
    },
    Search {
        query: String,
    },
    ClearSearch,
    OpenSettings,
    CloseSettings,
    OpenProfile {
        user: String,
    },
    CloseProfile,
    Screenshot {
        #[serde(default)]
        path: Option<String>,
    },
    AllowDestructive {
        enabled: bool,
    },
    Send,
    Toast {
        text: String,
    },
    MainView {
        view: String,
    },
    ActivitySelect {
        index: usize,
    },
}

fn default_move_delta() -> isize {
    1
}

impl AgentCommand {
    pub fn is_destructive(&self) -> bool {
        matches!(self, AgentCommand::Send)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentResponse {
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AgentResponse {
    pub fn ok(id: u64, data: Value) -> Self {
        Self {
            id,
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(id: u64, error: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            data: None,
            error: Some(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_commands_keep_their_wire_shape() {
        let request: AgentRequest =
            serde_json::from_str(r#"{"id":3,"cmd":"move","delta":-1}"#).unwrap();
        assert!(matches!(request.cmd, AgentCommand::Move { delta: -1 }));
        let response = AgentResponse::ok(3, serde_json::json!({"pong": true}));
        assert_eq!(
            serde_json::to_value(response).unwrap(),
            serde_json::json!({"id": 3, "ok": true, "data": {"pong": true}})
        );
    }
}
