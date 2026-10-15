# Mốc 2B: OCR local và SRT

Ngày triển khai/kiểm thử: 2026-10-14, Asia/Ho_Chi_Minh. Helper/extension source 0.3.0 đã có pipeline OCR, checkpoint, SRT/TXT và UI trạng thái trong recorder. **Chưa chốt nghiệm thu sản phẩm trên Chrome thật:** đăng ký/cập nhật host đang chờ xác nhận riêng; chất lượng cue trên phim thật còn cần cải thiện. Không tự chuyển mốc 3 từ kết quả fixture.

## Phạm vi

- Giữ nguyên capture/ACK/outbox 2A. `finalize` vẫn chỉ ACK ảnh/timestamp, không giả là SRT xong.
- Sau finalize, host chạy worker process group riêng, không khởi động Tauri GUI. Worker có thể tiếp tục khi native connection/Chrome đóng. Phiên cũ có nút Chạy OCR/Tiếp tục OCR; worker lỗi không xóa ảnh đã ACK.
- macOS hiện tại dùng adapter Swift Apple Vision khi API báo hỗ trợ tiếng Việt; nhận language thực tế `vi-VT`, không hardcode mã được phỏng đoán. Accurate recognition, tắt language correction. Tesseract 5.5.0 + `vie` fast là fallback; không Python, LLM sửa thoại hay API OCR cloud.
- Tesseract build từ source pin version/SHA-256 cùng static Leptonica/libpng; chỉ link thư viện hệ thống. Model 531.275 byte, Apache-2.0, pin commit/checksum; có trong Model Manager. Vision dùng model/framework của OS, ghi revision/system build vào provenance, không coi nó có traineddata checksum giống Tesseract.
- Fallback thử mask chữ có viền và ảnh gốc grayscale đã khoanh dòng, scale 2x. Hai engine không dùng chung calibration confidence; assembly ưu tiên ứng viên Vision đủ bằng chứng thay vì chọn số lớn nhất giữa engine. Giữ mọi candidate/raw text trong journal.
- Ghép cue theo exact text/consensus lân cận, không fuzzy-merge hai lời thoại gần giống nhau. Giữ cue ngắn, câu lặp sau khoảng trống, manual snapshots, segment/seek, uncertainty đầu/cuối. Recapture overlap được deduplicate khi text và khoảng thời gian cùng khớp; overlap khác giữ để review.
- Xuất UTF-8 NFC `subtitles.vi.srt`, `subtitles.vi.txt`, `cues.json`. Tái sử dụng parser/serializer SRT của app và round-trip trước commit. Mọi cue `reviewed: false`; confidence không chứng minh câu đúng.
- Recorder thêm progress, dừng/tiếp tục, preview SRT và trạng thái `Đã lưu SRT · Chưa review`. Không có sidebar/editor OCR/TTS/WAV ở mốc này.

## Storage Và Recovery

```text
ocr-sessions/<UUID>/
  session.json                         # Capture checkpoint 2A, không ghi đè bằng OCR
  journal.ndjson                       # Capture journal 2A
  frames/*.png                         # Giữ toàn bộ ảnh gốc
  ocr.json                             # Progress và commit pointer/hash OCR
  ocr-cancel.json                      # Cancel theo job UUID, không ảnh hưởng job mới
  ocr-work/<observation-fingerprint>/observations.ndjson
  ocr-revisions/<revision>/subtitles.vi.srt
  ocr-revisions/<revision>/subtitles.vi.txt
  ocr-revisions/<revision>/cues.json
  subtitles.vi.srt / subtitles.vi.txt / cues.json
```

Worker lock riêng theo phiên; capture phải finalize và được kiểm tra PNG/journal trước xử lý. Mỗi observation fsync trước cập nhật progress. Checksum dùng bytes JSON gốc qua `RawValue`, không parse/serialize lại số thực để verify. Đuôi chưa kết thúc dòng được truncate; record hoàn chỉnh hỏng fail closed. Replay kiểm tra sequence, image hash và timeline với capture nguồn.

SRT/TXT/cue files ghi atomically + fsync, sau đó mới commit `ocr.json` với revision/hash. Consumer tương lai phải đọc canonical revision được manifest trỏ tới, không chỉ thấy alias ở root rồi suy ra đã xong. Alias root phục vụ mở file; crash trước manifest chưa được báo saved. Revision cũ giữ nguyên. CLI reprocess giữ `previousResult`, dùng checkpoint cũ khi engine/preprocessing không đổi và từ chối ghi đè alias đã bị sửa ngoài OCR.

`state`: `not-started|queued|processing|interrupted|cancelled|error|partial|no-text`. `committed: true` là đã lưu output, **không phải thu đủ tập hoặc đã review**. `no-text` giữ file rỗng và bằng chứng, không tạo cue giả. Coverage chỉ mô tả ranges đã có mẫu; seek/gap giữ riêng, `wholeEpisodeVerified: false`.

## Kết Quả

| Bài kiểm tra | Kết quả |
| --- | --- |
| Native Rust | 25 unit + 1 stdio integration pass; benchmark cần asset/engine được tách thành test ignored |
| Extension | 20 test pass, gồm OCR status validation và các regression 2A |
| App | Frontend build và 28 Rust test pass |
| Rust lint | Clippy all-targets, warnings-as-errors pass |
| Native engine | Tesseract/Leptonica/libpng build portable; Vision adapter compile và probe có Vietnamese trên macOS 26.6.2 build 25G83 |
| Fixture 20 giây, 200 crop | 5/5 cue đúng text, CER 0, không sinh cue dư; câu hai dòng/dấu Việt và câu lặp giữ đúng. Biên mỗi cue lệch 50 ms so với timestamp fixture |
| Cancel/recovery thật | Hủy sau 10 mẫu, chạy tiếp; kill process group worker sau 20 mẫu; thêm đuôi journal dở; restart giữ đúng 200 observation không trùng |
| Host lifecycle | Đóng native host giữa OCR; worker vẫn commit. Mở host lại thấy cùng revision, request start lặp không tạo output khác |
| UI fixture | Desktop/mobile 390x844: status, SRT, progress, hủy và retry kiểm tra qua giao diện. Không tràn ngang; preview rỗng không chiếm màn hình bản thu đã finalize |

Fixture dùng HOME tạm và connector stdio thật, **không phải Chrome thật**. UI fixture dùng mock transport có chủ đích; không thay nghiệm thu extension/host đã cài. Evidence ignored: `browser-extension/tests/results/ocr-integration/report.json`, SRT và crop. Đây chỉ là 5 cue sạch, không chứng minh mục tiêu 95%/200 ms trên phim thật hoặc latency compositor.

### Phiên Phim Thật

Đọc bản sao nguyên vẹn của phiên 2A `6978a924-3a1e-4f1c-aee3-2d0ee943407e` trong HOME tạm, không ghi app data thật. Nguồn chính Tokyo Revengers tập 01; 2.092 PNG, ranges 00:00.013 -> 05:22.911 segment 0 và 05:16.986 -> 05:25.068 segment 2. Manifest/journal/PNG nguồn giữ nguyên.

- Benchmark Tesseract-only ban đầu: 466 cue, 465 nghi vấn. Không chấp nhận output đó làm phụ đề dùng được.
- Vision + fallback: xử lý đầy đủ mất khoảng 470,6 giây trong bài này. Ghi đủ observation và SRT; không cam kết OCR realtime.
- Assembly revision cuối từ cùng checkpoint: 329 cue, 249 cue có warning, còn nhiều split do chữ dao động/dấu và nhiễu. Có sub đọc được, nhưng **chưa đạt gate chất lượng phim thật**, không gọi là SRT hoàn chỉnh để tạo WAV cho trẻ xem.
- Inspector đối chiếu đủ output hashes, count/index/timing/NFC, image hashes và coverage/segment; đây là kiểm tra tính toàn vẹn, không phải đánh giá nghĩa câu. Evidence: `browser-extension/tests/results/ocr-primary/summary.json`, SRT/TXT/cues JSON.
- Chưa có ground truth đủ dài trên website để đo CER/recall/merge/split và cue timing. Crop có overlay của website; confidence engine không thay review thủ công.

## Vận Hành

```bash
npm run ocr:prepare
npm run native:build
npm run native:test
npm run extension:build
npm run extension:test
# Chỉ đăng ký sau xác nhận quyền native của người dùng:
npm run native:register -- odjbedbbdecdfnngkibilbjnpfnmaein
# Cho phiên native đã finalize, chạy đồng bộ bằng CLI nếu cần:
npm run ocr:process -- <UUID>
# Developer: ghép/xử lý lại có revision, không sửa ảnh nguồn:
browser-extension/native-host/target/release/long-tieng-capture-host --ocr-reprocess <UUID>
```

Reload extension sau đăng ký. Thu xong đợi `Đã lưu ảnh và timestamp`; phần OCR có trạng thái riêng, chỉ hiện saved SRT sau `committed`. SRT/TXT trong app data của Long Tieng, không Downloads. App library/editor đọc tự động ở mốc 3 chưa có; không suy ra app GUI đã thấy file từ việc helper ghi cùng thư mục.

`npm run ocr:integration` cần FFmpeg, fixture `tests/media/clock.webm` và engine đã prepare. Apple Vision cần truy cập framework/accelerator của OS; môi trường sandbox kiểm thử có thể cần quyền chạy thích hợp, không phải quyền screen recording mới. Thiếu Vietnamese support thì dùng Tesseract và chất lượng cần đo riêng.

Chưa chạy: bản helper 0.3 trên Chrome thực tế, finalize -> auto OCR -> mở app GUI, thu mới 5 phút với 2B, disk-full/OS power loss thật, release installer ký/notarize, fullscreen/đổi server, thu cả tập, chất lượng toàn tập và timing 200 ms. Cần đăng ký host sau xác nhận và giảm cue vụn/đo ground truth nguồn chính trước khi chốt 2B.

Nguồn API/engine: [Apple VNRecognizeTextRequest](https://developer.apple.com/documentation/vision/vnrecognizetextrequest), [Tesseract models](https://tesseract-ocr.github.io/tessdoc/Data-Files.html), [Tesseract build](https://tesseract-ocr.github.io/tessdoc/Compiling.html).
