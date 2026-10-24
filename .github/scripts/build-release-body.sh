#!/usr/bin/env bash
# Compose GitHub Release body: version section from RELEASE_NOTES.md + shared footer.
set -euo pipefail

VERSION="${1:?usage: build-release-body.sh v0.1.8}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${2:-release-body.md}"

notes="$ROOT/RELEASE_NOTES.md"
footer="$ROOT/.github/release-footer.md"

if [[ ! -f "$notes" ]] || [[ ! -f "$footer" ]]; then
  echo "Missing RELEASE_NOTES.md or .github/release-footer.md" >&2
  exit 1
fi

{
  echo "## TKS Node ${VERSION}"
  echo ""
  awk -v ver="$VERSION" '
    $0 ~ "^## " ver "$" { found=1; next }
    found && /^## v[0-9]/ { exit }
    found { print }
  ' "$notes"
  echo ""
  sed "s/RELEASE_VERSION/${VERSION}/g" "$footer"
} > "$OUT"

echo "Wrote ${OUT}"
