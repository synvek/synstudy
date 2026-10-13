#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Generate app icons from assets/logo.svg into assets/icons/.

The logo SVG is a base64-embedded PNG wrapper (a vector-less <image> tag), so
we extract the original 1024x1024 raster and downscale the sizes the app
needs. dx 0.8.0-alpha.1 consumes [bundle] icon (the 1024 png) only on
desktop platforms (macOS .icns / Windows .ico / Linux); its iOS and Android
paths ignore the field entirely (verified: no xcassets/actool code in the CLI
source), so scripts/inject_ios_icon.py and package_android.sh patch the
built .app / Gradle res folder from the sets generated here. The 256 png is
embedded into the desktop binary at compile time (include_bytes!) for the
runtime window/taskbar icon.

Usage:
  python3 scripts/gen_icons.py            # regenerate from assets/logo.svg
  python3 scripts/gen_icons.py --check    # verify outputs exist & match source

Requires Pillow: pip install pillow
"""
import argparse
import base64
import io
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LOGO_SVG = os.path.join(ROOT, "assets", "logo.svg")
OUT_DIR = os.path.join(ROOT, "assets", "icons")

# name -> edge size (px); 1024 is the [bundle] icon source for dx.
SIZES = {
    "app-icon.png": 1024,       # dx [bundle] icon: icns/ico generator (desktop)
    "app-icon-256.png": 256,    # runtime window icon (include_bytes!)
    "app-icon-32.png": 32,      # misc / linux .desktop preview
}

# iOS legacy icon set (CFBundleIconFiles in .app root). iOS rejects alpha in
# app icons, so these are flattened onto white. Base names listed in the
# plist are Icon-60 / Icon-Small / Icon-Spotlight-40 / Icon-76; the system
# resolves the @2x/@3x variants by filename.
IOS_SIZES = {
    "Icon-60.png": 60, "Icon-60@2x.png": 120, "Icon-60@3x.png": 180,
    "Icon-Small.png": 29, "Icon-Small@2x.png": 58, "Icon-Small@3x.png": 87,
    "Icon-Spotlight-40.png": 40, "Icon-Spotlight-40@2x.png": 80,
    "Icon-Spotlight-40@3x.png": 120,
    "Icon-76.png": 76, "Icon-76@2x.png": 152,
}

# Android launcher mipmaps (mdpi..xxxhdpi). package_android.sh copies these
# over the Gradle project's default webp icons.
ANDROID_SIZES = {
    "mipmap-mdpi/ic_launcher.png": 48,
    "mipmap-hdpi/ic_launcher.png": 72,
    "mipmap-xhdpi/ic_launcher.png": 96,
    "mipmap-xxhdpi/ic_launcher.png": 144,
    "mipmap-xxxhdpi/ic_launcher.png": 192,
}


# iOS brand background gradient (near-black graphite): the mark is purple, so
# any purple-ish backdrop blends into it — go dark for contrast.
IOS_BG_TOP = (13, 15, 26)     # #0d0f1a
IOS_BG_BOTTOM = (43, 48, 74)  # #2b304a
IOS_MARK_RATIO = 0.62
# The logo's white swoosh reads as two stray white patches once the canvas is
# no longer white; refill near-white mark pixels with the brand purple gradient
# (by row) so the ribbon stays solid but keeps its 3D shading elsewhere.
IOS_FILL_TOP = (129, 140, 248)   # #818cf8
IOS_FILL_BOTTOM = (109, 40, 217)  # #6d28d9
IOS_WHITE_MIN = 215  # per-channel: >= this counts as "near white"


def build_ios_master(src, Image):
    """Return a 1024px RGB icon: logo mark centered on a brand gradient."""
    if src.mode != "RGBA":
        src = src.convert("RGBA")
    bbox = src.getchannel("A").getbbox()  # the mark's opaque extent
    if not bbox:
        print("WARN: iOS master fallback, logo mark not detected")
        bbox = (0, 0, src.size[0], src.size[1])
    mark = src.crop(bbox)

    # Refill the white swoosh with purple (see IOS_FILL_* note above).
    px = mark.load()
    h = mark.size[1]
    for y in range(h):
        t = y / max(1, h - 1)
        fill = tuple(int(a + (b - a) * t)
                     for a, b in zip(IOS_FILL_TOP, IOS_FILL_BOTTOM))
        for x in range(mark.size[0]):
            r, g, b, a = px[x, y]
            if a > 0 and min(r, g, b) >= IOS_WHITE_MIN:
                px[x, y] = fill + (a,)

    edge = src.size[0]  # square canvas
    target = int(edge * IOS_MARK_RATIO)
    scale = target / max(mark.size)
    mark = mark.resize((max(1, int(mark.size[0] * scale)),
                        max(1, int(mark.size[1] * scale))), Image.LANCZOS)

    # Vertical gradient canvas, then center the mark (slight optical drop).
    strip = Image.new("RGB", (1, edge))
    for y in range(edge):
        t = y / (edge - 1)
        strip.putpixel((0, y), tuple(
            int(a + (b - a) * t) for a, b in zip(IOS_BG_TOP, IOS_BG_BOTTOM)))
    canvas = strip.resize((edge, edge))
    ox = (edge - mark.size[0]) // 2
    oy = (edge - mark.size[1]) // 2 + int(edge * 0.02)
    canvas.paste(mark.convert("RGB"), (ox, oy), mark.getchannel("A"))
    return canvas


def extract_png(svg_path):
    """Pull the first base64 data-URI PNG out of the SVG; return raw bytes."""
    with open(svg_path, "r", encoding="utf-8") as f:
        text = f.read()
    m = re.search(r"data:image/png;base64,([A-Za-z0-9+/=]+)", text)
    if not m:
        sys.exit("ERROR: no base64 PNG <image> found in {}".format(svg_path))
    return base64.b64decode(m.group(1))


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--check", action="store_true",
                    help="only verify generated icons exist and are current")
    args = ap.parse_args()

    if not os.path.exists(LOGO_SVG):
        sys.exit("ERROR: {} not found".format(LOGO_SVG))

    try:
        from PIL import Image
    except ImportError:
        sys.exit("ERROR: Pillow is required (pip install pillow)")

    src = Image.open(io.BytesIO(extract_png(LOGO_SVG))).convert("RGBA")
    if src.size != (1024, 1024):
        print("WARN: logo is {}x{}, expected 1024x1024".format(*src.size))

    os.makedirs(OUT_DIR, exist_ok=True)
    ios_master = build_ios_master(src, Image)
    jobs = [(OUT_DIR, name, edge, False, src) for name, edge in SIZES.items()]
    jobs += [(os.path.join(OUT_DIR, "ios"), n, e, True, ios_master)
             for n, e in IOS_SIZES.items()]
    jobs += [(os.path.join(OUT_DIR, "android"), n, e, False, src)
             for n, e in ANDROID_SIZES.items()]
    # Persist the iOS master for eyeballing / debugging the icon pipeline.
    master_path = os.path.join(OUT_DIR, "app-icon-ios.png")
    if args.check:
        ok = open(master_path, "rb").read() == _encode(ios_master)
        print("app-icon-ios.png: {}".format("up-to-date" if ok else "STALE"))
    else:
        with open(master_path, "wb") as f:
            f.write(_encode(ios_master))
        print("wrote {} (master, {}x{})".format(master_path, *ios_master.size))
    for out_dir, name, edge, flatten, base in jobs:
        out_path = os.path.join(out_dir, name)
        img = base if edge == base.size else base.resize((edge, edge), Image.LANCZOS)
        if flatten and img.mode == "RGBA":  # legacy safety: iOS icons opaque
            flat = Image.new("RGB", img.size, (255, 255, 255))
            flat.paste(img, (0, 0), img)
            img = flat
        os.makedirs(os.path.dirname(out_path), exist_ok=True)
        if args.check:
            if not os.path.exists(out_path):
                sys.exit("MISSING: {} (run gen_icons.py)".format(out_path))
            cur = Image.open(out_path).size
            ok = cur == img.size and open(out_path, "rb").read() == _encode(img)
            print("{}: {}x{} {}".format(name, *cur, "up-to-date" if ok else "STALE"))
        else:
            with open(out_path, "wb") as f:
                f.write(_encode(img))
            print("wrote {} ({}x{})".format(out_path, edge, edge))

    if not args.check:
        print("Next: desktop bundling uses [bundle] icon; iOS injection runs "
              "scripts/inject_ios_icon.py, Android mipmaps are copied by "
              "package_android.sh (dx alpha.1 ignores bundle.icon on mobile).")


def _encode(img):
    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=True)
    return buf.getvalue()


if __name__ == "__main__":
    main()
