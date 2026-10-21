use super::*;

#[derive(Clone)]
pub struct Page {
    raw: Value,
    rows: Vec<Value>,
    membership_ids: Vec<String>,
    next_marker: Option<String>,
    read: Read,
}
impl Page {
    pub(super) fn decode(read: &Read, raw: Value, keys: Option<&[String]>) -> Result<Self, Error> {
        if !raw.is_object()
            || raw.get("ok").is_some_and(|v| v != true)
            || raw.get("error").is_some_and(|v| !v.is_null())
        {
            return Err(invalid());
        }
        let rows = match raw.get(read.row_field()) {
            Some(Value::Array(rows)) => rows.clone(),
            Some(Value::Object(map))
                if matches!(
                    read.kind,
                    SearchKind::Members | SearchKind::Channels | SearchKind::ExternalTeams
                ) =>
            {
                let keys = keys.ok_or_else(invalid)?;
                let unique: BTreeSet<_> = keys.iter().collect();
                if keys.len() != map.len()
                    || unique.len() != keys.len()
                    || !keys.iter().all(|k| map.contains_key(k))
                {
                    return Err(invalid());
                }
                let mut rows = vec![];
                for key in keys {
                    match &map[key] {
                        Value::Array(values) => rows.extend(values.clone()),
                        Value::Object(_) => rows.push(map[key].clone()),
                        _ => return Err(invalid()),
                    }
                }
                rows
            }
            // The source generic fetcher accepts omitted results as empty.
            None if read.row_field() == "results" && read.kind != SearchKind::AvailableExternal => {
                vec![]
            }
            _ => return Err(invalid()),
        };
        if rows.len() > 10000
            || rows.iter().any(|row| {
                !row.is_object()
                    || !row
                        .get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| valid_row_id(read.kind, id))
            })
        {
            return Err(invalid());
        }
        let membership_ids = match raw.get("member_channels") {
            None => vec![],
            Some(Value::Array(ids)) => ids
                .iter()
                .map(|v| {
                    v.as_str()
                        .filter(|id| valid_row_id(SearchKind::Channels, id))
                        .map(str::to_owned)
                        .ok_or_else(invalid)
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(invalid()),
        };
        let field = if read.kind == SearchKind::InternalTeams {
            "next_cursor_mark"
        } else {
            "next_marker"
        };
        let next_marker = match raw.get(field) {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.is_empty() => None,
            Some(Value::String(s)) if marker(s) => Some(s.clone()),
            _ => return Err(invalid()),
        };
        if next_marker.as_ref().is_some_and(|next| {
            read.body
                .get(if read.kind == SearchKind::InternalTeams {
                    "cursor_mark"
                } else {
                    "marker"
                })
                .and_then(Value::as_str)
                == Some(next)
        }) {
            return Err(invalid());
        }
        Ok(Self {
            raw,
            rows,
            membership_ids,
            next_marker,
            read: read.clone(),
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub fn membership_ids(&self) -> &[String] {
        &self.membership_ids
    }
    pub fn next_marker(&self) -> Option<&str> {
        self.next_marker.as_deref()
    }
    pub fn read(&self) -> &Read {
        &self.read
    }
}
fn valid_row_id(kind: SearchKind, id: &str) -> bool {
    if matches!(
        kind,
        SearchKind::InternalTeams | SearchKind::ExternalTeams | SearchKind::AvailableExternal
    ) {
        return team_id(id);
    }
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && id.len() <= 256
        && bytes
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_')
        && match kind {
            SearchKind::Members => matches!(bytes[0], b'U' | b'W'),
            SearchKind::Channels => matches!(bytes[0], b'C' | b'G' | b'D' | b'Z'),
            _ => false,
        }
}
