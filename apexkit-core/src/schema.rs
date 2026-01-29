use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;
use rand::Rng;

// Helper to generate Hex ID
pub fn generate_hex_id() -> String {
    let mut rng = rand::thread_rng();
    format!("{:x}", rng.r#gen::<u32>())
}

fn default_true() -> bool { true }

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema, Default)]
pub struct CollectionSchema {
    pub fields: HashMap<String, FieldDefinition>,
    
    #[serde(default)]
    pub policies: CollectionPolicies,
    
    #[serde(default)]
    pub relations: HashMap<String, RelationDefinition>,

    // Tracks field renaming: "current_name" -> ["old_name_v1", "old_name_v2"]
    // Used to help users migrate data when schema changes
    #[serde(default)]
    pub field_history: HashMap<String, Vec<String>>,

    // --- Multi-Field Unique Constraints ---
    // Example: [ ["username", "organization_id"], ["slug", "locale"] ]
    #[serde(default)]
    pub composite_unique: Vec<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
pub struct RelationDefinition {
    pub target_collection: String,
    pub relation_type: RelationType,
    #[serde(default)]
    pub position: i32,
    #[serde(default = "generate_hex_id")]
    pub uid: String,
    #[serde(default)]
    pub required: bool,
    // Stable Index of target collection
    #[serde(default)]
    pub target_index: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RelationType {
    One,
    Many,
}

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
pub struct CollectionPolicies {
    pub read: String,
    pub create: String,
    pub update: String,
    pub delete: String,
}

impl Default for CollectionPolicies {
    fn default() -> Self {
        Self {
            read: "public".to_string(),
            create: "auth".to_string(),
            update: "admin".to_string(),
            delete: "admin".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
pub struct FieldDefinition {
    pub r#type: FieldType,
    pub required: bool,
    
    #[schema(value_type = Option<Object>)]
    pub default: Option<serde_json::Value>,
    
    // Tantivy Search Index (Full Text)
    #[serde(default)]
    pub ose_indexed: bool,

    // [NEW] SQL B-Tree Index (Filtering/Sorting)
    #[serde(default)]
    pub sql_indexed: bool,
    
    // AI Embeddings
    #[serde(default)]
    pub vectorize: bool, 

    // [NEW] Auto-inject owner ID (Defaults to true for Owner fields)
    #[serde(default = "default_true")]
    pub auto: bool,

    #[serde(default)]
    pub position: i32, 
    
    #[serde(default = "generate_hex_id")]
    pub uid: String,   

    // --- Dynamic Validation ---
    pub unique: Option<bool>,
    
    // Numbers
    pub min: Option<f64>,
    pub max: Option<f64>,
    
    // String / Text / Blob
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub pattern: Option<String>, 
    
    // Select
    pub options: Option<Vec<String>>,
    
    // File / Blob
    pub mime_types: Option<Vec<String>>,
    pub max_size: Option<usize>, 
    
    // Vector
    pub dimension: Option<usize>,
    
    // Relation / Owner
    pub relation_to: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    // Basic
    String,  // Short text (Varchar)
    Text,    // Long text
    Number,  // Int/Float
    Boolean, // True/False
    
    // Extended Strings
    Email,
    Url,
    
    // Complex
    Date,    // ISO 8601 String
    Select,  // String from Options
    Json,    // Structured Object/Array
    
    // Binary/Storage
    File,    // Filename/ID reference
    Blob,    // Base64 Encoded Data
    
    // Relational / AI
    Relation, // ID Reference to another record
    Owner,    // User ID reference
    Vector,   // Array<Float> for embeddings
    GeoPoint, // { "lat": 40.7, "lng": -74.0 }
}