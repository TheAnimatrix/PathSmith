// Copyright (C) 2026 Avarnic
// SPDX-License-Identifier: AGPL-3.0-or-later
// Commercial licensing: COMMERCIAL-LICENSE.md — creo@avarnic.com

//! PathSmith command-line interface.

mod bench;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "pathsmith", version, about = "PathSmith — raster -> SVG converter")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Convert one image to SVG with a chosen preset.
    Convert {
        input: PathBuf,
        output: PathBuf,
        #[arg(short, long, default_value = "hybrid")]
        pipeline: String,
    },
    /// List available presets ("variants") with descriptions.
    Presets,
    /// Trace every input image with every preset and write a quality report.
    Bench {
        #[arg(long, default_value = "input/png")]
        input: PathBuf,
        #[arg(long, default_value = "output")]
        out: PathBuf,
        /// per-channel tolerance for the "match%" metric.
        #[arg(long, default_value_t = 5)]
        tolerance: u8,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Convert { input, output, pipeline } => {
            let bytes = std::fs::read(&input)
                .with_context(|| format!("reading {}", input.display()))?;
            let svg = pathsmith_core::convert_bytes(&bytes, &pipeline)
                .map_err(|e| anyhow::anyhow!(e))?;
            std::fs::write(&output, &svg)
                .with_context(|| format!("writing {}", output.display()))?;
            println!("wrote {} ({} bytes) via pipeline={pipeline}", output.display(), svg.len());
        }
        Command::Presets => {
            for p in pathsmith_core::presets::all() {
                println!("{:<14} {}", p.name, p.description);
            }
        }
        Command::Bench { input, out, tolerance } => {
            bench::run(&input, &out, tolerance)?;
        }
    }
    Ok(())
}
