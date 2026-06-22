use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String, // email
    pub uid: i64,    // user id
    pub role: String,
    pub exp: usize,
    #[serde(default)]
    pub scope: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: i64,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: String, // "admin" or "user"
    pub metadata: Option<serde_json::Value>,
}
