// Copyright (C) 2026 Avarnic
// SPDX-License-Identifier: AGPL-3.0-or-later
// Commercial licensing: COMMERCIAL-LICENSE.md — creo@avarnic.com

//! Self-hostable HTTP server for PathSmith (Vectorize + Multicrop).
//!
//! Endpoints:
//!   GET  /healthz                          -> "ok"
//!   GET  /presets                          -> [{name, description}, ...]
//!   POST /convert?pipeline=hybrid          -> image/svg+xml   (single variant)
//!   POST /convert/batch?pipelines=all      -> JSON {results, errors}  (multi variant)
//!        ...&zip=1                          -> application/zip of all SVGs
//!   POST /crop?boxes=[{x,y,width,height}]  -> JSON {results, errors}  (PNG crops, base64)
//!        ...&zip=1                          -> application/zip of all PNG crops
//!   GET  /                                 -> embedded web UI (unless disabled)
//!
//! The request body is the raw image bytes (e.g. `curl --data-binary @img.png`
//! or browser `fetch(url, { method: 'POST', body: file })`).

use std::io::Write;
use std::time::Duration;

use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Query},
    http::{header, StatusCode, Uri},
    response::Response,
    routing::{get, post},
    Router,
};
use base64::Engine;
use clap::Parser;
use rayon::prelude::*;
use rust_embed::RustEmbed;
use serde::Deserialize;
use tower::limit::GlobalConcurrencyLimitLayer;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer};

#[derive(RustEmbed)]
#[folder = "frontend"]
struct Assets;

#[derive(Parser)]
#[command(name = "pathsmith-server", version, about = "PathSmith HTTP server + UI")]
struct Args {
    /// Port to listen on.
    #[arg(long, env = "PORT", default_value_t = 8080)]
    port: u16,
    /// Host/interface to bind.
    #[arg(long, env = "PATHSMITH_HOST", default_value = "0.0.0.0")]
    host: String,
    /// Disable the bundled web UI (pure API). Also via PATHSMITH_UI=0.
    #[arg(long = "no-ui")]
    no_ui: bool,
    /// Max upload size in bytes.
    #[arg(long, env = "PATHSMITH_MAX_BYTES", default_value_t = 20 * 1024 * 1024)]
    max_bytes: usize,
    /// Max concurrent in-flight conversions.
    #[arg(long, env = "PATHSMITH_MAX_CONCURRENCY", default_value_t = 64)]
    max_concurrency: usize,
    /// Per-request timeout in seconds.
    #[arg(long, env = "PATHSMITH_TIMEOUT_SECS", default_value_t = 60)]
    timeout_secs: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let ui_enabled = !args.no_ui && std::env::var("PATHSMITH_UI").as_deref() != Ok("0");

    let mut app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/presets", get(list_presets))
        .route("/convert", post(convert_one))
        .route("/convert/batch", post(convert_batch))
        .route("/crop", post(crop));

    if ui_enabled {
        app = app.fallback(static_handler);
    }

    let app = app.layer(
        ServiceBuilder::new()
            .layer(CorsLayer::permissive())
            .layer(DefaultBodyLimit::max(args.max_bytes))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                Duration::from_secs(args.timeout_secs),
            ))
            .layer(GlobalConcurrencyLimitLayer::new(args.max_concurrency)),
    );

    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!(
        "pathsmith-server listening on http://{addr}  (ui: {})",
        if ui_enabled { "on" } else { "off" }
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    println!("\nshutting down");
}

// ---- handlers ----------------------------------------------------------------

#[derive(Deserialize)]
struct ConvertQuery {
    pipeline: Option<String>,
}

async fn convert_one(Query(q): Query<ConvertQuery>, body: Bytes) -> Response {
    let pipeline = q.pipeline.unwrap_or_else(|| "hybrid".to_string());
    let work = tokio::task::spawn_blocking(move || pathsmith_core::convert_bytes(&body, &pipeline)).await;
    match work {
        Ok(Ok(svg)) => resp(StatusCode::OK, "image/svg+xml", svg.into_bytes()),
        Ok(Err(e)) => resp(StatusCode::BAD_REQUEST, "text/plain", e.into_bytes()),
        Err(e) => resp(StatusCode::INTERNAL_SERVER_ERROR, "text/plain", e.to_string().into_bytes()),
    }
}

#[derive(Deserialize)]
struct BatchQuery {
    pipelines: Option<String>,
    zip: Option<u8>,
}

async fn convert_batch(Query(q): Query<BatchQuery>, body: Bytes) -> Response {
    let names = resolve_pipelines(q.pipelines.as_deref());
    let as_zip = q.zip.unwrap_or(0) != 0;

    let work = tokio::task::spawn_blocking(move || -> Result<Vec<(String, Result<String, String>)>, String> {
        let img = pathsmith_core::decode(&body)?;
        Ok(names
            .par_iter()
            .map(|name| {
                let r = match pathsmith_core::presets::by_name(name) {
                    Some(p) => pathsmith_core::run_pipeline(&img, &p),
                    None => Err(format!("unknown preset: {name}")),
                };
                (name.clone(), r)
            })
            .collect())
    })
    .await;

    let results = match work {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return resp(StatusCode::BAD_REQUEST, "text/plain", e.into_bytes()),
        Err(e) => {
            return resp(StatusCode::INTERNAL_SERVER_ERROR, "text/plain", e.to_string().into_bytes())
        }
    };

    if as_zip {
        match build_zip(&results) {
            Ok(bytes) => zip_response(bytes, "pathsmith-variants.zip"),
            Err(e) => resp(StatusCode::INTERNAL_SERVER_ERROR, "text/plain", e.into_bytes()),
        }
    } else {
        batch_json(&results)
    }
}

#[derive(Deserialize)]
struct CropQuery {
    /// URL-encoded JSON array of boxes: `[{ "x", "y", "width", "height", "label"? }]`.
    boxes: Option<String>,
    zip: Option<u8>,
}

async fn crop(Query(q): Query<CropQuery>, body: Bytes) -> Response {
    let as_zip = q.zip.unwrap_or(0) != 0;
    let boxes: Vec<pathsmith_core::multicrop::CropBox> = match q.boxes.as_deref() {
        Some(s) if !s.is_empty() => match serde_json::from_str(s) {
            Ok(b) => b,
            Err(e) => return resp(StatusCode::BAD_REQUEST, "text/plain", format!("invalid boxes: {e}").into_bytes()),
        },
        _ => return resp(StatusCode::BAD_REQUEST, "text/plain", b"no boxes provided".to_vec()),
    };
    if boxes.is_empty() {
        return resp(StatusCode::BAD_REQUEST, "text/plain", b"no boxes provided".to_vec());
    }

    let work = tokio::task::spawn_blocking(move || pathsmith_core::multicrop::crop_bytes(&body, &boxes)).await;
    let results = match work {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return resp(StatusCode::BAD_REQUEST, "text/plain", e.into_bytes()),
        Err(e) => return resp(StatusCode::INTERNAL_SERVER_ERROR, "text/plain", e.to_string().into_bytes()),
    };

    if as_zip {
        match build_crop_zip(&results) {
            Ok(bytes) => zip_response(bytes, "pathsmith-crops.zip"),
            Err(e) => resp(StatusCode::INTERNAL_SERVER_ERROR, "text/plain", e.into_bytes()),
        }
    } else {
        crop_json(&results)
    }
}

async fn list_presets() -> Response {
    let list: Vec<serde_json::Value> = pathsmith_core::presets::all()
        .into_iter()
        .map(|p| serde_json::json!({ "name": p.name, "description": p.description }))
        .collect();
    json_response(StatusCode::OK, &serde_json::Value::Array(list))
}

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(content) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return resp(StatusCode::OK, mime.as_ref(), content.data.into_owned());
    }
    // SPA fallback.
    match Assets::get("index.html") {
        Some(c) => resp(StatusCode::OK, "text/html", c.data.into_owned()),
        None => resp(StatusCode::NOT_FOUND, "text/plain", b"not found".to_vec()),
    }
}

// ---- helpers -----------------------------------------------------------------

fn resolve_pipelines(arg: Option<&str>) -> Vec<String> {
    match arg {
        None | Some("all") | Some("") => pathsmith_core::presets::names(),
        Some(s) => s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
    }
}

fn batch_json(results: &[(String, Result<String, String>)]) -> Response {
    let mut ok = serde_json::Map::new();
    let mut errors = serde_json::Map::new();
    for (name, r) in results {
        match r {
            Ok(svg) => {
                ok.insert(
                    name.clone(),
                    serde_json::json!({ "svg": svg, "bytes": svg.len() }),
                );
            }
            Err(e) => {
                errors.insert(name.clone(), serde_json::Value::String(e.clone()));
            }
        }
    }
    json_response(
        StatusCode::OK,
        &serde_json::json!({ "results": ok, "errors": errors }),
    )
}

fn build_zip(results: &[(String, Result<String, String>)]) -> Result<Vec<u8>, String> {
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts: zip::write::SimpleFileOptions =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, r) in results {
        if let Ok(svg) = r {
            zw.start_file(format!("{name}.svg"), opts).map_err(|e| e.to_string())?;
            zw.write_all(svg.as_bytes()).map_err(|e| e.to_string())?;
        }
    }
    let cursor = zw.finish().map_err(|e| e.to_string())?;
    Ok(cursor.into_inner())
}

fn crop_json(results: &[(String, Result<Vec<u8>, String>)]) -> Response {
    let mut ok = serde_json::Map::new();
    let mut errors = serde_json::Map::new();
    for (name, r) in results {
        match r {
            Ok(png) => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(png);
                ok.insert(name.clone(), serde_json::json!({ "png": b64, "bytes": png.len() }));
            }
            Err(e) => {
                errors.insert(name.clone(), serde_json::Value::String(e.clone()));
            }
        }
    }
    json_response(StatusCode::OK, &serde_json::json!({ "results": ok, "errors": errors }))
}

fn build_crop_zip(results: &[(String, Result<Vec<u8>, String>)]) -> Result<Vec<u8>, String> {
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts: zip::write::SimpleFileOptions =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, r) in results {
        if let Ok(png) = r {
            zw.start_file(format!("{name}.png"), opts).map_err(|e| e.to_string())?;
            zw.write_all(png).map_err(|e| e.to_string())?;
        }
    }
    let cursor = zw.finish().map_err(|e| e.to_string())?;
    Ok(cursor.into_inner())
}

fn resp(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(body))
        .expect("valid response")
}

fn json_response(status: StatusCode, value: &serde_json::Value) -> Response {
    resp(status, "application/json", serde_json::to_vec(value).unwrap_or_default())
}

fn zip_response(bytes: Vec<u8>, filename: &str) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\""))
        .body(Body::from(bytes))
        .expect("valid response")
}
