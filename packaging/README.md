# Packaging

What installs ParCAD, and how each channel is fed. The channel list is kept
small on purpose: a channel is added when an issue asks for it, because each
one costs something every release.

| channel | source here | fed by | credential |
|---|---|---|---|
| GitHub Releases (app, CLI archives, `.mcpb`) | `.github/workflows/release.yml` | a `v*` tag, drafted; a human publishes | none |
| Homebrew, macOS and Linux | `homebrew/parcad.rb.in` | `publish.yml` pushes the rendered formula to `ierehon1905/homebrew-parcad` | `HOMEBREW_TAP_TOKEN`: fine-grained, contents read/write on the tap |
| winget | `winget/*.yaml.in` | `publish.yml`: the first version is submitted from these manifests; later ones are `komac update` | `WINGET_TOKEN`: classic token with `public_repo` (Komac cannot open the PR with a fine-grained one) |
| MCP Registry | `mcpb/server.json.in` | `publish.yml`, GitHub OIDC | none |
| Claude Code / Codex plugin | `plugin/` | the repository itself; see `plugin/README.md` | none |
| Claude Desktop extension | `mcpb/manifest.json` | `release.yml` packs it per platform | none |
| the desktop app's updater | `updater-manifest.py` | `release.yml`'s draft job signs each bundle and attaches `latest.json` | `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` |

## Templates

Every manifest that names a checksum is a template. `render.py` fills
`{{version}}` and `{{sha256:<asset>}}` from the published release's
`SHA256SUMS.txt`, so no hash in this repository was typed by hand, and a
template naming a file the release does not have fails the render rather than
writing a blank.

```bash
packaging/render.py --version v0.0.6 --sums SHA256SUMS.txt --out rendered/
```

## The updater

The app asks `releases/latest/download/latest.json` for a newer version, so
it only sees a release once that release is published. The manifest points at a
signed bundle per platform: the macOS `.app.tar.gz`, the AppImage, the `.deb`,
the NSIS installer and the `.msi`. An update is installed only if its
signature matches the public key in `app/src-tauri/tauri.conf.json`.

The private key and its password are the two secrets above. Only the draft
job reads them, and it builds nothing: the build jobs, which run every build
script and bundler plugin, never see the key. The owner keeps a copy in
`~/.tauri/parcad-updater.key` and `~/.tauri/parcad-updater.password`.
**Losing the key ends updates for every installed copy.** A tag without the
secrets fails at signing.

To try an update before releasing, pack and sign the new build the way
`release.yml` does (`tar`, then `tauri signer sign`), and build an old one with
`"version"` lowered and the updater's `endpoints` pointed at a local
`latest.json` (`"dangerousInsecureTransportProtocol":true` for http).

## A signed Mac build

The Developer ID never leaves the owner's Mac: no repository secret holds it,
and CI's macOS bundle is ad-hoc signed. Between the tag's draft and publishing
it, on a Mac with the *Developer ID Application* certificate in its keychain
and a `parcad-notary` notarytool profile,

```bash
tools/sign-mac-release.sh v<version>
```

downloads the draft's Mac zip and CLI archive, signs every binary in them with
the hardened runtime (`tools/sign-mac.sh`), notarises them, staples the app and
uploads both back with their new lines in `SHA256SUMS.txt`. It then downloads
the zip again, quarantines it as a browser would and checks Gatekeeper accepts
it. With the updater key present it also replaces the updater archive and
`latest.json`; without it the update stays ad-hoc, which is harmless, since an
update is never quarantined. It refuses a published release, whose checksums
Homebrew and winget already carry; `--dry-run` does everything but upload.

The `parcad-mcp-*.mcpb` bundle still carries the ad-hoc CLI. Whether Gatekeeper
stops it once Claude Desktop has unpacked it has not been checked.

`tools/release-mac.sh` builds the app and a DMG from source instead, notarises
the DMG and staples both.

## On each release

1. Bump the version everywhere; `packaging_manifests_carry_the_crate_version`
   fails until `mcpb/manifest.json` and the plugin manifests carry it.
2. Tag `v<version>`; `release.yml` drafts the release with every platform's files.
3. On the Mac with the Developer ID, `tools/sign-mac-release.sh v<version>`.
   The draft's notes say the Mac app is notarised; until this runs, it is not.
4. Read the draft, then publish it. `publish.yml` runs on that publish.
   A channel whose secret is missing skips with a notice and leaves its rendered
   manifest in the run's `manifests` artifact.

To check a release's manifests without sending anything:

```bash
gh workflow run publish.yml -f tag=v0.0.6
```
