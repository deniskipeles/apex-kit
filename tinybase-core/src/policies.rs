// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/policies.rs start here ===========================
use crate::auth::Claims;
use serde_json::Value;

pub enum Action {
    Read,
    Create,
    Update,
    Delete,
}

pub fn check_access(
    policy_string: &str,
    user: Option<&Claims>,
    record_data: Option<&Value>,
) -> bool {
    match policy_string {
        "public" => true,
        "auth" => user.is_some(),
        "admin" => user.map(|u| u.role == "admin").unwrap_or(false),
        rule if rule.starts_with("owner:") => {
            // Syntax: "owner:user_id_field" (e.g., "owner:created_by")
            if let Some(u) = user {
                // Admin bypasses ownership checks
                if u.role == "admin" { return true; }

                let field_name = &rule[6..]; // strip "owner:"
                
                if let Some(data) = record_data {
                     // Check if data[field_name] == user.id
                    if let Some(owner_val) = data.get(field_name) {
                         // Handle number/string conversions loosely
                         if let Some(owner_id) = owner_val.as_i64() {
                             return owner_id == u.uid;
                         }
                         if let Some(owner_str) = owner_val.as_str() {
                             return owner_str == u.uid.to_string();
                         }
                    }
                }
                // If checking 'Create' or 'Read' (list), record_data might be None or Partial.
                // For strict ownership on Create, we usually enforce the user ID injection in the handler.
                false
            } else {
                false
            }
        }
        _ => false, // Default deny
    }
}
// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-core/src/policies.rs ends here ===========================