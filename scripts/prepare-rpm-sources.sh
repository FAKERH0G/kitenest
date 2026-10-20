#!/usr/bin/env bash
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)"
if [ -z "$version" ]; then
    printf 'Could not read package version from Cargo.toml\n' >&2
    exit 1
fi
project="$(basename "$root")"

out="${1:-$root/rpmbuild/SOURCES}"
mkdir -p "$root/.cargo" "$root/vendor" "$out"
(
    cd "$root"
    cargo vendor --locked vendor > .cargo/config.toml
)
tar -czf "$out/kitenest-$version-vendor.tar.gz" \
    -C "$root" vendor .cargo/config.toml
tar -czf "$out/kitenest-$version.tar.gz" \
    --exclude=.git --exclude=target --exclude=vendor --exclude=rpmbuild \
    --transform="s,^${project},${project}-${version}," \
    -C "$(dirname "$root")" "$project"
printf 'RPM sources prepared in %s\n' "$out"
