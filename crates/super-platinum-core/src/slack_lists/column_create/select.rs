//! Closed ordinary Select creation options; no defaults or arbitrary metadata.
use super::{ListError, Value, json};
use std::collections::HashSet;
const COLORS: [&str; 9] = [
    "indigo", "blue", "cyan", "pink", "green", "gray", "purple", "orange", "brown",
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectCreateOption {
    label: String,
    value: String,
    color: String,
}
impl SelectCreateOption {
    pub fn valid_label(label: &str) -> bool {
        !label.trim().is_empty()
            && label.trim().encode_utf16().count() <= 256
            && !label.chars().any(char::is_control)
    }
    pub fn capture(label: &str, value: &str, color: &str) -> Result<Self, ListError> {
        if !Self::valid_label(label)
            || value.len() != 11
            || !value.starts_with("Opt")
            || !value.as_bytes()[3..]
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            || !COLORS.contains(&color)
        {
            return Err(ListError(
                "Use a nonempty option label of at most 256 UTF-16 units and a supported option identity and color.",
            ));
        }
        Ok(Self {
            label: label.trim().into(),
            value: value.into(),
            color: color.into(),
        })
    }
    /// Action-time generation only. UUID entropy is already an existing dependency.
    /// Rejection sampling avoids modulo bias; version/variant bytes are excluded.
    pub fn generate(label: &str) -> Result<Self, ListError> {
        if !Self::valid_label(label) {
            return Err(ListError(
                "Enter an option label on one line, at most 256 UTF-16 units.",
            ));
        }
        let mut samples = Vec::with_capacity(9);
        for _ in 0..4 {
            let id = uuid::Uuid::new_v4();
            for (index, byte) in id.as_bytes().iter().copied().enumerate() {
                if !matches!(index, 6 | 8) && byte < 252 {
                    samples.push(byte);
                }
                if samples.len() == 9 {
                    break;
                }
            }
            if samples.len() == 9 {
                break;
            }
        }
        if samples.len() != 9 {
            return Err(ListError(
                "Could not create an option identity. Try adding the option again.",
            ));
        }
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut value = String::from("Opt");
        for byte in &samples[..8] {
            value.push(alphabet[(*byte % 36) as usize] as char);
        }
        Self::capture(label, &value, COLORS[(samples[8] % 9) as usize])
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn value(&self) -> &str {
        &self.value
    }
    pub fn color(&self) -> &str {
        &self.color
    }
    fn wire(&self) -> Value {
        json!({"label":self.label,"value":self.value,"color":self.color})
    }
}
/// Ordinary Select cardinality; neither variant introduces defaults or a new schema type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectCreateFormat {
    #[default]
    Single,
    Multiple,
}
impl SelectCreateFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single_select",
            Self::Multiple => "multi_select",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectCreateOptions {
    format: SelectCreateFormat,
    choices: Vec<SelectCreateOption>,
}
impl SelectCreateOptions {
    pub const MAX_CHOICES: usize = 8;
    pub fn capture(choices: Vec<SelectCreateOption>) -> Result<Self, ListError> {
        Self::capture_format(SelectCreateFormat::Single, choices)
    }
    pub fn capture_format(
        format: SelectCreateFormat,
        choices: Vec<SelectCreateOption>,
    ) -> Result<Self, ListError> {
        let mut seen = HashSet::new();
        if choices.is_empty()
            || choices.len() > Self::MAX_CHOICES
            || choices.iter().any(|c| !seen.insert(c.value.as_str()))
        {
            return Err(ListError(
                "Add between one and eight options with distinct identities.",
            ));
        }
        let result = Self { format, choices };
        if result.wire().to_string().len() > 8192 {
            return Err(ListError(
                "These Select options exceed the supported request size.",
            ));
        }
        Ok(result)
    }
    /// Exact shape validator shared by the narrow transport gate. Never repairs wire data.
    pub fn from_wire(raw: &Value) -> Result<Self, ListError> {
        let error = || ListError("Unsupported Select creation options.");
        let map = raw.as_object().ok_or_else(error)?;
        if map.len() != 2 {
            return Err(error());
        }
        let format = match raw["format"].as_str() {
            Some("single_select") => SelectCreateFormat::Single,
            Some("multi_select") => SelectCreateFormat::Multiple,
            _ => return Err(error()),
        };
        let values = raw["choices"].as_array().ok_or_else(error)?;
        if values.is_empty() || values.len() > Self::MAX_CHOICES {
            return Err(error());
        }
        let mut choices = Vec::with_capacity(values.len());
        for value in values {
            let map = value.as_object().ok_or_else(error)?;
            if map.len() != 3 {
                return Err(error());
            }
            choices.push(SelectCreateOption::capture(
                value["label"].as_str().ok_or_else(error)?,
                value["value"].as_str().ok_or_else(error)?,
                value["color"].as_str().ok_or_else(error)?,
            )?);
        }
        let result = Self::capture_format(format, choices)?;
        if result.wire() != *raw {
            return Err(error());
        }
        Ok(result)
    }
    pub fn format(&self) -> SelectCreateFormat {
        self.format
    }
    pub fn choices(&self) -> &[SelectCreateOption] {
        &self.choices
    }
    pub fn wire(&self) -> Value {
        json!({"format":self.format.as_str(),"choices":self.choices.iter().map(SelectCreateOption::wire).collect::<Vec<_>>()})
    }
}
