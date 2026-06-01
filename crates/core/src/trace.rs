// Copyright (C) 2026 Avarnic
// SPDX-License-Identifier: AGPL-3.0-or-later
// Commercial licensing: COMMERCIAL-LICENSE.md — creo@avarnic.com

//! vtracer wrapper. Traces an in-memory RGBA image to an SVG string.
//!
//! Unlike the Python reference (which round-tripped through temp files), this
//! uses vtracer's in-memory `convert` — no filesystem I/O, thread-safe, and it
//! works under WASM where there is no filesystem.

use crate::config::TraceConfig;
use image::RgbaImage;
use vtracer::{ColorImage, ColorMode, Config, Hierarchical};
use visioncortex::PathSimplifyMode;

pub fn trace(img: &RgbaImage, cfg: &TraceConfig) -> Result<String, String> {
    let (w, h) = img.dimensions();
    let color_image = ColorImage {
        pixels: img.as_raw().clone(),
        width: w as usize,
        height: h as usize,
    };

    let config = Config {
        color_mode: match cfg.colormode.as_str() {
            "binary" => ColorMode::Binary,
            _ => ColorMode::Color,
        },
        hierarchical: match cfg.hierarchical.as_str() {
            "cutout" => Hierarchical::Cutout,
            _ => Hierarchical::Stacked,
        },
        mode: match cfg.mode.as_str() {
            "polygon" => PathSimplifyMode::Polygon,
            "none" => PathSimplifyMode::None,
            _ => PathSimplifyMode::Spline,
        },
        filter_speckle: cfg.filter_speckle,
        color_precision: cfg.color_precision,
        layer_difference: cfg.layer_difference,
        corner_threshold: cfg.corner_threshold,
        length_threshold: cfg.length_threshold,
        max_iterations: cfg.max_iterations,
        splice_threshold: cfg.splice_threshold,
        // path_precision controls coordinate decimals at the source, which
        // replaces the Python tool's fragile post-hoc regex rounding pass.
        path_precision: Some(cfg.path_precision),
    };

    let svg = vtracer::convert(color_image, config)?;
    Ok(svg.to_string())
}
