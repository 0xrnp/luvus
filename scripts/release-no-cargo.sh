#!/usr/bin/env bash
#
# release-no-cargo.sh — publish release metadata to Homebrew for an existing GitHub release.
#
#   scripts/release-no-cargo.sh 0.1.1             # bump tap and push
#   scripts/release-no-cargo.sh 0.1.1 --dry-run   # show what would change
#   scripts/release-no-cargo.sh 0.1.1 --yes       # skip confirm prompt
#
# This script assumes binaries are already available in the GitHub release.
# It does not run cargo, build, test, or crate publish steps.
set -euo pipefail

REPO="RizRiyz/luvus"

die()  { printf '\033[31merror:\033[0m %s\n' "$1" >&2; exit 1; }
step() { printf '\n\033[36m▸ %s\033[0m\n' "$1"; }
need()  { command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"; }

# Each prebuilt release asset has its own sha256 sidecar file.
FORMULA_TARGETS="aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-musl aarch64-unknown-linux-musl"

wait_for_assets() {
  local waited=0
  while [ "$waited" -lt 600 ]; do
    local missing=0
    local assets
    assets="$(gh release view "$TAG" --repo "$REPO" --json assets --jq '.assets[].name' 2>/dev/null || true)"
    if printf '%s\n' "$assets" | grep -q '^bohay-'; then
      printf '%s\n' "$assets" | grep '^bohay-' | sed 's/^/  /'
      die "legacy bohay assets detected for $TAG; 0.11+ releases must ship only luvus artifacts"
    fi
    for t in $FORMULA_TARGETS; do
      printf '%s\n' "$assets" | grep -qx "luvus-$TAG-$t.sha256" || missing=1
    done
    [ "$missing" = 0 ] && return 0
    printf '  waiting for release binaries… (%ss)\r' "$waited"
    sleep 15
    waited=$((waited + 15))
  done
  die "release assets for $TAG never appeared — run again when the release workflow finishes"
}

asset_sha() {
  gh release download "$TAG" --repo "$REPO" --pattern "luvus-$TAG-$1.sha256" -O - 2>/dev/null | awk '{print $1}'
}

bump_formula() {
  local f="$1" t sha
  perl -0pi -e "s/^  version \"[0-9.]+\"/  version \"$VERSION\"/m" "$f"
  for t in $FORMULA_TARGETS; do
    sha="$(asset_sha "$t")"
    [ -n "$sha" ] || die "no published checksum for $t"
    perl -0pi -e "s{releases/download/v[0-9.]+/luvus-v[0-9.]+-$t\\.tar\\.gz}{releases/download/$TAG/luvus-$TAG-$t.tar.gz}g" "$f"
    perl -0pi -e "s{(luvus-$TAG-$t\\.tar\\.gz\"\\n\\s*sha256 \")[0-9a-f]{64}}{\\$1$sha}s" "$f"
  done
  perl -0pi -e "s{archive/refs/tags/v[0-9.]+\\.tar\\.gz}{archive/refs/tags/$TAG.tar.gz}g" "$f"
}

VERSION="${1:-}"
MODE="${2:-}"
TAP="${LUVUS_TAP_DIR:-homebrew-luvus}"
[ -n "$VERSION" ] || die "usage: scripts/release-no-cargo.sh X.Y.Z [--dry-run|--yes]"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version must be semver X.Y.Z (got '$VERSION')"
TAG="v$VERSION"

cd "$(git rev-parse --show-toplevel)"

step "Preconditions"
need gh
need perl
need awk
need grep
need git
git fetch --tags --quiet
gh release view "$TAG" --repo "$REPO" >/dev/null \
  || die "GitHub release $TAG not found in $REPO"

TAP_FORMULA="$TAP/Formula/luvus.rb"
if [ -f "$TAP_FORMULA" ]; then
  [ -z "$(git -C "$TAP" status --porcelain)" ] || die "tap '$TAP' has uncommitted changes"
  echo "  tap: $TAP"
else
  die "tap '$TAP' not found or missing Formula/luvus.rb"
fi

step "Check released assets"
wait_for_assets

step "Verify checksums"
for t in $FORMULA_TARGETS; do
  sha="$(asset_sha "$t")"
  [ -n "$sha" ] || die "no checksum found for $t"
  echo "  $t: $sha"
done

if [ "$MODE" = "--dry-run" ]; then
  step "Dry run: preview formula update"
  formula_tmp=$(mktemp)
  cp "$TAP_FORMULA" "$formula_tmp"
  bump_formula "$formula_tmp"
  echo "  would update: $TAP_FORMULA"
  diff -u "$TAP_FORMULA" "$formula_tmp" | sed -n '1,220p'
  rm -f "$formula_tmp"
  exit 0
fi

if [ "$MODE" != "--yes" ]; then
  printf "\nRelease metadata for \033[1m%s\033[0m (no cargo). Continue? [y/N] " "$TAG"
  read -r ans
  [ "$ans" = "y" ] || [ "$ans" = "Y" ] || die "aborted"
fi

step "Update tap"
bump_formula "$TAP_FORMULA"
git -C "$TAP" add Formula/luvus.rb
git -C "$TAP" commit -m "luvus $TAG"
git -C "$TAP" push

step "Done"
echo "  tap:  ✓ $(git -C "$TAP" config --get remote.origin.url) updated to $TAG"
echo "  install: brew install $REPO/luvus"
