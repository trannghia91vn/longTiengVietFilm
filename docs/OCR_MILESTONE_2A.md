# Mốc 2A: native helper và lưu ảnh/timestamp

Ngày triển khai/nghiệm thu: 2026-10-14, Asia/Ho_Chi_Minh. Helper đã được người dùng cho phép đăng ký/chạy trên Chrome thật. **Gate đường truyền, phục hồi, bài thu ít nhất 5 phút khi GUI đóng và persistence qua Chrome restart đã đạt trên máy này. Có thể chuyển 2B.** Phạm vi tương thích vẫn chỉ là Chrome/macOS và player đã thử dưới đây.

## Phạm vi đã triển khai

- Crate Rust độc lập ở `browser-extension/native-host/`, không phụ thuộc Tauri GUI/Python/server HTTP. Library protocol/storage dùng được ở mốc app library sau này.
- App-data resolver theo `dirs::data_dir()/com.basushi.longtieng`, cùng quy tắc Tauri hiện tại; test identifier với cấu hình app.
- Native Messaging version 1, connection dài hạn ở service worker. Request identity/sequence/checksum, PNG base64, payload cap 16 MiB; response dưới 1 MiB.
- PNG + fsync/directory trước journal, journal fsync/checkpoint trước ACK. Lock phiên và quota toàn thư viện; replay không tạo ảnh trùng; xung đột hoặc ảnh commit hỏng báo lỗi.
- Outbox IndexedDB v2 giữ ảnh/message trước truyền; ACK giải phóng ảnh khỏi queue, giữ metadata và một preview cuối. Giới hạn 128 MiB dữ liệu chưa ACK, có 1 MiB dự phòng control/checkpoint.
- Retry 1/2/4/8/tối đa 10 giây; đầy bộ đệm hoặc helper từ chối lưu thì pause và giữ dữ liệu. Không điều khiển phim hoặc tự resume capture.
- Stop ngừng stream, queue finalize sau dữ liệu đã thu; chỉ hiển thị "Đã lưu ảnh và timestamp" sau finalize ACK và queue đã drain.
- Installer dev macOS cấp user, copy binary/config vào app data, chỉ đăng ký một extension ID cụ thể; không cần admin. Chưa tích hợp installer/binary vào bản release Tauri.
- Recorder có trạng thái helper, pending/saved/bytes, retry, mở thư mục native. Các archive mốc 1 không đổi và vẫn xuất ZIP.

Không có OCR/SRT/sidebar OCR/WAV. Bản ghi ở app data chưa tự xuất hiện trong GUI app vì thư viện OCR thuộc mốc 3. Native manifest `capturedOnly: true` không có nghĩa thu đủ tập hay mọi cue đạt chất lượng.

## Kết quả kiểm tra hiện tại

| Kiểm tra | Kết quả |
| --- | --- |
| `npm run extension:build` | Pass TypeScript strict và build MV3 |
| `npm run extension:test` | 19 test pass: clock/ROI, v1 migration, ACK/outbox race, rollback, buffer quota, cumulative 132 MiB, recovery, checksum và bridge handshake/reconnect |
| `npm run native:build` | Pass Rust release binary |
| `npm run native:test` | 11 unit + 1 integration pass |
| Rust lint | `cargo clippy --all-targets -- -D warnings` pass |
| Binary stdio integration | HOME tạm; origin sai bị từ chối; commit sample, kill trước đọc ACK, restart/replay/finalize; đúng một PNG được giữ |
| Storage fault | Checkpoint failure sau journal fsync vẫn phục hồi sample; tail chưa ACK được cắt; record hoàn chỉnh hỏng/PNG commit hỏng bị chặn |
| Quota/locks | Session quota, total quota, free-space guard và writer lock được test với limits/môi trường tạm; chưa làm đầy ổ đĩa thật |
| `npm run build` | App frontend pass |
| `cargo test` tại `src-tauri` | 28 test pass; backend app không đổi |
| UI preview | Desktop và 390x844 kiểm tra screenshot, không tràn ngang; đây là preview chưa kết nối helper |

## Nghiệm thu Chrome thật

- Chrome 155.0.8059.40, extension/helper 0.2.0, ID `odjbedbbdecdfnngkibilbjnpfnmaein`, macOS 26.6.2 (25G83). URL chính tập 01, iframe `storage.googleapiscdn.com`, video 1280x720, capture 1920x1080, tốc độ 1x. Không mở DevTools trên website.
- Phiên `43d2aeda-7585-4754-b5b2-f63ceebd3f5f`: 4.748 PNG, 580,54 MiB, 4.761 journal messages; finalize ACK, `capturedOnly: true`. Video 11:18.706 -> 23:41.841; elapsed capture 768,300 ms, media span 743,135 ms. Đây là thu một phần tập, không phải toàn tập.
- Clock round-trip median 1 ms, p95 2,80 ms; đây là RTT lấy clock, không phải độ chính xác timing cue. Có pause nguồn 14,998 giây và waiting 10,172 giây; không có ảnh ở bên trong hai khoảng đó (loại 250 ms ranh giới). Các event giữ nguyên timestamp, segment 0 không lùi bất thường. Capture interval median 190,2 ms, p95 208,4 ms; mục tiêu 10 Hz không phải tốc độ thực tế được bảo đảm.
- Đã gửi SIGSTOP đúng helper PID 98015 lúc 14:25:54. Request timeout làm native port ngắt, Chrome kết thúc process cũ và tạo PID 98243. UI quan sát 73 ảnh chờ / 8,7 MiB rồi drain về 0; capture vẫn tiếp tục. Lệnh SIGKILL sau đó trả process không còn tồn tại, không tính là lần kill thành công thứ hai.
- Inspector đối chiếu đủ count, PNG SHA-256, wire checksum, sequence liên tục, byte totals và finalize. Evidence ignored: `browser-extension/tests/results/native-long/summary.json` và crop đầu/giữa/cuối.
- Tập 01 kết thúc rồi website tự chuyển tập 02: extension chốt tập cũ với event `ended`/`capture-stopped`, không nối ảnh tập mới vào phiên cũ.
- GUI đã cài được thoát bằng menu; GUI dev PID 92208 được gửi SIGTERM theo xác nhận riêng. `pgrep -x long-tieng` không còn process lúc 14:38 và sau khoảng thu 5 phút. Phiên GUI đóng `6978a924-3a1e-4f1c-aee3-2d0ee943407e`: playback liên tục 323,042 giây trước lần pause chủ động đầu tiên, 2.037 ảnh segment 0. Sau tua/thu tiếp: tổng 2.092 PNG, 254,13 MiB, 2.103 journal messages, finalize ACK và pending 0. Clock RTT median 1 ms, p95 2,40 ms; sample interval segment 0 median 192,70 ms, p95 207,50 ms.
- Trong phiên GUI đóng, SIGKILL PID 98243 thành công lúc khoảng 14:40:52; Chrome khởi động PID 99639. Recorder vẫn `Đang thu`, pending về 0. Inspector cuối phiên xác nhận không mất PNG đã commit hoặc thêm PNG trùng do replay.
- Chủ động pause 29,600 giây, không có ảnh bên trong khoảng pause (loại 250 ms ranh giới); tua lùi từ 05:22.955 về 05:17.000, event `seeking`/`seeked` tạo segment 2 rồi thu tiếp. Segment có thể không liên tiếp vì mỗi event seek tăng counter; sequence ảnh/message vẫn liên tục.
- Nút Mở thư mục mở đúng `ocr-sessions/6978a924-3a1e-4f1c-aee3-2d0ee943407e/` trong Finder. Thoát Chrome bằng menu Quit; `pgrep` xác nhận Chrome và helper đều không còn. Sau đó inspector đọc lại hai phiên và đối chiếu thành công manifest, toàn bộ journal/checksum/PNG/count. Mở lại Chrome, khôi phục cửa sổ/tab cũ: 6 bản ghi (3 native + 3 legacy) còn, native preview mở được, pending 0, không tự capture.
- Evidence ignored cho bài GUI đóng: `browser-extension/tests/results/native-gui-closed/summary.json`, crop-1/1046/2092 PNG. Crop cuối thấy sub Việt; có một phần nút overlay đỏ của website sát mép dưới/phải ROI. Đường truyền giữ ảnh đúng vùng đã chọn; xử lý overlay và chất lượng nhận chữ thuộc nghiệm thu 2B, không suy ra OCR sạch từ gate 2A.

### Lỗi tìm thấy và sửa trong nghiệm thu

Snapshot lúc pause 08:45.333 và mẫu playback đầu 08:45.279 lệch 54 ms, vượt guard 50 ms và gây pause lưu. Đã tách manual snapshot khỏi mốc monotonic của stream, mở segment mới khi resume/bắt đầu trên phiên có ảnh và thêm regression Rust. Outbox lỗi ban đầu đã replay/drain thành công sau cập nhật, không sửa hoặc xóa dữ liệu. Phiên `6c8c6b42-e370-4fbb-91a1-ce5135fa320a` giữ hai PNG; không coi khoảng bị pause đó là đã thu đủ.

### Kết luận gate

Đạt bài thu GUI đóng >5 phút, vượt 128 MiB tổng phiên, ngắt/kết nối lại helper, replay toàn vẹn, pause/tua, finalize và persistence sau Chrome đóng/mở. Build app/helper/extension và các test tự động đều pass. Chỉ bàn giao ảnh/timestamp; chưa có text OCR/SRT và chưa tuyên bố timing cue 200 ms.

Chưa test disk-full thật, OS crash/power loss, Chrome restart giữa capture, mọi server/fullscreen/buffering hoặc thu cả tập. Các test fixture mô phỏng không thay những case browser/hardware này. Timing vẫn là ước lượng, chưa hiệu chỉnh compositor và chưa OCR để đo cue timing.

## Vận Hành

- Root commands: `npm run native:build`, `npm run native:test`, `npm run native:register -- <extension-id>`; xem [native-host README](../browser-extension/native-host/README.md).
- Dữ liệu macOS: `~/Library/Application Support/com.basushi.longtieng/ocr-sessions/<UUID>/`; helper log: `diagnostics/capture-host.log` trong cùng app data.
- Inspector chỉ đọc nguồn: `node browser-extension/scripts/inspect-native.mjs '/path/to/session' browser-extension/tests/results/native-run`; evidence output ignored, không commit ảnh phim.
- Quota ảnh 2 GiB/session, 10 GiB/library và giữ 1 GiB free. Không xóa dữ liệu tự động; lỗi lưu giữ queue và không giả báo finalize.
- Khi đổi helper build, đăng ký lại và reload extension. Khi di chuyển checkout, kiểm tra extension ID; installer chỉ cho phép ID đã chỉ định.
