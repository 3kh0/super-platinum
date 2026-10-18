//! Renderer inputs from the captured collaboration payload, not a fabricated
//! display-name lookup. Absence remains absence so the retained component can
//! render its own loading/default branch while acquisition runs separately.
use super::{CanvasCollabSnapshot, Error, RawMessage};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MountedContext {
    team: String,
    users: Vec<User>,
    channels: Vec<Channel>,
    salesforce: Vec<Salesforce>,
    emojis: Vec<Emoji>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct User {
    id: String,
    display_name: Option<String>,
}
#[derive(Serialize)]
struct Channel {
    id: String,
    name: Option<String>,
    deleted: Option<bool>,
    nonexistent: Option<bool>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Salesforce {
    id: String,
    error: Option<String>,
    fields: Vec<Field>,
    structured_fields: Option<Vec<StructuredField>>,
}
#[derive(Serialize)]
struct Field {
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StructuredField {
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Emoji {
    id: String,
    display_name: Option<String>,
    team_id: Option<String>,
    display_url: Option<String>,
    deleted: Option<bool>,
    medium: Option<String>,
    large: Option<String>,
    xlarge: Option<String>,
    static_small: Option<String>,
    static_medium: Option<String>,
    static_large: Option<String>,
    static_xlarge: Option<String>,
    unicode: Option<String>,
}
fn string(raw: &RawMessage, tag: u32) -> Result<Option<String>, Error> {
    Ok(raw.string(tag)?.map(str::to_owned))
}
fn records(raw: &RawMessage, tag: u32) -> Result<BTreeMap<String, RawMessage>, Error> {
    let mut records = BTreeMap::new();
    for value in raw.messages(tag)? {
        let id = value
            .string(1)?
            .filter(|s| !s.is_empty())
            .ok_or(Error::Unsupported)?;
        if records.insert(id.to_owned(), value).is_some() {
            return Err(Error::Unsupported);
        }
    }
    Ok(records)
}
impl MountedContext {
    pub fn capture(snapshot: &CanvasCollabSnapshot) -> Result<Self, Error> {
        Self::from_payload(
            snapshot.init.workspace_id(),
            &snapshot.load.payload()?.ok_or(Error::Unsupported)?,
        )
    }
    fn from_payload(team: &str, payload: &RawMessage) -> Result<Self, Error> {
        let users = records(payload, 57)?
            .into_iter()
            .map(|(id, value)| {
                Ok(User {
                    id,
                    display_name: string(&value, 2)?,
                })
            })
            .collect::<Result<_, Error>>()?;
        let channels = records(payload, 56)?
            .into_iter()
            .map(|(id, value)| {
                Ok(Channel {
                    id,
                    name: string(&value, 2)?,
                    deleted: value.boolean(8)?,
                    nonexistent: value.boolean(9)?,
                })
            })
            .collect::<Result<_, Error>>()?;
        let salesforce = records(payload, 71)?
            .into_iter()
            .map(|(id, value)| {
                let fields = value
                    .messages(3)?
                    .into_iter()
                    .map(|field| {
                        Ok(Field {
                            key: string(&field, 1)?,
                            value: string(&field, 2)?,
                        })
                    })
                    .collect::<Result<_, Error>>()?;
                let structured_fields = if value.has(6) {
                    Some(
                        value
                            .messages(6)?
                            .into_iter()
                            .map(|field| {
                                Ok(StructuredField {
                                    label: string(&field, 1)?,
                                    api_name: string(&field, 2)?,
                                    value: string(&field, 3)?,
                                })
                            })
                            .collect::<Result<_, Error>>()?,
                    )
                } else {
                    None
                };
                Ok(Salesforce {
                    id,
                    error: string(&value, 4)?,
                    fields,
                    structured_fields,
                })
            })
            .collect::<Result<_, Error>>()?;
        let emojis = records(payload, 58)?
            .into_iter()
            .map(|(id, value)| {
                Ok(Emoji {
                    id,
                    display_name: string(&value, 2)?,
                    team_id: string(&value, 3)?,
                    display_url: string(&value, 4)?,
                    deleted: value.boolean(5)?,
                    medium: string(&value, 6)?,
                    large: string(&value, 7)?,
                    xlarge: string(&value, 8)?,
                    static_small: string(&value, 9)?,
                    static_medium: string(&value, 10)?,
                    static_large: string(&value, 11)?,
                    static_xlarge: string(&value, 12)?,
                    unicode: string(&value, 13)?,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            team: team.into(),
            users,
            channels,
            salesforce,
            emojis,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::wire::EncodedField as F;
    fn text(tag: u32, value: &str) -> F {
        F::bytes(tag, value.as_bytes()).unwrap()
    }
    fn message(tag: u32, fields: &[F]) -> F {
        F::message(tag, &RawMessage::from_fields(fields).unwrap()).unwrap()
    }
    fn project(fields: &[F]) -> serde_json::Value {
        serde_json::to_value(
            MountedContext::from_payload("T1", &RawMessage::from_fields(fields).unwrap()).unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn title_context_preserves_absence_empty_values_and_original_user_namespace() {
        let value = project(&[
            message(57, &[text(1, "su:U1"), text(2, "")]),
            message(57, &[text(1, "su:U2")]),
            message(
                56,
                &[text(1, "sc:C1"), text(2, "Hidden"), F::uint(9, 1).unwrap()],
            ),
        ]);
        assert_eq!(value["team"], "T1");
        assert_eq!(value["users"][0]["id"], "su:U1");
        assert_eq!(value["users"][0]["displayName"], "");
        assert!(value["users"][1]["displayName"].is_null());
        assert_eq!(value["channels"][0]["nonexistent"], true);
        assert!(value["channels"][0]["deleted"].is_null());
    }
    #[test]
    fn structured_salesforce_and_emoji_fields_remain_separate_and_ordered() {
        let value = project(&[
            message(
                71,
                &[
                    text(1, "ssr:R"),
                    message(3, &[text(1, "legacy"), text(2, "old")]),
                    message(6, &[text(1, "Label"), text(2, "Api"), text(3, "")]),
                    message(6, &[text(1, "Api"), text(2, "Other"), text(3, "last")]),
                ],
            ),
            message(
                58,
                &[
                    text(1, "se:custom/T1"),
                    text(4, "https://example.test/live"),
                    text(9, "https://example.test/static"),
                    text(13, "unicode"),
                ],
            ),
        ]);
        assert_eq!(value["salesforce"][0]["fields"][0]["key"], "legacy");
        assert_eq!(
            value["salesforce"][0]["structuredFields"][0]["apiName"],
            "Api"
        );
        assert_eq!(value["salesforce"][0]["structuredFields"][0]["value"], "");
        assert_eq!(
            value["salesforce"][0]["structuredFields"][1]["value"],
            "last"
        );
        assert_eq!(
            value["emojis"][0]["displayUrl"],
            "https://example.test/live"
        );
        assert_eq!(
            value["emojis"][0]["staticSmall"],
            "https://example.test/static"
        );
        assert_eq!(value["emojis"][0]["unicode"], "unicode");
    }
    #[test]
    fn duplicate_or_malformed_cached_context_cannot_become_a_title_model() {
        let item = message(57, &[text(1, "su:U1"), text(2, "Name")]);
        let duplicate = RawMessage::from_fields(&[item.clone(), item]).unwrap();
        assert!(MountedContext::from_payload("T1", &duplicate).is_err());
        let malformed = RawMessage::from_fields(&[message(56, &[text(2, "No identity")])]).unwrap();
        assert!(MountedContext::from_payload("T1", &malformed).is_err());
        let empty = project(&[]);
        assert_eq!(empty["users"], serde_json::json!([]));
        assert_eq!(empty["salesforce"], serde_json::json!([]));
    }
}
