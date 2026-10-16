use super::*;
pub(super) fn label(op: Op) -> &'static str {
    match op {
        Op::Equal => "is equal to",
        Op::NotEqual => "is not equal to",
        Op::LessThan => "is less than",
        Op::GreaterThan => "is greater than",
        Op::IsEmpty => "is empty",
        Op::IsNotEmpty => "is not empty",
    }
}

#[cfg(test)]
mod tests;
