// Copyright (C) 2026 TheAnimatrix
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Benchmark harness: PNG -> SVG -> PNG, scored against the original.
//!
//! Ports `legacy/test.py`. For every input image and every preset it traces to
//! SVG, re-rasterizes the SVG with resvg at the original size, and reports pixel
//! match%, MAE and a structural-similarity score. Runs image x preset pairs in
//! parallel with rayon.

use anyhow::{Context, Result};
use image::{RgbImage, RgbaImage};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct Row {
    pub image: String,
    pub pipeline: String,
    pub result: std::result::Result<Score, String>,
    pub secs: f64,
}

pub struct Score {
    pub match_pct: f64,
    pub mae: f64,
    pub ssim: f64,
    pub svg_bytes: usize,
}

pub fn run(input_dir: &Path, out_dir: &Path, tolerance: u8) -> Result<()> {
    let mut pngs: Vec<PathBuf> = std::fs::read_dir(input_dir)
        .with_context(|| format!("reading {}", input_dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("png")).unwrap_or(false))
        .collect();
    pngs.sort();
    anyhow::ensure!(!pngs.is_empty(), "no PNGs in {}", input_dir.display());

    let presets = pathsmith_core::presets::all();

    // Build the full work matrix, then process in parallel.
    let jobs: Vec<(PathBuf, pathsmith_core::config::Pipeline)> = pngs
        .iter()
        .flat_map(|p| presets.iter().map(move |pipe| (p.clone(), pipe.clone())))
        .collect();

    let rows: Vec<Row> = jobs
        .par_iter()
        .map(|(png_path, pipe)| run_one(png_path, pipe, out_dir, tolerance))
        .collect();

    write_report(&rows, &out_dir.join("report.md"))?;
    print_summary(&rows);
    Ok(())
}

fn run_one(png_path: &Path, pipe: &pathsmith_core::config::Pipeline, out_dir: &Path, tol: u8) -> Row {
    let stem = png_path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let name = pipe.name.clone();
    let t0 = Instant::now();

    let result = (|| -> std::result::Result<Score, String> {
        let bytes = std::fs::read(png_path).map_err(|e| e.to_string())?;
        let reference = pathsmith_core::decode(&bytes)?;
        let (w, h) = reference.dimensions();

        let svg = pathsmith_core::run_pipeline(&reference, pipe)?;
        let rendered = render_svg(&svg, w, h)?;

        // persist artefacts
        let svg_dir = out_dir.join("svg").join(&name);
        let png_dir = out_dir.join("png").join(&name);
        let _ = std::fs::create_dir_all(&svg_dir);
        let _ = std::fs::create_dir_all(&png_dir);
        let _ = std::fs::write(svg_dir.join(format!("{stem}.svg")), &svg);
        let _ = rendered.save(png_dir.join(format!("{stem}.png")));

        let (match_pct, mae, ssim) = compare(&reference, &rendered, tol)?;
        Ok(Score { match_pct, mae, ssim, svg_bytes: svg.len() })
    })();

    if let Err(ref e) = result {
        eprintln!("   FAILED {stem} :: {name}: {e}");
    } else {
        println!("-> {stem} :: {name}");
    }

    Row { image: stem, pipeline: name, result, secs: t0.elapsed().as_secs_f64() }
}

/// Render an SVG string to an RGBA image at the given size with resvg.
fn render_svg(svg: &str, width: u32, height: u32) -> std::result::Result<RgbaImage, String> {
    use resvg::{tiny_skia, usvg};
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| format!("usvg: {e}"))?;
    let size = tree.size();
    let sx = width as f32 / size.width();
    let sy = height as f32 / size.height();
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).ok_or("zero-sized pixmap".to_string())?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(sx, sy), &mut pixmap.as_mut());

    // tiny_skia stores premultiplied RGBA.
    RgbaImage::from_raw(width, height, pixmap.data().to_vec())
        .ok_or("pixmap -> image".to_string())
}

/// Composite straight-alpha RGBA onto white -> RGB.
fn composite_white(img: &RgbaImage) -> RgbImage {
    let (w, h) = img.dimensions();
    let mut out = RgbImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels() {
        let a = p.0[3] as f32 / 255.0;
        let blend = |c: u8| ((c as f32) * a + 255.0 * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
        out.put_pixel(x, y, image::Rgb([blend(p.0[0]), blend(p.0[1]), blend(p.0[2])]));
    }
    out
}

/// Composite premultiplied RGBA (resvg output) onto white -> RGB.
fn composite_white_premul(img: &RgbaImage) -> RgbImage {
    let (w, h) = img.dimensions();
    let mut out = RgbImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels() {
        let inv = 255u16 - p.0[3] as u16; // white * (1 - alpha)
        let blend = |c: u8| ((c as u16 + inv).min(255)) as u8;
        out.put_pixel(x, y, image::Rgb([blend(p.0[0]), blend(p.0[1]), blend(p.0[2])]));
    }
    out
}

fn compare(
    reference: &RgbaImage,
    candidate_premul: &RgbaImage,
    tol: u8,
) -> std::result::Result<(f64, f64, f64), String> {
    let a = composite_white(reference);
    let b = composite_white_premul(candidate_premul);

    let (mut within, mut total_abs, n) = (0u64, 0u64, (a.width() * a.height()) as u64);
    for (pa, pb) in a.pixels().zip(b.pixels()) {
        let dr = (pa.0[0] as i16 - pb.0[0] as i16).unsigned_abs();
        let dg = (pa.0[1] as i16 - pb.0[1] as i16).unsigned_abs();
        let db = (pa.0[2] as i16 - pb.0[2] as i16).unsigned_abs();
        let maxd = dr.max(dg).max(db);
        if maxd as u8 <= tol {
            within += 1;
        }
        total_abs += dr as u64 + dg as u64 + db as u64;
    }
    let match_pct = within as f64 / n as f64 * 100.0;
    let mae = total_abs as f64 / (n as f64 * 3.0);

    // structural similarity (hybrid colour metric, 0..1) — not identical to
    // skimage SSIM but directionally comparable for preset tuning.
    let ssim = image_compare::rgb_hybrid_compare(&a, &b)
        .map_err(|e| format!("ssim: {e}"))?
        .score;

    Ok((match_pct, mae, ssim))
}

fn write_report(rows: &[Row], path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = String::new();
    out.push_str("# PNG -> SVG -> PNG comparison report\n\n");
    out.push_str("| image | pipeline | match% (<=tol) | MAE | SSIM | SVG bytes | secs |\n");
    out.push_str("|---|---|---:|---:|---:|---:|---:|\n");
    for r in rows {
        match &r.result {
            Ok(s) => out.push_str(&format!(
                "| {} | {} | {:.2} | {:.2} | {:.4} | {} | {:.2} |\n",
                r.image, r.pipeline, s.match_pct, s.mae, s.ssim, s.svg_bytes, r.secs
            )),
            Err(e) => out.push_str(&format!(
                "| {} | {} | ERROR | — | — | — | — | <!-- {} -->\n",
                r.image, r.pipeline, e
            )),
        }
    }
    std::fs::write(path, out)?;
    println!("\nreport: {}", path.display());
    Ok(())
}

fn print_summary(rows: &[Row]) {
    use std::collections::BTreeMap;
    println!("\n=== summary (best preset per image, by SSIM) ===");
    let mut by_image: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    for r in rows {
        by_image.entry(&r.image).or_default().push(r);
    }
    for (image, group) in by_image {
        let best = group
            .iter()
            .filter_map(|r| r.result.as_ref().ok().map(|s| (r, s)))
            .max_by(|a, b| a.1.ssim.partial_cmp(&b.1.ssim).unwrap());
        match best {
            Some((r, s)) => println!(
                "{image}: best={} match={:.1}% SSIM={:.3}",
                r.pipeline, s.match_pct, s.ssim
            ),
            None => println!("{image}: all pipelines failed"),
        }
    }
}
