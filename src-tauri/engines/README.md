# Bundled engines

Generated macOS arm64 binaries are placed here by `scripts/prepare-engines-macos.sh` and included as Tauri resources. Binaries are intentionally ignored by Git; release automation must run the preparation script before `tauri build`.

Runtime lookup order is: bundled resources, this development directory, then `PATH`.
