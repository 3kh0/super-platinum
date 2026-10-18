//! Current-document grouping identity, independent of authoring eligibility.
//! Includes local read refinements. The cache is bounded by current views.
use super::*;

#[derive(Clone, PartialEq)]
struct ColumnIdentity {
    id: String,
    key: String,
    kind: String,
    options: Option<Value>,
}
impl ColumnIdentity {
    fn matches(&self, column: &super_platinum_core::slack_lists::Column) -> bool {
        self.id == column.id
            && self.key == column.key
            && self.kind == column.kind
            && self.options.as_ref() == column.raw.get("options")
    }
}
#[derive(Clone)]
struct Identity {
    kind: Option<Value>,
    raw: Option<Value>,
    columns: Vec<ColumnIdentity>,
}
impl Identity {
    fn columns<'a>(
        document: &'a Document,
        raw: &'a Value,
    ) -> impl Iterator<Item = &'a super_platinum_core::slack_lists::Column> {
        let key = raw
            .pointer("/grouping/group_by")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        document
            .columns
            .iter()
            .filter(move |c| Some(c.key.as_str()) == key)
    }
    fn capture(document: &Document, raw: &Value) -> Self {
        Self {
            kind: raw.get("type").cloned(),
            raw: raw.get("grouping").cloned(),
            columns: Self::columns(document, raw)
                .map(|c| ColumnIdentity {
                    id: c.id.clone(),
                    key: c.key.clone(),
                    kind: c.kind.clone(),
                    options: c.raw.get("options").cloned(),
                })
                .collect(),
        }
    }
    fn matches(&self, document: &Document, raw: &Value) -> bool {
        self.kind.as_ref() == raw.get("type") && self.raw.as_ref() == raw.get("grouping") && {
            let mut columns = Self::columns(document, raw);
            self.columns
                .iter()
                .all(|old| columns.next().is_some_and(|c| old.matches(c)))
                && columns.next().is_none()
        }
    }
}
#[derive(Clone, Default)]
pub(super) struct State(BTreeMap<String, Vec<Identity>>);

/// Runs on accepted state rebuilds, including conflicting Check/read snapshots.
/// Compare borrowed values on unchanged rebuilds; clone only new/changed identities.
pub(super) fn reconcile(view: &mut View) -> bool {
    let Some(loaded) = view.loaded.as_ref() else {
        return false;
    };
    // Provider order is intentionally separate: a local lane move must not
    // discard collapsed groups. Group/layout refinements do change identity.
    let effective = if view.layout.refinements.is_empty() {
        None
    } else {
        match view.layout.refinements.all_document(&loaded.document) {
            Ok(document) => Some(document),
            Err(_) => return false,
        }
    };
    let doc = effective.as_ref().unwrap_or(&loaded.document);
    let mut present: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
    for raw in &doc.views {
        if let Some(id) = raw.get("id").and_then(Value::as_str) {
            present.entry(id).or_default().push(raw);
        }
    }
    let selected = view.layout.selected.as_deref();
    let mut changed = selected
        .is_some_and(|id| view.layout.grouping.0.contains_key(id) && !present.contains_key(id));
    view.layout
        .grouping
        .0
        .retain(|id, _| present.contains_key(id.as_str()));
    view.layout
        .collapsed
        .retain(|id, _| present.contains_key(id.as_str()));
    for (id, raws) in present {
        let previous = view.layout.grouping.0.get(id);
        let same = previous.is_some_and(|old| {
            old.len() == raws.len() && old.iter().zip(&raws).all(|(a, b)| a.matches(doc, b))
        });
        if !same {
            if previous.is_some() {
                view.layout.collapsed.remove(id);
                changed |= selected == Some(id);
            }
            view.layout.grouping.0.insert(
                id.into(),
                raws.into_iter()
                    .map(|raw| Identity::capture(doc, raw))
                    .collect(),
            );
        }
    }
    if changed {
        view.layout.grouping_revision = next_id();
    }
    changed
}
