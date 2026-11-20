#!/bin/sh
# Builds ManageBIM-<version>-freebsd-x86_64.tar.gz from target/release/ManageBIM.
# Unpack anywhere and run ./ManageBIM/ManageBIM (needs mesa + libxkbcommon from pkg, see README).
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
version="$(grep -m1 '^version' "$root/Cargo.toml" | cut -d'"' -f2)"
stage="$root/target/freebsd-stage/ManageBIM"
rm -rf "$root/target/freebsd-stage"
mkdir -p "$stage" "$root/dist"
cp "$root/target/release/ManageBIM" "$stage/"
cp "$root/packaging/linux/managebim.png" "$root/packaging/linux/managebim.desktop" "$stage/"
cp "$root/LICENSE" "$root/THIRD_PARTY.md" "$stage/" 2>/dev/null || true
tar -C "$root/target/freebsd-stage" -czf "$root/dist/ManageBIM-$version-freebsd-x86_64.tar.gz" ManageBIM
echo "Built dist/ManageBIM-$version-freebsd-x86_64.tar.gz"
