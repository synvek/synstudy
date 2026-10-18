#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Inject the app icon into a built macOS .app bundle.

dx 0.8.0-alpha.1's desktop bundling is inconsistent about the icon: `dx build`
writes CFBundleIconFile=icon.icns but never produces the file, while `dx bundle`
emits its own <App>.icns that is structurally broken -- besides the modern PNG
representations it embeds legacy bitmap pairs (is32/s8mk, il32/l8mk, it32/t8mk)
whose payload lengths violate the icns spec, and IconServices picks those up for
small sizes (16/24px render as a blurry square with a white ring and palette
junk). Copy the app-icon.icns produced by gen_icons.py into
Contents/Resources, set CFBundleIconFile to it AND delete the other .icns files
so no broken representation can leak back in.

Run AFTER `dx bundle` and BEFORE any codesign -- touching a signed bundle
invalidates its signature (same constraint as the iOS db/icon injection).

Usage:
  python3 scripts/inject_macos_icon.py <path/to/Foo.app>
"""
import os
import plistlib
import shutil
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ICNS = os.path.join(ROOT, "assets", "icons", "app-icon.icns")


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__.strip().splitlines()[-1])
    app = sys.argv[1]
    contents = os.path.join(app, "Contents")
    res = os.path.join(contents, "Resources")
    plist_path = os.path.join(contents, "Info.plist")
    if not os.path.isdir(res) or not os.path.isfile(plist_path):
        sys.exit("ERROR: {} is not a macOS .app (no Contents/Resources / Info.plist)".format(app))
    if not os.path.isfile(ICNS):
        sys.exit("ERROR: {} missing (run scripts/gen_icons.py on macOS)".format(ICNS))
    shutil.copy2(ICNS, os.path.join(res, "icon.icns"))
    # Remove dx's own broken <App>.icns (and any stale leftovers): IconServices
    # scans every .icns in Resources, so a malformed file must not stay behind.
    removed = []
    for name in sorted(os.listdir(res)):
        path = os.path.join(res, name)
        if name.endswith(".icns") and name != "icon.icns" and os.path.isfile(path):
            os.remove(path)
            removed.append(name)
    with open(plist_path, "rb") as f:
        info = plistlib.load(f)
    info["CFBundleIconFile"] = "icon.icns"
    with open(plist_path, "wb") as f:
        plistlib.dump(info, f)
    print("macOS icon injected -> {} (Contents/Resources/icon.icns, CFBundleIconFile=icon.icns{})".format(
        app, ", removed: " + ", ".join(removed) if removed else ""))


if __name__ == "__main__":
    main()
