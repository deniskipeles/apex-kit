// =========================== /teamspace/studios/this_studio/tinybase/tinybase/tinybase-api/src/css_compiler.rs ===========================
use std::sync::Arc;
use std::collections::HashSet;
use tinybase_core::Db;
use regex::Regex;
use tracing::{info, error, debug};
use std::time::Instant;

// Embed the CSS.
const FULL_CSS: &str = include_str!("../../static/tailwind-dark.full.css");

// Standard Tailwind Preflight (Reset)
// We inject this manually to avoid parsing complex selectors like *, ::before, ::after
const PREFLIGHT: &str = r#"
*,::before,::after{box-sizing:border-box;border-width:0;border-style:solid;border-color:#e5e7eb}
html{line-height:1.5;-webkit-text-size-adjust:100%;-moz-tab-size:4;tab-size:4;font-family:ui-sans-serif,system-ui,sans-serif}
body{margin:0;line-height:inherit}
hr{height:0;color:inherit;border-top-width:1px}
abbr:where([title]){text-decoration:underline dotted}
h1,h2,h3,h4,h5,h6{font-size:inherit;font-weight:inherit}
a{color:inherit;text-decoration:inherit}
b,strong{font-weight:bolder}
code,kbd,samp,pre{font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace;font-size:1em}
small{font-size:80%}
sub,sup{font-size:75%;line-height:0;position:relative;vertical-align:baseline}
sub{bottom:-.25em}
sup{top:-.5em}
table{text-indent:0;border-color:inherit;border-collapse:collapse}
button,input,optgroup,select,textarea{font-family:inherit;font-size:100%;font-weight:inherit;line-height:inherit;color:inherit;margin:0;padding:0}
button,select{text-transform:none}
button,[type=button],[type=reset],[type=submit]{-webkit-appearance:button;background-color:transparent;background-image:none}
:-moz-focusring{outline:auto}
:-moz-ui-invalid{box-shadow:none}
progress{vertical-align:baseline}
::-webkit-inner-spin-button,::-webkit-outer-spin-button{height:auto}
[type=search]{-webkit-appearance:textfield;outline-offset:-2px}
::-webkit-search-decoration{-webkit-appearance:none}
::-webkit-file-upload-button{-webkit-appearance:button;font:inherit}
summary{display:list-item}
blockquote,dl,dd,h1,h2,h3,h4,h5,h6,hr,figure,p,pre{margin:0}
fieldset{margin:0;padding:0}
legend{padding:0}
ol,ul,menu{list-style:none;margin:0;padding:0}
textarea{resize:vertical}
input::placeholder,textarea::placeholder{opacity:1;color:#9ca3af}
button,[role=button]{cursor:pointer}
:disabled{cursor:default}
img,svg,video,canvas,audio,iframe,embed,object{display:block;vertical-align:middle}
img,video{max-width:100%;height:auto}
[hidden]{display:none}
"#;

pub async fn compile_styles(db: Arc<dyn Db>) -> Result<String, String> {
    let start_total = Instant::now();

    if FULL_CSS.is_empty() {
        error!("CSS Compiler: Embedded tailwind.full.css is empty!");
        return Ok(PREFLIGHT.to_string());
    }

    // 1. Remove comments to clean up the input
    let clean_source = remove_css_comments(FULL_CSS);
    
    // 2. Fetch Templates
    let templates = db.list_templates().await.map_err(|e| e.to_string())?;
    
    // 3. Extract Classes
    let mut used_classes = HashSet::new();
    for tmpl in &templates {
        let extracted = extract_classes_from_html(&tmpl.content);
        used_classes.extend(extracted);
    }
    
    info!("CSS Compiler: Extracted {} unique classes.", used_classes.len());

    // 4. Normalize Classes
    // e.g. "w-1/2" -> ".w-1\/2"
    let mut search_selectors = HashSet::new();
    for cls in used_classes {
        search_selectors.insert(escape_class_name(&cls));
    }

    // 5. Purge
    // We only scan for UTILITY classes (starting with dot) to avoid the mess with element selectors
    let purged_utilities = parse_utilities(&clean_source, &search_selectors);

    // 6. Combine Preflight + Utilities
    let final_css = format!("{}\n{}", PREFLIGHT, purged_utilities);

    info!("CSS Compiler: Compiled in {:?}. Final size: {} bytes", start_total.elapsed(), final_css.len());

    Ok(final_css)
}

fn remove_css_comments(css: &str) -> String {
    let re = Regex::new(r"(?s)/\*.*?\*/").unwrap();
    re.replace_all(css, "").to_string()
}

fn extract_classes_from_html(html: &str) -> HashSet<String> {
    let class_attr = Regex::new(r#"class\s*=\s*["']([^"']+)["']"#).unwrap();
    let mut classes = HashSet::new();
    for caps in class_attr.captures_iter(html) {
        if let Some(match_str) = caps.get(1) {
            for c in match_str.as_str().split_whitespace() {
                if !c.contains("{{") && !c.contains("%}") {
                    classes.insert(c.to_string());
                }
            }
        }
    }
    classes
}

fn escape_class_name(class: &str) -> String {
    let mut escaped = String::new();
    for c in class.chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            escaped.push(c);
        } else {
            escaped.push('\\');
            escaped.push(c);
        }
    }
    escaped
}

/// Parses ONLY utility classes (starting with .) and @media blocks containing them.
/// Ignores element selectors to prevent the nesting issues you saw.
fn parse_utilities(content: &str, search_selectors: &HashSet<String>) -> String {
    let mut output = String::new();
    let mut current_media_query: Option<String> = None;
    let mut buffer = String::new();
    let mut is_inside_rule = false;
    let mut keep_current_rule = false;
    
    // STRICTER Regex: Matches start of class rule:  .classname {
    // Does NOT match element selectors like "html {" or "* {"
    let class_rule_regex = Regex::new(r"^\s*(\.[^{]+)\{\s*$").unwrap();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        // 1. Media Queries
        if trimmed.starts_with("@media") && trimmed.ends_with("{") {
            current_media_query = Some(line.to_string());
            continue;
        }
        if trimmed == "}" && current_media_query.is_some() && !is_inside_rule {
            current_media_query = None;
            continue;
        }

        // 2. Start of a Class Rule
        if let Some(caps) = class_rule_regex.captures(line) {
            is_inside_rule = true;
            buffer.clear();
            buffer.push_str(line); // e.g. ".bg-red-500 {"
            // Removed newline for minification effect
            
            let selector_str = caps.get(1).unwrap().as_str();
            keep_current_rule = false;

            // Check if ANY part of the selector matches our used classes
            for part in selector_str.split(',') {
                let part_trim = part.trim();
                // Check if part contains ".escaped-class"
                // This is a naive check but works well for Tailwind's generated output
                for search in search_selectors {
                    let dot_search = format!(".{}", search);
                    // Ensure exact match or boundary match (e.g. .text-red-500:hover)
                    if part_trim.contains(&dot_search) {
                        keep_current_rule = true;
                        break;
                    }
                }
                if keep_current_rule { break; }
            }
            continue;
        }

        // 3. Inside Rule
        if is_inside_rule {
            // Simple minification: trim spaces
            buffer.push_str(trimmed); 

            if trimmed == "}" {
                is_inside_rule = false;
                
                if keep_current_rule {
                    if let Some(ref mq) = current_media_query {
                        output.push_str(mq);
                        output.push_str(&buffer);
                        output.push('}'); // Close media
                    } else {
                        output.push_str(&buffer);
                    }
                    output.push('\n');
                }
                buffer.clear();
            }
        }
    }

    output
}