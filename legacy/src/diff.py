"""Render SVG → PNG and compare to a reference PNG."""
from __future__ import annotations

from dataclasses import dataclass
from io import BytesIO

import numpy as np
import resvg_py
from PIL import Image
from skimage.metrics import structural_similarity as ssim


@dataclass
class DiffResult:
    pixel_match_pct: float       # % of pixels within tolerance
    mean_abs_err: float          # 0–255, lower better
    ssim: float                  # -1..1, higher better
    width: int
    height: int


def render_svg(svg: str, width: int, height: int) -> Image.Image:
    """Render an SVG string to an RGBA PIL image at the given size."""
    png_bytes = resvg_py.svg_to_bytes(
        svg_string=svg,
        width=width,
        height=height,
    )
    # resvg_py may return list of ints in some versions; normalize
    if isinstance(png_bytes, list):
        png_bytes = bytes(png_bytes)
    return Image.open(BytesIO(png_bytes)).convert("RGBA")


def _composite_on_white(img: Image.Image) -> np.ndarray:
    """Flatten RGBA onto white for fair comparison."""
    bg = Image.new("RGBA", img.size, (255, 255, 255, 255))
    return np.array(Image.alpha_composite(bg, img).convert("RGB"))


def compare(ref: Image.Image, candidate: Image.Image, tolerance: int = 5) -> DiffResult:
    if candidate.size != ref.size:
        candidate = candidate.resize(ref.size, Image.LANCZOS)

    a = _composite_on_white(ref.convert("RGBA"))
    b = _composite_on_white(candidate.convert("RGBA"))

    diff = np.abs(a.astype(np.int16) - b.astype(np.int16))
    per_pixel_max = diff.max(axis=2)
    pct = float((per_pixel_max <= tolerance).mean() * 100.0)
    mae = float(diff.mean())

    # SSIM expects grayscale or per-channel; use channel_axis for color SSIM
    s = float(ssim(a, b, channel_axis=2, data_range=255))

    return DiffResult(
        pixel_match_pct=pct,
        mean_abs_err=mae,
        ssim=s,
        width=ref.width,
        height=ref.height,
    )
