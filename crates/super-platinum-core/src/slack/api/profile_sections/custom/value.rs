use super::*;
use chrono::NaiveDate;
use std::sync::LazyLock;

#[derive(Clone, PartialEq)]
pub enum SectionValue {
    Text(String),
    Date(String),
    Link {
        display: String,
        uri: String,
    },
    Tags(Vec<String>),
    Persons(Vec<String>),
    Location {
        city: String,
        country: String,
        description: String,
        state: String,
    },
    LongText(Vec<Value>),
}
#[derive(Clone, serde::Deserialize)]
pub struct Country {
    pub name: String,
    pub code: String,
}
/// Exact English names/codes from captured B70HT; phone metadata is not copied.
pub fn countries() -> &'static [Country] {
    static ALL: LazyLock<Vec<Country>> = LazyLock::new(|| {
        serde_json::from_str(include_str!("countries.json")).expect("captured country table")
    });
    &ALL
}
fn strings(e: &Value, key: &str) -> Result<Vec<String>, Error> {
    match e.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(v)) => v
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid("The profile list has an unsupported value."))
            })
            .collect(),
        _ => Err(invalid("The profile list is malformed.")),
    }
}
impl SectionValue {
    pub(super) fn read(e: &Value) -> Result<Self, Error> {
        let text = |key| schema::text(e, key).map(str::to_owned);
        Ok(match required(e, "__typename")? {
            "ProfileTextElement" => Self::Text(text("text")?),
            "ProfileDateElement" => Self::Date(text("date")?),
            "ProfileLinkElement" => Self::Link {
                display: text("displayText")?,
                uri: text("uri")?,
            },
            "ProfileTagsElement" => Self::Tags(strings(e, "tags")?),
            "ProfilePersonElement" => Self::Persons(strings(e, "persons")?),
            "ProfileCurrentLocationElement" => Self::Location {
                city: text("city")?,
                country: text("country")?,
                description: text("description")?,
                state: text("state")?,
            },
            "ProfileLongTextElement" => {
                let blocks = text("blocks")?;
                Self::LongText(if blocks.is_empty() {
                    Vec::new()
                } else {
                    serde_json::from_str::<Vec<Value>>(&blocks)
                        .map_err(|_| invalid("The rich profile field is malformed."))?
                })
            }
            _ => return Err(invalid("This profile field requires another editor.")),
        })
    }
    pub(super) fn kind(&self) -> u8 {
        match self {
            Self::Text(_) => 0,
            Self::Date(_) => 1,
            Self::Link { .. } => 2,
            Self::Tags(_) => 3,
            Self::Persons(_) => 4,
            Self::Location { .. } => 5,
            Self::LongText(_) => 6,
        }
    }
    pub(super) fn wire(&self, id: &str) -> Value {
        let mut item = json!({"element_id":id});
        match self {
            Self::Text(v) => item["text"] = json!({"text":v}),
            Self::Date(v) => item["date"] = json!({"date":v}),
            Self::Link { display, uri } => item["link"] = json!({"displayText":display,"uri":uri}),
            Self::Tags(v) => item["tags"] = json!({"tags":v}),
            Self::Persons(v) => item["person"] = json!({"persons":v}),
            Self::Location {
                city,
                country,
                description,
                state,
            } => {
                item["currentLocation"] =
                    json!({"city":city,"country":country,"description":description,"state":state})
            }
            Self::LongText(v) => {
                item["long_text"] = json!({"blocks":if v.is_empty(){String::new()}else{serde_json::to_string(v).unwrap()}})
            }
        }
        item
    }
    pub(super) fn validate(&self, field: &SectionField) -> Result<(), Error> {
        let length = |v: &str| v.encode_utf16().count();
        let valid = match self {
            Self::Text(v) => {
                length(v) <= 256
                    && (v.is_empty() || field.options.is_empty() || field.options.contains(v))
            }
            Self::Date(v) => {
                v.is_empty()
                    || (v.len() == 10
                        && NaiveDate::parse_from_str(v, "%Y-%m-%d")
                            .is_ok_and(|date| date.format("%Y-%m-%d").to_string() == *v))
            }
            Self::Link { display, uri } => length(display) <= 256 && valid_url(uri),
            Self::Tags(v) => v.len() <= 75,
            Self::Persons(v) => {
                v.iter()
                    .all(|id| crate::block_actions::form::valid_message_user(id))
                    && v.iter().collect::<HashSet<_>>().len() == v.len()
            }
            Self::Location {
                city,
                country,
                description,
                state,
            } => {
                // Captured QulO string.min skips only null/undefined, not "".
                // Preserve the captured minimum even for Clear Location: source
                // renders optional labels but its form validator rejects empties.
                [city, description, state].iter().all(|v| length(v) >= 2)
                    && (country.is_empty() || countries().iter().any(|c| c.code == *country))
            }
            Self::LongText(blocks) => {
                let original = match &field.value {
                    Some(Self::LongText(v)) => v.as_slice(),
                    _ => &[],
                };
                super::rich::validate(blocks, original)?;
                true
            }
        };
        if valid {
            Ok(())
        } else {
            Err(invalid("Check this profile field's value or length."))
        }
    }
}
/// Captured 3Uzt / usIq: no spaces/javascript/about; http(s) requires a host
/// with an ASCII alphanumeric dot pair or localhost. Other absolute schemes
/// with a path are accepted. Browser-relative fallback is deliberately excluded.
fn valid_url(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    if text.contains(' ') {
        return false;
    }
    let Ok(url) = url::Url::parse(text) else {
        return false;
    };
    if matches!(url.scheme(), "javascript" | "about")
        || (url.path().is_empty() && url.host_str().is_none())
    {
        return false;
    }
    if !matches!(url.scheme(), "http" | "https") {
        return true;
    }
    url.host_str().is_some_and(|host| {
        host == "localhost"
            || host.as_bytes().windows(3).any(|w| {
                w[0].is_ascii_alphanumeric() && w[1] == b'.' && w[2].is_ascii_alphanumeric()
            })
    })
}
