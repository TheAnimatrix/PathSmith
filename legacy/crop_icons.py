"""Crop the 10 icons from the source mockup into icons/, centered with padding."""

from pathlib import Path

import numpy as np
from PIL import Image

SRC = Path(__file__).parent / "ChatGPT Image May 20, 2026, 11_26_57 AM.png"
OUT = Path(__file__).parent / "icons"
OUT.mkdir(exist_ok=True)

BG = np.array([252, 250, 248])
THRESHOLD = 25   # how far from BG a pixel must be to count as content
PAD_FRAC = 0.12  # padding around content as fraction of bbox max-side
OUT_SIZE = 512   # final square output


def tight_bbox(arr: np.ndarray, region: tuple[int, int, int, int]) -> tuple[int, int, int, int]:
    x1, y1, x2, y2 = region
    sub = arr[y1:y2, x1:x2].astype(int)
    diff = np.abs(sub - BG).sum(axis=2)
    mask = diff > THRESHOLD
    if not mask.any():
        raise RuntimeError(f"no content in region {region}")
    ys, xs = np.where(mask)
    return (x1 + xs.min(), y1 + ys.min(), x1 + xs.max() + 1, y1 + ys.max() + 1)


def crop_centered(
    im: Image.Image,
    bbox: tuple[int, int, int, int],
    safe: tuple[int, int, int, int],
) -> Image.Image:
    x1, y1, x2, y2 = bbox
    w, h = x2 - x1, y2 - y1
    side = max(w, h)
    pad = int(side * PAD_FRAC)
    side += pad * 2
    cx, cy = (x1 + x2) // 2, (y1 + y2) // 2
    half = side // 2

    canvas = Image.new("RGB", (side, side), tuple(BG.tolist()))
    # clamp source rectangle to BOTH image bounds and safe region (avoid grabbing
    # neighbouring icons / labels / dividers when the square overshoots the bbox).
    sx1 = max(cx - half, safe[0])
    sy1 = max(cy - half, safe[1])
    sx2 = min(cx + half, safe[2])
    sy2 = min(cy + half, safe[3])
    piece = im.crop((sx1, sy1, sx2, sy2))
    paste_x = half - (cx - sx1)
    paste_y = half - (cy - sy1)
    canvas.paste(piece, (paste_x, paste_y))
    return canvas.resize((OUT_SIZE, OUT_SIZE), Image.LANCZOS)


# (name, search-region (x1, y1, x2, y2)) — region must contain ONE icon and no text/divider
# Bounds derived by scanning the source (divider at x=740 and y=883).
REGIONS: list[tuple[str, tuple[int, int, int, int]]] = [
    # feature icons, top-right column (icon band x=800..920, between divider and text)
    ("01_nest_topics",    ( 800,  150,  920,  280)),
    ("02_create_qnas",    ( 800,  325,  920,  455)),
    ("03_ai_powered",     ( 800,  505,  920,  635)),
    ("04_learn_together", ( 800,  690,  920,  800)),
    # bottom-row mascots: y band 920..1145 (between horizontal divider and labels)
    ("05_focus",          (  40,  920,  265, 1145)),
    ("06_explore",        ( 290,  920,  480, 1145)),
    ("07_recall",         ( 505,  920,  710, 1145)),
    ("08_share",          ( 720,  920,  955, 1145)),
    ("09_app_icon",       ( 985,  920, 1215, 1145)),
    # hero mascot, center-left (below logo+tagline, above horizontal divider, left of vertical divider)
    ("10_hero",           ( 130,  300,  640,  830)),
]


def main() -> None:
    im = Image.open(SRC).convert("RGB")
    arr = np.array(im)
    for name, region in REGIONS:
        bbox = tight_bbox(arr, region)
        out = crop_centered(im, bbox, region)
        path = OUT / f"{name}.png"
        out.save(path, "PNG")
        print(f"{name}: bbox={bbox} -> {path}")


if __name__ == "__main__":
    main()
