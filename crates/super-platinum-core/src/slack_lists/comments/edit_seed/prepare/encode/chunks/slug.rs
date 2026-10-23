use super::*;
fn or(value: Option<&Value>, fallback: Value) -> Value {
    value
        .filter(|v| truthy(Some(v)))
        .cloned()
        .unwrap_or(fallback)
}
fn field(chunk: &mut Value, key: &str, value: Option<&Value>) {
    if let Some(v) = value.filter(|v| truthy(Some(v))) {
        chunk[key] = v.clone();
    }
}
pub(super) fn encode(op: &Value, preformatted: bool) -> Result<Vec<Value>> {
    let slug = op["attributes"]["slackslug"]
        .as_object()
        .ok_or(SeedUnavailable::MalformedMessage)?;
    let mut value = if !truthy(slug.get("type")) {
        let mut value = json!({"type":"link"});
        if let Some(id) = slug.get("id") {
            value["url"] = id.clone();
        }
        value
    } else {
        let kind = slug["type"]
            .as_str()
            .ok_or(SeedUnavailable::MalformedMessage)?;
        let id = slug.get("id");
        let secondary = slug.get("secondaryObjectId");
        let tertiary = slug.get("tertiaryObjectId");
        let url = slug.get("url");
        let label = slug.get("label");
        let id_or_empty = or(id, json!(""));
        let mut value = match kind {
            "message" => {
                let mut v = json!({"type":"message_mention","message_ts":id_or_empty,"channel_id":or(secondary,json!(""))});
                if let Some(text) = url.filter(|v| truthy(Some(v))).or(secondary) {
                    v["text"] = text.clone();
                }
                field(&mut v, "url", url);
                field(&mut v, "author_id", slug.get("authorId"));
                field(&mut v, "thread_ts", tertiary);
                v
            }
            "canvas" | "file" | "list" => {
                let mut v =
                    json!({"type":if kind=="list"{"list_record"}else{kind},"file_id":id_or_empty});
                if let Some(text) = url.filter(|v| truthy(Some(v))).or(id) {
                    v["text"] = text.clone();
                }
                field(&mut v, "url", url);
                if kind == "canvas" {
                    field(&mut v, "section_id", secondary);
                    if slug.get("is_skill_invocation") == Some(&json!(true)) {
                        v["is_skill_invocation"] = json!(true);
                    }
                }
                if kind == "list" {
                    field(&mut v, "record_id", secondary);
                    field(&mut v, "view_id", tertiary);
                }
                v
            }
            "skill" => {
                let mut v = json!({"type":"skill","skill_ref":id_or_empty});
                field(&mut v, "url", url);
                if slug.get("is_skill_invocation") == Some(&json!(true)) {
                    v["is_skill_invocation"] = json!(true);
                }
                v
            }
            "channel_tab" => {
                let mut v = json!({"type":"channel","channel_id":or(secondary,json!(""))});
                if let Some(id) = id {
                    v["tab_id"] = id.clone();
                }
                v
            }
            "workflow" => {
                let mut v = json!({"type":"workflow_mention","function_trigger_id":id_or_empty,"workflow_id":or(secondary,json!(""))});
                if let Some(text) = label.filter(|v| truthy(Some(v))).or(id) {
                    v["text"] = text.clone();
                }
                field(&mut v, "url", url);
                v
            }
            "link" => {
                let mut v = json!({"type":"link"});
                if let Some(id) = id {
                    v["url"] = id.clone();
                }
                if Some(&op["insert"]) != id {
                    v["text"] = op["insert"].clone();
                }
                if label != id {
                    v["truncated"] = json!(true);
                }
                v
            }
            "work_object" | "attachment" => {
                let mut v = json!({"type":if kind=="work_object"{"work_object_mention"}else{"attachment_mention"},"url":or(url,or(id,json!("")))});
                let text = or(label, or(Some(&op["insert"]), json!("")));
                if kind == "work_object" || truthy(Some(&text)) {
                    v["text"] = text;
                }
                if kind == "work_object" {
                    v["entity_id"] = or(secondary, json!(""));
                    v["app_id"] = or(tertiary, json!(""));
                } else {
                    field(&mut v, "entity_id", secondary);
                    field(&mut v, "app_id", tertiary);
                    field(
                        &mut v,
                        "reference_object_type",
                        slug.get("referenceObjectType"),
                    );
                }
                for (from, to) in [
                    ("iconUrl", "icon_url"),
                    ("iconName", "icon_name"),
                    ("fullSizePreviewEnabled", "full_size_preview_enabled"),
                    ("productName", "product_name"),
                ] {
                    field(&mut v, to, slug.get(from));
                }
                v
            }
            _ => return Err(SeedUnavailable::UnsupportedToken(format!("slug:{kind}"))),
        };
        // Retained getter methods preserve known source metadata values.
        if !value.is_object() {
            return Err(SeedUnavailable::MalformedMessage);
        }
        value
    };
    if !preformatted {
        value = text::annotate(value, op);
    }
    Ok(vec![value])
}
