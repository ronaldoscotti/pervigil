"""Frame a raw panel capture as a floating macOS window: rounded corners + shadow.
Usage: python3 screenshot-frame.py <in.png> <out.png>"""

import sys

from PIL import Image, ImageDraw, ImageFilter

src = Image.open(sys.argv[1]).convert("RGBA")
w, h = src.size
radius = round(w * 0.045)  # scales with the (retina) capture width

corners = Image.new("L", (w, h), 0)
ImageDraw.Draw(corners).rounded_rectangle([0, 0, w - 1, h - 1], radius=radius, fill=255)
src.putalpha(corners)

pad = round(w * 0.14)
canvas = Image.new("RGBA", (w + pad * 2, h + pad * 2), (0, 0, 0, 0))

shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
drop = round(pad * 0.28)
ImageDraw.Draw(shadow).rounded_rectangle(
    [pad, pad + drop, pad + w, pad + h + drop], radius=radius, fill=(0, 0, 0, 115)
)
shadow = shadow.filter(ImageFilter.GaussianBlur(round(pad * 0.42)))

canvas = Image.alpha_composite(canvas, shadow)
canvas.paste(src, (pad, pad), src)
canvas.save(sys.argv[2])
print(f"wrote {sys.argv[2]} ({canvas.size[0]}x{canvas.size[1]})")
