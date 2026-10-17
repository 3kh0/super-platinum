//! Canonical leaf choices preserve existing zero/single-rule consumers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewSortChoice {
    None,
    Number {
        column_id: String,
        ascending: bool,
    },
    Field {
        column_id: String,
        ascending: bool,
    },
    /// Flat ordered two-or-more rules. Nested/None rows and duplicate fields refuse.
    Ordered(Vec<ViewSortChoice>),
}
impl ViewSortChoice {
    pub fn rules(&self) -> &[Self] {
        match self {
            Self::None => &[],
            Self::Ordered(rows) => rows,
            _ => std::slice::from_ref(self),
        }
    }
    pub fn from_rules(mut rows: Vec<Self>) -> Self {
        match rows.len() {
            0 => Self::None,
            1 => rows.pop().unwrap(),
            _ => Self::Ordered(rows),
        }
    }
    pub fn field(&self) -> Option<(&str, bool)> {
        match self {
            Self::Number {
                column_id,
                ascending,
            }
            | Self::Field {
                column_id,
                ascending,
            } => Some((column_id, *ascending)),
            _ => None,
        }
    }
}
