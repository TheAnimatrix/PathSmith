"""Deterministic test harness.

For every PNG in input/png and every pipeline preset:
  1. PNG → SVG via pipeline
  2. SVG → PNG via resvg (at the original size)
  3. Compare to original PNG (pixel match %, MAE, SSIM)

Writes:
  output/svg/<pipeline>/<name>.svg
  output/png/<pipeline>/<name>.png   (re-rastered SVG, for visual diff)
  output/report.md
"""
from __future__ import annotations

import time
from pathlib import Path

from PIL import Image

from src import pipeline as pipelines
from src.diff import compare, render_svg

ROOT = Path(__file__).parent
INPUT_DIR = ROOT / "input" / "png"
OUT_SVG = ROOT / "output" / "svg"
OUT_PNG = ROOT / "output" / "png"
REPORT = ROOT / "output" / "report.md"


def main() -> None:
    pngs = sorted(INPUT_DIR.glob("*.png"))
    if not pngs:
        raise SystemExit(f"no PNGs in {INPUT_DIR}")

    presets = [fn() for fn in pipelines.ALL_PIPELINES]
    rows: list[dict] = []

    for png_path in pngs:
        ref = Image.open(png_path).convert("RGBA")
        for pipe in presets:
            tag = f"{png_path.stem} :: {pipe.name}"
            print(f"-> {tag}", flush=True)
            t0 = time.perf_counter()
            try:
                svg = pipe.run(ref)
                rendered = render_svg(svg, ref.width, ref.height)
                res = compare(ref, rendered)
                elapsed = time.perf_counter() - t0

                svg_dir = OUT_SVG / pipe.name
                png_dir = OUT_PNG / pipe.name
                svg_dir.mkdir(parents=True, exist_ok=True)
                png_dir.mkdir(parents=True, exist_ok=True)
                (svg_dir / f"{png_path.stem}.svg").write_text(svg, encoding="utf-8")
                rendered.save(png_dir / f"{png_path.stem}.png", "PNG")

                rows.append({
                    "image": png_path.stem,
                    "pipeline": pipe.name,
                    "match_pct": res.pixel_match_pct,
                    "mae": res.mean_abs_err,
                    "ssim": res.ssim,
                    "svg_bytes": len(svg),
                    "secs": elapsed,
                    "ok": True,
                })
            except Exception as e:
                rows.append({
                    "image": png_path.stem,
                    "pipeline": pipe.name,
                    "error": repr(e),
                    "ok": False,
                })
                print(f"   FAILED: {e!r}")

    _write_report(rows)
    _print_summary(rows)


def _write_report(rows: list[dict]) -> None:
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "# PNG → SVG → PNG comparison report",
        "",
        "| image | pipeline | match% (≤5/255) | MAE | SSIM | SVG bytes | secs |",
        "|---|---|---:|---:|---:|---:|---:|",
    ]
    for r in rows:
        if not r["ok"]:
            lines.append(f"| {r['image']} | {r['pipeline']} | ERROR | — | — | — | — |  <!-- {r['error']} -->")
            continue
        lines.append(
            f"| {r['image']} | {r['pipeline']} "
            f"| {r['match_pct']:.2f} "
            f"| {r['mae']:.2f} "
            f"| {r['ssim']:.4f} "
            f"| {r['svg_bytes']:,} "
            f"| {r['secs']:.2f} |"
        )
    REPORT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"\nreport: {REPORT}")


def _print_summary(rows: list[dict]) -> None:
    print("\n=== summary ===")
    by_image: dict[str, list[dict]] = {}
    for r in rows:
        by_image.setdefault(r["image"], []).append(r)
    for image, group in by_image.items():
        ok = [r for r in group if r["ok"]]
        if not ok:
            print(f"{image}: all pipelines failed")
            continue
        best = max(ok, key=lambda r: r["ssim"])
        print(
            f"{image}: best={best['pipeline']} "
            f"match={best['match_pct']:.1f}% SSIM={best['ssim']:.3f}"
        )


if __name__ == "__main__":
    main()
