use super::*;

/// Original normalized string or embedded-object first occurrence only.
/// Missing/empty files has no selection; malformed first never selects later.
pub fn first_file_id(message: &Value) -> Result<Option<&str>, Error> {
    let object = message.as_object().ok_or(Error::Shape)?;
    let files = match object.get("files") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Array(files)) => files,
        _ => return Err(Error::Shape),
    };
    let Some(first) = files.first() else {
        return Ok(None);
    };
    let id = if let Some(id) = first.as_str() {
        valid_id(id)?;
        id
    } else {
        raw_id(first)?
    };
    // Ltli canonical selection uses the strict normalized file ID shape,
    // independently of the bounded raw store intake identity check.
    if !(9..=15).contains(&id.len())
        || !id.starts_with('F')
        || !id[1..]
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err(Error::Identity);
    }
    Ok(Some(id))
}

/// Explicit raw-message adapter. No typed Message serialization. Normalized IDs
/// are not upserts; check_file_info rows are not canonical file observations.
/// D8Sp deduplicates before processFiles, preserving the last raw row per ID.
/// The caller supplies supported ordinary messages only: D8Sp's separate
/// excluded-message classifier and temporary-document branches are not ported.
pub fn embedded_files(messages: &[Value]) -> Result<Vec<Value>, Error> {
    if messages.len() > MAX_BATCH {
        return Err(Error::Limit);
    }
    let mut rows = Vec::new();
    let mut indices = BTreeMap::new();
    let mut bytes = 0usize;
    let mut occurrences = 0usize;
    for message in messages {
        let object = message.as_object().ok_or(Error::Shape)?;
        let files = match object.get("files") {
            None | Some(Value::Null) => continue,
            Some(Value::Array(files)) => files,
            _ => return Err(Error::Shape),
        };
        for file in files {
            occurrences += 1;
            if occurrences > MAX_BATCH {
                return Err(Error::Limit);
            }
            if let Some(id) = file.as_str() {
                valid_id(id)?;
                continue;
            }
            let id = raw_id(file)?;
            if file.get("file_access").and_then(Value::as_str) == Some("check_file_info") {
                continue;
            }
            bytes += serde_json::to_vec(file).map_err(|_| Error::Shape)?.len();
            if bytes > MAX_BATCH_BYTES {
                return Err(Error::Limit);
            }
            if let Some(index) = indices.get(id) {
                rows[*index] = file.clone();
            } else {
                if rows.len() >= MAX_BATCH {
                    return Err(Error::Limit);
                }
                indices.insert(id.to_owned(), rows.len());
                rows.push(file.clone());
            }
        }
    }
    Ok(rows)
}
