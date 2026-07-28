#!/usr/bin/env bash
#
# nixpkgs-update.sh — bump the bohay package in a nixpkgs checkout to a released
# version and recompute its hashes, so nixpkgs stays in sync with a
# `scripts/release.sh` release. Run it *after* the crates.io/GitHub release, once
# the tag exists (and after bohay is already merged into nixpkgs — the first
# submission is a manual `init` PR, see nix/README.md).
#
#   BOHAY_NIXPKGS_DIR=~/nixpkgs scripts/nixpkgs-update.sh              # newest tag
#   BOHAY_NIXPKGS_DIR=~/nixpkgs scripts/nixpkgs-update.sh 0.9.6        # a version
#   scripts/nixpkgs-update.sh 0.9.6 --nixpkgs ~/nixpkgs --pr           # + open the PR
#
# Prereqs: a nixpkgs git checkout that already contains bohay, plus `nix`,
# `nix-update`, and (for --pr) `gh`. Install nix-update with
# `nix profile install nixpkgs#nix-update` or run inside `nix-shell -p nix-update`.
set -euo pipefail

die()  { printf '\033[31merror:\033[0m %s\n' "$1" >&2; exit 1; }
step() { printf '\n\033[36m▸ %s\033[0m\n' "$1"; }

PKG_PATH="pkgs/by-name/bo/bohay/package.nix"

# The bohay repo this script lives in (for the default version = its newest tag).
BOHAY_REPO="$(cd "$(dirname "$0")/.." && git rev-parse --show-toplevel)"

# ── args ──
VERSION=""
NIXPKGS="${BOHAY_NIXPKGS_DIR:-}"
OPEN_PR=0
while [ $# -gt 0 ]; do
  case "$1" in
    --nixpkgs) NIXPKGS="${2:-}"; shift 2 ;;
    --pr)      OPEN_PR=1; shift ;;
    -*)        die "unknown flag: $1" ;;
    *)         VERSION="$1"; shift ;;
  esac
done

# Default the target to the newest tag in the bohay repo.
if [ -z "$VERSION" ]; then
  VERSION="$(git -C "$BOHAY_REPO" tag | sort -V | tail -1)"
  VERSION="${VERSION#v}"
fi
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version must be X.Y.Z (got '$VERSION')"

[ -n "$NIXPKGS" ] || die "set \$BOHAY_NIXPKGS_DIR or pass --nixpkgs <dir>"
[ -d "$NIXPKGS/.git" ] || die "'$NIXPKGS' is not a git checkout"
[ -f "$NIXPKGS/$PKG_PATH" ] || die "no bohay package at $NIXPKGS/$PKG_PATH — submit the init PR first (nix/README.md)"
command -v nix-update >/dev/null || die "nix-update not found — 'nix profile install nixpkgs#nix-update'"
[ -z "$(git -C "$NIXPKGS" status --porcelain)" ] || die "nixpkgs checkout '$NIXPKGS' is dirty — commit or stash first"

cd "$NIXPKGS"
OLD="$(sed -nE 's/^[[:space:]]*version = "([0-9.]+)";.*/\1/p' "$PKG_PATH" | head -1)"
[ -n "$OLD" ] || die "could not read the current version from $PKG_PATH"
if [ "$OLD" = "$VERSION" ]; then
  echo "  nixpkgs is already at $VERSION — nothing to do"
  exit 0
fi
echo "  $OLD  →  $VERSION   (in $NIXPKGS)"

step "Branch off the current master"
git fetch --quiet origin 2>/dev/null || true
git checkout -B "bohay-$VERSION"

step "Bump version + recompute src and cargo hashes (nix-update)"
# nix-update rewrites `version`, the source hash, and `cargoHash` from the new tag.
nix-update bohay --version "$VERSION"

step "Build + smoke test"
nix-build -A bohay
GOT="$(./result/bin/bohay --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)"
[ "$GOT" = "$VERSION" ] || die "built binary reports '$GOT', expected $VERSION"
echo "  ✓ result/bin/bohay --version → $GOT"

step "Commit"
git add "$PKG_PATH"
git commit -m "bohay: $OLD -> $VERSION"

if [ "$OPEN_PR" = 1 ]; then
  step "Push + open PR"
  command -v gh >/dev/null || die "gh not found — push + open the PR by hand"
  git push -u origin "bohay-$VERSION"
  gh pr create --repo NixOS/nixpkgs --base master \
    --title "bohay: $OLD -> $VERSION" \
    --body "Updates bohay to $VERSION.

- Built on aarch64-darwin; \`result/bin/bohay --version\` prints $VERSION.
- \`doCheck = false\` (tests need PTYs and a writable \$HOME); upstream CI runs the full suite."
  echo "  ✓ PR opened"
else
  step "Done — review, then push + open the PR"
  echo "    git -C $NIXPKGS push -u origin bohay-$VERSION"
  echo "    gh pr create --repo NixOS/nixpkgs --base master --title \"bohay: $OLD -> $VERSION\""
  echo "  (or re-run with --pr to do both automatically)"
fi
