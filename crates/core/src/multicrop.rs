// Copyright (C) 2026 Avarnic
// SPDX-License-Identifier: AGPL-3.0-or-later
// Commercial licensing: COMMERCIAL-LICENSE.md — creo@avarnic.com

//! Multicrop: cut several rectangular crops out of one image in a single pass.
//!
//! Each [`CropBox`] is given in pixel coordinates of the source image. Boxes are
//! clamped to the image bounds, so a box that runs past an edge is trimmed rather
//! than rejected; a box with no overlap (zero area after clamping) errors. Every
//! crop is PNG-encoded — lossless and format-agnostic regardless of the source.

use crate::decode;
use image::{DynamicImage, RgbaImage};
use serde::Deserialize;
use std::io::Cursor;

/// A rectangle to crop, in source-image pixels. `label`, when present, names the
/// output file (`<label>.png`); otherwise crops are named `crop-1`, `crop-2`, ….
#[derive(Debug, Clone, Deserialize)]
pub struct CropBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub label: Option<String>,
}

/// Crop one box out of an already-decoded image and return PNG bytes.
pub fn crop_one(img: &RgbaImage, b: &CropBox) -> Result<Vec<u8>, String> {
    let (iw, ih) = img.dimensions();
    // Clamp the rectangle to the image; trim instead of failing on overhang.
    let x = b.x.min(iw);
    let y = b.y.min(ih);
    let w = b.width.min(iw.saturating_sub(x));
    let h = b.height.min(ih.saturating_sub(y));
    if w == 0 || h == 0 {
        return Err("box has zero area within the image".to_string());
    }
    let crop = image::imageops::crop_imm(img, x, y, w, h).to_image();
    let mut buf = Vec::new();
    DynamicImage::ImageRgba8(crop)
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .map_err(|e| format!("PNG encode failed: {e}"))?;
    Ok(buf)
}

/// Crop every box out of one decoded image. Returns `(name, Result<png_bytes>)`
/// in input order; names come from `label` (sanitized) or fall back to `crop-N`.
pub fn crop_all(img: &RgbaImage, boxes: &[CropBox]) -> Vec<(String, Result<Vec<u8>, String>)> {
    boxes
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let name = match &b.label {
                Some(l) if !sanitize(l).is_empty() => sanitize(l),
                _ => format!("crop-{}", i + 1),
            };
            (name, crop_one(img, b))
        })
        .collect()
}

/// Decode raw image bytes, then crop every box. Convenience for callers that hold
/// undecoded bytes (e.g. the HTTP server).
pub fn crop_bytes(data: &[u8], boxes: &[CropBox]) -> Result<Vec<(String, Result<Vec<u8>, String>)>, String> {
    let img = decode(data)?;
    Ok(crop_all(&img, boxes))
}

/// Reduce a user label to a safe, single-segment filename stem (no path
/// separators, control chars, or surrounding whitespace).
fn sanitize(label: &str) -> String {
    label
        .trim()
        .chars()
        .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 0, 255]))
    }

    #[test]
    fn crops_to_expected_dimensions() {
        let src = img(10, 10);
        let png = crop_one(&src, &CropBox { x: 2, y: 3, width: 4, height: 5, label: None }).unwrap();
        let out = image::load_from_memory(&png).unwrap();
        assert_eq!(out.width(), 4);
        assert_eq!(out.height(), 5);
    }

    #[test]
    fn overhanging_box_is_trimmed() {
        let src = img(10, 10);
        let png = crop_one(&src, &CropBox { x: 8, y: 8, width: 100, height: 100, label: None }).unwrap();
        let out = image::load_from_memory(&png).unwrap();
        assert_eq!((out.width(), out.height()), (2, 2));
    }

    #[test]
    fn zero_area_box_errors() {
        let src = img(10, 10);
        assert!(crop_one(&src, &CropBox { x: 20, y: 0, width: 5, height: 5, label: None }).is_err());
    }

    #[test]
    fn names_default_and_sanitize() {
        let src = img(10, 10);
        let boxes = vec![
            CropBox { x: 0, y: 0, width: 2, height: 2, label: None },
            CropBox { x: 0, y: 0, width: 2, height: 2, label: Some("a/b".into()) },
        ];
        let out = crop_all(&src, &boxes);
        assert_eq!(out[0].0, "crop-1");
        assert_eq!(out[1].0, "a_b");
        assert!(out.iter().all(|(_, r)| r.is_ok()));
    }
}
