//! qKv8 -> 20Mt -> YCoE canvas search, scoped to the retained Home workflow.
use super::*;
#[derive(Clone, PartialEq)]
pub struct CanvasChoice {
    workflow: Workflow,
    raw: Value,
    id: String,
    label: String,
}
impl CanvasChoice {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}
impl Preflight {
    pub fn canvas_request(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
        current: &View,
        query: &str,
    ) -> Result<PreparedRequest, Error> {
        if !self.workflow.same_view(current)
            || self.workflow.user != w.user_id
            || !self.context().iter().any(|i| i.kind == InputType::Canvas)
            || query.len() > 1024
            || query.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        Ok(client.rest_form(
            w,
            "search.autocomplete.files",
            vec![
                ("query", format!("type:quip {query}")),
                ("include_shares", "true".into()),
            ],
        ))
    }
    pub fn canvas_choices(&self, response: &Value) -> Result<Vec<CanvasChoice>, Error> {
        if response["ok"] != true || response.to_string().len() > 2 * 1024 * 1024 {
            return Err(invalid());
        }
        let files = response["files"]
            .as_array()
            .filter(|a| a.len() <= 1000)
            .ok_or_else(invalid)?;
        let mut out = Vec::new();
        for raw in files {
            let suggested = match raw.get("is_ai_suggested") {
                None | Some(Value::Null) => false,
                Some(Value::Bool(v)) => *v,
                _ => return Err(invalid()),
            };
            // Exact 8Bbq onlyCanvases predicate and XNVs definitions.
            if (raw["mode"] != "quip" && raw["filetype"] != "quip") || suggested {
                continue;
            }
            let id = bounded(raw, "id", 255)?;
            if !super::id(&id) || !id.starts_with('F') {
                return Err(invalid());
            }
            if out.iter().any(|c: &CanvasChoice| c.id == id) {
                continue;
            }
            let label = raw
                .get("title")
                .or_else(|| raw.get("name"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && s.len() <= 8192)
                .ok_or_else(invalid)?
                .to_owned();
            out.push(CanvasChoice {
                workflow: self.workflow.clone(),
                raw: raw.clone(),
                id,
                label,
            });
            if out.len() == 100 {
                break;
            }
        }
        Ok(out)
    }
    pub fn select_canvas(
        &mut self,
        field: &str,
        choice: &CanvasChoice,
        current: &View,
        w: &WorkspaceSession,
    ) -> Result<(), Error> {
        if choice.workflow != self.workflow
            || !self.workflow.same_view(current)
            || self.workflow.user != w.user_id
            || !self
                .context()
                .iter()
                .any(|i| i.name == field && i.kind == InputType::Canvas)
        {
            return Err(invalid());
        }
        self.set_context(field, choice.id.clone())
    }
}
