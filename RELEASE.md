# recorder_mic 1.0

Windows x64 portable recorder based on Eco, with direct streaming MP3 recording using `rusty_mp3` 1.1.0. Requires Microsoft Edge WebView2 Runtime. Application source, Cargo.lock, build instructions and license notices are included in the repository and GitHub-generated source archives.

## Download

- Asset: **recorder_mic.exe** — the UPX-compressed executable.
- Size: **2,149,376 bytes** (about 2.05 MiB); original uncompressed build: 9,395,712 bytes.
- SHA-256: `458d79fc50a12620cec955ed4a01fbdabb7f4f3291bc41c1fc9a93c5c2c3642e`.
- Compression tool: official UPX 5.3.0.

## Validation and limitations

- UPX internal integrity test (`upx -t recorder_mic.exe`) passed (exit 0), including a fresh test of the supplied compressed artifact before publication.
- Earlier local reports record successful decompression and offline synthetic MP3 self-tests for the original and compressed executables, plus an independent decoder check. Those earlier tests were not rerun as part of release publication.
- **Byte-for-byte equivalence between the UPX-decompressed file and the original executable was not confirmed**: the earlier comparison reported a mismatch. Passing UPX integrity tests does not establish that equivalence.
- **The live GUI, microphone capture and real audio-device operation have not been tested for this release.** Offline codec tests are not end-to-end recording validation.
- UPX may cause antivirus false positives or compatibility issues. No security audit, antivirus clearance, perceptual parity with other MP3 encoders or reproducible-build guarantee is claimed.
- The internal application/package versions remain those of the supplied build; `v1.0` is the GitHub distribution tag, not a claim that the executable was rebuilt with a new internal version.

## Attribution

Based on [Eco / EndingVE](https://github.com/EndingVE/Eco-Windows-Recorder), MIT, Copyright (c) 2026 EndingVE. MP3 encoding: [rusty_mp3 / Remade With Rust](https://github.com/Remade-With-Rust/remade_ffmpeg_rs/tree/main/crates/rusty_mp3), Apache-2.0. Compression: [UPX](https://github.com/upx/upx), GPL-2.0-or-later with its special exception for compressed executables. See `LICENSE`, `LICENSE-CODEC.txt`, `THIRD-PARTY-NOTICES.md` and `licenses/` in the source archive.
