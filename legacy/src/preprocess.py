"""Pre-processing passes applied to a raster image before tracing."""
from __future__ import annotations

from dataclasses import dataclass

import cv2
import numpy as np
from PIL import Image


@dataclass
class PreprocessConfig:
    bilateral: bool = False
    bilateral_d: int = 7
    bilateral_sigma_color: float = 50.0
    bilateral_sigma_space: float = 50.0

    quantize: bool = False
    quantize_colors: int = 16
    # If True, use k-means (perceptually better for blurry line icons) instead
    # of PIL MEDIANCUT. Clusters in Lab space when lab=True.
    quantize_kmeans: bool = False
    quantize_lab: bool = True

    upscale: float = 1.0  # >1 traces a larger image then SVG scales back down

    # Thicken dark pixels (outline-fix). Pixels darker than dark_threshold (0-255 max channel)
    # get morphologically dilated by `dilate_dark` pixels before tracing, so outlines overlap
    # adjacent color fills and vtracer's halo gap disappears.
    dilate_dark: int = 0
    dark_threshold: int = 60
    # Morphological close radius applied to the dark mask before dilation.
    # Bridges hairline gaps inside anti-aliased outlines so vtracer sees one
    # connected outline instead of dashes/dots.
    close_outline: int = 0


def _pil_to_cv(img: Image.Image) -> np.ndarray:
    arr = np.array(img.convert("RGBA"))
    return cv2.cvtColor(arr, cv2.COLOR_RGBA2BGRA)


def _cv_to_pil(arr: np.ndarray) -> Image.Image:
    rgba = cv2.cvtColor(arr, cv2.COLOR_BGRA2RGBA)
    return Image.fromarray(rgba)


def _kmeans_quantize(rgb: Image.Image, k: int, use_lab: bool) -> Image.Image:
    """K-means cluster to k colors. Clusters in Lab if use_lab (perceptually
    closer to how humans group color); otherwise RGB."""
    arr = np.array(rgb)  # H, W, 3 in RGB
    h, w = arr.shape[:2]
    if use_lab:
        lab = cv2.cvtColor(arr, cv2.COLOR_RGB2LAB)
        samples = lab.reshape(-1, 3).astype(np.float32)
    else:
        samples = arr.reshape(-1, 3).astype(np.float32)

    criteria = (cv2.TERM_CRITERIA_EPS + cv2.TERM_CRITERIA_MAX_ITER, 20, 1.0)
    _, labels, centers = cv2.kmeans(
        samples, k, None, criteria, 5, cv2.KMEANS_PP_CENTERS,
    )
    centers = centers.astype(np.uint8)
    quantized = centers[labels.flatten()].reshape(h, w, 3)
    if use_lab:
        quantized = cv2.cvtColor(quantized, cv2.COLOR_LAB2RGB)
    return Image.fromarray(quantized)


def preprocess(img: Image.Image, cfg: PreprocessConfig) -> Image.Image:
    if cfg.upscale != 1.0:
        w, h = img.size
        img = img.resize(
            (int(w * cfg.upscale), int(h * cfg.upscale)),
            Image.LANCZOS,
        )

    if cfg.bilateral:
        cv_img = _pil_to_cv(img)
        # bilateral on BGR only, preserve alpha
        bgr = cv_img[:, :, :3]
        alpha = cv_img[:, :, 3:]
        bgr = cv2.bilateralFilter(
            bgr, cfg.bilateral_d,
            cfg.bilateral_sigma_color, cfg.bilateral_sigma_space,
        )
        img = _cv_to_pil(np.concatenate([bgr, alpha], axis=2))

    if cfg.dilate_dark > 0 or cfg.close_outline > 0:
        cv_img = _pil_to_cv(img)
        bgr = cv_img[:, :, :3]
        alpha = cv_img[:, :, 3:]
        max_c = bgr.max(axis=2)
        dark_mask = (max_c <= cfg.dark_threshold).astype(np.uint8) * 255

        if cfg.close_outline > 0:
            k = cfg.close_outline * 2 + 1
            kernel = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (k, k))
            dark_mask = cv2.morphologyEx(dark_mask, cv2.MORPH_CLOSE, kernel)

        if cfg.dilate_dark > 0:
            k = cfg.dilate_dark * 2 + 1
            kernel = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (k, k))
            dark_mask = cv2.dilate(dark_mask, kernel, iterations=1)

        # Paint mask region pure black so vtracer sees one solid outline,
        # not a gradient of AA pixels split across multiple thin layers.
        bgr[dark_mask > 0] = (0, 0, 0)
        img = _cv_to_pil(np.concatenate([bgr, alpha], axis=2))

    if cfg.quantize:
        rgba = img.convert("RGBA")
        a = rgba.split()[3]

        if cfg.quantize_kmeans:
            rgb = _kmeans_quantize(
                rgba.convert("RGB"), cfg.quantize_colors, cfg.quantize_lab,
            )
        else:
            rgb = rgba.convert("RGB").quantize(
                colors=cfg.quantize_colors,
                method=Image.MEDIANCUT, dither=Image.NONE,
            ).convert("RGB")

        img = Image.merge("RGBA", (*rgb.split(), a))

    return img
