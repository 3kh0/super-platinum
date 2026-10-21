use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewKind {
    Available,
    AccessDenied,
    Error(String),
    Unknown,
    NonExistent,
    Missing,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    id: String,
    kind: PreviewKind,
    raw: Option<Value>,
}
impl Preview {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn kind(&self) -> &PreviewKind {
        &self.kind
    }
    pub fn raw(&self) -> Option<&Value> {
        self.raw.as_ref()
    }
    pub fn displayable(&self) -> bool {
        matches!(
            self.kind,
            PreviewKind::Available | PreviewKind::AccessDenied
        )
    }
    /// wcWH, with no separately acquired full workflow or passed shortcut.
    pub fn title(&self) -> Option<&str> {
        self.metadata("name", "title")
    }
    pub fn description(&self) -> Option<&str> {
        self.metadata("description", "description")
    }
    fn metadata(&self, trigger_key: &str, workflow_key: &str) -> Option<&str> {
        if self.kind != PreviewKind::Available {
            return None;
        }
        let raw = self.raw.as_ref()?;
        raw.get(trigger_key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                raw.get("workflow")?
                    .get(workflow_key)?
                    .as_str()
                    .filter(|s| !s.is_empty())
            })
    }
    pub fn workflow_id(&self) -> Option<&str> {
        (self.kind == PreviewKind::Available).then_some(())?;
        self.raw
            .as_ref()?
            .pointer("/workflow/workflow_id")?
            .as_str()
    }
    /// Actual acquired app/icon fields, not a bookmark icon or an app lookup.
    /// Full-trigger/canonical-app fallback acquisition remains desktop-owned.
    pub fn app(&self) -> Option<&Value> {
        (self.kind == PreviewKind::Available).then_some(())?;
        self.raw
            .as_ref()?
            .pointer("/workflow/app")
            .filter(|v| v.is_object())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Previews {
    raw: Value,
    items: Vec<Preview>,
}
impl Previews {
    pub fn decode(ids: &[String], raw: Value) -> Result<Self, Error> {
        validate_ids(ids)?;
        success(&raw)?;
        let rows = raw
            .get("triggers")
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        if rows.len() > MAX_PREVIEW_IDS {
            return Err(invalid());
        }
        let mut indexed = std::collections::HashMap::new();
        for row in rows {
            let id = string_id(row, "id")?;
            if !ids.contains(&id) || indexed.contains_key(&id) {
                return Err(invalid());
            }
            for key in ["isUnknown", "isNonExistent"] {
                if row.get(key).is_some_and(|v| !v.is_boolean()) {
                    return Err(invalid());
                }
            }
            let error = optional_string(row, "error")?;
            let kind = if row["isUnknown"] == true {
                PreviewKind::Unknown
            } else if row["isNonExistent"] == true {
                PreviewKind::NonExistent
            } else if error.as_deref() == Some("access_denied") {
                PreviewKind::AccessDenied
            } else if let Some(error) = error.filter(|s| !s.is_empty()) {
                PreviewKind::Error(error)
            } else {
                PreviewKind::Available
            };
            // Malformed metadata cannot become a successful blank row. Denied
            // and error entries need only their actual identity/error fields.
            if kind == PreviewKind::Available {
                for key in ["name", "description"] {
                    optional_string(row, key)?;
                }
                if let Some(workflow) = row.get("workflow").filter(|v| !v.is_null()) {
                    if !workflow.is_object() {
                        return Err(invalid());
                    }
                    for key in ["title", "description", "workflow_id"] {
                        optional_string(workflow, key)?;
                    }
                    if workflow
                        .get("app")
                        .is_some_and(|v| !v.is_null() && !v.is_object())
                    {
                        return Err(invalid());
                    }
                }
            }
            indexed.insert(
                id.clone(),
                Preview {
                    id,
                    kind,
                    raw: Some(row.clone()),
                },
            );
        }
        let items = ids
            .iter()
            .map(|id| {
                indexed.remove(id).unwrap_or_else(|| Preview {
                    id: id.clone(),
                    kind: PreviewKind::Missing,
                    raw: None,
                })
            })
            .collect();
        Ok(Self { raw, items })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn items(&self) -> &[Preview] {
        &self.items
    }
    /// A successful envelope alone is not a complete available collection.
    pub fn all_displayable(&self) -> bool {
        self.items.iter().all(Preview::displayable)
    }
    pub fn project(&self, collections: &Collections) -> Result<Rows, Error> {
        if self
            .items
            .iter()
            .map(|p| &p.id)
            .ne(collections.preview_ids().iter())
        {
            return Err(invalid());
        }
        let project = |reference: &Reference, featured: bool| {
            let preview = self
                .items
                .iter()
                .find(|p| p.id == reference.trigger_id)
                .expect("validated source IDs")
                .clone();
            let title = if featured && preview.kind == PreviewKind::AccessDenied {
                reference.title.clone()
            } else {
                preview.title().map(str::to_owned)
            };
            Row {
                source: reference.raw.clone(),
                title,
                preview,
            }
        };
        Ok(Rows {
            featured: collections
                .featured
                .iter()
                .map(|r| project(r, true))
                .collect(),
            bookmarked: collections
                .bookmarked
                .iter()
                .map(|r| project(r, false))
                .collect(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    source: Value,
    title: Option<String>,
    preview: Preview,
}
impl Row {
    pub fn source(&self) -> &Value {
        &self.source
    }
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
    pub fn preview(&self) -> &Preview {
        &self.preview
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Rows {
    featured: Vec<Row>,
    bookmarked: Vec<Row>,
}
impl Rows {
    /// Includes unavailable states; callers must not mistake a filtered empty
    /// display list for an observed empty source collection.
    pub fn featured(&self) -> &[Row] {
        &self.featured
    }
    pub fn bookmarked(&self) -> &[Row] {
        &self.bookmarked
    }
}
