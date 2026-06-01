"""Single-file CLI: convert one PNG to SVG with a chosen pipeline."""
from __future__ import annotations

import argparse
from pathlib import Path

from PIL import Image

from . import pipeline as pipelines


def main() -> None:
    ap = argparse.ArgumentParser(description="PNG → SVG converter")
    ap.add_argument("input", type=Path)
    ap.add_argument("output", type=Path)
    ap.add_argument(
        "--pipeline", "-p", default="hybrid",
        choices=[fn().name for fn in pipelines.ALL_PIPELINES],
    )
    args = ap.parse_args()

    pipe = {fn().name: fn for fn in pipelines.ALL_PIPELINES}[args.pipeline]()
    img = Image.open(args.input).convert("RGBA")
    svg = pipe.run(img)
    args.output.write_text(svg, encoding="utf-8")
    print(f"wrote {args.output} ({len(svg):,} bytes) via pipeline={args.pipeline}")


if __name__ == "__main__":
    main()
