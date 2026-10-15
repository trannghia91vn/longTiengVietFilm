# Native Host Integration

Thư mục này chứa crate Rust độc lập (library protocol/storage/OCR/review và binary `long-tieng-capture-host`). Helper nhận PNG/timestamp, lưu bền vững và chạy worker OCR để ghi SRT/TXT; app dùng chung library để review rồi tạo WAV. Capture thông minh đang thử nghiệm; xem [báo cáo tối ưu](../../docs/OCR_SMART_CAPTURE.md) cho schema mới, số đo và gate chưa đạt.

Chrome khởi động helper qua Native Messaging dù GUI app đóng. Mốc 2A cung cấp installer dev macOS; tích hợp đóng gói/release app sẽ thực hiện ở mốc sau. Không cần Python hoặc server HTTP.

## Trách nhiệm

- Nhận phiên/crop/timestamp từ service worker theo protocol có version, length prefix, sequence và giới hạn payload.
- Checkpoint metadata và ảnh bằng chứng theo session; OCR worker riêng sau finalize, hủy/chạy tiếp và output theo revision.
- Resolve đúng app data của app Long Tieng qua logic dùng chung; không dựa vào working directory của process.
- Ghi PNG atomically, fsync PNG/directory trước journal, fsync journal và checkpoint trước ACK. Finalize chỉ báo ảnh/timestamp đã lưu, chưa có SRT.
- Log tách khỏi stdout protocol; hỗ trợ queue đầy, crash, retry và hủy theo session.
- Giới hạn extension được phép kết nối theo ID và không cho trang phim gửi lệnh filesystem/shell.

## Build và đăng ký dev macOS

Chạy từ root repo:

```bash
npm run ocr:prepare
npm run native:build
npm run native:test
npm run native:register -- odjbedbbdecdfnngkibilbjnpfnmaein
```

ID trên là unpacked extension đã dùng ở máy phát triển; kiểm tra ID thực trong Chrome nếu clone/di chuyển repo. Đăng ký chỉ cho phép đúng ID truyền vào, không wildcard. Installer không cần admin, copy binary sang app data thay vì trỏ vào build output có thể bị dọn. Đây là bước cấp quyền chạy helper local; cần người dùng xác nhận trước khi agent thực hiện.

Binary/config nằm tại `~/Library/Application Support/com.basushi.longtieng/native-host/`. Manifest host nằm tại `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.basushi.longtieng.capture.json`. Sau build lại helper, đăng ký lại, reload extension để Chrome chạy binary mới. Không sửa ID khi nâng cấp cùng checkout.

## Protocol/storage

Protocol version 1 dùng JSON UTF-8 + length prefix 32-bit native endian. Extension dùng `runtime.connectNative` từ service worker. Request gồm `version`, `requestId`, `operation`; mutations thêm `sessionId`, `messageSeq`, `payloadJson`, `checksum` và `imageBase64` với sample. Checksum SHA-256 của `operation + "\n" + sessionId + "\n" + messageSeq + "\n" + payloadJson` (UTF-8), nối bytes PNG. Giữ chuỗi JSON gốc để tránh khác biệt số thực giữa JavaScript/Rust.

Capture operations: `hello`, `status`, `begin`, `appendSample`, `appendObservations`, `appendEvent`, `finalize`, `openFolder`. Folder chỉ mở session UUID do helper resolve, không nhận đường dẫn/shell tùy ý. 2B thêm `ocrStart`, `ocrStatus`, `ocrCancel`; chỉ session UUID, không đường dẫn engine/model/output do extension gửi. Protocol vẫn version 1; capture sequence/checksum không đổi. Message tối đa 16 MiB, PNG decode tối đa 12 MiB, capture không quá 1920x1080 và kích thước PNG phải khớp ROI. Response nhỏ hơn 1 MiB.

Hello có capability `smart-observations-v1`. Schema 2 gửi batch `appendObservations` gồm 1..5 mẫu, `observationSeq` riêng với sequence PNG/message. Observation phải tham chiếu PNG đã commit cùng group/segment/ROI/size/layout/source. ACK durable/replay như sample; schema 1 vẫn hợp lệ. OCR schema 2 lấy coverage từ observation, nhận dạng đại diện nhóm trước và đọc toàn bộ ảnh giữ lại nếu bất đồng. Không đổi review pointer hoặc cache của phiên raw cũ.

`ackSequence`, `savedSamples`, `savedBytes`, `finalized`, `sessionPath` mô tả prefix đã lưu bền vững. Replay cùng operation/sequence/checksum trả trạng thái hiện tại; checksum xung đột hoặc sequence không liên tục bị từ chối. Lock theo session và lock quota toàn thư viện bằng `fs2`.

`ocr-sessions/<UUID>/session.json` là checkpoint atomically; `journal.ndjson` là nguồn phục hồi; `frames/<sample-sequence-8-digits>.png` giữ ảnh gốc. Manifest có `nativeStore.status` là `collecting|interrupted|captured` và `capturedOnly: true`. `captured` không chứng minh thu đủ tập hoặc OCR chính xác.

Quota ảnh: 2 GiB/session, 10 GiB/thư viện, giữ tối thiểu 1 GiB free. Metadata và file tạm cũng tiêu thụ đĩa thực tế; kiểm tra free space tính trước mỗi PNG. Không tự xóa phiên cũ. Lỗi disk/quota giữ dữ liệu đã ACK và pause thu; reclaim dung lượng hoặc bắt đầu phiên mới, không báo đã finalize khi outbox còn chờ.

Recovery chỉ cắt đuôi journal chưa kết thúc dòng (chưa ACK); record hoàn chỉnh hỏng hoặc ảnh đã commit mất/hỏng báo lỗi, không bỏ qua âm thầm. Quota reservation phục hồi từ dung lượng thực. PNG orphan chưa có record có thể được ghi lại khi replay; không làm mất ảnh đã commit.

Ảnh mẫu thủ công giữ timestamp gốc nhưng không làm mốc monotonic cho stream playback. Khi bắt đầu/tiếp tục thu một phiên đã có ảnh, extension mở segment mới. Điều này tránh lỗi ở ranh giới pause -> play khi timestamp ước lượng đầu playback lệch nhỏ so với ảnh mẫu; bước lùi bất thường trong cùng stream vẫn bị từ chối.

```bash
node browser-extension/scripts/inspect-native.mjs '/path/to/ocr-sessions/<UUID>' browser-extension/tests/results/native-run
```

Script đọc và đối chiếu journal/checksum/ảnh/count/clock; evidence output ignored. Library không phụ thuộc Tauri và có thể dùng lại cho thư viện app ở mốc 3. Test stdio chạy binary trong HOME tạm, không ghi app data thật.

Tham khảo [Chrome Native Messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging) và [kế hoạch OCR](../../docs/OCR_PLAN.md).

## OCR Worker 2B

Installer copy `vision-ocr`, `tesseract`, license và model `ocr-vie-fast` checksum-verified; config `autoOcr: true` chạy worker sau finalize ACK. Vision chỉ được chọn khi OS báo hỗ trợ Việt; Tesseract fallback dùng model trong app data. Worker process group riêng, stdout không đi vào native protocol, stderr ở `diagnostics/ocr-worker.log`.

`ocr.json` tách capture manifest. Observation journal giữ raw candidates, checksum bytes JSON gốc và image hashes; mỗi ảnh checkpoint fsync. Output canonical nằm trong `ocr-revisions/<revision>/`, SRT/TXT alias ở root để mở. Consumer phải verify commit/hash trước dùng. Không xóa ảnh nguồn, không báo đã review/thu đủ tập.

```bash
npm run ocr:process -- <session UUID>
npm run ocr:integration
node browser-extension/scripts/inspect-ocr.mjs '/path/to/session' browser-extension/tests/results/ocr-run
```
