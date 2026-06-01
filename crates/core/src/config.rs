// Copyright (C) 2026 TheAnimatrix
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Configuration types for the conversion pipeline. Mirrors the dataclasses in
//! the original Python tool so behaviour stays auditable against the reference.

use serde::{Deserialize, Serialize};

/// Pre-processing passes applied to a raster image before tracing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PreprocessConfig {
    pub bilateral: bool,
    pub bilateral_d: u32,
    pub bilateral_sigma_color: f64,
    pub bilateral_sigma_space: f64,

    pub quantize: bool,
    pub quantize_colors: usize,
    /// k-means clustering (perceptually better) instead of plain median-cut.
    pub quantize_kmeans: bool,
    /// cluster in CIELAB space when true, else sRGB.
    pub quantize_lab: bool,

    /// >1 traces a larger image then the SVG scales back down.
    pub upscale: f64,

    /// Thicken dark pixels by this radius (outline fix). 0 = off.
    pub dilate_dark: u32,
    /// Max-channel value (0-255) below which a pixel counts as "dark".
    pub dark_threshold: u8,
    /// Morphological close radius on the dark mask (bridges hairline gaps). 0 = off.
    pub close_outline: u32,
}

impl Default for PreprocessConfig {
    fn default() -> Self {
        Self {
            bilateral: false,
            bilateral_d: 7,
            bilateral_sigma_color: 50.0,
            bilateral_sigma_space: 50.0,
            quantize: false,
            quantize_colors: 16,
            quantize_kmeans: false,
            quantize_lab: true,
            upscale: 1.0,
            dilate_dark: 0,
            dark_threshold: 60,
            close_outline: 0,
        }
    }
}

/// vtracer tracing parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TraceConfig {
    pub colormode: String,    // "color" | "binary"
    pub hierarchical: String, // "stacked" | "cutout"
    pub mode: String,         // "spline" | "polygon" | "none"
    pub filter_speckle: usize,
    pub color_precision: i32,
    pub layer_difference: i32,
    pub corner_threshold: i32,
    pub length_threshold: f64,
    pub max_iterations: usize,
    pub splice_threshold: i32,
    pub path_precision: u32,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            colormode: "color".into(),
            hierarchical: "stacked".into(),
            mode: "spline".into(),
            filter_speckle: 4,
            color_precision: 6,
            layer_difference: 16,
            corner_threshold: 60,
            length_threshold: 4.0,
            max_iterations: 10,
            splice_threshold: 45,
            path_precision: 3,
        }
    }
}

/// Post-processing passes applied to the SVG string after tracing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PostprocessConfig {
    pub strip_tiny_paths: bool,
    /// drop `<path>` whose `d` attribute is shorter than this many chars.
    pub tiny_path_max_len: usize,

    /// add `stroke="<fill>"` so each path self-expands and closes hairline gaps.
    pub seal_gaps: bool,
    pub seal_stroke_width: f64,
    /// only seal paths whose fill max-channel is <= this (0-255). 255 = seal all.
    pub seal_max_brightness: u8,
}

impl Default for PostprocessConfig {
    fn default() -> Self {
        Self {
            strip_tiny_paths: false,
            tiny_path_max_len: 40,
            seal_gaps: false,
            seal_stroke_width: 0.6,
            seal_max_brightness: 255,
        }
    }
}

/// A named pipeline: preprocess -> trace -> postprocess (or raster passthrough).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub pre: PreprocessConfig,
    #[serde(default)]
    pub tracer: TraceConfig,
    #[serde(default)]
    pub post: PostprocessConfig,
    /// embed PNG as base64 instead of tracing (100% match, not real vector).
    #[serde(default)]
    pub passthrough: bool,
}

impl Pipeline {
    /// Build a pipeline with the given name/description and default stages.
    pub fn named(name: &str, description: &str) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            pre: PreprocessConfig::default(),
            tracer: TraceConfig::default(),
            post: PostprocessConfig::default(),
            passthrough: false,
        }
    }
}
