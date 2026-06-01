"""Preset pipelines composing preprocess → trace → postprocess."""
from __future__ import annotations

import base64
from dataclasses import dataclass, field
from io import BytesIO

from PIL import Image

from .preprocess import PreprocessConfig, preprocess
from .trace import TraceConfig, trace
from .postprocess import PostprocessConfig, postprocess


@dataclass
class Pipeline:
    name: str
    pre: PreprocessConfig = field(default_factory=PreprocessConfig)
    tracer: TraceConfig = field(default_factory=TraceConfig)
    post: PostprocessConfig = field(default_factory=PostprocessConfig)
    passthrough: bool = False  # if True, embed PNG as base64 instead of tracing

    def run(self, img: Image.Image) -> str:
        if self.passthrough:
            return _passthrough_svg(img)
        stage1 = preprocess(img, self.pre)
        svg = trace(stage1, self.tracer)
        return postprocess(svg, self.post)


def _passthrough_svg(img: Image.Image) -> str:
    """Embed the raster as base64 inside an SVG. Round-trip is bit-identical
    (modulo PNG re-encoding) but this isn't a real vector — useful as a
    100%-match baseline / fallback for inputs that trace poorly."""
    buf = BytesIO()
    img.convert("RGBA").save(buf, "PNG", optimize=True)
    b64 = base64.b64encode(buf.getvalue()).decode("ascii")
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" '
        f'width="{img.width}" height="{img.height}" '
        f'viewBox="0 0 {img.width} {img.height}">'
        f'<image width="{img.width}" height="{img.height}" '
        f'href="data:image/png;base64,{b64}"/>'
        f'</svg>'
    )


def raw() -> Pipeline:
    """Default vtracer, no extra passes. The baseline."""
    return Pipeline(name="raw")


def smooth() -> Pipeline:
    """Bilateral pre-smoothing, slightly looser tracer — softens noisy edges."""
    return Pipeline(
        name="smooth",
        pre=PreprocessConfig(bilateral=True),
        tracer=TraceConfig(filter_speckle=6, corner_threshold=70),
    )


def raw_sealed() -> Pipeline:
    """Raw vtracer + dark-only seal. No bilateral, so colors are not shifted —
    purest match% from tracing alone."""
    return Pipeline(
        name="raw_sealed",
        post=PostprocessConfig(
            seal_gaps=True, seal_stroke_width=0.8,
            seal_max_brightness=80,
        ),
    )


def smooth_sealed() -> Pipeline:
    """Smooth + seal-stroke on dark paths only.

    Bilateral pre-smoothing produces the cleanest match%; adding a same-color
    stroke just to dark paths closes the residual halo gaps around outlines
    without muddying saturated regions (the way unrestricted seal_gaps does)."""
    return Pipeline(
        name="smooth_sealed",
        pre=PreprocessConfig(bilateral=True),
        tracer=TraceConfig(filter_speckle=6, corner_threshold=70),
        post=PostprocessConfig(
            seal_gaps=True, seal_stroke_width=0.8,
            seal_max_brightness=80,
        ),
    )


def flat() -> Pipeline:
    """Color quantization for poster/logo-like flat art."""
    return Pipeline(
        name="flat",
        pre=PreprocessConfig(quantize=True, quantize_colors=16),
        tracer=TraceConfig(color_precision=8, layer_difference=8, filter_speckle=4),
    )


def hybrid() -> Pipeline:
    """Bilateral + light quantization + tighter tracer + path rounding.
    Aims for fewer paths, smoother curves, smaller files."""
    return Pipeline(
        name="hybrid",
        pre=PreprocessConfig(
            bilateral=True, bilateral_d=9,
            bilateral_sigma_color=60, bilateral_sigma_space=60,
            quantize=True, quantize_colors=32,
        ),
        tracer=TraceConfig(
            filter_speckle=6, color_precision=7, layer_difference=12,
            corner_threshold=65, length_threshold=4.5, splice_threshold=50,
        ),
        post=PostprocessConfig(round_numbers=True, decimals=1),
    )


def outlined() -> Pipeline:
    """For art with strong dark outlines (mascots, cartoons, line-art).

    Bridges hairline gaps inside anti-aliased outlines so vtracer sees one
    connected dark layer (no dashed/fragmented lines), without thickening
    the outline overall. Inter-path halo gaps are sealed by the stroke trick."""
    return Pipeline(
        name="outlined",
        pre=PreprocessConfig(
            dark_threshold=120,
            close_outline=2,
        ),
        tracer=TraceConfig(
            filter_speckle=2,
            color_precision=6,
            layer_difference=14,
            corner_threshold=60,
        ),
        post=PostprocessConfig(
            seal_gaps=True, seal_stroke_width=0.8,
            seal_max_brightness=80,
        ),
    )


def lineart_bold() -> Pipeline:
    """Like `outlined` but explicitly thickens dark pixels.
    Use only when you want a bolder, more graphic look."""
    return Pipeline(
        name="lineart_bold",
        pre=PreprocessConfig(
            dark_threshold=120,
            close_outline=2,
            dilate_dark=1,
        ),
        tracer=TraceConfig(
            filter_speckle=2,
            color_precision=6,
            layer_difference=14,
            corner_threshold=60,
        ),
        post=PostprocessConfig(
            seal_gaps=True, seal_stroke_width=1.0,
            seal_max_brightness=80,
        ),
    )


def max_fidelity() -> Pipeline:
    """All knobs tuned for highest possible pixel match.

    Light bilateral (denoise but preserve color); maximum tracer precision
    (small speckle, high color/path precision, low layer_difference);
    dark-only seal to kill halos without muddying. No quantize, no upscale.
    Produces larger SVGs but closest to input."""
    return Pipeline(
        name="max_fidelity",
        pre=PreprocessConfig(
            bilateral=True,
            bilateral_d=5,
            bilateral_sigma_color=30, bilateral_sigma_space=30,
        ),
        tracer=TraceConfig(
            filter_speckle=1,
            color_precision=8,
            layer_difference=4,
            corner_threshold=50,
            length_threshold=3.0,
            splice_threshold=40,
            path_precision=5,
        ),
        post=PostprocessConfig(
            seal_gaps=True, seal_stroke_width=0.8,
            seal_max_brightness=80,
        ),
    )


def mono_lineart() -> Pipeline:
    """Strict 2-color k-means: background + ink. Eliminates inner halos by
    forcing every AA pixel into one of two pure colors. Use for monochrome
    line icons. For colored multi-stroke icons, use clean_color."""
    return Pipeline(
        name="mono_lineart",
        pre=PreprocessConfig(
            quantize=True,
            quantize_colors=2,
            quantize_kmeans=True,
            quantize_lab=True,
        ),
        tracer=TraceConfig(
            filter_speckle=4,
            corner_threshold=80,
            length_threshold=5.0,
            splice_threshold=60,
        ),
    )


def clean_lineart() -> Pipeline:
    """For blurry low-quality line-art icons. Idealizes blur into clean uniform
    colors via k-means clustering in Lab space (3 colors: bg + ink + one shadow
    band). Tracer is set to smooth — high corner_threshold for round curves,
    moderate length_threshold to drop small noise blobs."""
    return Pipeline(
        name="clean_lineart",
        pre=PreprocessConfig(
            quantize=True,
            quantize_colors=3,
            quantize_kmeans=True,
            quantize_lab=True,
        ),
        tracer=TraceConfig(
            filter_speckle=4,
            color_precision=6,
            layer_difference=16,
            corner_threshold=80,
            length_threshold=5.0,
            splice_threshold=60,
        ),
    )


def clean_color() -> Pipeline:
    """K-means with k=8 for multi-color icons with small accent regions
    (logos, mascots with secondary colors). Higher k prevents small color
    regions from being absorbed into dominant clusters."""
    return Pipeline(
        name="clean_color",
        pre=PreprocessConfig(
            quantize=True,
            quantize_colors=8,
            quantize_kmeans=True,
            quantize_lab=True,
        ),
        tracer=TraceConfig(
            filter_speckle=4,
            color_precision=6,
            layer_difference=12,
            corner_threshold=75,
            length_threshold=4.5,
        ),
    )


def passthrough() -> Pipeline:
    """SVG wrapping a base64 PNG. Guarantees 100% match, but the SVG is just
    a raster container — no real vector data. Use as fallback when tracing
    quality is unacceptable for a given input."""
    return Pipeline(name="passthrough", passthrough=True)


def hi_detail() -> Pipeline:
    """Upscale x2 before tracing to capture fine detail; vtracer keeps precision."""
    return Pipeline(
        name="hi_detail",
        pre=PreprocessConfig(upscale=2.0),
        tracer=TraceConfig(
            filter_speckle=2, color_precision=8, layer_difference=8,
            corner_threshold=50, path_precision=4,
        ),
    )


ALL_PIPELINES = [
    raw, raw_sealed, smooth, smooth_sealed, flat, hybrid,
    outlined, lineart_bold,
    mono_lineart, clean_lineart, clean_color,
    max_fidelity, hi_detail, passthrough,
]
