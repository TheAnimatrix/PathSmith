// Copyright (C) 2026 Avarnic
// SPDX-License-Identifier: AGPL-3.0-or-later
// Commercial licensing: COMMERCIAL-LICENSE.md — creo@avarnic.com

//! Pre-processing passes applied to a raster image before tracing.
//!
//! Ports `legacy/src/preprocess.py`. Quantization uses k-means (kmeans_colors)
//! in CIELAB or sRGB; bilateral is a small joint-colour implementation matching
//! OpenCV's parameters; dark-mask close/dilate uses imageproc morphology.

use crate::config::PreprocessConfig;
use image::{imageops::FilterType, GrayImage, RgbaImage};
use imageproc::distance_transform::Norm;
use kmeans_colors::get_kmeans_hamerly;
use palette::{cast::from_component_slice, FromColor, IntoColor, Lab, Srgb};

/// Fixed seed so quantization-based presets are reproducible (the Python/OpenCV
/// version was unseeded and could wobble run-to-run).
const KMEANS_SEED: u64 = 0x5005_2017;

pub fn preprocess(img: &RgbaImage, cfg: &PreprocessConfig) -> RgbaImage {
    let mut img = img.clone();

    if (cfg.upscale - 1.0).abs() > f64::EPSILON {
        let (w, h) = img.dimensions();
        let nw = ((w as f64) * cfg.upscale).round().max(1.0) as u32;
        let nh = ((h as f64) * cfg.upscale).round().max(1.0) as u32;
        img = image::imageops::resize(&img, nw, nh, FilterType::Lanczos3);
    }

    if cfg.bilateral {
        img = bilateral_rgba(
            &img,
            cfg.bilateral_d.max(1),
            cfg.bilateral_sigma_color.max(1.0),
            cfg.bilateral_sigma_space.max(1.0),
        );
    }

    if cfg.dilate_dark > 0 || cfg.close_outline > 0 {
        img = apply_dark_mask(&img, cfg);
    }

    if cfg.quantize {
        img = quantize(&img, cfg.quantize_colors.max(1), cfg.quantize_lab);
    }

    img
}

/// Joint-colour bilateral filter on the RGB channels; alpha is preserved.
/// `d` is the neighbourhood diameter (matching OpenCV's `d`).
fn bilateral_rgba(img: &RgbaImage, d: u32, sigma_color: f64, sigma_space: f64) -> RgbaImage {
    let (w, h) = img.dimensions();
    let radius = (d / 2).max(1) as i32;
    let gc = -1.0 / (2.0 * sigma_color * sigma_color);
    let gs = -1.0 / (2.0 * sigma_space * sigma_space);

    // Precompute spatial weights for the window.
    let win = (2 * radius + 1) as usize;
    let mut space_w = vec![0.0f64; win * win];
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let idx = ((dy + radius) as usize) * win + (dx + radius) as usize;
            let dist2 = (dx * dx + dy * dy) as f64;
            space_w[idx] = (gs * dist2).exp();
        }
    }

    let mut out = img.clone();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let center = img.get_pixel(x as u32, y as u32).0;
            let (cr, cg, cb) = (center[0] as f64, center[1] as f64, center[2] as f64);
            let (mut sr, mut sg, mut sb, mut wsum) = (0.0, 0.0, 0.0, 0.0);
            for dy in -radius..=radius {
                let ny = y + dy;
                if ny < 0 || ny >= h as i32 {
                    continue;
                }
                for dx in -radius..=radius {
                    let nx = x + dx;
                    if nx < 0 || nx >= w as i32 {
                        continue;
                    }
                    let p = img.get_pixel(nx as u32, ny as u32).0;
                    let (pr, pg, pb) = (p[0] as f64, p[1] as f64, p[2] as f64);
                    let cdist2 =
                        (pr - cr) * (pr - cr) + (pg - cg) * (pg - cg) + (pb - cb) * (pb - cb);
                    let sidx = ((dy + radius) as usize) * win + (dx + radius) as usize;
                    let wgt = space_w[sidx] * (gc * cdist2).exp();
                    sr += pr * wgt;
                    sg += pg * wgt;
                    sb += pb * wgt;
                    wsum += wgt;
                }
            }
            let px = out.get_pixel_mut(x as u32, y as u32);
            px.0[0] = (sr / wsum).round().clamp(0.0, 255.0) as u8;
            px.0[1] = (sg / wsum).round().clamp(0.0, 255.0) as u8;
            px.0[2] = (sb / wsum).round().clamp(0.0, 255.0) as u8;
            // alpha (px.0[3]) untouched
        }
    }
    out
}

/// Paint the dark mask (optionally closed/dilated) pure black so vtracer sees
/// one solid outline instead of a gradient of anti-aliased pixels.
fn apply_dark_mask(img: &RgbaImage, cfg: &PreprocessConfig) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut mask = GrayImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels() {
        let max_c = p.0[0].max(p.0[1]).max(p.0[2]);
        mask.put_pixel(x, y, image::Luma([if max_c <= cfg.dark_threshold { 255 } else { 0 }]));
    }

    if cfg.close_outline > 0 {
        mask = imageproc::morphology::close(&mask, Norm::LInf, cfg.close_outline as u8);
    }
    if cfg.dilate_dark > 0 {
        mask = imageproc::morphology::dilate(&mask, Norm::LInf, cfg.dilate_dark as u8);
    }

    let mut out = img.clone();
    for (x, y, m) in mask.enumerate_pixels() {
        if m.0[0] > 0 {
            let px = out.get_pixel_mut(x, y);
            px.0[0] = 0;
            px.0[1] = 0;
            px.0[2] = 0;
        }
    }
    out
}

/// k-means colour quantization to `k` colours, clustering in CIELAB or sRGB.
/// Alpha is preserved per-pixel; clustering is on RGB only.
fn quantize(img: &RgbaImage, k: usize, lab: bool) -> RgbaImage {
    let (w, h) = img.dimensions();
    let n = (w * h) as usize;

    // RGB bytes only (drop alpha for clustering).
    let mut rgb = Vec::with_capacity(n * 3);
    for p in img.pixels() {
        rgb.push(p.0[0]);
        rgb.push(p.0[1]);
        rgb.push(p.0[2]);
    }
    let srgb: &[Srgb<u8>] = from_component_slice(&rgb);

    let mut out = img.clone();

    if lab {
        let samples: Vec<Lab> = srgb
            .iter()
            .map(|c| c.into_format::<f32>().into_color())
            .collect();
        let result = get_kmeans_hamerly(k, 20, 1.0, false, &samples, KMEANS_SEED);
        let palette: Vec<Srgb<u8>> = result
            .centroids
            .iter()
            .map(|lab| Srgb::from_color(*lab).into_format())
            .collect();
        write_quantized(&mut out, &result.indices, &palette);
    } else {
        let samples: Vec<Srgb<f32>> = srgb.iter().map(|c| c.into_format()).collect();
        let result = get_kmeans_hamerly(k, 20, 1.0, false, &samples, KMEANS_SEED);
        let palette: Vec<Srgb<u8>> =
            result.centroids.iter().map(|c| c.into_format()).collect();
        write_quantized(&mut out, &result.indices, &palette);
    }

    out
}

fn write_quantized(out: &mut RgbaImage, indices: &[u8], palette: &[Srgb<u8>]) {
    for (px, &idx) in out.pixels_mut().zip(indices.iter()) {
        let c = palette[idx as usize];
        px.0[0] = c.red;
        px.0[1] = c.green;
        px.0[2] = c.blue;
        // alpha preserved
    }
}
