//! CIuT publication display getters. These are observations, not permission or
//! publishability proofs; workflowCanBePublished additionally needs its actual
//! blocking-error, step and permission selectors.
use super::*;

#[derive(Clone, Copy, Debug)]
pub struct Publication<'a> {
    workflow: &'a Value,
}
impl Editor {
    pub fn has_been_published(&self) -> bool {
        self.publication().has_been_published()
    }
    pub fn publication(&self) -> Publication<'_> {
        Publication {
            workflow: self.raw_workflow(),
        }
    }
}
impl Publication<'_> {
    pub fn is_published(&self) -> bool {
        self.workflow.get("is_published").is_some_and(truthy)
    }
    pub fn has_been_published(&self) -> bool {
        self.workflow
            .get("last_published_version_id")
            .is_some_and(truthy)
    }
    /// lodash.get defaults only an absent property, not explicit null/false/0.
    pub fn last_published_version_id(&self) -> Value {
        self.workflow
            .get("last_published_version_id")
            .cloned()
            .unwrap_or(Value::Null)
    }
    pub fn last_published_date(&self) -> Value {
        self.workflow
            .get("last_published_date")
            .cloned()
            .unwrap_or(json!(""))
    }
    pub fn unpublished_change_count(&self) -> Value {
        self.workflow
            .get("unpublished_change_count")
            .cloned()
            .unwrap_or(json!(0))
    }
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_and_prior_publication_are_independent() {
        let raw = json!({"is_published":false,"last_published_version_id":"V1","last_published_date":42,"unpublished_change_count":2});
        let p = Publication { workflow: &raw };
        assert!(!p.is_published());
        assert!(p.has_been_published());
        assert_eq!(p.last_published_date(), json!(42));
        assert_eq!(p.unpublished_change_count(), json!(2));
    }
    #[test]
    fn getter_defaults_do_not_normalize_explicit_values() {
        let raw = json!({});
        let p = Publication { workflow: &raw };
        assert!(!p.is_published());
        assert!(!p.has_been_published());
        assert_eq!(p.last_published_version_id(), Value::Null);
        assert_eq!(p.last_published_date(), json!(""));
        assert_eq!(p.unpublished_change_count(), json!(0));
        for value in [
            Value::Null,
            json!(false),
            json!(0),
            json!(""),
            json!("0"),
            json!([]),
            json!({}),
        ] {
            let raw = json!({"is_published":value,"last_published_version_id":value,"last_published_date":value,"unpublished_change_count":value});
            let p = Publication { workflow: &raw };
            assert_eq!(p.is_published(), truthy(&value));
            assert_eq!(p.has_been_published(), truthy(&value));
            assert_eq!(p.last_published_date(), value);
            assert_eq!(p.unpublished_change_count(), value);
        }
    }
}
