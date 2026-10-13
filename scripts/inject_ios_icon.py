#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Inject the app icon into a built iOS .app bundle.

dx 0.8.0-alpha.1 ignores [bundle] icon for iOS (no xcassets/actool support in
the CLI source), so the simulator shows the translucent placeholder. We use
the legacy CFBundleIconFiles mechanism instead: copy the icon PNGs generated
by gen_icons.py into the .app root and list the base names in Info.plist.
This renders correctly on device and simulator for dev/ad-hoc installs.

MUST run BEFORE codesign (package_ios.sh / deploy_ios_device.sh call it
right after the dictionary-db injection step) — touching a signed .app
invalidates its signature.

Usage:
  python3 scripts/inject_ios_icon.py <path/to/Foo.app>
"""
import os
import plistlib
import shutil
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ICON_DIR = os.path.join(ROOT, "assets", "icons", "ios")

# Base names; iOS auto-resolves the @2x/@3x siblings by filename.
ICON_BASES = ["Icon-60", "Icon-Small", "Icon-Spotlight-40", "Icon-76"]


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__.strip().splitlines()[-1])
    app = sys.argv[1]
    plist_path = os.path.join(app, "Info.plist")
    if not os.path.isfile(plist_path):
        sys.exit("ERROR: {} is not an .app bundle (no Info.plist)".format(app))
    if not os.path.isdir(ICON_DIR):
        sys.exit("ERROR: {} missing (run scripts/gen_icons.py)".format(ICON_DIR))

    for fname in sorted(os.listdir(ICON_DIR)):
        shutil.copy2(os.path.join(ICON_DIR, fname), os.path.join(app, fname))
    with open(plist_path, "rb") as f:
        info = plistlib.load(f)
    info["CFBundleIconFiles"] = ICON_BASES
    # Legacy key some iOS versions still consult for the non-retina slot.
    info["CFBundleIconFile"] = "Icon-60.png"
    with open(plist_path, "wb") as f:
        plistlib.dump(info, f)
    print("iOS icon injected -> {} ({} pngs, CFBundleIconFiles={})".format(
        app, len(os.listdir(ICON_DIR)), ",".join(ICON_BASES)))


if __name__ == "__main__":
    main()
