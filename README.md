<img src="src-tauri/icons/128x128.png" alt="recorder_mic" width="96">

# recorder_mic

A lightweight Windows microphone recorder with direct MP3 recording, based on [Eco by EndingVE](https://github.com/EndingVE/Eco-Windows-Recorder).

This fork adds direct, streaming MP3 encoding through `rusty_mp3`, microphone-first defaults and a separate application identity. The audio engine also supports WASAPI system-audio loopback and WAV output. The interface uses Rust, Tauri 2 and static HTML/CSS/JavaScript; no Node.js build is required.

## Download

Download **recorder_mic.exe** from [release 1.0](https://github.com/desktopy/recorder_mic/releases/tag/v1.0). This is the **UPX-compressed** Windows x64 executable, not an installer. It requires Microsoft Edge WebView2 Runtime (normally available on Windows 11). No external FFmpeg, LAME or codec DLL is required by the application.

The release executable is 2,149,376 bytes (about 2.05 MiB). See [RELEASE.md](RELEASE.md) for validation limits and the SHA-256 checksum. UPX can trigger antivirus false positives or compatibility problems; the release is not claimed to be antivirus-certified.

## Recording

Choose the microphone and output format before starting. MP3 is encoded directly while recording; it does not require a temporary WAV file or a post-record conversion step. Existing WAV recordings remain supported. Tray controls, configurable global shortcuts, recording library and optional system-audio capture are inherited from Eco.

Recordings and settings are user data; neither is included in this repository. Settings are stored under the application's Windows AppData directory. Select your recording destination in the UI. Live microphone recording and the GUI have **not** been verified for this release; do not treat offline encoder tests as end-to-end audio-device validation.

## Build from source (Windows x64)

Install:

- Rust with the `x86_64-pc-windows-msvc` toolchain. The supplied local build reports used Rust 1.100.0; the old package `rust-version = 1.77` is not sufficient for all locked dependencies (`rusty_mp3` requires at least 1.85).
- Visual Studio 2022 Build Tools, Desktop development with C++, MSVC x64 and Windows SDK.
- Microsoft Edge WebView2 Runtime for running the GUI.

From **Developer PowerShell for VS 2022**:

```powershell
cd src-tauri
cargo build --release --locked --bin recorder_mic
```

Output: `src-tauri/target/release/recorder_mic.exe` (uncompressed). Cargo uses `src-tauri/Cargo.lock`; crates.io access or an existing Cargo cache is required. The exact local path dependency is supplied in `third-party/rusty_mp3-1.1.0/`.

The optional helper `powershell -NoProfile -ExecutionPolicy Bypass -File tools/build.ps1 build` assumes the default VS 2022 Build Tools installation. To run audio-engine unit tests, use `cargo test --release --locked -p eco-audio --lib` from `src-tauri`.

To produce a compressed executable without overwriting the normal build, use the official, unmodified [UPX 5.3.0](https://github.com/upx/upx/releases/tag/v5.3.0):

```powershell
New-Item -ItemType Directory -Path release-staging
Copy-Item src-tauri/target/release/recorder_mic.exe release-staging/recorder_mic.exe
upx --best release-staging/recorder_mic.exe
upx -t release-staging/recorder_mic.exe
```

These are build instructions, not a guarantee of bit-for-bit reproduction of the prebuilt asset. Release 1.0 is a distribution label; the original internal package/config versions are preserved to avoid implying the supplied binary was rebuilt.

## Layout and licensing

- `src-tauri/`: application, audio engine and Cargo lockfile.
- `ui/`: embedded static interface.
- `third-party/rusty_mp3-1.1.0/`: vendored MP3 library source and Apache-2.0 license.
- `dependencies/eco5/`: previously published dependency documentation, retained unchanged; the buildable application is now at the repository root.
- `LICENSE`: original Eco MIT license, Copyright (c) 2026 EndingVE.
- `LICENSE-CODEC.txt`, `THIRD-PARTY-NOTICES.md`, `licenses/`: codec and UPX notices/licenses.

This fork is not an official upstream Eco release. Third-party dependencies retain their respective licenses; no complete dependency security or license audit is claimed.
