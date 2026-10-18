use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    Ascending,
    #[default]
    Descending,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListQuery {
    pub query: String,
    pub published: Option<bool>,
    pub sort: Sort,
    pub cursor: Option<String>,
}
impl ListQuery {
    pub fn request(&self) -> Result<RequestPlan, Error> {
        if self.query.len() > 8192
            || self
                .cursor
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 8192)
        {
            return Err(invalid());
        }
        let mut filters = json!({"name": self.query});
        if let Some(published) = self.published {
            filters["is_published"] = json!(published);
        }
        let mut args = json!({"limit":50,"filter_options":filters,
            "sort_options":{"column":"date_updated","direction":match self.sort { Sort::Ascending => "asc", Sort::Descending => "desc" }},
            "workflow_builder_only":true});
        if let Some(cursor) = &self.cursor {
            args["cursor"] = json!(cursor);
        }
        Ok(RequestPlan::new(
            "functions.workflows.list",
            "automations_gallery",
            args,
        ))
    }
    fn same_filter(&self, other: &Self) -> bool {
        self.query == other.query && self.published == other.published && self.sort == other.sort
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ListWorkflows {
    query: ListQuery,
    raw: Value,
    cursor: Option<String>,
}
impl ListWorkflows {
    pub fn decode(query: ListQuery, raw: Value) -> Result<Self, Error> {
        query.request()?;
        success(&raw)?;
        for workflow in array(&raw, "workflows")? {
            identifier(workflow, "id")?;
            identifier(workflow, "app_id")?;
        }
        let cursor = match raw.pointer("/response_metadata/next_cursor") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.is_empty() => None,
            Some(Value::String(s)) if s.len() <= 8192 => Some(s.clone()),
            _ => return Err(invalid()),
        };
        // Prevent an infinite continuation loop without discarding the raw page.
        if cursor.is_some() && cursor == query.cursor {
            return Err(invalid());
        }
        Ok(Self { query, raw, cursor })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn triggers_request(&self) -> RequestPlan {
        let apps = self.raw["workflows"]
            .as_array()
            .expect("decoded list")
            .iter()
            .map(|v| v["app_id"].as_str().expect("decoded app"))
            .collect::<Vec<_>>()
            .join(",");
        // wBUp performs this even when the workflow array is empty.
        RequestPlan::new(
            "workflows.triggers.list",
            "automations_gallery",
            json!({"app_ids":apps}),
        )
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ListRow {
    raw: Value,
    trigger: Option<Value>,
}
impl ListRow {
    pub fn id(&self) -> &str {
        self.raw["id"].as_str().expect("decoded workflow")
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn trigger(&self) -> Option<&Value> {
        self.trigger.as_ref()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ListPage {
    workflows: ListWorkflows,
    triggers: Value,
    rows: Vec<ListRow>,
}
impl ListPage {
    pub fn decode(workflows: ListWorkflows, triggers: Value) -> Result<Self, Error> {
        success(&triggers)?;
        let list = array(&triggers, "triggers")?;
        for trigger in list {
            identifier(trigger, "id")?;
        }
        let rows = array(&workflows.raw, "workflows")?
            .iter()
            .map(|workflow| ListRow {
                raw: workflow.clone(),
                trigger: list
                    .iter()
                    .find(|trigger| trigger.pointer("/workflow/app/id") == workflow.get("app_id"))
                    .cloned(),
            })
            .collect();
        Ok(Self {
            workflows,
            triggers,
            rows,
        })
    }
    pub fn query(&self) -> &ListQuery {
        &self.workflows.query
    }
    pub fn cursor(&self) -> Option<&str> {
        self.workflows.cursor.as_deref()
    }
    pub fn raw_workflows(&self) -> &Value {
        &self.workflows.raw
    }
    pub fn raw_triggers(&self) -> &Value {
        &self.triggers
    }
    pub fn rows(&self) -> &[ListRow] {
        &self.rows
    }
}
/// HKCz appends pages before ETAU's first-ID-wins deduplication and empty filter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct List {
    pages: Vec<ListPage>,
}
impl List {
    pub fn pages(&self) -> &[ListPage] {
        &self.pages
    }
    pub fn append(&mut self, page: ListPage) -> Result<(), Error> {
        if let Some(previous) = self.pages.last() {
            if !previous.query().same_filter(page.query())
                || previous.cursor().is_none()
                || previous.cursor() != page.query().cursor.as_deref()
                || page.cursor().is_some_and(|c| {
                    self.pages
                        .iter()
                        .any(|p| p.query().cursor.as_deref() == Some(c))
                })
            {
                return Err(invalid());
            }
        } else if page.query().cursor.is_some() {
            return Err(invalid());
        }
        self.pages.push(page);
        Ok(())
    }
    pub fn next_query(&self) -> Option<ListQuery> {
        let last = self.pages.last()?;
        let mut query = last.query().clone();
        query.cursor = Some(last.cursor()?.into());
        Some(query)
    }
    pub fn visible_rows(&self) -> Vec<&ListRow> {
        let mut seen = BTreeSet::new();
        self.pages
            .iter()
            .flat_map(|p| p.rows())
            .filter(|row| seen.insert(row.id()))
            .filter(|row| row.raw.get("is_empty") != Some(&Value::Bool(true)))
            .collect()
    }
}
