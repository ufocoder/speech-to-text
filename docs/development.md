# S2T development

[Back to the README](../README.md)

## Build and run

S2T uses Rust and GPUI Kit. Install stable Rust with rustup. On macOS, install Xcode and its command-line tools. On Windows x64, install the Visual Studio C++ build tools and the Windows SDK. On the first build, Cargo downloads dependencies, including the native sherpa-onnx and ONNX Runtime libraries.

Run these commands from the repository root.

```sh
cargo run --locked
cargo check --locked
cargo test --locked
```

## Rebuild on file changes

On macOS, install Watchexec with Homebrew and start it from the repository root.

```sh
brew install watchexec

watchexec --restart \
  --watch src \
  --watch locales \
  --watch Cargo.toml \
  --watch Cargo.lock \
  --watch build.rs \
  --watch Info.plist \
  -- cargo run --locked
```

Watchexec restarts the app when source files change. Restarting interrupts any active recording. To check the code without launching the app, replace the command after `--` with `cargo check --locked`. Press Ctrl+C to stop watching.

## Project structure

| Path | Purpose |
| --- | --- |
| [src/ui](../src/ui) | Desktop screens and components |
| [src/audio](../src/audio) | Audio recording, preparation, and playback |
| [src/recognition.rs](../src/recognition.rs), [src/diarization.rs](../src/diarization.rs) | Speech recognition and speaker diarization |
| [src/models.rs](../src/models.rs) | Model catalog, downloads, and file verification |
| [src/storage.rs](../src/storage.rs) | Recording and transcript storage |
| [locales](../locales) | English and Russian interface translations |
| [macos/Resources](../macos/Resources) | Dock icon and localized microphone permission prompt |

## Models

S2T runs the models from Rust. T-one uses greedy CTC decoding and carries state across chunks. GigaAM and Parakeet process long recordings in chunks of up to 24 and 60 seconds, respectively.

`SPEECH_MODEL_CACHE` overrides the cache directory. When running from source on macOS, the default is `.model-cache/hub` in the repository root. `SPEECH_MODEL_BASE_URL` sets a mirror that serves files at `/<repo>/resolve/<revision>/<file>`. Model sources and pinned revisions are defined in [src/models.rs](../src/models.rs).

Model repositories are available on Hugging Face. [T-one](https://huggingface.co/t-tech/T-one), [GigaAM-v3 INT8](https://huggingface.co/nocmt/GigaAM-v3-E2E-CTC-int8-ONNX), and [Parakeet v3 INT8](https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8).

## Preview the website locally

The static landing page lives in [docs](.). Start the server from the repository root.

```sh
python3 -m http.server 8000 --bind 127.0.0.1 --directory docs
```

Open [127.0.0.1:8000](http://127.0.0.1:8000). The download buttons get their build information from the latest GitHub release. Run `node --check docs/app.mjs` to check the JavaScript syntax. GitHub Pages deploys when changes in `docs/` reach `main` or when you run the workflow manually. [Website workflow](../.github/workflows/pages.yml)

## Build macOS packages

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./scripts/build-macos.sh
```

The script writes packages to `dist/macos/arm64/` and `dist/macos/x86_64/`, includes localized resources, and applies an ad hoc signature. Use Developer ID signing and notarization for trusted distribution on macOS.

To build both packages before pushing from macOS, enable the Git hook for this clone.

```sh
git config --local core.hooksPath .githooks
```

The hook aborts the push if the build fails. After editing the Dock icon SVG, install `librsvg` and run `./scripts/build-macos-icon.sh` before packaging. [Dock icon SVG](../macos/Assets/Microphone.svg)

## Publish a release

Pushing a tag that matches `v*` triggers the desktop release workflow. It builds ZIP archives for Apple Silicon, Intel Macs, and Windows x64 and attaches them to a GitHub release. [Desktop release workflow](../.github/workflows/macos-release.yml)

Update the version in `Cargo.toml` and `Cargo.lock`, verify the build, and commit the changes. Tag the release commit. The example below uses version `0.4.0`.

```sh
git tag -a v0.4.0 -m "S2T v0.4.0"
git push origin main
git push origin v0.4.0
```
