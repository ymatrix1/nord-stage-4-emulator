# Releasing

How to cut `vX.Y.Z`. The pipeline is `.github/workflows/release.yml`; packaging and checks are
`tools/release_package.py` and `tools/release_verify.py` (unit-tested under `tools/tests/`).

## What a release contains

One archive per platform, each with a `.sha256` file, plus a combined `SHA256SUMS.txt` on the release page.

| Platform | Runner | Target triple | Archive |
|---|---|---|---|
| Linux x86_64 | `ubuntu-24.04` | `x86_64-unknown-linux-gnu` | `nord-stage-4-emulator-vX.Y.Z-linux-x86_64.tar.gz` |
| Windows x86_64 | `windows-latest` | `x86_64-pc-windows-msvc` | `nord-stage-4-emulator-vX.Y.Z-windows-x86_64.zip` |
| macOS Apple silicon | `macos-latest` | `aarch64-apple-darwin` | `nord-stage-4-emulator-vX.Y.Z-macos-arm64.tar.gz` |

Each archive has one top-level folder holding `ns4`, `nord-stage4`, `nord-plugin-standalone`,
`nord-plugin.clap`, `nord-plugin.vst3`, `README.txt` (install paths per OS, source:
[RELEASE_README.txt](RELEASE_README.txt)) and `HOST_COMPATIBILITY.md`. Intel macOS and Linux arm64 are not built.

Plugin layout inside the archive:

| OS | CLAP | VST3 module |
|---|---|---|
| Windows | `nord-plugin.clap` (a DLL) | `nord-plugin.vst3/Contents/x86_64-win/nord-plugin.vst3` |
| Linux | `nord-plugin.clap` (a shared object) | `nord-plugin.vst3/Contents/x86_64-linux/nord-plugin.so` |
| macOS | `nord-plugin.clap/Contents/{Info.plist,MacOS/nord-plugin}` | `nord-plugin.vst3/Contents/{Info.plist,MacOS/nord-plugin}` |

Builds are unsigned (no Authenticode, no Apple notarization). The macOS README explains `xattr -dr com.apple.quarantine`.
The plugin is not verified in any DAW; every archive README and the release body say so. Remove that wording only
after [HOST_COMPATIBILITY.md](HOST_COMPATIBILITY.md) records real host tests.

## Version bump locations

The version lives in two places. Change both in the release PR; nothing else needs editing.

| File | Field | Notes |
|---|---|---|
| `Cargo.toml` | `[workspace.package] version` | Every crate uses `version.workspace = true`; `ns4 info` and the plugin report it via `CARGO_PKG_VERSION` |
| `pyproject.toml` | `[project] version` | Python tooling only; a unit test fails if it differs from `Cargo.toml` |

Then run `cargo check --workspace` so `Cargo.lock` picks up the new version, and commit the lock file with the bump.
The tag must equal `v` plus the `Cargo.toml` version (`v0.2.0` for `0.2.0`, `v0.2.0-rc.1` for `0.2.0-rc.1`).
The workflow fails in its first job if they differ, before any build minutes are spent.
A tag with a hyphen is published as a GitHub pre-release.

## Pre-release checklist

- [ ] Version bumped in both files above, `Cargo.lock` updated, merged to `main` through a PR.
- [ ] CI is green on that exact `main` commit (all three OS, Python job). The release workflow does not repeat the test suite; CI also runs on `v*` tags, so the tagged commit gets a second full result.
- [ ] Checks from `AGENTS.md` pass locally if the bump touched anything besides the version.
- [ ] Dry run is green (next section) and the artifacts were downloaded and inspected at least once for this release.
- [ ] `docs/HOST_COMPATIBILITY.md` is current. If a DAW test result changed, update the README wording in `RELEASE_README.txt`.
- [ ] No `.reference/`, firmware, backup or preset files are staged (`git status`, and `tools/release_verify.py` enforces it for archives).
- [ ] `LICENSE` file(s) exist in the repo root; `tools/release_package.py` includes any `LICENSE*` it finds.

## Dry run (publishes nothing)

GitHub: Actions, Release, Run workflow, branch `main`, leave **tag empty**. This builds all three platforms, packages,
runs `release_verify.py`, and uploads the archives as workflow artifacts (14 days). The `publish` job is skipped.
Dry-run archives are named `...-vX.Y.Z-dev-<platform>...`, so they cannot be mistaken for a release.

Locally (any OS, after the build steps in the workflow):

```bash
T=x86_64-pc-windows-msvc   # your host triple from the table above
cargo build --release --locked --target $T -p nord-core --bin ns4
cargo build --release --locked --target $T -p nord-gui --bin nord-stage4
cargo build --release --locked --target $T -p nord-plugin --bin nord-plugin-standalone
cargo xtask bundle nord-plugin --release --locked --target $T
python tools/release_package.py --target $T
python tools/release_verify.py dist/nord-stage-4-emulator-*.zip --target $T
```

`cargo xtask bundle` locates the workspace root as the outermost ancestor directory containing a `Cargo.toml`.
Run it from a checkout that is not nested inside another Cargo workspace (for example a git worktree under the repository),
or it builds the outer checkout instead.

## Cut the release

1. Merge the bump PR. Wait for CI on `main`.
2. Tag the merge commit and push the tag (this is the only step that publishes):
   ```bash
   git switch main && git pull --ff-only
   git tag -a vX.Y.Z -m "vX.Y.Z"
   git push origin vX.Y.Z
   ```
3. The `Release` workflow runs: `plan` (tag format, tag exists, tag equals `Cargo.toml` version) then `build` x3
   (build, CLI smoke test, package, verify, upload) then `publish` (combine and check `SHA256SUMS.txt`, create the release).
4. Open the release page and read the body. Release notes are generated by GitHub from merged PRs since the previous tag,
   below a fixed header that states the plugin is not DAW-verified and the builds are unsigned. Edit the notes there if needed.

To re-publish an existing tag (for example after a failed run): Actions, Release, Run workflow, enter the tag. The workflow
checks out that tag, not the branch, and refuses a tag that does not exist (it never creates one).
To drop a bad release: delete the release and tag on GitHub (`gh release delete vX.Y.Z --cleanup-tag`), fix, bump, retag.
Never move a published tag.

## Verify a download (checksums)

Put the archive and its `.sha256` file in one folder, then:

```bash
sha256sum -c nord-stage-4-emulator-vX.Y.Z-linux-x86_64.tar.gz.sha256     # Linux
shasum -a 256 -c nord-stage-4-emulator-vX.Y.Z-macos-arm64.tar.gz.sha256  # macOS
# Windows PowerShell: compare the printed hash with the line in the .sha256 file
(Get-FileHash .
ord-stage-4-emulator-vX.Y.Z-windows-x86_64.zip -Algorithm SHA256).Hash
```

`SHA256SUMS.txt` on the release page holds the same three lines.
The `.sha256` files and `SHA256SUMS.txt` are LF-terminated, two-space `sha256sum` format; the verifier rejects CRLF.
Checksums protect against corruption, not against a compromised release: there is no signature yet.

## What the pipeline checks

`tools/release_verify.py` fails the build when any of these is false:

| Check | Detail |
|---|---|
| One top folder, expected files only present | Includes the VST3 module at its per-OS path |
| Nothing forbidden | No `.reference`, `references`, `factory_programs`, `target`, `.git`, `.beans`, `.agents`, `.claude` path part; no `.ns4*`, `.nsmp4`, `.npno`, `.pdf`, `.bin`, `.fw`, `.ldr`, `.dxe`, `.hex`, `.asm`, `.pyc`, `.pdb` file |
| Allowlist packaging | `release_package.py` reads only the named binaries, the two plugin bundles, the README, `HOST_COMPATIBILITY.md` and `LICENSE*`; stray files in `target/` or the repo are never read |
| No manufacturer name | Case-exact `Clavia` / `CLAVIA` absent from the CLI, GUI, standalone and plugin binaries |
| Plugin entry points | PE export table parse on Windows (`clap_entry`, `GetPluginFactory`); symbol-name search in the ELF/Mach-O image on Linux and macOS |
| README disclaimer | `README.txt` must say the plugin has not been verified in any DAW |
| Checksum file | Matches the archive, LF line endings |

Not checked by the pipeline: that a DAW loads the plugin, audio output, GUI start-up, macOS code signing, glibc floor of the Linux build
(built on `ubuntu-24.04`, so it needs glibc 2.39 or newer).

## Cost per run

Dry run or release: 3 build jobs (Linux, Windows, macOS) plus two small ubuntu jobs. The full test suite is not rerun
(the earlier workflow compiled and ran it in release mode on all three OSes). Rust cache is restored but not saved.
