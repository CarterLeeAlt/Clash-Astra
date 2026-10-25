# Third-Party Notices

This repository vendors selected third-party Rust crates to ensure reproducible builds and avoid build-time dependence on external Git repositories. Original copyright and license notices are preserved.

## auto-launch

- Original project: auto-launch
- Original author/copyright holder: zzzgydi
- Original repository: https://github.com/zzzgydi/auto-launch
- Vendored commit: 2d94a103ca20652a3baf581ca2c296791c35c09b
- Version used by this project: 0.5.0
- License: MIT
- Local path: src-tauri/vendor-crates/auto-launch
- Local modifications to vendored source: None, unless explicitly recorded below.

## sysproxy-rs

- Original project: sysproxy-rs
- Crate name used by this project: sysproxy
- Original author/copyright holder: zzzgydi
- Original repository: https://github.com/zzzgydi/sysproxy-rs
- Vendored commit: add5bbdf2edfa150f4469d3c3e4f8f2ec6d17fcb
- Version used by this project: 0.3.0
- License: MIT
- Local path: src-tauri/vendor-crates/sysproxy-rs
- Local modifications to vendored source: None, unless explicitly recorded below.

## tao

- Original project: tao
- Original author/copyright holder: Tauri Programme within The Commons Conservancy, The winit contributors
- Original repository: https://github.com/tauri-apps/tao
- Vendored commit: f9f81f2fd761c60abb5b46a6469864562a811c6d (crates.io release 0.16.7)
- Version used by this project: 0.16.7
- License: Apache-2.0
- Local path: src-tauri/vendor-crates/tao (wired in via `[patch.crates-io]` in src-tauri/Cargo.toml)
- Local modifications to vendored source: `src/platform_impl/windows/icon.rs` builds the Windows icon AND mask as a 1-bit-per-pixel, WORD-aligned bitmap (upstream passed one byte per pixel); `Cargo.toml` adds `[lints.rust] warnings = "allow"`. Only the files needed to build were copied. Details: src-tauri/vendor-crates/tao/VENDORED.md.
