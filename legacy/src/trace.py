"""vtracer wrapper. Traces a PIL image to an SVG string."""
from __future__ import annotations

import io
import tempfile
from dataclasses import dataclass
from pathlib import Path

import vtracer
from PIL import Image


@dataclass
class TraceConfig:
    colormode: str = "color"        # "color" | "binary"
    hierarchical: str = "stacked"   # "stacked" | "cutout"
    mode: str = "spline"            # "spline" | "polygon" | "none"
    filter_speckle: int = 4
    color_precision: int = 6
    layer_difference: int = 16
    corner_threshold: int = 60
    length_threshold: float = 4.0
    max_iterations: int = 10
    splice_threshold: int = 45
    path_precision: int = 3


def trace(img: Image.Image, cfg: TraceConfig) -> str:
    """Trace a PIL image to an SVG string."""
    # vtracer's Python API takes file paths; round-trip via tmp files.
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        in_path = tmp / "in.png"
        out_path = tmp / "out.svg"
        img.convert("RGBA").save(in_path, "PNG")
        vtracer.convert_image_to_svg_py(
            str(in_path),
            str(out_path),
            colormode=cfg.colormode,
            hierarchical=cfg.hierarchical,
            mode=cfg.mode,
            filter_speckle=cfg.filter_speckle,
            color_precision=cfg.color_precision,
            layer_difference=cfg.layer_difference,
            corner_threshold=cfg.corner_threshold,
            length_threshold=cfg.length_threshold,
            max_iterations=cfg.max_iterations,
            splice_threshold=cfg.splice_threshold,
            path_precision=cfg.path_precision,
        )
        return out_path.read_text(encoding="utf-8")
