use apexkit_core::database::traits::Db;
use railwind::{CollectionOptions, Source, parse_to_string, warning::Warning};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

pub async fn compile_styles(db: Arc<dyn Db>) -> Result<String, String> {
    let start_total = Instant::now();

    // 1. Fetch all HTML/HTMX templates from the database
    let templates = db.list_templates().await.map_err(|e| e.to_string())?;

    if templates.is_empty() {
        return Ok("/* No templates found */".to_string());
    }

    // 2. Concatenate all template content into one giant string for analysis
    let mut html_content = String::new();
    for tmpl in &templates {
        html_content.push_str(&tmpl.content);
        html_content.push('\n'); // Ensure spacing between templates
    }

    // 3. Run the pure Rust Railwind JIT Compiler
    let source = Source::String(html_content, CollectionOptions::Html);
    let mut warnings: Vec<Warning> = Vec::new();

    // parse_to_string(source, include_preflight, &mut warnings)
    let final_css = parse_to_string(source, true, &mut warnings);

    // If there were any non-fatal warnings (e.g. unknown classes), log them
    for warning in warnings {
        warn!("Railwind Compiler Warning: {:?}", warning);
    }

    info!(
        "🎨 CSS Compiler: Compiled in {:?}. Final size: {} bytes",
        start_total.elapsed(),
        final_css.len()
    );

    Ok(final_css)
}
