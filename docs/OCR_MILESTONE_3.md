# Mốc 3: Thư viện OCR và Review Trong App

Ngày kiểm tra: 2026-10-14. Đã triển khai workspace và kiểm thử trên macOS hiện tại. Không coi kết quả này là nghiệm thu chất lượng cả tập hoặc Chrome helper 0.3 của mốc 2B.

## Phạm Vi

- Sidebar OCR tự đọc `ocr-sessions` theo app data Tauri. Phiên capture-only vẫn có tên, nguồn, ngày thu và số ảnh; chỉ mở editor khi có revision đã commit.
- Poll 2.5 giây khi tab OCR hiện; trạng thái queued/processing/interrupted/error/cancelled/partial/no-text và progress theo phiên.
- Danh sách cue, tìm kiếm, lọc nghi vấn/chưa kiểm tra/đã kiểm tra; xem SRT hiện tại và OCR gốc, vùng thu và gián đoạn.
- Sửa text/timestamp millisecond, tách/gộp/xóa, undo/redo, đánh dấu review. Sửa text/thời gian reset review flag. Không gộp qua seek segment.
- PNG đối chiếu/phóng to, lựa chọn ảnh bằng chứng, confidence/uncertainty và candidates. OCR lại ảnh chỉ trả candidates; người dùng chọn để đưa vào nháp.
- OCR tiếp tục/chạy lại cả phiên đã finalize; hủy theo UUID/job hiện tại, dùng checkpoint 2B. Bản sửa vẫn được giữ khi worker có kết quả mới.
- Save revision, export UTF-8 SRT/TXT, mở thư mục dữ liệu. Giữ nháp khi chuyển sidebar; đổi phiên/tải lại cần xác nhận bỏ nháp. Conflict không tự ghi đè.

Chưa có dự án audio-only, TTS/WAV từ OCR hoặc playback extension. Chưa có UI duyệt/khôi phục tất cả revision cũ; các revision vẫn được giữ trên đĩa. Nháp chưa lưu không có crash recovery; save là điểm cam kết bền vững.

## Contract Lưu Bản Sửa

```text
ocr-sessions/<UUID>/
  session.json, journal.ndjson, frames/      capture 2A, không sửa
  ocr.json, ocr-revisions/<hash>/           worker 2B, không sửa bởi editor
  review.json                              commit pointer của editor
  .review.lock                             lock lưu bản sửa
  review-revisions/<hash>/
    subtitles.vi.srt
    subtitles.vi.txt
    cues.json
    manifest.json
```

Pointer có schema version, UUID, revision, parent revision, `baseOcr` và SHA-256 của ba output. Mọi đường dẫn nội bộ được dựng từ UUID/hash đã validate, không tin `srtPath` trong manifest. Save validate text/timing/provenance, chuẩn hóa NFC, sắp thời gian/đánh lại index; giữ warning OCR, chỉ thêm `user-edited` với câu đổi text/thời gian. Fsync output và thư mục trước commit pointer atomically. Revision còn dang dở không có pointer là orphan, không tự publish/xóa. File đã commit bị hỏng thì báo lỗi, không rơi về bản OCR gốc để che việc mất bản sửa.

Khóa review tách khóa OCR: đọc/sửa snapshot gốc immutable được khi worker đang chạy; worker không biết và không ghi `review.json`. Pointer pin `baseOcr` cũ, app báo có OCR mới thay vì tự rebase. Reader/export dùng revision/hash đã commit, không alias SRT root. Bước tạo WAV sau này phải lấy revision này làm input/cache identity, không lấy alias của worker.

## Kiểm Thử

- App Rust: 28 test pass; native crate: 32 unit + 1 stdio pass, hai test engine/phim thật mặc định ignored.
- Extension: 20 test pass; editor helpers: 5 test pass. Tổng bộ mặc định: 86 test pass.
- Test phim thật ignored được chạy riêng trên bản sao: 329 cue, hai range/segment; ảnh checksum đúng, sửa/lưu/mở lại, giữ OCR gốc.
- Integration OCR chạy riêng: 200 ảnh, 5/5 câu fixture đúng (CER 0), lượt cuối khoảng 39.9 giây gồm hủy/resume/kill worker/journal tail/đóng host và explicit reprocess cùng revision; canonical không đổi. Timing fixture 50 ms, không suy ra timing cue của phim thật.
- Backend review: stale revision, lock, traversal/symlink, evidence giả/ảnh sửa, invalid timing/SRT block injection, orphan trước commit, output hỏng sau commit, worker có kết quả mới, tách/gộp/xóa và revision rỗng đều pass. Worker không overwrite file canonical đã có nếu bytes khác cùng revision; test hồi quy giữ nguyên bằng chứng cũ.
- Tauri bundle thật: mở bản sao session phim, thấy 329 cue/249 nghi vấn; sửa một câu, review, save, đóng/mở lại vẫn còn text/flag; export SRT qua hộp thoại native và nhận dạng lại ảnh bằng engine packaged. Sau lưu bản cuối: 329 cue/1 reviewed/248 nghi vấn. Đọc dữ liệu thật thấy ba phiên capture-only (2092, 4748, 2 ảnh), không import và không tạo SRT giả.
- UI fixture IAB/Playwright: sửa, đổi sidebar giữ nháp, đổi phiên cảnh báo, save/reload, tách/undo, filter reviewed và zoom ảnh. Kiểm tra screenshot desktop 1280px/mobile 390px; sửa lỗi min-content gây tràn ngang, cuối cùng document width bằng viewport, PNG render có kích thước thực.
- Build frontend, helper release, extension và Tauri debug `.app` pass. Chưa thay app trong `/Applications`, chưa notarize/phát hành.
- Native Clippy `-D warnings` pass. App strict Clippy còn 5 lint có sẵn ở `pipeline.rs`/`project.rs` (too-many-arguments/map-or/lifetimes); chạy với ba nhóm lint baseline này được allow thì pass. Không refactor pipeline ngoài scope.

## Giới Hạn Còn Mở

- Chrome/helper 0.3 chưa được cập nhật/nghiệm thu; cần xác nhận native tại thời điểm cài/chạy. Dữ liệu thu thật vẫn capture-only, không đưa bản QA vào app data thật.
- Phim thật còn cue vụn, nhiễu quảng cáo/chữ trong hình và nhiều câu cần review. Mốc 3 không tăng chất lượng engine hoặc chứng minh đã thu đủ cả tập.
- App-started OCR chạy trong backend app; đóng app có thể ngắt job, cần mở lại/tiếp tục. Worker khởi động từ helper vẫn độc lập GUI như 2B.
- Chưa thử ép crash/power-loss bằng OS trong lúc save editor; test mô phỏng orphan trước pointer và corruption sau commit, cùng restart app thực tế. Không tuyên bố đã test mất điện/disk-full thật cho review.
- Export SRT đã test native; TXT có test contract nhưng chưa thử hộp thoại GUI riêng. Không khẳng định compatibility Windows/Linux.

## Chạy Lại

```bash
npm run build
npm run ocr:editor:test
npm run extension:test
npm run extension:build
npm run native:test
npm run native:build
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run ocr:integration
npm run tauri build -- --debug --bundles app
```

Không cần cài thêm skill/runtime cho người dùng. Root `SKILL.md`/`AGENT.md` đã được cập nhật ownership review và cách kiểm thử; engine vẫn offline/local.
