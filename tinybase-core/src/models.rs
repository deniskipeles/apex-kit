use crate::schema::CollectionSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub struct Collection {
    pub name: String,
    pub schema: Option<CollectionSchema>,
}

#[derive(Deserialize)]
pub struct Record {
    pub data: Value,
}
