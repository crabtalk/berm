#!/usr/bin/env bash
#
# Assemble berm.app.
#
# macOS reads an app's icon from its bundle and nowhere else — gpui's `icon` is
# X11-only — so this, and not the binary, is what puts the mark on the Dock.
#
#   ./apps/gui/bundle.sh          # release
#   ./apps/gui/bundle.sh debug    # skip the release build while iterating
set -euo pipefail

profile=${1:-release}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
exports="$here/icons/Icon Exports"
app="$root/target/berm.app"

if [ "$profile" = release ]; then
	cargo build --manifest-path "$root/Cargo.toml" -p berm-gui --release
else
	cargo build --manifest-path "$root/Cargo.toml" -p berm-gui
fi

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$root/target/$profile/berm-gui" "$app/Contents/MacOS/berm"

# The ten macOS sizes, each named for the slot it fills, out of a design tool's
# export of every platform's. The last one is the 1024 square: macOS asks for
# 512@2x, which is the same pixels under a different name.
if [ -d "$exports" ]; then
	iconset=$(mktemp -d)/berm.iconset
	mkdir -p "$iconset"
	for slot in \
		"16x16:16x16@1x" \
		"16x16@2x:16x16@2x" \
		"32x32:32x32@1x" \
		"32x32@2x:32x32@2x" \
		"128x128:128x128@1x" \
		"128x128@2x:128x128@2x" \
		"256x256:256x256@1x" \
		"256x256@2x:256x256@2x" \
		"512x512:512x512@1x" \
		"512x512@2x:1024x1024@1x"; do
		cp "$exports/Icon-iOS-Default-${slot#*:}.png" "$iconset/icon_${slot%%:*}.png"
	done
	iconutil --convert icns "$iconset" --output "$app/Contents/Resources/berm.icns"
	rm -rf "$(dirname "$iconset")"
else
	echo "warning: no icon exports at $exports — bundling without one" >&2
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key><string>berm</string>
	<key>CFBundleDisplayName</key><string>berm</string>
	<key>CFBundleExecutable</key><string>berm</string>
	<key>CFBundleIdentifier</key><string>com.crabtalk.berm</string>
	<key>CFBundleIconFile</key><string>berm</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>CFBundleShortVersionString</key><string>$version</string>
	<key>CFBundleVersion</key><string>$version</string>
	<key>NSHighResolutionCapable</key><true/>
	<key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST

# The icon cache keys off the bundle's mtime, so a rebuilt app keeps showing the
# old icon until something touches it.
touch "$app"
echo "$app"
