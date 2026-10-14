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
runtime window/taskbar icon, and splash-logo.png (mark cropped to its alpha
bbox, transparent) is embedded by src/components/splash.rs for the mobile
in-app startup splash.

The DESKTOP icons (app-icon.png / -256 / -32) are rendered from the same
branded master as iOS -- the mark on the dark graphite gradient with the white
swoosh knocked out -- so the running app's Dock icon and the Finder icon match
the mobile brand instead of showing the bare transparent mark. On macOS we also
emit app-icon.icns (via iconutil): dx writes CFBundleIconFile=icon.icns into
the .app but never produces the file, so scripts/inject_macos_icon.py copies
this icns into Contents/Resources after `dx bundle`.

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
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LOGO_SVG = os.path.join(ROOT, "assets", "logo.svg")
OUT_DIR = os.path.join(ROOT, "assets", "icons")

# name -> edge size (px); 1024 is the [bundle] icon source for dx. All three
# are rendered from the branded master (see build_ios_master), not the bare
# transparent mark, so desktop matches the iOS/splash brand.
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

# Android launcher icons (mdpi..xxxhdpi), copied over the Gradle project's
# defaults by package_android.sh. We ship TWO sets because launchers read
# different resources per API level:
#  * legacy `ic_launcher.png` (API <26; our min_sdk=24 fallback) -- sourced from
#    `ios_master`, i.e. a full-bleed OPAQUE plate. Using the raw transparent
#    logo here is what made the icon look nothing like iOS: a launcher puts a
#    WHITE PLATE behind a transparent legacy icon and shrinks a padded bitmap
#    further, so you got "白底 + 图案偏小" at once.
#  * adaptive layers (API 26+): `ic_launcher_background.png` +
#    `ic_launcher_foreground.png` on a 108dp canvas, referenced by the
#    `mipmap-anydpi-v26/ic_launcher.xml` that package_android.sh writes. This
#    is what the launcher masks (circle/squircle/pill), so the mark has to sit
#    inside the visible 72dp circle. The dx-generated project ships only the
#    AS template's vector drawables and NO such xml, which is why the legacy
#    path was always taken before.
ANDROID_SIZES = {
    "mipmap-mdpi/ic_launcher.png": 48,
    "mipmap-hdpi/ic_launcher.png": 72,
    "mipmap-xhdpi/ic_launcher.png": 96,
    "mipmap-xxhdpi/ic_launcher.png": 144,
    "mipmap-xxxhdpi/ic_launcher.png": 192,
}

# Adaptive canvas is 108dp; per density that is this many px of edge.
ANDROID_DENSITIES = {"mdpi": 108, "hdpi": 162, "xhdpi": 216, "xxhdpi": 324, "xxxhdpi": 432}


# iOS brand background gradient (near-black graphite): the mark is purple, so
# any purple-ish backdrop blends into it — go dark for contrast.
IOS_BG_TOP = (13, 15, 26)     # #0d0f1a
IOS_BG_BOTTOM = (43, 48, 74)  # #2b304a
IOS_MARK_RATIO = 0.62
# Mark edge as a fraction of the ADAPTIVE canvas, for visual parity with iOS:
# there the mark spans IOS_MARK_RATIO of the visible plate, and the adaptive
# visible circle is 72dp of the 108dp canvas.
ANDROID_FG_MARK_RATIO = IOS_MARK_RATIO * (72.0 / 108.0)  # ~0.413
# The logo's white swoosh reads as two stray white patches once placed on a
# coloured background. Knock it out to TRANSPARENCY (not recolouring): alpha
# fades to 0 as a pixel's minimum channel rises toward white. The purple ribbon
# always has a low channel (min <= ~130) and the white swoosh a high one
# (min >= ~240), so feathering between these bounds removes the swoosh AND the
# soft-grey shadow / anti-aliased halo a single hard threshold left behind.
WHITE_OPAQUE_BELOW = 135       # min(r,g,b) <= this: fully opaque
WHITE_TRANSPARENT_ABOVE = 200  # >= this: fully transparent; between: feathered


def punch_white_to_alpha(mark):
    """Fade near-white pixels' alpha to 0 (feathered) in an RGBA image, in place.

    Only the alpha channel is touched -- surviving pixels keep their original
    colour, so this removes the white patches without recolouring anything.
    """
    px = mark.load()
    w, h = mark.size
    span = float(WHITE_TRANSPARENT_ABOVE - WHITE_OPAQUE_BELOW)
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a == 0:
                continue
            m = min(r, g, b)
            if m <= WHITE_OPAQUE_BELOW:
                continue
            t = (WHITE_TRANSPARENT_ABOVE - m) / span
            if t < 0:
                t = 0.0
            px[x, y] = (r, g, b, int(a * t))
    return mark


def build_ios_master(src, Image):
    """Return a 1024px RGB icon: logo mark centered on a brand gradient."""
    if src.mode != "RGBA":
        src = src.convert("RGBA")
    bbox = src.getchannel("A").getbbox()  # the mark's opaque extent
    if not bbox:
        print("WARN: iOS master fallback, logo mark not detected")
        bbox = (0, 0, src.size[0], src.size[1])
    mark = src.crop(bbox)

    edge = src.size[0]  # square canvas
    target = int(edge * IOS_MARK_RATIO)
    scale = target / max(mark.size)
    mark = mark.resize((max(1, int(mark.size[0] * scale)),
                        max(1, int(mark.size[1] * scale))), Image.LANCZOS)
    # Knock out the white swoosh AFTER scaling so the resized anti-aliased edge
    # is swept up too.
    punch_white_to_alpha(mark)

    # Vertical gradient canvas, then center the mark (slight optical drop). The
    # knocked-out swoosh lets the gradient show through, so the icon stays
    # opaque (iOS icons forbid alpha) with no white patches.
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


# In-app splash foreground: the logo mark cropped tight to its alpha bbox with
# the white swoosh knocked out to transparency (so the splash's CSS gradient
# shows through the cut-out). See build_splash_logo.
SPLASH_LOGO_EDGE = 512


def build_splash_logo(src, Image):
    """Return the logo mark (white swoosh transparent) on a transparent canvas."""
    if src.mode != "RGBA":
        src = src.convert("RGBA")
    bbox = src.getchannel("A").getbbox()
    mark = src.crop(bbox) if bbox else src
    scale = SPLASH_LOGO_EDGE / max(mark.size)
    mark = mark.resize((max(1, int(mark.size[0] * scale)),
                        max(1, int(mark.size[1] * scale))), Image.LANCZOS)
    punch_white_to_alpha(mark)
    canvas = Image.new("RGBA", (SPLASH_LOGO_EDGE, SPLASH_LOGO_EDGE), (0, 0, 0, 0))
    ox = (SPLASH_LOGO_EDGE - mark.size[0]) // 2
    oy = (SPLASH_LOGO_EDGE - mark.size[1]) // 2
    canvas.paste(mark, (ox, oy), mark)
    return canvas


def build_android_adaptive(master, mark_plate, edge, Image):
    """Return (background, foreground) adaptive layers, each `edge` px square.

    `master` is the opaque branded icon (its gradient becomes the background
    layer); `mark_plate` is the tight, white-knocked-out mark that fills its
    own canvas, so resizing it to ANDROID_FG_MARK_RATIO*edge puts the mark at
    exactly the size iOS shows it. Two separate layers (instead of pasting the
    branded plate into the foreground) means no seam where the plate edge would
    cross the launcher mask.
    """
    bg = master.convert("RGBA").resize((edge, edge), Image.LANCZOS)
    fg = Image.new("RGBA", (edge, edge), (0, 0, 0, 0))
    plate = max(1, int(round(edge * ANDROID_FG_MARK_RATIO)))
    mark = mark_plate.resize((plate, plate), Image.LANCZOS)
    # Centered (no iOS's 2% optical drop: Android's mask is already generous and
    # the adaptive spec asks for the keyline centre).
    fg.paste(mark, ((edge - plate) // 2, (edge - plate) // 2), mark)
    return bg, fg


# macOS .icns iconset layout (filename -> pixel edge). iconutil folds these into
# the multi-resolution icon the Finder/Dock read via CFBundleIconFile.
MAC_ICONSET = [
    ("icon_16x16.png", 16), ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32), ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128), ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256), ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512), ("icon_512x512@2x.png", 1024),
]


def build_macos_icns(master, out_path, Image):
    """Render the .iconset from `master` and run iconutil to make an .icns.

    macOS-only (needs iconutil); returns True on success, False when skipped so
    non-mac hosts simply don't get an .icns. dx writes CFBundleIconFile=icon.icns
    into the .app but never generates it, so inject_macos_icon.py copies the
    produced file into Contents/Resources after `dx bundle`.
    """
    if sys.platform != "darwin" or not shutil.which("iconutil"):
        return False
    with tempfile.TemporaryDirectory() as td:
        iconset = os.path.join(td, "app.iconset")
        os.makedirs(iconset)
        for name, px in MAC_ICONSET:
            master.resize((px, px), Image.LANCZOS).save(
                os.path.join(iconset, name))
        r = subprocess.run(["iconutil", "-c", "icns", iconset, "-o", out_path],
                           capture_output=True, text=True)
        if r.returncode != 0:
            print("WARN: iconutil failed: " + (r.stderr or r.stdout).strip())
            return False
    return True


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
    splash_logo = build_splash_logo(src, Image)
    # (out_dir, name, edge, flatten, base, punch). Everything renders from
    # `ios_master` (branded, opaque, mark at 62%) -- desktop, iOS and the
    # Android legacy mipmaps alike -- so all platforms show the same brand. The
    # Android adaptive layers below are the exception: their foreground must be
    # TRANSPARENT, so they are built from `splash_logo` (the tight mark with the
    # white swoosh already knocked out) over the master's gradient background.
    jobs = [(OUT_DIR, name, edge, False, ios_master, False) for name, edge in SIZES.items()]
    jobs += [(os.path.join(OUT_DIR, "ios"), n, e, True, ios_master, False)
             for n, e in IOS_SIZES.items()]
    jobs += [(os.path.join(OUT_DIR, "android"), n, e, False, ios_master, False)
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
    # Splash foreground consumed by src/components/splash.rs (include_bytes!).
    splash_path = os.path.join(OUT_DIR, "splash-logo.png")
    if args.check:
        ok = open(splash_path, "rb").read() == _encode(splash_logo)
        print("splash-logo.png: {}".format("up-to-date" if ok else "STALE"))
    else:
        with open(splash_path, "wb") as f:
            f.write(_encode(splash_logo))
        print("wrote {} (splash logo, {}x{})".format(splash_path, *splash_logo.size))
    for out_dir, name, edge, flatten, base, punch in jobs:
        out_path = os.path.join(out_dir, name)
        img = base if edge == base.size else base.resize((edge, edge), Image.LANCZOS)
        if img.mode != "RGBA":
            img = img.convert("RGBA")
        if punch:  # knock the white swoosh out to transparency (Android)
            img = punch_white_to_alpha(img)
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

    # Android adaptive layers: package_android.sh wires these into the
    # mipmap-anydpi-v26/ic_launcher.xml it writes (the dx template has none).
    for dpi, edge in ANDROID_DENSITIES.items():
        bg, fg = build_android_adaptive(ios_master, splash_logo, edge, Image)
        for tag, img in (("ic_launcher_background", bg), ("ic_launcher_foreground", fg)):
            rel = "mipmap-{}/{}.png".format(dpi, tag)
            out_path = os.path.join(OUT_DIR, "android", rel)
            if args.check:
                ok = os.path.exists(out_path) and open(out_path, "rb").read() == _encode(img)
                print("{}: {}x{} {}".format(rel, edge, edge, "up-to-date" if ok else "STALE"))
            else:
                os.makedirs(os.path.dirname(out_path), exist_ok=True)
                with open(out_path, "wb") as f:
                    f.write(_encode(img))
                print("wrote {} ({}x{})".format(out_path, edge, edge))

    # macOS bundle icon (.icns): dx declares CFBundleIconFile=icon.icns but
    # never builds the file, so inject_macos_icon.py drops this into the .app's
    # Contents/Resources after `dx bundle`.
    icns_path = os.path.join(OUT_DIR, "app-icon.icns")
    if args.check:
        print("app-icon.icns: {}".format(
            "up-to-date (present)" if os.path.exists(icns_path)
            else "MISSING (run gen_icons.py on macOS)"))
    elif build_macos_icns(ios_master, icns_path, Image):
        print("wrote {} (macOS bundle icon)".format(icns_path))
    else:
        print("skip app-icon.icns (needs macOS + iconutil)")

    if not args.check:
        print("Next: desktop bundling uses [bundle] icon; iOS injection runs "
              "scripts/inject_ios_icon.py, Android legacy mipmaps + adaptive "
              "layers are copied and wired into ic_launcher.xml by "
              "package_android.sh (dx alpha.1 ignores bundle.icon on mobile).")


def _encode(img):
    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=True)
    return buf.getvalue()


if __name__ == "__main__":
    main()
