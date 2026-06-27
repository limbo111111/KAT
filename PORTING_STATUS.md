# Porting Status

## Current Progress

- Added GitHub Actions workflow (`.github/workflows/termux-build.yml`) for cross-compiling KAT to `aarch64-linux-android`.
- Workflow uses `cargo-ndk` to correctly link with the Android NDK.
- It produces a Termux-compatible `.deb` package containing the compiled binary located at `data/data/com.termux/files/usr/bin/kat`.
- The `.deb` artifact is uploaded to GitHub automatically.

## Next Steps

- Integrate GitHub Action into the main repository completely.
