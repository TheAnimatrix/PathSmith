"""Post-processing passes applied to the SVG string after tracing."""
from __future__ import annotations

import re
from dataclasses import dataclass


@dataclass
class PostprocessConfig:
    round_numbers: bool = False
    decimals: int = 1
    strip_tiny_paths: bool = False
    tiny_path_max_len: int = 40  # 'd' attribute length threshold

    # Add stroke="<fill>" stroke-width=N to every path so each path self-expands
    # and closes hairline gaps between adjacent color regions (the vtracer halo fix).
    seal_gaps: bool = False
    seal_stroke_width: float = 0.6
    # Only seal paths whose fill is darker than this max-channel value (0-255).
    # 255 = seal everything (muddies colors). ~80 = outlines only.
    seal_max_brightness: int = 255


_NUM_RE = re.compile(r"-?\d+\.\d+")


def _round_match(decimals: int):
    def repl(m: re.Match) -> str:
        val = round(float(m.group(0)), decimals)
        if decimals <= 0:
            return str(int(val))
        # strip trailing zeros and trailing dot
        s = f"{val:.{decimals}f}".rstrip("0").rstrip(".")
        return s if s else "0"
    return repl


def postprocess(svg: str, cfg: PostprocessConfig) -> str:
    if cfg.round_numbers:
        svg = _NUM_RE.sub(_round_match(cfg.decimals), svg)

    if cfg.seal_gaps:
        sw = cfg.seal_stroke_width
        bright_cap = cfg.seal_max_brightness

        def _max_channel(hex_color: str) -> int:
            s = hex_color.lstrip("#")
            if len(s) == 3:
                s = "".join(c * 2 for c in s)
            if len(s) != 6:
                return 255
            try:
                return max(int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16))
            except ValueError:
                return 255

        def add_stroke(m: re.Match) -> str:
            tag = m.group(0)
            if "stroke=" in tag:
                return tag
            fill_m = re.search(r'fill="([^"]+)"', tag)
            if not fill_m:
                return tag
            fill_val = fill_m.group(1)
            if fill_val.lower() in ("none", "transparent"):
                return tag
            if bright_cap < 255 and _max_channel(fill_val) > bright_cap:
                return tag
            inject = f' stroke="{fill_val}" stroke-width="{sw}" stroke-linejoin="round"'
            if tag.endswith("/>"):
                return tag[:-2] + inject + "/>"
            return tag[:-1] + inject + ">"

        svg = re.sub(r"<path\b[^>]*>", add_stroke, svg)

    if cfg.strip_tiny_paths:
        def drop_tiny(m: re.Match) -> str:
            d = m.group(1)
            if len(d) <= cfg.tiny_path_max_len:
                return ""
            return m.group(0)
        svg = re.sub(r'<path[^/]*d="([^"]+)"[^/]*/>', drop_tiny, svg)

    return svg
