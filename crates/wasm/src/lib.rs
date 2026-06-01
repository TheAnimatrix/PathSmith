// Copyright (C) 2026 Avarnic
// SPDX-License-Identifier: AGPL-3.0-or-later
// Commercial licensing: COMMERCIAL-LICENSE.md — creo@avarnic.com

//! WebAssembly bindings for PathSmith. Lets any web app convert a raster image to
//! SVG entirely client-side (no upload, no server).
//!
//! ```js
//! import init, { convert, presets } from "./pkg/pathsmith_wasm.js";
//! await init();
//! const svg = convert(new Uint8Array(await file.arrayBuffer()), "hybrid");
//! ```
//!
//! Conversion is CPU-bound and synchronous; for large images run it inside a Web
//! Worker so the UI thread stays responsive.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Convert raw image bytes (PNG/JPEG/WebP/…) to an SVG string using the named
/// preset. Throws a JS error on failure.
#[wasm_bindgen]
pub fn convert(data: &[u8], preset: &str) -> Result<String, JsValue> {
    pathsmith_core::convert_bytes(data, preset).map_err(|e| JsValue::from_str(&e))
}

/// JSON string: `[{ "name": ..., "description": ... }, ...]`.
#[wasm_bindgen]
pub fn presets() -> String {
    let list: Vec<_> = pathsmith_core::presets::all()
        .into_iter()
        .map(|p| serde_json::json!({ "name": p.name, "description": p.description }))
        .collect();
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string())
}
