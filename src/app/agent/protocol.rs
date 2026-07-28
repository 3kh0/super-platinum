use super::*;

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
    pub(super) fn is_destructive(&self) -> bool {
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
    pub(super) fn ok(id: u64, data: Value) -> Self {
        Self {
            id,
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub(super) fn err(id: u64, error: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            data: None,
            error: Some(error.into()),
        }
    }
}
