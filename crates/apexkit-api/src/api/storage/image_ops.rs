use super::backends::get_storage_path;
use super::dto::FileParams;
use crate::api::site::assets::Assets;
use crate::{AppError, AppState, DatabaseConnection, StorageConnection};
use apexkit_core::storage::{LocalStorage, StorageBackend};
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use image::ImageEncoder;
use image::codecs::avif::AvifEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use std::io::Cursor;
use std::sync::Arc;

pub async fn process_image(
    state: &AppState,
    headers: &HeaderMap,
    original_bytes: Vec<u8>,
    original_mime: &str,
    cache_key: String,
    dim_str: Option<String>,
    req_format: Option<String>,
    req_quality: Option<u8>,
    req_blur: Option<f32>,
) -> Result<Response, AppError> {
    let cache_header_val = "public, max-age=31536000, immutable";
    let etag = format!("\"{:x}\"", md5::compute(&original_bytes));

    if let Some(if_none_match) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    {
        if if_none_match == etag {
            return Ok(Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .body(Body::empty())
                .unwrap());
        }
    }

    if (dim_str.is_none() && req_format.is_none() && req_quality.is_none() && req_blur.is_none())
        || original_mime.contains("svg")
    {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, original_mime)
            .header(header::CACHE_CONTROL, cache_header_val)
            .header(header::ETAG, etag)
            .body(Body::from(original_bytes))
            .unwrap());
    }

    let quality = req_quality.unwrap_or(80).clamp(1, 100);

    let (target_format, target_mime) =
        match req_format.as_deref().unwrap_or("").to_lowercase().as_str() {
            "webp" => (image::ImageFormat::WebP, "image/webp"),
            "jpg" | "jpeg" => (image::ImageFormat::Jpeg, "image/jpeg"),
            "png" => (image::ImageFormat::Png, "image/png"),
            "avif" => (image::ImageFormat::Avif, "image/avif"),
            "gif" => (image::ImageFormat::Gif, "image/gif"),
            _ => (
                image::ImageFormat::from_mime_type(original_mime)
                    .unwrap_or(image::ImageFormat::Png),
                original_mime,
            ),
        };

    let dim_part = dim_str.as_deref().unwrap_or("orig");
    let fmt_part = target_mime.split('/').next_back().unwrap_or("bin");
    let blur_part = req_blur
        .map(|b| format!("_blur{:.1}", b))
        .unwrap_or_default();
    let full_cache_key = format!(
        "{}_{}_{}_q{}{}",
        cache_key, dim_part, fmt_part, quality, blur_part
    );

    if let Some(cached_bytes) = state.thumb_cache.get(&full_cache_key).await {
        let thumb_etag = format!("\"{:x}\"", md5::compute(cached_bytes.as_ref()));

        if let Some(if_none_match) = headers
            .get(header::IF_NONE_MATCH)
            .and_then(|v| v.to_str().ok())
        {
            if if_none_match == thumb_etag {
                return Ok(Response::builder()
                    .status(StatusCode::NOT_MODIFIED)
                    .body(Body::empty())
                    .unwrap());
            }
        }

        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, target_mime)
            .header(header::CACHE_CONTROL, cache_header_val)
            .header(header::ETAG, thumb_etag)
            .body(Body::from(cached_bytes.as_ref().clone()))
            .unwrap());
    }

    let (w, h) = if let Some(d) = &dim_str {
        parse_dimensions(d).unwrap_or((0, 0))
    } else {
        (0, 0)
    };
    let bytes_for_processing = original_bytes.clone();

    let img_result = tokio::task::spawn_blocking(move || {
        let img = image::load_from_memory(&bytes_for_processing)?;

        let mut processed_img = if w > 0 || h > 0 {
            let target_w = if w == 0 { u32::MAX } else { w };
            let target_h = if h == 0 { u32::MAX } else { h };
            img.resize(target_w, target_h, FilterType::Triangle)
        } else {
            img
        };

        if let Some(sigma) = req_blur {
            let safe_sigma = sigma.clamp(0.1, 50.0);
            processed_img = processed_img.blur(safe_sigma);
        }

        Ok::<_, image::ImageError>(processed_img)
    })
    .await
    .map_err(|e| AppError::UnknownError(e.to_string()))?;

    match img_result {
        Ok(processed_img) => {
            let mut buffer = Cursor::new(Vec::new());
            let encoding_success = match target_format {
                image::ImageFormat::WebP => {
                    if let Ok(encoder) = webp::Encoder::from_image(&processed_img) {
                        let webp_memory = encoder.encode(quality as f32);
                        std::io::Write::write_all(&mut buffer, &webp_memory).is_ok()
                    } else {
                        false
                    }
                }
                image::ImageFormat::Jpeg => JpegEncoder::new_with_quality(&mut buffer, quality)
                    .encode_image(&processed_img)
                    .is_ok(),
                image::ImageFormat::Avif => {
                    AvifEncoder::new_with_speed_quality(&mut buffer, 8, quality)
                        .write_image(
                            processed_img.as_bytes(),
                            processed_img.width(),
                            processed_img.height(),
                            processed_img.color(),
                        )
                        .is_ok()
                }
                _ => processed_img.write_to(&mut buffer, target_format).is_ok(),
            };

            if !encoding_success {
                return Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, original_mime)
                    .header(header::CACHE_CONTROL, cache_header_val)
                    .header(header::ETAG, etag)
                    .body(Body::from(original_bytes))
                    .unwrap());
            }

            let thumb_bytes = buffer.into_inner();
            state
                .thumb_cache
                .insert(full_cache_key, Arc::new(thumb_bytes.clone()))
                .await;

            let thumb_etag = format!("\"{:x}\"", md5::compute(&thumb_bytes));
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, target_mime)
                .header(header::CACHE_CONTROL, cache_header_val)
                .header(header::ETAG, thumb_etag)
                .body(Body::from(thumb_bytes))
                .unwrap())
        }
        Err(_) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, original_mime)
            .header(header::CACHE_CONTROL, cache_header_val)
            .header(header::ETAG, etag)
            .body(Body::from(original_bytes))
            .unwrap()),
    }
}

pub fn parse_dimensions(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse::<u32>().ok()?;
        let h = parts[1].parse::<u32>().ok()?;
        return Some((w, h));
    }
    None
}

pub fn get_default_logo() -> Result<(Vec<u8>, String, String), AppError> {
    let default_path = "images/apexkit-logo.svg";
    match Assets::get(default_path) {
        Some(content) => {
            let mime = mime_guess::from_path(default_path).first_or_octet_stream();
            Ok((
                content.data.to_vec(),
                mime.to_string(),
                "default".to_string(),
            ))
        }
        None => Err(AppError::NotFound("Default logo asset missing".into())),
    }
}

#[utoipa::path(get, path = "/logo", params(FileParams), responses((status = 200, description = "App Logo")))]
pub async fn serve_app_logo(
    DatabaseConnection(db): DatabaseConnection,
    StorageConnection(storage): StorageConnection,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<FileParams>,
) -> Result<Response, AppError> {
    let settings = db
        .get_config("general")
        .await
        .map_err(|e| AppError::UnknownError(e.to_string()))?;
    let (bytes, mime, cache_key_base) = if let Some(val) = settings {
        if let Some(logo_filename) = val
            .get("app_logo")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            match storage.get(logo_filename).await {
                Ok(b) => {
                    let m = mime_guess::from_path(logo_filename).first_or_octet_stream();
                    (b, m.to_string(), logo_filename.to_string())
                }
                Err(_) => {
                    let root_local =
                        LocalStorage::new(&get_storage_path("storage/system/uploads"), "/").await;
                    match root_local.get(logo_filename).await {
                        Ok(b) => {
                            let m = mime_guess::from_path(logo_filename).first_or_octet_stream();
                            (b, m.to_string(), logo_filename.to_string())
                        }
                        Err(_) => get_default_logo()?,
                    }
                }
            }
        } else {
            get_default_logo()?
        }
    } else {
        get_default_logo()?
    };

    process_image(
        &state,
        &headers,
        bytes,
        &mime,
        format!("logo_{}", cache_key_base),
        params.thumb,
        params.format,
        params.quality,
        params.blur,
    )
    .await
}
