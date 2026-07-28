# Submitting bohay to nixpkgs

`package.nix` in this folder is a ready-to-submit nixpkgs definition. It builds a
**released tag** (not the local tree), so it is independent of `flake.nix` (which
is for `nix run github:RizRiyz/bohay` and dev shells). Follow these steps to get
`pkgs.bohay` into nixpkgs.

## 1. Version (kept in sync automatically)

`scripts/release.sh` bumps `package.nix`'s `version` on every release and resets
the two hashes to `lib.fakeHash`, so the version here always matches the newest
**released tag**. You only ever need to recompute the hashes (next step). If you
are packaging by hand, confirm `version` matches an existing tag at
`github.com/RizRiyz/bohay/tags` first.

## 2. Fill in the two hashes

Both start as `lib.fakeHash`; Nix prints the real value on the first build. Do the
**source** hash first, then the **cargo** hash.

```sh
# From a nixpkgs checkout (or with this file copied into one), build the package.
# The build fails with "hash mismatch ... got: sha256-XXXX=" — paste that in.
nix-build -A bohay

# Repeat once more: after fixing the src hash, the next failure gives cargoHash.
```

Or compute the source hash directly, without a build:

```sh
nix-prefetch-url --unpack \
  https://github.com/RizRiyz/bohay/archive/refs/tags/v0.9.4.tar.gz
# convert to SRI: nix hash to-sri --type sha256 <the-hash-it-printed>
```

## 3. Add yourself as a maintainer

Edit `maintainers/maintainer-list.nix` in your nixpkgs fork and add an entry
(alphabetical by handle):

```nix
rizriyz = {
  email = "ariestiyansyah.rizky@gmail.com";
  github = "RizRiyz";
  githubId = 2667489;
  name = "Riz";
};
```

Then reference it in `package.nix`:

```nix
maintainers = with lib.maintainers; [ rizriyz ];
```

## 4. Place the file and build

```sh
git clone https://github.com/NixOS/nixpkgs   # or your fork
mkdir -p nixpkgs/pkgs/by-name/bo/bohay
cp package.nix nixpkgs/pkgs/by-name/bo/bohay/package.nix
cd nixpkgs
nix-build -A bohay            # must succeed and produce ./result/bin/bohay
./result/bin/bohay --version  # smoke test
```

## 5. Review-check and open the PR

```sh
nix-shell -p nixpkgs-review --run "nixpkgs-review rev HEAD"   # after committing
```

Commit message follows the nixpkgs convention:

```
bohay: init at 0.9.4
```

Open the PR against `NixOS/nixpkgs` (base branch `master`). Expect ofborg CI and a
human review; keep the PR description short and note that tests are disabled
(`doCheck = false`) because they need PTYs and a writable `$HOME`, and that CI
runs the full suite upstream.

## Keeping it updated

nixpkgs does **not** auto-track your releases. After the initial `init`, updates
come from either a manual PR (`bohay: 0.9.4 -> 0.9.5`, bumping `version` + both
hashes) or the nixpkgs `r-ryantm` auto-update bot, which lags your tags. Your
`flake.nix` remains the always-current path for Nix users who add it as an input.
