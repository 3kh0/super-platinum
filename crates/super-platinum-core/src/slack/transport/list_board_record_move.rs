//! Exact Board reorder/cell-only forms, shared single-attempt transport.
use super::*;

impl Transport {
    pub async fn execute_list_board_record_move(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListColumnRenameFailure> {
        self.execute_list_rename_once(request, current, allowed)
            .await
    }
}
impl Transport {
    /// Fresh population preflight/readback through the same bounded one-shot executor.
    pub async fn execute_list_board_record_move_read(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListColumnRenameFailure> {
        self.execute_list_rename_once(request, current, |r| {
            super::list_column_rename_read::allowed(r)
                || super::list_board_choice::allowed_records(r)
        })
        .await
    }
}

fn allowed(request: &PreparedRequest) -> bool {
    if request.method != "POST" {
        return false;
    }
    let Ok(url) = url::Url::parse(&request.url) else {
        return false;
    };
    let origin = url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host.ends_with(".slack.com"))
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !matches!(
            url.path(),
            "/api/lists.records.reorder" | "/api/lists.cells.update"
        )
        || url.query_pairs().any(|(key, _)| {
            !matches!(
                key.as_ref(),
                "_x_id"
                    | "_x_csid"
                    | "_x_desktop_ia"
                    | "_x_foreground"
                    | "_x_frontend_build_type"
                    | "_x_gantry"
                    | "_x_num_retries"
                    | "_x_version_ts"
                    | "fp"
                    | "slack_route"
            )
        })
    {
        return false;
    }
    let RequestBody::Form(fields) = &request.body else {
        return false;
    };
    let reorder = url.path() == "/api/lists.records.reorder";
    let expected = if reorder {
        if fields.len() == 7 {
            vec![
                "token",
                "list_id",
                "id",
                "view_id",
                "position",
                "cells",
                "_x_reason",
            ]
        } else {
            vec!["token", "list_id", "id", "view_id", "position", "_x_reason"]
        }
    } else {
        vec!["token", "list_id", "cells", "_x_reason"]
    };
    if fields.len() != expected.len()
        || fields.iter().zip(expected).any(|((k, _), e)| k != e)
        || fields
            .iter()
            .map(|(k, v)| k.len().saturating_add(v.len()))
            .try_fold(0usize, usize::checked_add)
            .is_none_or(|n| n > 16384)
    {
        return false;
    }
    let identity =
        |s: &str| !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control);
    if fields[0].1.is_empty()
        || !identity(&fields[1].1)
        || fields.last().unwrap().1
            != if reorder {
                "reorder-record"
            } else {
                "bulk-update-cells"
            }
    {
        return false;
    }
    if reorder
        && (!identity(&fields[2].1) || !identity(&fields[3].1) || !valid_position(&fields[4].1))
    {
        return false;
    }
    if let Some((_, cells)) = fields.iter().find(|(key, _)| key == "cells") {
        let Ok(value) = crate::slash_commands::codec::decode(cells.as_bytes(), 8192) else {
            return false;
        };
        let Some(cells) = value.as_array().filter(|a| a.len() == 1) else {
            return false;
        };
        let Some(cell) = cells[0].as_object().filter(|o| o.len() == 3) else {
            return false;
        };
        let Some(row) = cell
            .get("row_id")
            .and_then(Value::as_str)
            .filter(|s| identity(s))
        else {
            return false;
        };
        if reorder && row != fields[2].1 {
            return false;
        }
        if !cell
            .get("column_id")
            .and_then(Value::as_str)
            .is_some_and(identity)
        {
            return false;
        }
        if !cell.get("checkbox").is_some_and(Value::is_boolean)
            && !cell
                .get("select")
                .and_then(Value::as_array)
                .is_some_and(|ids| {
                    ids.len() <= 1 && ids.iter().all(|id| id.as_str().is_some_and(identity))
                })
        {
            return false;
        }
    }
    true
}
fn valid_position(key: &str) -> bool {
    let b = key.as_bytes();
    (10..=1024).contains(&b.len())
        && b[..10].iter().all(u8::is_ascii_digit)
        && (b.len() == 10
            || b.len() > 11
                && b[10] == b'.'
                && b[11..].iter().all(u8::is_ascii_alphanumeric)
                && b.last() != Some(&b'0'))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod read_tests;
