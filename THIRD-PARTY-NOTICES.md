# Third-party notices

## Eco upstream

This application is a modified fork of [Eco / Eco-Windows-Recorder by EndingVE](https://github.com/EndingVE/Eco-Windows-Recorder).
Copyright (c) 2026 EndingVE. Licensed under MIT; full original text is preserved in `LICENSE`.
Fork changes include the recorder_mic identity, microphone-first defaults and direct streaming MP3 recording with an application-side encoder adapter. This is not an official Eco release.

## MP3 encoder

`rusty_mp3` 1.1.0, Copyright Mata Network and contributors / Remade With Rust.
Upstream: https://github.com/Remade-With-Rust/remade_ffmpeg_rs/tree/main/crates/rusty_mp3
License: Apache License, Version 2.0. Full text: `LICENSE-CODEC.txt` and `third-party/rusty_mp3-1.1.0/LICENSE`.
Library source is supplied in `third-party/rusty_mp3-1.1.0/`; no library modifications are claimed. The encoder is statically linked and uses the public VBR API. FFmpeg, LAME and Shine are not linked into this build; FFmpeg was used separately for earlier offline decoder checks only.

## UPX

The release executable is compressed with official, unmodified UPX 5.3.0.
Project/source: https://github.com/upx/upx
Version: https://github.com/upx/upx/releases/tag/v5.3.0
UPX authors include Markus F. X. J. Oberhumer, Laszlo Molnar and John Reiser; all original copyright notices remain applicable.
UPX is GPL-2.0-or-later with a special exception permitting distribution of compressed executables under the conditions in its license. Full upstream license, exception and GPL text are preserved in `licenses/UPX-LICENSE.txt` and `licenses/UPX-COPYING.txt`. The UPX tool itself is not redistributed as a release asset.

## Other components

Other Rust dependencies are pinned in `src-tauri/Cargo.lock` and retain their own licenses. Windows SDK, MSVC and WebView2 are Microsoft components and are not redistributed here. No complete independent security or license audit is claimed.
