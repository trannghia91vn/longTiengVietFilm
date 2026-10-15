# Bundled engines

Generated macOS arm64 binaries are placed here by `scripts/prepare-engines-macos.sh` and included as Tauri resources. Binaries are intentionally ignored by Git; release automation must run the preparation script before `tauri build`.

Runtime lookup order is: bundled resources, this development directory, then `PATH`.

OCR 2B adds `tesseract` (static Leptonica/libpng, system-only dylibs) and `vision-ocr` (Swift adapter to macOS Vision). `npm run ocr:prepare` prepares both; the full preparation script calls it too. Model `vie.traineddata` is checksum-verified in ignored dev runtime cache, then installed into `models/ocr-vie-fast` in app data by the dev native-host installer or downloaded through Model Manager. Native host packaging/registration remains a separate dev flow, not automatically installed by a Tauri release bundle.
