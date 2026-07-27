#!/usr/bin/env python3
"""Generate the tray icon set from one source mask.

The source's *alpha channel* is the glyph; its colours are ignored. Outputs
bare, 1..9 and overflow, each in a black-ink `-light` and a white-ink `-dark`
variant. Developer-only — the results are committed.

    python3 scripts/gen-tray-icons.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "assets" / "tray-owl.png"
OUT = ROOT / "src-tauri" / "icons" / "tray"

# Tall enough that macOS's rescale to 18pt has room on a 2x display, small enough
# that eleven states in two variants stay a rounding error in the binary.
HEIGHT = 128

# Below this the model's soft glow is not ink. Thresholding here is what turns a
# generated image into a mask with hard edges; the anti-aliasing that matters is
# added later by the downscale.
ALPHA_FLOOR = 128

BADGE_RATIO = 0.62
GAP_RATIO = 0.10

FONTS = [
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
]

INKS = {"light": (0, 0, 0), "dark": (255, 255, 255)}


def glyph_mask() -> Image.Image:
    """The source's alpha, hardened and trimmed, at the target height."""
    alpha = Image.open(SOURCE).convert("RGBA").getchannel("A")
    hard = alpha.point(lambda v: 255 if v >= ALPHA_FLOOR else 0)
    hard = hard.crop(hard.getbbox())
    width = round(hard.width * HEIGHT / hard.height)
    return hard.resize((width, HEIGHT), Image.LANCZOS)


def load_font(size: int) -> ImageFont.FreeTypeFont:
    for path in FONTS:
        if Path(path).exists():
            return ImageFont.truetype(path, size)
    raise SystemExit(f"no usable font found; tried:\n  " + "\n  ".join(FONTS))


def badge_mask(label: str) -> Image.Image:
    """A filled pill with the label knocked out of it.

    Knocked out, not drawn on top: a template image is an alpha mask, so a digit
    painted in another colour would vanish the moment macOS tints the glyph.
    """
    diameter = round(HEIGHT * BADGE_RATIO)
    font = load_font(round(diameter * 0.72))

    probe = ImageDraw.Draw(Image.new("L", (1, 1)))
    left, top, right, bottom = probe.textbbox((0, 0), label, font=font)
    text_width, text_height = right - left, bottom - top

    width = max(diameter, text_width + diameter // 2)
    mask = Image.new("L", (width, diameter), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0, width - 1, diameter - 1), radius=diameter // 2, fill=255)
    draw.text(
        ((width - text_width) / 2 - left, (diameter - text_height) / 2 - top),
        label,
        font=font,
        fill=0,
    )
    return mask


def compose(glyph: Image.Image, label: str | None) -> Image.Image:
    """Glyph alone, or glyph and badge side by side on a wider canvas.

    Side by side rather than overlaid: at 18 points a corner badge is about five
    pixels across, which is not a digit, it is a smudge.
    """
    if label is None:
        return glyph

    badge = badge_mask(label)
    gap = round(HEIGHT * GAP_RATIO)
    canvas = Image.new("L", (glyph.width + gap + badge.width, HEIGHT), 0)
    canvas.paste(glyph, (0, 0))
    canvas.paste(badge, (glyph.width + gap, (HEIGHT - badge.height) // 2))
    return canvas


def ink(mask: Image.Image, colour: tuple[int, int, int]) -> Image.Image:
    out = Image.new("RGBA", mask.size, colour + (0,))
    out.putalpha(mask)
    return out


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    glyph = glyph_mask()

    states = [("bare", None), *((str(n), str(n)) for n in range(1, 10)), ("overflow", "9+")]

    for name, label in states:
        mask = compose(glyph, label)
        for variant, colour in INKS.items():
            path = OUT / f"{name}-{variant}.png"
            ink(mask, colour).save(path)
    print(f"wrote {len(states) * len(INKS)} icons to {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
