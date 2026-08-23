use super::backends::get_storage_path;
use super::dto::{OgItem, OpenGraphQuery};
use super::image_ops::{get_default_logo, process_image};
use crate::{AppError, AppState, DatabaseConnection, StorageConnection};
use apexkit_core::Db;
use apexkit_core::storage::{LocalStorage, StorageBackend};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::sync::Arc;
use tera::Tera;
use tiny_skia::{Pixmap, Transform};
use usvg::{Options, Tree};

const DEFAULT_OG_TEMPLATE: &str = r##"
<svg width="1200" height="630" viewBox="0 0 1200 630" fill="none" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg-grad" x1="0" y1="0" x2="1200" y2="630" gradientUnits="userSpaceOnUse">
      <stop stop-color="#0F172A" />
      <stop offset="0.5" stop-color="#1E293B" />
      <stop offset="1" stop-color="#090D16" />
    </linearGradient>

    <radialGradient id="glow-neon" cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse"
      gradientTransform="translate(200 200) scale(500)">
      <stop stop-color="#CCFF00" stop-opacity="0.15" />
      <stop offset="1" stop-color="#CCFF00" stop-opacity="0" />
    </radialGradient>

    <radialGradient id="glow-purple" cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse"
      gradientTransform="translate(1000 500) scale(600)">
      <stop stop-color="#8B5CF6" stop-opacity="0.12" />
      <stop offset="1" stop-color="#8B5CF6" stop-opacity="0" />
    </radialGradient>

    <filter id="3d-shadow" x="-30%" y="-30%" width="160%" height="160%">
      <feDropShadow dx="-8" dy="24" stdDeviation="28" flood-color="#000000" flood-opacity="0.5" />
      <feDropShadow dx="0" dy="8" stdDeviation="10" flood-color="#000000" flood-opacity="0.3" />
    </filter>

    <clipPath id="card-clip">
      <rect width="388" height="388" rx="26" />
    </clipPath>
  </defs>

  <rect width="1200" height="630" fill="url(#bg-grad)" />
  <rect width="1200" height="630" fill="url(#glow-neon)" />
  <rect width="1200" height="630" fill="url(#glow-purple)" />

  <g transform="translate(100, 115) rotate(-6, 200, 200)" filter="url(#3d-shadow)">
    <rect x="0" y="0" width="400" height="400" rx="32" fill="#1E293B" stroke="#334155" stroke-width="1.5" />
    <g transform="translate(6, 6)">
      <image href="{{ IMAGE_URL }}" x="0" y="0" width="388" height="388" preserveAspectRatio="xMidYMid slice"
        clip-path="url(#card-clip)" />
    </g>
    <rect x="6" y="6" width="388" height="388" rx="26" fill="none" stroke="#FFFFFF" stroke-opacity="0.2"
      stroke-width="2" />
  </g>

  <!-- Brand Badge -->
  <g transform="translate(560, 115)">
    <rect x="0" y="0" width="220" height="38" rx="19" fill="#CCFF00" />
    <circle cx="20" cy="19" r="5" fill="#0F172A" />
    <text x="36" y="24" font-family="sans-serif" font-size="14" font-weight="bold" fill="#0F172A" letter-spacing="0.5">
      {{ SITE_NAME | default(value="apexkit.pages.dev") | upper }}
    </text>
  </g>

  <!-- DYNAMIC TERA TITLE (Max 2 Lines, Smart Word Wrap) -->
  <text x="560" y="210" font-family="sans-serif" font-size="44" font-weight="bold" fill="#FFFFFF" letter-spacing="-1">
    {% set t_words = TITLE | default(value="What if a single node was all you ever needed?") | split(pat=" ") %}
    {% set_global t_line = "" %}
    {% set_global t_count = 0 %}

    {% for w in t_words %}
    {% if t_count < 2 %} {% if t_line | length > 0 %}{% set test = t_line ~ " " ~ w %}{% else %}{% set test = w %}{%
      endif %}
      {% if test | length > 22 %}
      {% if t_count == 1 %}
      <tspan x="560" dy="50">{{ t_line }}...</tspan>
      {% set_global t_count = 2 %}
      {% else %}
      <tspan x="560" dy="{% if t_count == 0 %}0{% else %}50{% endif %}">{{ t_line }}</tspan>
      {% set_global t_line = w %}
      {% set_global t_count = t_count + 1 %}
      {% endif %}
      {% else %}
      {% set_global t_line = test %}
      {% endif %}
      {% endif %}
      {% endfor %}
      {% if t_count < 2 and t_line | length > 0 %}
        <tspan x="560" dy="{% if t_count == 0 %}0{% else %}50{% endif %}">{{ t_line }}</tspan>
        {% endif %}
  </text>

  <!-- DYNAMIC TERA SUBTITLE (Max 3 Lines, Smart Word Wrap) -->
  <text x="560" y="340" font-family="sans-serif" font-size="20" font-weight="500" fill="#94A3B8">
    {% set s_words = SUBTITLE | default(value="ApexKit was born from the obsession to maximize modern hardware. By using Rust's safety and SQLite's simplicity, we've built a kit that allows you to build excellent applications with extreme efficiency.") | split(pat=" ") %}
    {% set_global s_line = "" %}
    {% set_global s_count = 0 %}

    {% for w in s_words %}
    {% if s_count < 3 %} {% if s_line | length > 0 %}{% set test = s_line ~ " " ~ w %}{% else %}{% set test = w %}{%
      endif %}
      {% if test | length > 45 %}
      {% if s_count == 2 %}
      <tspan x="560" dy="28">{{ s_line }}...</tspan>
      {% set_global s_count = 3 %}
      {% else %}
      <tspan x="560" dy="{% if s_count == 0 %}0{% else %}28{% endif %}">{{ s_line }}</tspan>
      {% set_global s_line = w %}
      {% set_global s_count = s_count + 1 %}
      {% endif %}
      {% else %}
      {% set_global s_line = test %}
      {% endif %}
      {% endif %}
      {% endfor %}
      {% if s_count < 3 and s_line | length > 0 %}
        <tspan x="560" dy="{% if s_count == 0 %}0{% else %}28{% endif %}">{{ s_line }}</tspan>
        {% endif %}
  </text>

  <!-- Photographer / Source Credit Footer -->
  <g transform="translate(560, 475)">
    <rect x="0" y="0" width="420" height="54" rx="16" fill="#1E293B" stroke="#334155" stroke-width="1" />
    <text x="20" y="22" font-family="sans-serif" font-size="11" font-weight="bold" fill="#64748B"
      letter-spacing="0.5">POWERED BY</text>
    <text x="20" y="41" font-family="sans-serif" font-size="15" font-weight="bold" fill="#F8FAFC">
      {{ PHOTOGRAPHER | default(value="ApexKit Platform") | truncate(length=24) }}
    </text>

    <!-- Platform Badge -->
    <rect x="295" y="12" width="105" height="30" rx="15" fill="#334155" />
    <text x="347" y="31" font-family="sans-serif" font-size="12" font-weight="bold" fill="#CCFF00" text-anchor="middle">
      {{ PLATFORM | default(value="APEXKIT") | upper }}
    </text>
  </g>
</svg>
"##;

pub async fn get_scope_logo_base64(db: &Arc<dyn Db>, storage: &Arc<dyn StorageBackend>) -> String {
    let mut logo_data: Option<(Vec<u8>, String)> = None;

    if let Ok(Some(settings)) = db.get_config("general").await {
        if let Some(logo_filename) = settings
            .get("app_logo")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            if let Ok(bytes) = storage.get(logo_filename).await {
                let mime = mime_guess::from_path(logo_filename)
                    .first_or_octet_stream()
                    .to_string();
                logo_data = Some((bytes, mime));
            } else {
                let root_local =
                    LocalStorage::new(&get_storage_path("storage/system/uploads"), "/").await;
                if let Ok(bytes) = root_local.get(logo_filename).await {
                    let mime = mime_guess::from_path(logo_filename)
                        .first_or_octet_stream()
                        .to_string();
                    logo_data = Some((bytes, mime));
                }
            }
        }
    }

    let (bytes, mime) = match logo_data {
        Some(res) => res,
        None => match get_default_logo() {
            Ok((b, m, _)) => (b, m),
            Err(_) => return "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=".to_string(),
        }
    };

    format!("data:{};base64,{}", mime, STANDARD.encode(&bytes))
}

#[utoipa::path(
    get,
    path = "/api/v1/storage/files/opengraph",
    params(OpenGraphQuery),
    responses((status = 200, description = "Generated Image"))
)]
pub async fn generate_opengraph(
    DatabaseConnection(db): DatabaseConnection,
    StorageConnection(storage): StorageConnection,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<OpenGraphQuery>,
) -> Result<Response, AppError> {
    let data_str = if params.data.trim().is_empty() {
        "[]".to_string()
    } else {
        params.data.clone()
    };

    if data_str.len() > 100_000 {
        return Err(AppError::Forbidden("Payload too large".into()));
    }

    let items: Vec<OgItem> = serde_json::from_str(&data_str)
        .map_err(|e| AppError::JsonError(format!("Invalid data JSON array: {}", e)))?;

    if items.len() > 8 {
        return Err(AppError::Forbidden(
            "Maximum of 8 data objects allowed".into(),
        ));
    }

    let format_str = params.format.as_deref().unwrap_or("png");
    let quality = params.quality.unwrap_or(80);
    let data_hash = md5::compute(data_str.as_bytes());
    let cache_key = format!(
        "og_{}_{}_{}_{:x}",
        params.template, format_str, quality, data_hash
    );

    if let Some(cached_bytes) = state.thumb_cache.get(&cache_key).await {
        let mime = mime_guess::from_ext(format_str)
            .first_or_octet_stream()
            .to_string();
        let etag = format!("\"{:x}\"", md5::compute(cached_bytes.as_ref()));

        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, "public, max-age=86400, immutable")
            .header(header::ETAG, etag)
            .body(axum::body::Body::from(cached_bytes.as_ref().clone()))
            .unwrap());
    }

    let raw_svg = if params.template == "default" {
        DEFAULT_OG_TEMPLATE.to_string()
    } else if let Ok(Some(tmpl)) = db.get_template_by_slug(&params.template).await {
        tmpl.content
    } else if let Ok(decoded) = STANDARD.decode(&params.template) {
        String::from_utf8_lossy(&decoded).to_string()
    } else {
        return Err(AppError::NotFound(
            "Template slug not found and not a valid base64 string".into(),
        ));
    };

    let mut context = tera::Context::new();
    
    let default_logo_b64 = get_scope_logo_base64(&db, &storage).await;
    context.insert("IMAGE_URL", &default_logo_b64);
    let fallback_logo_cache = default_logo_b64;

    for item in items {
        if item.r#type == "image" {
            let b64_src = if item.value.starts_with("data:image") {
                item.value
            } else if item.value.starts_with("http://") || item.value.starts_with("https://") {
                match reqwest::get(&item.value).await {
                    Ok(res) if res.status().is_success() => {
                        let mime = res
                            .headers()
                            .get(reqwest::header::CONTENT_TYPE)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("image/jpeg")
                            .to_string();
                        if let Ok(bytes) = res.bytes().await {
                            format!("data:{};base64,{}", mime, STANDARD.encode(&bytes))
                        } else {
                            fallback_logo_cache.clone()
                        }
                    }
                    _ => fallback_logo_cache.clone(),
                }
            } else {
                let (clean_filename, query_str) = if let Some((f, q)) = item.value.split_once('?') {
                    (f, Some(q))
                } else {
                    (item.value.as_str(), None)
                };

                match storage.get(clean_filename).await {
                    Ok(bytes) => {
                        let mut final_bytes = bytes.clone();

                        if let Some(qs) = query_str {
                            let mut blur: Option<f32> = None;
                            for pair in qs.split('&') {
                                if let Some((k, v)) = pair.split_once('=') {
                                    if k == "blur" {
                                        blur = v.parse().ok();
                                    }
                                }
                            }

                            if blur.is_some() {
                                if let Ok(processed_img) = tokio::task::spawn_blocking(move || {
                                    let mut img = image::load_from_memory(&bytes)?;
                                    if let Some(b) = blur {
                                        img = img.blur(b.clamp(0.1, 50.0));
                                    }
                                    let mut buf = std::io::Cursor::new(Vec::new());
                                    let mut encoder =
                                        image::codecs::jpeg::JpegEncoder::new_with_quality(
                                            &mut buf, 85,
                                        );
                                    encoder.encode_image(&img)?;
                                    Ok::<_, image::ImageError>(buf.into_inner())
                                })
                                .await
                                .unwrap()
                                {
                                    final_bytes = processed_img;
                                }
                            }
                        }

                        let mime = mime_guess::from_path(clean_filename)
                            .first_or_octet_stream()
                            .to_string();
                        format!("data:{};base64,{}", mime, STANDARD.encode(&final_bytes))
                    }
                    Err(_) => {
                        tracing::warn!(
                            "OpenGraph: Local image '{}' not found. Falling back to App Logo.",
                            clean_filename
                        );
                        fallback_logo_cache.clone()
                    }
                }
            };
            context.insert(item.target, &b64_src);
        } else {
            context.insert(item.target, &item.value);
        }
    }

    let rendered_svg = Tera::one_off(&raw_svg, &context, true)
        .map_err(|e| AppError::JsonError(format!("Template injection error: {}", e)))?;

    let font_reg_key = "og_font_roboto_reg".to_string();
    let font_reg = if let Some(cached) = state.thumb_cache.get(&font_reg_key).await {
        cached.as_ref().clone()
    } else {
        let bytes = reqwest::get("https://fonts.gstatic.com/s/roboto/v30/KFOmCnqEu92Fr1Me5Q.ttf")
            .await
            .map_err(|_| AppError::UnknownError("Font DL fail".into()))?
            .bytes()
            .await
            .map_err(|_| AppError::UnknownError("Font bytes fail".into()))?
            .to_vec();
        state
            .thumb_cache
            .insert(font_reg_key, Arc::new(bytes.clone()))
            .await;
        bytes
    };

    let font_bold_key = "og_font_roboto_bold".to_string();
    let font_bold = if let Some(cached) = state.thumb_cache.get(&font_bold_key).await {
        cached.as_ref().clone()
    } else {
        let bytes =
            reqwest::get("https://fonts.gstatic.com/s/roboto/v30/KFOlCnqEu92Fr1MmWUlvAw.ttf")
                .await
                .map_err(|_| AppError::UnknownError("Font DL fail".into()))?
                .bytes()
                .await
                .map_err(|_| AppError::UnknownError("Font bytes fail".into()))?
                .to_vec();
        state
            .thumb_cache
            .insert(font_bold_key, Arc::new(bytes.clone()))
            .await;
        bytes
    };

    let safe_svg = rendered_svg
        .replace("system-ui", "sans-serif")
        .replace("font-weight=\"800\"", "font-weight=\"700\"")
        .replace("font-weight=\"600\"", "font-weight=\"700\"");

    let png_bytes = {
        let mut fontdb = usvg::fontdb::Database::new();
        fontdb.load_system_fonts();
        fontdb.load_font_data(font_reg);
        fontdb.load_font_data(font_bold);

        fontdb.set_sans_serif_family("Roboto");
        fontdb.set_serif_family("Roboto");
        fontdb.set_monospace_family("Roboto");

        let mut opt = Options::default();
        opt.font_family = "Roboto".to_string();
        opt.fontdb = Arc::new(fontdb);

        let tree = Tree::from_str(&safe_svg, &opt)
            .map_err(|e| AppError::JsonError(format!("Invalid SVG XML layout: {}", e)))?;

        let pixmap_size = tree.size().to_int_size();
        let mut pixmap = Pixmap::new(pixmap_size.width(), pixmap_size.height())
            .ok_or_else(|| AppError::UnknownError("Failed to allocate canvas memory".into()))?;

        resvg::render(&tree, Transform::default(), &mut pixmap.as_mut());
        pixmap
            .encode_png()
            .map_err(|e| AppError::UnknownError(format!("PNG Encoding failed: {}", e)))?
    };

    process_image(
        &state,
        &headers,
        png_bytes,
        "image/png",
        cache_key,
        None,
        params.format,
        params.quality,
        None,
    )
    .await
}