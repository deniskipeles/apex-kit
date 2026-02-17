use regex::Regex;
use std::fmt::Write;
use ring::{rand::{SystemRandom, SecureRandom}, digest, hmac};
use serde_json::{Value, Map};

/// Converts a byte slice to a lowercase hex string
pub fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

/// Converts a text string into a URL-friendly slug.
/// Example: "Hello World!" -> "hello-world"
pub fn slugify(text: &str) -> String {
    let re_invalid = Regex::new(r"[^a-z0-9]+").unwrap();
    let lower = text.to_lowercase();
    let slug = re_invalid.replace_all(&lower, "-");
    slug.trim_matches('-').to_string()
}

/// Generates a cryptographically secure random hex string of specified byte length.
/// Example: len=4 -> 8 hex characters (e.g. "a1b2c3d4")
pub fn generate_random_hex(len: usize) -> String {
    let rng = SystemRandom::new();
    let mut bytes = vec![0u8; len];
    rng.fill(&mut bytes).unwrap();
    to_hex(&bytes)
}

/// Computes SHA256 hash of input string
pub fn sha256(text: &str) -> String {
    let hash = digest::digest(&digest::SHA256, text.as_bytes());
    to_hex(hash.as_ref())
}

/// Computes SHA512 hash of input string
pub fn sha512(text: &str) -> String {
    let hash = digest::digest(&digest::SHA512, text.as_bytes());
    to_hex(hash.as_ref())
}

/// Computes HMAC-SHA256 signature
pub fn hmac_sha256(key: &str, data: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key.as_bytes());
    let tag = hmac::sign(&key, data.as_bytes());
    to_hex(tag.as_ref())
}


pub fn apply_projection(data: &mut Value, fields_param: &str) {
    if fields_param.trim().is_empty() || fields_param == "*" { return; }

    let raw_fields: Vec<&str> = fields_param.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if raw_fields.is_empty() { return; }

    // Check mode based on first field (exclude starts with '-')
    let is_exclude = raw_fields[0].starts_with('-');

    // Build a tree structure for nested fields: 
    // "author.name" -> tree["author"] = ["name"]
    let mut tree: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut root_fields: Vec<String> = Vec::new();

    for rf in raw_fields {
        let clean = rf.trim_start_matches('-');
        
        // Validation: Ensure mixed modes aren't used (silent fail or strict?)
        // For resilience, we stick to the mode detected from the first valid field.
        if is_exclude && !rf.starts_with('-') { continue; } 
        if !is_exclude && rf.starts_with('-') { continue; }

        if let Some((root, rest)) = clean.split_once('.') {
            tree.entry(root.to_string()).or_default().push(rest.to_string());
            // Implicitly include the root key if we are in include mode so we can traverse it
            if !is_exclude {
                if !root_fields.contains(&root.to_string()) {
                    root_fields.push(root.to_string());
                }
            }
        } else {
            root_fields.push(clean.to_string());
        }
    }

    apply_projection_recursive(data, &root_fields, &tree, is_exclude);
}

fn apply_projection_recursive(
    value: &mut Value, 
    roots: &[String], 
    tree: &std::collections::HashMap<String, Vec<String>>, 
    is_exclude: bool
) {
    match value {
        Value::Object(map) => {
            // 1. Handle Root Keys (Top-level data fields)
            if is_exclude {
                for key in roots {
                    // Check if this key has nested exclusions
                    if let Some(sub_fields) = tree.get(key) {
                        // If exclusion targets "meta.private", we don't delete "meta", we recurse.
                        if let Some(sub_val) = map.get_mut(key) {
                             // Re-join subfields to pass down
                             let sub_param = sub_fields.iter().map(|s| format!("-{}", s)).collect::<Vec<_>>().join(",");
                             apply_projection(sub_val, &sub_param);
                        }
                    } else {
                        // Simple exclusion "email" -> delete it
                        map.remove(key);
                    }
                }
            } else {
                // Include Mode
                // Create replacement map
                let mut new_map = Map::new();
                
                // Always keep "id", "created", "updated" unless explicitly excluded?
                // Standard API practice: IDs usually stick around, but strict projection removes everything else.
                // Let's stick to strict projection. If user wants id, they ask for it.
                // Exception: We apply this to the full RecordResponse which has {id, data, expand}.
                // So this logic typically runs on the `data` object.
                
                for key in roots {
                    if let Some(val) = map.remove(key) {
                        new_map.insert(key.clone(), val);
                    }
                }
                
                // 2. Handle Nested Inclusions via "expand"
                // If the user requested "author.name", we kept "author" in roots.
                // Now we need to recurse into "author" and filter it.
                for (key, sub_fields) in tree {
                    // If key is in new_map (it should be if in roots), recurse
                    if let Some(sub_val) = new_map.get_mut(key) {
                        let sub_param = sub_fields.join(",");
                        apply_projection(sub_val, &sub_param);
                    } else if let Some(_val) = map.remove(key) {
                         // Case: "expand.author" might exist in the record but wasn't in roots?
                         // If "author.name" was requested, "author" IS in roots.
                         // But if we are processing the `expand` object separately, we need care.
                    }
                }

                *map = new_map;
            }
        },
        Value::Array(arr) => {
            // Apply to all items in array
            for item in arr {
                apply_projection_recursive(item, roots, tree, is_exclude);
            }
        },
        _ => {}
    }
}