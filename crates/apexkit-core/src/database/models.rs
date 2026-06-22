use serde_json::Value;

pub mod ai;
pub mod data;
pub mod schema;
pub mod script;

pub use ai::*;
pub use data::*;
pub use schema::*;
pub use script::*;

pub struct ExpandableItem<'a> {
    pub(crate) data: &'a Value,
    pub(crate) expand: &'a mut Option<Value>,
}
