//! AjOk/UFSg/H08Z/Kzm6 Activity reads. No request dispatch, permission grant,
//! polling, automatic pagination, or missing-execution inference lives here.
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActivityFilters {
    /// AjOk forwards the URL/filter value; its ordinary choices are SUCCESS,
    /// PENDING, ERROR, CANCELED, PENDING_USER_ACTION and the empty string.
    pub status: Option<String>,
    pub user_id: Option<String>,
    /// The renderer computes local start/end-of-day Unix seconds. This core
    /// adapter does not guess a timezone or replace absent dates with zero.
    pub from_timestamp: Option<i64>,
    pub to_timestamp: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityQuery {
    workflow: String,
    parent_execution: Option<String>,
    limit: u16,
    cursor: Option<String>,
    filters: ActivityFilters,
}
impl ActivityQuery {
    pub fn new(workflow: &str, filters: ActivityFilters) -> Result<Self, Error> {
        if !id(workflow)
            || filters.status.as_deref().is_some_and(|s| s.len() > 1024)
            || filters.user_id.as_deref().is_some_and(|s| s.len() > 1024)
        {
            return Err(invalid());
        }
        Ok(Self {
            workflow: workflow.into(),
            parent_execution: None,
            limit: 100,
            cursor: None,
            filters,
        })
    }
    pub fn workflow_id(&self) -> &str {
        &self.workflow
    }
    pub fn parent_execution_id(&self) -> Option<&str> {
        self.parent_execution.as_deref()
    }
    pub fn limit(&self) -> u16 {
        self.limit
    }
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
    pub fn filters(&self) -> &ActivityFilters {
        &self.filters
    }
    /// A limit/filter/parent change starts at the beginning. Parent UI owns its
    /// bounded Previous cursor window and must retain the full query with it.
    pub fn with_limit(mut self, limit: u16) -> Result<Self, Error> {
        if ![20, 50, 75, 100].contains(&limit) {
            return Err(invalid());
        }
        self.limit = limit;
        self.cursor = None;
        Ok(self)
    }
    pub fn with_parent(mut self, execution: Option<&str>) -> Result<Self, Error> {
        if execution.is_some_and(|s| !execution_id(s)) {
            return Err(invalid());
        }
        self.parent_execution = execution.map(str::to_owned);
        self.cursor = None;
        Ok(self)
    }
    pub fn with_cursor(mut self, cursor: Option<&str>) -> Result<Self, Error> {
        if cursor.is_some_and(|s| s.is_empty() || s.len() > 16_384) {
            return Err(invalid());
        }
        self.cursor = cursor.map(str::to_owned);
        Ok(self)
    }
    pub fn request(&self) -> RequestPlan {
        let mut filters = serde_json::Map::new();
        for (key, value) in [
            (
                "workflow_instance_status",
                self.filters.status.as_ref().map(|s| json!(s)),
            ),
            ("user_id", self.filters.user_id.as_ref().map(|s| json!(s))),
            (
                "from_timestamp",
                self.filters.from_timestamp.map(|v| json!(v)),
            ),
            ("to_timestamp", self.filters.to_timestamp.map(|v| json!(v))),
        ] {
            if let Some(value) = value {
                filters.insert(key.into(), value);
            }
        }
        let mut args = json!({"workflow_id":self.workflow,"limit":self.limit,"filters":filters});
        if let Some(parent) = &self.parent_execution {
            args["parent_workflow_execution_id"] = json!(parent);
        }
        if let Some(cursor) = &self.cursor {
            args["cursor"] = json!(cursor);
        }
        RequestPlan::new(
            "workflows.activity.list",
            "multi-click-activity-list-fetcher",
            args,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivityPage {
    query: ActivityQuery,
    raw: Value,
    next: Option<String>,
}
impl ActivityPage {
    /// Only positive envelopes enter a page. Missing/malformed results are an
    /// error, never an empty success. Extra row fields and exact order survive.
    pub fn decode(query: &ActivityQuery, raw: Value) -> Result<Self, Error> {
        success(&raw)?;
        let rows = array(&raw, "workflow_events")?;
        if rows.len() > usize::from(query.limit)
            || rows.iter().any(|row| {
                !row.is_object()
                    || !row["workflow_execution_id"]
                        .as_str()
                        .is_some_and(execution_id)
                    || !matching_optional(row, "workflow_id", &query.workflow)
            })
        {
            return Err(invalid());
        }
        let next = match raw.get("response_metadata") {
            None | Some(Value::Null) => None,
            Some(Value::Object(meta)) => match meta.get("next_cursor") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) if s.is_empty() => None,
                Some(Value::String(s))
                    if s.len() <= 16_384 && Some(s.as_str()) != query.cursor() =>
                {
                    Some(s.clone())
                }
                _ => return Err(invalid()),
            },
            _ => return Err(invalid()),
        };
        Ok(Self {
            query: query.clone(),
            raw,
            next,
        })
    }
    pub fn query(&self) -> &ActivityQuery {
        &self.query
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn rows(&self) -> &[Value] {
        self.raw["workflow_events"]
            .as_array()
            .expect("decoded page")
    }
    pub fn next_cursor(&self) -> Option<&str> {
        self.next.as_deref()
    }
    pub fn next_query(&self) -> Option<ActivityQuery> {
        self.next.as_deref().map(|s| {
            self.query
                .clone()
                .with_cursor(Some(s))
                .expect("decoded cursor")
        })
    }
    pub fn detail_request(&self, execution: &str) -> Result<ActivityDetailRequest, Error> {
        if !self
            .rows()
            .iter()
            .any(|r| r["workflow_execution_id"].as_str() == Some(execution))
        {
            return Err(invalid());
        }
        ActivityDetailRequest::new(&self.query.workflow, execution)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityDetailRequest {
    workflow: String,
    execution: String,
}
impl ActivityDetailRequest {
    /// Also supports an explicit source-backed execution route. This key is not
    /// a fabricated event or permission; the real read may fail normally.
    pub fn new(workflow: &str, execution: &str) -> Result<Self, Error> {
        if !id(workflow) || !execution_id(execution) {
            return Err(invalid());
        }
        Ok(Self {
            workflow: workflow.into(),
            execution: execution.into(),
        })
    }
    pub fn workflow_id(&self) -> &str {
        &self.workflow
    }
    pub fn execution_id(&self) -> &str {
        &self.execution
    }
    pub fn request(&self) -> RequestPlan {
        RequestPlan::new(
            "workflows.activity.get",
            "Load instance steps for activity tab",
            json!({"workflow_id":self.workflow,"workflow_execution_id":self.execution}),
        )
    }
    pub fn decode(&self, raw: Value) -> Result<ActivityDetail, Error> {
        success(&raw)?;
        let row = &raw["workflow_event"];
        if !row.is_object()
            || !row["step_events"].is_array()
            || !matching_optional(row, "workflow_id", &self.workflow)
            || !matching_optional(row, "workflow_execution_id", &self.execution)
        {
            return Err(invalid());
        }
        Ok(ActivityDetail {
            request: self.clone(),
            raw,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ActivityDetail {
    request: ActivityDetailRequest,
    raw: Value,
}
impl ActivityDetail {
    pub fn request_key(&self) -> &ActivityDetailRequest {
        &self.request
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn event(&self) -> &Value {
        &self.raw["workflow_event"]
    }
    pub fn steps(&self) -> &[Value] {
        self.event()["step_events"]
            .as_array()
            .expect("decoded detail")
    }
    pub fn trace_id(&self) -> Option<&str> {
        self.event()["trace_id"].as_str()
    }
    pub fn status(&self) -> Option<&str> {
        self.event()["workflow_status"].as_str()
    }
    pub fn trigger_message(&self) -> Option<&Value> {
        self.event().get("trigger_message")
    }
    pub fn started(&self) -> Option<&Value> {
        self.event().get("started")
    }
}
fn execution_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096
}
fn matching_optional(raw: &Value, key: &str, expected: &str) -> bool {
    raw.get(key)
        .is_none_or(|value| value.as_str() == Some(expected))
}

#[cfg(test)]
mod tests;
