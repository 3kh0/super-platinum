use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

mod activity;
mod conversations;
mod core;
mod search;
mod users;

pub use activity::*;
pub use conversations::*;
pub use core::*;
pub use search::*;
pub use users::*;

#[cfg(test)]
mod fixture_tests;
