//! Explicit existing-token display witnesses. No completed query authority.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::block_actions::name_policy;
mod reply;

pub(crate) struct FileTitles {
    team: String,
    canvas_locale: Option<super::super::super::title_locale::CanvasTitle>,
    localized: Option<BTreeMap<String, CanonicalTitle>>,
    source: Arc<file_source::Snapshot>,
    tokens: Vec<Value>,
    excluded_tokens: Vec<Value>,
    required: BTreeSet<String>,
    users: Option<Arc<users::Users>>,
    policy: Option<name_policy::Snapshot>,
    owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
}
impl FileTitles {
    pub(crate) fn tokens(blocks: &[Value]) -> Vec<Value> {
        let mut out = Vec::new();
        let mut pending: Vec<_> = blocks.iter().rev().collect();
        while let Some(v) = pending.pop() {
            if matches!(v["type"].as_str(), Some("canvas" | "file")) {
                out.push(v.clone());
            }
            if let Some(elements) = v["elements"].as_array() {
                pending.extend(elements.iter().rev());
            }
        }
        out
    }
    /// Only positively observed supported user-title dependencies add a gate.
    /// Other persisted token variants keep their pre-existing fallback behavior.
    pub(crate) fn dependent_tokens(shell: &ShellState, team: &str, blocks: &[Value]) -> Vec<Value> {
        let Ok(source) = file_source::Snapshot::capture(shell, team) else {
            return Vec::new();
        };
        let Some(user) = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(team))
            .map(|s| s.user_id.as_str())
        else {
            return Vec::new();
        };
        Self::tokens(blocks)
            .into_iter()
            .filter(|t| {
                if t["type"] != "canvas"
                    || ["section_id", "record_id", "is_skill_invocation"]
                        .iter()
                        .any(|k| truth(&t[*k]))
                {
                    return false;
                }
                let Some(id) = t["file_id"].as_str() else {
                    return false;
                };
                let Ok(title) = source.title(id) else {
                    return false;
                };
                kind(title.effective()).is_ok_and(|k| k == file_titles::Kind::Canvas)
                    && file_titles::user_ids(title.effective()).is_ok_and(|ids| !ids.is_empty())
                    && RichTextContext::new_ordered(team, user)
                        .and_then(|mut c| c.file(t, "File".into()).map(|_| ()))
                        .is_ok()
            })
            .collect()
    }
    pub(crate) fn capture(
        shell: &ShellState,
        team: &str,
        tokens: Vec<Value>,
        owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
    ) -> Result<Self, Error> {
        Self::capture_with_context(shell, team, tokens, owner, None)
    }
    /// Explicit caller-owned presentation, never an implicit shared default.
    pub(crate) fn capture_with_context(
        shell: &ShellState,
        team: &str,
        tokens: Vec<Value>,
        owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
        canvas_locale: Option<super::super::super::title_locale::CanvasTitle>,
    ) -> Result<Self, Error> {
        if !owner(shell) {
            return Err(unavailable());
        }
        let source = file_source::Snapshot::capture(shell, team)?;
        let mut required = BTreeSet::new();
        for token in &tokens {
            let id = token["file_id"].as_str().ok_or_else(unavailable)?;
            let title = source.title(id)?;
            if kind(title.effective())? == file_titles::Kind::Canvas {
                required
                    .extend(file_titles::user_ids(title.effective()).map_err(|_| unavailable())?);
            }
        }
        Ok(Self {
            team: team.into(),
            canvas_locale,
            localized: None,
            source,
            tokens,
            excluded_tokens: Vec::new(),
            required,
            users: None,
            policy: None,
            owner,
        })
    }
    pub(crate) fn required(&self) -> &BTreeSet<String> {
        &self.required
    }
    pub(crate) fn source_current(&self, shell: &ShellState) -> bool {
        (self.owner)(shell) && self.metadata_current(shell)
    }
    /// Original file receipts only, for distinguishing a new same-file source
    /// from policy/member retirement. This is not display or caller authority.
    pub(crate) fn metadata_current(&self, shell: &ShellState) -> bool {
        if self.canvas_locale.is_some()
            && !self.tokens.is_empty()
            && self.excluded_tokens.is_empty()
        {
            self.tokens.iter().all(|token| {
                token["file_id"]
                    .as_str()
                    .is_some_and(|id| self.source.title_current(shell, id))
            })
        } else {
            self.source.current(shell)
        }
    }
    pub(crate) fn complete(
        mut self,
        shell: &ShellState,
        users: Option<Arc<users::Users>>,
        policy: Option<name_policy::Snapshot>,
        guard: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
    ) -> Result<Self, Error> {
        self.users = users;
        self.policy = policy;
        let prior = self.owner;
        self.owner = Arc::new(move |s| prior(s) && guard(s));
        if !self.current(shell) {
            return Err(unavailable());
        }
        // An opted-in owner retains the same canonical/Quip receipt for
        // actual display and passive proof; later runtime replacement refuses it.
        if self.canvas_locale.is_some() {
            let ids = self
                .tokens
                .iter()
                .map(|t| t["file_id"].as_str().ok_or_else(unavailable))
                .collect::<Result<BTreeSet<_>, _>>()?;
            self.localized = Some(
                ids.into_iter()
                    .map(|id| {
                        self.canonical(shell, id)
                            .map(|title| (id.to_owned(), title))
                    })
                    .collect::<Result<_, _>>()?,
            );
        }
        // Validate every actual token variant before publishing a mount label.
        self.labels(shell)?;
        Ok(self)
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.source_current(shell)
            && self
                .localized
                .as_ref()
                .is_none_or(|titles| titles.values().all(|t| t.current_quip(shell)))
            && (self.required.is_empty()
                || (self.policy.is_some() && self.users.as_ref().is_some_and(|u| u.current(shell))))
    }
    fn canonical(&self, shell: &ShellState, id: &str) -> Result<CanonicalTitle, Error> {
        if !self.current(shell) {
            return Err(unavailable());
        }
        if let Some(titles) = &self.localized {
            return titles.get(id).cloned().ok_or_else(unavailable);
        }
        CanonicalTitle::capture_with_context(
            shell,
            &self.source.title(id)?,
            id,
            self.users.as_deref().zip(self.policy.as_ref()),
            self.canvas_locale,
        )
    }
    pub(crate) fn label(&self, shell: &ShellState, token: &Value) -> Result<String, Error> {
        if !self.tokens.contains(token) {
            return Err(unavailable());
        }
        let id = token["file_id"].as_str().ok_or_else(unavailable)?;
        let canonical = self.canonical(shell, id)?;
        let expected = if canonical.kind == file_titles::Kind::Canvas {
            "canvas"
        } else {
            "file"
        };
        if token["type"] != expected || !canonical.current_quip(shell) {
            return Err(unavailable());
        }
        let user = &shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&self.team))
            .ok_or_else(unavailable)?
            .user_id;
        RichTextContext::new_ordered(&self.team, user)?.file(token, canonical.label.clone())?;
        Ok(canonical.label)
    }
    pub(crate) fn labels(&self, shell: &ShellState) -> Result<BTreeMap<String, String>, Error> {
        self.tokens
            .iter()
            .map(|t| {
                Ok((
                    serde_json::to_string(t).map_err(|_| unavailable())?,
                    self.label(shell, t)?,
                ))
            })
            .collect()
    }
    pub(crate) fn snapshot(&self, shell: &ShellState) -> Value {
        let mut value = json!({"current":self.current(shell),"tokens":self.tokens,"required":self.required,
            "files":self.tokens.iter().map(|t|{let id=t["file_id"].as_str().unwrap_or_default(); self.source.title(id).map(|r|json!({"id":id,"source":r.source(),"effective":r.effective(),"receipt":r.receipt()})).unwrap_or(Value::Null)}).collect::<Vec<_>>(),
            "users":self.users.as_ref().map(|u|u.snapshot(shell)),"labels":self.labels(shell).ok()});
        if !self.excluded_tokens.is_empty() {
            value["excludedTokens"] = json!(self.excluded_tokens);
        }
        if let (Some(locale), Some(titles)) = (self.canvas_locale, &self.localized) {
            let proof = titles
                .iter()
                .filter(|(_, title)| title.kind == file_titles::Kind::Canvas)
                .map(|(id, title)| {
                    let mut proof = locale.proof(self.current(shell));
                    proof["quipTitle"] = match &title.quip {
                        None => Value::Null,
                        Some(quip) => {
                            let mut p = json!({"fileId":id,"current":quip.current(shell)});
                            match quip.value() {
                                file_titles::QuipTitle::Nullish => p["value"] = json!("nullish"),
                                file_titles::QuipTitle::Text(text) => {
                                    p["value"] = json!("text");
                                    p["text"] = json!(text);
                                }
                                file_titles::QuipTitle::Unavailable => {
                                    p["value"] = json!("unavailable")
                                }
                            }
                            p
                        }
                    };
                    (id.clone(), proof)
                })
                .collect::<serde_json::Map<String, Value>>();
            value["canvasFallback"] = Value::Object(proof);
        }
        value
    }
    pub(super) fn target(
        &self,
        shell: &ShellState,
        team: &str,
        raw: &Value,
    ) -> Result<CanonicalTitle, Error> {
        if self.team != team || !self.tokens.iter().any(|t| t["file_id"] == raw["file_id"]) {
            return Err(unavailable());
        }
        self.canonical(shell, raw["file_id"].as_str().ok_or_else(unavailable)?)
    }
}
