//! Original Save-owned string observations, without popup Search/Lease custody.
use super::{
    CollationReply, Field, LowerReply, channel_name, compare, decode_collation, decode_lower,
    model_query, same_locale, ticket,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use super_platinum_core::{
    block_actions::home::workflow::directory_local::{PreparedQuery, normalize_lowered_query},
    slack::{Error, api::custom::texty::ranking},
    slack_lists::comments::edit_seed::local_search,
};

const KEY: &str = "ml_fuzzy_match_beyond_word_boundaries_v2";

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Options {
    ticket: String,
    key: String,
    search: String,
    #[serde(deserialize_with = "nullable_string")]
    forced: Option<String>,
    #[serde(deserialize_with = "nullable_string")]
    storage: Option<String>,
    storage_failed: bool,
    group: String,
    pub(super) enabled: bool,
}
fn nullable_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}
impl Options {
    fn validate(&self, ticket: &str) -> Result<(), Error> {
        if self.ticket != ticket
            || self.key != KEY
            || self.forced.is_some() && self.storage.is_some()
            || self.storage_failed && (self.forced.is_some() || self.storage.is_some())
            || self.group
                != self
                    .forced
                    .as_deref()
                    .or(self.storage.as_deref())
                    .unwrap_or("noGroup")
            || self.enabled != matches!(self.group.as_str(), "on" | "treatment")
        {
            return Err(unavailable());
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Eligibility {
    ticket: String,
    query: String,
    locale: String,
    segmenter_locale: String,
    segments: Vec<String>,
    pub(super) eligible: bool,
}
impl Eligibility {
    pub(super) fn validate(&self, ticket: &str, query: &str, locale: &str) -> Result<(), Error> {
        same_locale(locale, &self.locale)?;
        if self.ticket != ticket
            || self.query != query
            || self.segmenter_locale.is_empty()
            || self.segments.len() > 4
            || self.segments.iter().any(String::is_empty)
            || self.eligible != (self.segments.len() == 4)
            || !query.starts_with(&self.segments.concat())
            || !self.eligible && self.segments.concat() != query
        {
            return Err(unavailable());
        }
        Ok(())
    }
}

pub(crate) struct ChannelStrings {
    pub(crate) query: PreparedQuery,
    pub(crate) beyond_boundaries: bool,
    pub(crate) beyond_boundary_eligible: Option<bool>,
    raw_query: String,
    formatted: String,
    locale: String,
    rows: BTreeMap<String, Value>,
    fields: BTreeMap<String, Field>,
    classes: BTreeMap<String, usize>,
    options: Options,
    eligibility: Option<Eligibility>,
}

impl ChannelStrings {
    /// The caller owns original canonical rows, policy and Save revision. The
    /// supplied predicate is checked before and after every bounded native read.
    pub(crate) async fn capture(
        raw_query: &str,
        formatted: &str,
        rows: &[Value],
        current: &impl Fn() -> bool,
    ) -> Result<Self, Error> {
        let (mut inputs, original_rows) = inputs(raw_query, formatted, rows)?;
        let ticket = ticket()?;
        let options = options(&ticket, current).await?;
        let first = lower(current, &ticket, 0, formatted, &[]).await?;
        let normalized = normalize_lowered_query(&first.query);
        let second = lower(current, &ticket, 1, &normalized, &[]).await?;
        same_locale(&first.locale, &second.locale)?;
        let query = PreparedQuery {
            normalized,
            matcher_normalized: normalize_lowered_query(&second.query),
        };
        let eligibility = if options.enabled {
            let reply: Eligibility = evaluate(
                current,
                Function::Eligibility,
                json!({"ticket":ticket,"query":query.matcher_normalized}),
            )
            .await?;
            reply.validate(&ticket, &query.matcher_normalized, &first.locale)?;
            Some(reply)
        } else {
            None
        };
        inputs.extend([query.normalized.clone(), query.matcher_normalized.clone()]);
        let inputs: Vec<_> = inputs.into_iter().collect();
        let mut fields = BTreeMap::new();
        for (stage, batch) in inputs.chunks(128).enumerate() {
            let reply = lower(current, &ticket, stage + 2, "", batch).await?;
            same_locale(&first.locale, &reply.locale)?;
            fields.extend(
                reply
                    .fields
                    .into_iter()
                    .map(|field| (field.text.clone(), field)),
            );
        }
        let names: Vec<_> = rows
            .iter()
            .map(|row| {
                channel_name(&fields, row)
                    .map(|name| name.comparison_lowercase)
                    .map_err(|_| unavailable())
            })
            .collect::<Result<BTreeSet<_>, _>>()?
            .into_iter()
            .collect();
        let extra: Vec<_> = names
            .iter()
            .filter(|s| !fields.contains_key(*s))
            .cloned()
            .collect();
        for (stage, batch) in extra.chunks(128).enumerate() {
            let reply = lower(current, &ticket, inputs.len() + stage + 2, "", batch).await?;
            same_locale(&first.locale, &reply.locale)?;
            fields.extend(
                reply
                    .fields
                    .into_iter()
                    .map(|field| (field.text.clone(), field)),
            );
        }
        let reply: CollationReply = evaluate(
            current,
            Function::Collation,
            json!({"ticket":ticket,"names":names}),
        )
        .await?;
        let classes = decode_collation(&ticket, &first.locale, &names, reply)?;
        // No live DOM/global cache is installed. Refuse configuration drift
        // across these reads; the caller also compares its earlier Plan flag.
        if options != self::options(&ticket, current).await? || !current() {
            return Err(unavailable());
        }
        Ok(Self {
            beyond_boundaries: options.enabled,
            beyond_boundary_eligible: eligibility.as_ref().map(|v| v.eligible),
            query,
            raw_query: raw_query.into(),
            formatted: formatted.into(),
            locale: first.locale,
            rows: original_rows,
            fields,
            classes,
            options,
            eligibility,
        })
    }

    pub(crate) fn name(&self, row: &Value) -> Result<local_search::Name, ranking::Error> {
        if row["id"].as_str().and_then(|id| self.rows.get(id)) != Some(row) {
            return Err(missing("original Save channel"));
        }
        let name = channel_name(&self.fields, row)?;
        Ok(local_search::Name {
            normalized: name.normalized,
            comparison_lowercase: name.comparison_lowercase,
        })
    }
    pub(crate) fn normalize(&self, text: &str) -> Result<String, ranking::Error> {
        self.fields
            .get(text)
            .map(|field| normalize_lowered_query(&field.locale_lowered))
            .ok_or_else(|| missing(text))
    }
    pub(crate) fn compare(&self, a: &str, b: &str) -> Result<Ordering, ranking::Error> {
        compare(&self.classes, a, b)
    }
    pub(crate) fn snapshot(&self) -> Value {
        json!({"rawQuery":self.raw_query,"formattedQuery":self.formatted,"locale":self.locale,
            "query":{"normalized":self.query.normalized,"matcherNormalized":self.query.matcher_normalized},
            "options":self.options,"eligibility":self.eligibility,
            "fields":self.fields.values().map(|f| json!({"text":f.text,"lowered":f.lowered,"localeLowered":f.locale_lowered})).collect::<Vec<_>>(),
            "classes":self.classes})
    }
}

fn inputs(
    raw: &str,
    formatted: &str,
    rows: &[Value],
) -> Result<(BTreeSet<String>, BTreeMap<String, Value>), Error> {
    if rows.len() > 32768
        || [raw, formatted]
            .iter()
            .any(|s| s.encode_utf16().count() > 32768)
    {
        return Err(unavailable());
    }
    let mut fields = BTreeSet::from([
        String::new(),
        raw.into(),
        formatted.into(),
        model_query(formatted).into(),
    ]);
    let mut originals = BTreeMap::new();
    for row in rows {
        let id = row["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(unavailable)?;
        let name = row["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(unavailable)?;
        if name.encode_utf16().count() > 32768 || originals.insert(id.into(), row.clone()).is_some()
        {
            return Err(unavailable());
        }
        fields.insert(name.into());
        fields.extend(name.split('\n').map(str::to_owned));
        if fields.len() > 262144 {
            return Err(unavailable());
        }
    }
    Ok((fields, originals))
}

pub(crate) async fn beyond_boundaries(current: &impl Fn() -> bool) -> Result<bool, Error> {
    Ok(options(&ticket()?, current).await?.enabled)
}
pub(super) async fn options(ticket: &str, current: &impl Fn() -> bool) -> Result<Options, Error> {
    let reply: Options = evaluate(current, Function::Options, json!({"ticket":ticket})).await?;
    reply.validate(ticket)?;
    Ok(reply)
}
pub(super) async fn lower(
    current: &impl Fn() -> bool,
    ticket: &str,
    stage: usize,
    query: &str,
    fields: &[String],
) -> Result<LowerReply, Error> {
    let reply = evaluate(
        current,
        Function::Lower,
        json!({"ticket":ticket,"stage":stage,"query":query,"fields":fields}),
    )
    .await?;
    decode_lower(ticket, stage, fields, reply)
}
pub(super) enum Function {
    Options,
    Eligibility,
    Lower,
    Collation,
}
pub(super) async fn evaluate<T: serde::de::DeserializeOwned>(
    current: &impl Fn() -> bool,
    function: Function,
    input: Value,
) -> Result<T, Error> {
    if !current() {
        return Err(unavailable());
    }
    let function = match function {
        Function::Options => "saveChannelOptions",
        Function::Eligibility => "saveChannelEligibility",
        Function::Lower => "profileChannelLower",
        Function::Collation => "profileChannelCollation",
    };
    let script = format!(
        "{}\n{}\ndioxus.send({function}({input}));await dioxus.recv();",
        include_str!("../render.js"),
        include_str!("save.js")
    );
    let mut eval = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(5), eval.recv::<Option<T>>())
        .await
        .map_err(|_| unavailable())
        .and_then(|r| r.map_err(|_| unavailable()))
        .and_then(|r| r.ok_or_else(unavailable));
    let _ = eval.send(());
    if !current() {
        return Err(unavailable());
    }
    result
}
fn missing(text: &str) -> ranking::Error {
    ranking::Error::MissingObservation(text.into())
}
fn unavailable() -> Error {
    Error::Api("list_save_channel_strings_unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_preserve_actual_empty_precedence_and_reject_contradictions() {
        let mut value = Options {
            ticket: "1".into(),
            key: KEY.into(),
            search: String::new(),
            forced: Some(String::new()),
            storage: None,
            storage_failed: false,
            group: String::new(),
            enabled: false,
        };
        assert!(value.validate("1").is_ok());
        value.group = "noGroup".into();
        assert!(value.validate("1").is_err());
        value.forced = None;
        assert!(value.validate("1").is_ok());
        value.storage_failed = true;
        assert!(value.validate("1").is_ok());
        value.enabled = true;
        assert!(value.validate("1").is_err());
        assert!(value.validate("2").is_err());
        let mut raw = serde_json::to_value(&value).unwrap();
        raw.as_object_mut().unwrap().remove("storage");
        assert!(serde_json::from_value::<Options>(raw).is_err());
    }
    #[test]
    fn original_rows_and_renderer_fields_are_separate_from_save_owner() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<ChannelStrings>();
        let row = json!({"id":"C1","name":"I\nCAFÉ","previous_names":["unused"]});
        let (fields, rows) = inputs("#Query", "Query", std::slice::from_ref(&row)).unwrap();
        assert!(fields.contains("I"));
        assert!(fields.contains("CAFÉ"));
        assert!(!fields.contains("unused"));
        assert_eq!(rows["C1"], row);
        assert!(inputs("x", "x", &[row.clone(), row]).is_err());
        assert!(inputs("x", "x", &[json!({"id":"C2"})]).is_err());
    }
    #[test]
    fn eligibility_binds_original_second_query_and_actual_prefix() {
        let mut value = Eligibility {
            ticket: "1".into(),
            query: "ábcdZ".into(),
            locale: "en-US".into(),
            segmenter_locale: "en-US".into(),
            segments: vec!["á".into(), "b".into(), "c".into(), "d".into()],
            eligible: true,
        };
        assert!(value.validate("1", "ábcdZ", "en-US").is_ok());
        assert!(value.validate("1", "abcdZ", "en-US").is_err());
        assert!(value.validate("2", "ábcdZ", "en-US").is_err());
        value.segments[0] = "wrong".into();
        assert!(value.validate("1", "ábcdZ", "en-US").is_err());
    }
}
