// Copyright (C) 2026 Avarnic
// SPDX-License-Identifier: AGPL-3.0-or-later
// Commercial licensing: COMMERCIAL-LICENSE.md — creo@avarnic.com

//! pathsmith-core: raster -> SVG conversion pipelines + multicrop.
//!
//! The pipeline is preprocess -> trace (vtracer) -> postprocess, selected by a
//! named [`config::Pipeline`] ("preset"/"variant"). This crate is pure Rust and
//! has no filesystem or platform dependencies, so it powers the CLI, the HTTP
//! server, and the WASM/browser build alike.

pub mod config;
pub mod multicrop;
pub mod postprocess;
pub mod preprocess;
pub mod presets;
pub mod trace;

use base64::Engine;
use config::Pipeline;
use image::RgbaImage;
use std::io::Cursor;

/// Run a fully-specified pipeline against an already-decoded RGBA image.
pub fn run_pipeline(img: &RgbaImage, pipe: &Pipeline) -> Result<String, String> {
    if pipe.passthrough {
        return Ok(passthrough_svg(img));
    }
    let pre = preprocess::preprocess(img, &pipe.pre);
    let svg = trace::trace(&pre, &pipe.tracer)?;
    Ok(postprocess::postprocess(&svg, &pipe.post))
}

/// Decode raw image bytes (PNG/JPEG/WebP/…) and convert with the named preset.
pub fn convert_bytes(data: &[u8], preset: &str) -> Result<String, String> {
    let pipe = presets::by_name(preset).ok_or_else(|| format!("unknown preset: {preset}"))?;
    let img = decode(data)?;
    run_pipeline(&img, &pipe)
}

/// Decode raw image bytes into an RGBA image.
pub fn decode(data: &[u8]) -> Result<RgbaImage, String> {
    Ok(image::load_from_memory(data)
        .map_err(|e| format!("decode failed: {e}"))?
        .to_rgba8())
}

/// Embed the raster as a base64 PNG inside an SVG wrapper. 100% match, but the
/// result is a raster container, not real vector data.
fn passthrough_svg(img: &RgbaImage) -> String {
    let (w, h) = img.dimensions();
    let mut buf = Vec::new();
    // Re-encode as PNG; unwrap is safe (writing to an in-memory buffer).
    image::DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .expect("PNG encode to memory");
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\"><image width=\"{w}\" height=\"{h}\" \
         href=\"data:image/png;base64,{b64}\"/></svg>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny 2x2 RGBA image for smoke tests.
    fn tiny() -> RgbaImage {
        RgbaImage::from_fn(2, 2, |x, _y| {
            if x == 0 {
                image::Rgba([10, 10, 10, 255])
            } else {
                image::Rgba([240, 30, 30, 255])
            }
        })
    }

    #[test]
    fn every_preset_runs() {
        let img = tiny();
        for pipe in presets::all() {
            let svg = run_pipeline(&img, &pipe).expect(&pipe.name);
            assert!(svg.contains("<svg"), "{} produced no svg", pipe.name);
        }
    }

    #[test]
    fn passthrough_is_100_match_container() {
        let svg = run_pipeline(&tiny(), &presets::by_name("passthrough").unwrap()).unwrap();
        assert!(svg.contains("data:image/png;base64,"));
    }

    #[test]
    fn unknown_preset_errors() {
        assert!(convert_bytes(&[], "nope").is_err());
    }
}
