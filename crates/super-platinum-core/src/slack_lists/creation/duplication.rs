//! B grid/board duplicate callers use iLfW.x0 then +E6Z.AO, not display filters.
//! S Ommm.TU generates fixed-ten-digit/base62 fractional order keys.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

mod position;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DuplicateSource {
    pub record: Record,
    pub existing_ids: BTreeSet<String>,
    pub position: String,
    next_id: Option<String>,
}

impl CreationIntent {
    /// Check permissions and canonical source without cloning or sorting rows.
    /// Capture additionally verifies the exact source ordering calculation.
    pub fn duplicate_eligible(
        document: &Document,
        page: &RecordsPage,
        source_id: &str,
        allow_lists: Option<&str>,
        personal_tasks: Option<&PersonalTaskLists>,
        selected_view: Option<&str>,
    ) -> Result<(), ListError> {
        Self::eligible(document, allow_lists, personal_tasks, selected_view)?;
        if !page.complete() {
            return Err(ListError("Load all List items before duplicating an item."));
        }
        let record = page
            .record(source_id)
            .ok_or(ListError("The original List item is unavailable."))?;
        if record.id.trim() != record.id
            || record
                .raw
                .get("list_id")
                .is_some_and(|id| id.as_str() != Some(&document.id))
            || ["archived", "is_deleted"].iter().any(|key| {
                record
                    .raw
                    .get(key)
                    .is_some_and(|value| !value.is_null() && value != &Value::Bool(false))
            })
            || record.raw.get("parent_record_id").is_some_and(|parent| {
                !parent.is_null()
                    && parent
                        .as_str()
                        .is_none_or(|id| id.trim() != id || id == source_id)
            })
        {
            return Err(ListError("The original List item is unavailable."));
        }
        Ok(())
    }

    pub fn capture_duplicate(
        document: &Document,
        page: &RecordsPage,
        source_id: &str,
        allow_lists: Option<&str>,
        personal_tasks: Option<&PersonalTaskLists>,
        selected_view: Option<&str>,
    ) -> Result<Self, ListError> {
        Self::duplicate_eligible(
            document,
            page,
            source_id,
            allow_lists,
            personal_tasks,
            selected_view,
        )?;
        let mut intent = Self::capture(document, allow_lists, personal_tasks, selected_view)?;
        let (position, next_id) = adjacent(page, source_id, selected_view)?;
        intent.duplicate = Some(DuplicateSource {
            record: page.record(source_id).unwrap().clone(),
            existing_ids: page
                .records
                .iter()
                .map(|record| record.id.clone())
                .collect(),
            position,
            next_id,
        });
        Ok(intent)
    }

    /// The same fresh preflight serves Add and Duplicate. A duplicate also owns
    /// its exact original record and the canonical adjacent ordering boundary.
    pub fn revalidate_with_records(
        &self,
        document: &Document,
        page: &RecordsPage,
        allow_lists: Option<&str>,
        personal_tasks: Option<&PersonalTaskLists>,
        selected_view: Option<&str>,
    ) -> Result<(), ListError> {
        self.revalidate_document(document, allow_lists, personal_tasks, selected_view)?;
        if let Some(source) = &self.duplicate {
            Self::duplicate_eligible(
                document,
                page,
                &source.record.id,
                allow_lists,
                personal_tasks,
                selected_view,
            )?;
            let (position, next_id) = adjacent(page, &source.record.id, selected_view)?;
            if page.record(&source.record.id) != Some(&source.record)
                || position != source.position
                || next_id != source.next_id
                || page
                    .records
                    .iter()
                    .map(|record| record.id.clone())
                    .collect::<BTreeSet<_>>()
                    != source.existing_ids
            {
                return Err(ListError(
                    "This List item or its position changed. Review it before duplicating.",
                ));
            }
        }
        Ok(())
    }
}

fn ordering_error() -> ListError {
    ListError(
        "This List item's position cannot be duplicated safely. Refresh the List and try again.",
    )
}

/// Decoded iLfW records default a missing position to "". Truthy view overrides
/// are case insensitive. Numeric base positions stringify only in +E6Z.AO;
/// numeric view overrides reach Ommm unchanged and fail its string validator.
fn key(record: &Record, view: Option<&str>) -> Result<(String, bool), ListError> {
    if let Some(view) = view {
        if let Some(positions) = record
            .raw
            .get("view_positions")
            .filter(|value| !value.is_null())
        {
            let positions = positions.as_object().ok_or_else(ordering_error)?;
            if positions
                .iter()
                .filter(|(id, _)| id.eq_ignore_ascii_case(view))
                .count()
                > 1
                || positions
                    .values()
                    .any(|value| !value.is_null() && !value.is_string() && !value.is_number())
            {
                return Err(ordering_error());
            }
        }
    }
    let override_value = view.and_then(|view| {
        record
            .raw
            .get("view_positions")?
            .as_object()?
            .iter()
            .find(|(id, _)| id.eq_ignore_ascii_case(view))
            .map(|(_, value)| value)
    });
    let value = override_value.filter(|value| match value {
        Value::String(value) => !value.is_empty(),
        Value::Number(value) => value.as_f64() != Some(0.0),
        _ => false,
    });
    let is_override = value.is_some();
    let value = value.or_else(|| record.raw.get("position"));
    let (key, numeric) = match value {
        None | Some(Value::Null) => (String::new(), false),
        Some(Value::String(value)) => (value.clone(), false),
        Some(Value::Number(value)) if !is_override => {
            let numeric = value.as_f64().ok_or_else(ordering_error)?;
            if !numeric.is_finite()
                || numeric < 0.0
                || numeric >= 10_000_000_000.0
                || numeric.fract() != 0.0
            {
                return Err(ordering_error());
            }
            let number = numeric as u64;
            (
                if number == 0 {
                    String::new()
                } else {
                    number.to_string()
                },
                true,
            )
        }
        _ => return Err(ordering_error()),
    };
    if !key.is_empty() {
        position::validate(&key)?;
    }
    Ok((key, numeric))
}

fn adjacent(
    page: &RecordsPage,
    source_id: &str,
    view: Option<&str>,
) -> Result<(String, Option<String>), ListError> {
    let mut rows = Vec::with_capacity(page.records.len());
    let mut numeric_positions = None;
    let mut date_types = BTreeMap::new();
    for record in &page.records {
        let (key, numeric) = key(record, view)?;
        // Avoid replacing JavaScript's mixed-type coercion with native total
        // ordering. Homogeneous numbers and strings have the captured order.
        if numeric_positions.is_some_and(|previous| previous != numeric) {
            return Err(ordering_error());
        }
        numeric_positions = Some(numeric);
        let date = record
            .raw
            .get("date_created")
            .filter(|value| !value.is_null());
        if date.is_some_and(|value| !value.is_string() && !value.is_number()) {
            return Err(ordering_error());
        }
        let textual_date = date.is_some_and(Value::is_string);
        if date_types
            .insert(key.clone(), textual_date)
            .is_some_and(|previous| previous != textual_date)
        {
            return Err(ordering_error());
        }
        rows.push((record, key, date));
    }
    rows.sort_by(|a, b| {
        a.1.cmp(&b.1).then_with(|| match (a.2, b.2) {
            // JavaScript relational strings compare UTF-16 code units, not
            // UTF-8 bytes. Finite numeric ties preserve stable order, including
            // -0/+0, which total_cmp would incorrectly distinguish.
            (Some(Value::String(a)), Some(Value::String(b))) => {
                a.encode_utf16().cmp(b.encode_utf16())
            }
            (a, b) => a
                .and_then(Value::as_f64)
                .unwrap_or(9_007_199_254_740_991.0)
                .partial_cmp(&b.and_then(Value::as_f64).unwrap_or(9_007_199_254_740_991.0))
                .unwrap_or(std::cmp::Ordering::Equal),
        })
    });
    let index = rows
        .iter()
        .position(|(record, _, _)| record.id == source_id)
        .ok_or_else(ordering_error)?;
    let lower = (!rows[index].1.is_empty()).then_some(rows[index].1.as_str());
    let next = rows.get(index + 1);
    let upper = next.and_then(|(_, key, _)| (!key.is_empty()).then_some(key.as_str()));
    let position = if lower.is_none() && upper.is_none() {
        String::new()
    } else {
        position::between(lower, upper)?
    };
    Ok((position, next.map(|(record, _, _)| record.id.clone())))
}
