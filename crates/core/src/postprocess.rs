// Copyright (C) 2026 TheAnimatrix
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Post-processing passes on the traced SVG.
//!
//! Ports `legacy/src/postprocess.py`'s `seal_gaps` and `strip_tiny_paths`, but
//! parses the SVG with quick-xml instead of brittle attribute-order regexes.
//! (The `round_numbers` pass is dropped — vtracer's `path_precision` already
//! controls coordinate decimals at the source.)

use crate::config::PostprocessConfig;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};

pub fn postprocess(svg: &str, cfg: &PostprocessConfig) -> String {
    if !cfg.seal_gaps && !cfg.strip_tiny_paths {
        return svg.to_string();
    }

    let mut reader = Reader::from_str(svg);
    let mut writer = Writer::new(Vec::new());

    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::Empty(e)) if e.name().as_ref() == b"path" => {
                match transform_path(&e, cfg) {
                    Some(elem) => {
                        let _ = writer.write_event(Event::Empty(elem));
                    }
                    None => {} // dropped by strip_tiny_paths
                }
            }
            Ok(Event::Start(e)) if e.name().as_ref() == b"path" => {
                if let Some(elem) = transform_path(&e, cfg) {
                    let _ = writer.write_event(Event::Start(elem));
                }
            }
            Ok(ev) => {
                let _ = writer.write_event(ev);
            }
            Err(_) => return svg.to_string(),
        }
    }

    String::from_utf8(writer.into_inner()).unwrap_or_else(|_| svg.to_string())
}

/// Returns the (possibly stroke-augmented) element, or `None` to drop it.
fn transform_path(e: &BytesStart, cfg: &PostprocessConfig) -> Option<BytesStart<'static>> {
    let mut d_val: Option<String> = None;
    let mut fill_val: Option<String> = None;
    let mut has_stroke = false;

    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        let val = String::from_utf8_lossy(&attr.value).into_owned();
        match key {
            b"d" => d_val = Some(val),
            b"fill" => fill_val = Some(val),
            b"stroke" => has_stroke = true,
            _ => {}
        }
    }

    // strip_tiny_paths: drop paths with a short `d` attribute.
    if cfg.strip_tiny_paths {
        let len = d_val.as_ref().map(|d| d.len()).unwrap_or(0);
        if len <= cfg.tiny_path_max_len {
            return None;
        }
    }

    // Copy all original attributes into an owned element.
    let mut elem = BytesStart::new("path");
    for attr in e.attributes().flatten() {
        elem.push_attribute(attr);
    }

    if cfg.seal_gaps && !has_stroke {
        if let Some(fill) = fill_val.as_deref() {
            if should_seal(fill, cfg.seal_max_brightness) {
                elem.push_attribute(("stroke", fill));
                let sw = format!("{}", cfg.seal_stroke_width);
                elem.push_attribute(("stroke-width", sw.as_str()));
                elem.push_attribute(("stroke-linejoin", "round"));
            }
        }
    }

    Some(elem.into_owned())
}

fn should_seal(fill: &str, bright_cap: u8) -> bool {
    let f = fill.trim().to_ascii_lowercase();
    if f == "none" || f == "transparent" {
        return false;
    }
    if bright_cap >= 255 {
        return true;
    }
    max_channel(&f).map(|m| m <= bright_cap).unwrap_or(true)
}

/// Max RGB channel of a `#rgb` / `#rrggbb` colour, or None if unparseable.
fn max_channel(hex: &str) -> Option<u8> {
    let s = hex.strip_prefix('#')?;
    let full = match s.len() {
        3 => s.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => s.to_string(),
        _ => return None,
    };
    let r = u8::from_str_radix(&full[0..2], 16).ok()?;
    let g = u8::from_str_radix(&full[2..4], 16).ok()?;
    let b = u8::from_str_radix(&full[4..6], 16).ok()?;
    Some(r.max(g).max(b))
}
