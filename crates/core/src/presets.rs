// Copyright (C) 2026 TheAnimatrix
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The preset pipelines ("variants"). Ports `legacy/src/pipeline.py`.

use crate::config::{Pipeline, PostprocessConfig, PreprocessConfig, TraceConfig};

/// All presets, in display order. Each is a closure so we always hand back a
/// fresh, owned `Pipeline`.
pub fn all() -> Vec<Pipeline> {
    vec![
        raw(),
        raw_sealed(),
        smooth(),
        smooth_sealed(),
        flat(),
        hybrid(),
        outlined(),
        lineart_bold(),
        mono_lineart(),
        clean_lineart(),
        clean_color(),
        max_fidelity(),
        hi_detail(),
        passthrough(),
    ]
}

/// Look up a preset by name (case-sensitive, matching the `name` field).
pub fn by_name(name: &str) -> Option<Pipeline> {
    all().into_iter().find(|p| p.name == name)
}

/// Names of all presets, in order.
pub fn names() -> Vec<String> {
    all().into_iter().map(|p| p.name).collect()
}

fn raw() -> Pipeline {
    Pipeline::named("raw", "Default vtracer, no extra passes. The baseline.")
}

fn raw_sealed() -> Pipeline {
    let mut p = Pipeline::named(
        "raw_sealed",
        "Raw vtracer + dark-only seal. No bilateral, so colours aren't shifted — purest match from tracing alone.",
    );
    p.post = PostprocessConfig {
        seal_gaps: true,
        seal_stroke_width: 0.8,
        seal_max_brightness: 80,
        ..Default::default()
    };
    p
}

fn smooth() -> Pipeline {
    let mut p = Pipeline::named(
        "smooth",
        "Bilateral pre-smoothing, slightly looser tracer — softens noisy edges.",
    );
    p.pre = PreprocessConfig { bilateral: true, ..Default::default() };
    p.tracer = TraceConfig { filter_speckle: 6, corner_threshold: 70, ..Default::default() };
    p
}

fn smooth_sealed() -> Pipeline {
    let mut p = Pipeline::named(
        "smooth_sealed",
        "Smooth + seal-stroke on dark paths only. Cleanest match without muddying saturated regions.",
    );
    p.pre = PreprocessConfig { bilateral: true, ..Default::default() };
    p.tracer = TraceConfig { filter_speckle: 6, corner_threshold: 70, ..Default::default() };
    p.post = PostprocessConfig {
        seal_gaps: true,
        seal_stroke_width: 0.8,
        seal_max_brightness: 80,
        ..Default::default()
    };
    p
}

fn flat() -> Pipeline {
    let mut p = Pipeline::named("flat", "Colour quantization for poster/logo-like flat art.");
    p.pre = PreprocessConfig { quantize: true, quantize_colors: 16, ..Default::default() };
    p.tracer = TraceConfig {
        color_precision: 8,
        layer_difference: 8,
        filter_speckle: 4,
        ..Default::default()
    };
    p
}

fn hybrid() -> Pipeline {
    let mut p = Pipeline::named(
        "hybrid",
        "Bilateral + light quantization + tighter tracer + path rounding. Fewer paths, smoother curves, smaller files.",
    );
    p.pre = PreprocessConfig {
        bilateral: true,
        bilateral_d: 9,
        bilateral_sigma_color: 60.0,
        bilateral_sigma_space: 60.0,
        quantize: true,
        quantize_colors: 32,
        ..Default::default()
    };
    p.tracer = TraceConfig {
        filter_speckle: 6,
        color_precision: 7,
        layer_difference: 12,
        corner_threshold: 65,
        length_threshold: 4.5,
        splice_threshold: 50,
        ..Default::default()
    };
    // round_numbers/decimals=1 in the Python version is expressed via path_precision.
    p.tracer.path_precision = 1;
    p
}

fn outlined() -> Pipeline {
    let mut p = Pipeline::named(
        "outlined",
        "For art with strong dark outlines (mascots, cartoons, line-art). Bridges hairline gaps without thickening.",
    );
    p.pre = PreprocessConfig { dark_threshold: 120, close_outline: 2, ..Default::default() };
    p.tracer = TraceConfig {
        filter_speckle: 2,
        color_precision: 6,
        layer_difference: 14,
        corner_threshold: 60,
        ..Default::default()
    };
    p.post = PostprocessConfig {
        seal_gaps: true,
        seal_stroke_width: 0.8,
        seal_max_brightness: 80,
        ..Default::default()
    };
    p
}

fn lineart_bold() -> Pipeline {
    let mut p = Pipeline::named(
        "lineart_bold",
        "Like `outlined` but explicitly thickens dark pixels. Bolder, more graphic look.",
    );
    p.pre = PreprocessConfig {
        dark_threshold: 120,
        close_outline: 2,
        dilate_dark: 1,
        ..Default::default()
    };
    p.tracer = TraceConfig {
        filter_speckle: 2,
        color_precision: 6,
        layer_difference: 14,
        corner_threshold: 60,
        ..Default::default()
    };
    p.post = PostprocessConfig {
        seal_gaps: true,
        seal_stroke_width: 1.0,
        seal_max_brightness: 80,
        ..Default::default()
    };
    p
}

fn mono_lineart() -> Pipeline {
    let mut p = Pipeline::named(
        "mono_lineart",
        "Strict 2-colour k-means: background + ink. Eliminates inner halos. For monochrome line icons.",
    );
    p.pre = PreprocessConfig {
        quantize: true,
        quantize_colors: 2,
        quantize_kmeans: true,
        quantize_lab: true,
        ..Default::default()
    };
    p.tracer = TraceConfig {
        filter_speckle: 4,
        corner_threshold: 80,
        length_threshold: 5.0,
        splice_threshold: 60,
        ..Default::default()
    };
    p
}

fn clean_lineart() -> Pipeline {
    let mut p = Pipeline::named(
        "clean_lineart",
        "For blurry low-quality line-art icons. Idealizes blur into clean colours via Lab k-means (bg + ink + shadow).",
    );
    p.pre = PreprocessConfig {
        quantize: true,
        quantize_colors: 3,
        quantize_kmeans: true,
        quantize_lab: true,
        ..Default::default()
    };
    p.tracer = TraceConfig {
        filter_speckle: 4,
        color_precision: 6,
        layer_difference: 16,
        corner_threshold: 80,
        length_threshold: 5.0,
        splice_threshold: 60,
        ..Default::default()
    };
    p
}

fn clean_color() -> Pipeline {
    let mut p = Pipeline::named(
        "clean_color",
        "k-means k=8 for multi-colour icons with small accent regions (logos, mascots with secondary colours).",
    );
    p.pre = PreprocessConfig {
        quantize: true,
        quantize_colors: 8,
        quantize_kmeans: true,
        quantize_lab: true,
        ..Default::default()
    };
    p.tracer = TraceConfig {
        filter_speckle: 4,
        color_precision: 6,
        layer_difference: 12,
        corner_threshold: 75,
        length_threshold: 4.5,
        ..Default::default()
    };
    p
}

fn max_fidelity() -> Pipeline {
    let mut p = Pipeline::named(
        "max_fidelity",
        "All knobs tuned for highest pixel match. Larger SVGs but closest to input.",
    );
    p.pre = PreprocessConfig {
        bilateral: true,
        bilateral_d: 5,
        bilateral_sigma_color: 30.0,
        bilateral_sigma_space: 30.0,
        ..Default::default()
    };
    p.tracer = TraceConfig {
        filter_speckle: 1,
        color_precision: 8,
        layer_difference: 4,
        corner_threshold: 50,
        length_threshold: 3.0,
        splice_threshold: 40,
        path_precision: 5,
        ..Default::default()
    };
    p.post = PostprocessConfig {
        seal_gaps: true,
        seal_stroke_width: 0.8,
        seal_max_brightness: 80,
        ..Default::default()
    };
    p
}

fn hi_detail() -> Pipeline {
    let mut p = Pipeline::named(
        "hi_detail",
        "Upscale x2 before tracing to capture fine detail; vtracer keeps precision.",
    );
    p.pre = PreprocessConfig { upscale: 2.0, ..Default::default() };
    p.tracer = TraceConfig {
        filter_speckle: 2,
        color_precision: 8,
        layer_difference: 8,
        corner_threshold: 50,
        path_precision: 4,
        ..Default::default()
    };
    p
}

fn passthrough() -> Pipeline {
    let mut p = Pipeline::named(
        "passthrough",
        "SVG wrapping a base64 PNG. 100% match but not real vector data — fallback only.",
    );
    p.passthrough = true;
    p
}
