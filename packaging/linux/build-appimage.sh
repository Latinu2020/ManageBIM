#!/usr/bin/env bash
# Builds ManageBIM-<version>-x86_64.AppImage from target/release/ManageBIM.
# A single file: chmod +x and run it - no installation, no root.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
version="$(grep -m1 '^version' "$root/Cargo.toml" | cut -d'"' -f2)"
app="$root/target/AppDir"
rm -rf "$app"
mkdir -p "$app/usr/bin" "$app/usr/share/applications" "$app/usr/share/icons/hicolor/256x256/apps" "$app/usr/share/managebim"
cp "$root/target/release/ManageBIM" "$app/usr/bin/"
cp "$here/managebim.desktop" "$app/usr/share/applications/"
cp "$here/managebim.desktop" "$app/"
cp "$here/managebim.png" "$app/usr/share/icons/hicolor/256x256/apps/"
cp "$here/managebim.png" "$app/managebim.png"
cp "$root/LICENSE" "$root/THIRD_PARTY.md" "$app/usr/share/managebim/" 2>/dev/null || true
cat > "$app/AppRun" <<'EOF'
#!/bin/sh
here="$(dirname "$(readlink -f "$0")")"
exec "$here/usr/bin/ManageBIM" "$@"
EOF
chmod +x "$app/AppRun" "$app/usr/bin/ManageBIM"
tool="$root/target/appimagetool-x86_64.AppImage"
if [ ! -x "$tool" ]; then
  curl -fsSL -o "$tool" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x "$tool"
fi
mkdir -p "$root/dist"
ARCH=x86_64 "$tool" --appimage-extract-and-run "$app" "$root/dist/ManageBIM-$version-x86_64.AppImage"
echo "Built dist/ManageBIM-$version-x86_64.AppImage"
