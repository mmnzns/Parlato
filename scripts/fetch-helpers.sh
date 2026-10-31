#!/usr/bin/env bash
# Builds Parlato's local AI helper (parlato-llm) from the shared engine and puts it in src-tauri/binaries/, where Tauri
# looks for the externalBin sidecar. For local development on a Mac; releases use the engine's reusable workflow
# (see docs/building-helpers.md in the engine).
#
#   scripts/fetch-helpers.sh
#
# The engine is cloned at the pinned tag into a cache folder outside this repository and built there (the build reads
# the engine's own .cargo/config.toml, which Cargo would not do inside Parlato). The first run builds llama.cpp and
# takes several minutes; later runs reuse the cache. src-tauri/binaries/ is git-ignored.
#
# Override: ENGINE_REF (tag, default v0.3.1), PARLATO_ENGINE_CACHE (cache folder).
set -euo pipefail

ENGINE_REF="${ENGINE_REF:-v0.3.1}"
CACHE="${PARLATO_ENGINE_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/parlato/voice-engine-$ENGINE_REF}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/src-tauri/binaries"

die() { printf 'fetch-helpers: %s\n' "$1" >&2; exit 1; }

command -v git >/dev/null || die "git is required"
command -v cargo >/dev/null || die "cargo is required"
command -v jq >/dev/null || die "jq is required (the engine's build script reads Cargo's JSON with it)"
command -v cmake >/dev/null || die "cmake is required (llama.cpp is built from source)"

TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
case "$TRIPLE" in
  aarch64-apple-darwin) ;;
  *) die "this script is for the Mac (aarch64-apple-darwin); this computer is $TRIPLE. Windows helpers come from the release workflow." ;;
esac

if [[ ! -d "$CACHE/.git" ]]; then
  mkdir -p "$(dirname "$CACHE")"
  git clone --quiet https://github.com/mmnzns/voice-engine "$CACHE"
fi
git -C "$CACHE" fetch --quiet --tags origin
git -C "$CACHE" checkout --quiet --detach "$ENGINE_REF"

# The build folder lives inside the cache, not in this repository.
export CARGO_TARGET_DIR="$CACHE/target"

mkdir -p "$OUT"
"$CACHE/scripts/build-helpers.sh" "$TRIPLE" "$OUT" --engine-ref "$ENGINE_REF" --helpers llm --llm-name parlato-llm

# Tauri only bundles what externalBin lists; the record files are not needed.
rm -f "$OUT/SHA256SUMS" "$OUT/build-info.txt"
echo "fetch-helpers: $OUT/parlato-llm-$TRIPLE is ready"
