# Bản sửa ổn định capture

Cập nhật 14/10/2026. Extension build **0.3.3**; helper/protocol/OCR engine giữ nguyên. Đây là sửa đường capture, không thay detector thông minh, quota hoặc SRT/WAV. Không tự xóa/sửa các bản thu cũ.

## Thay đổi

- Bỏ fingerprint chứa tất cả iframe. `SelectedPlayer` dùng `webNavigation.getAllFrames` cho **tab được chọn** để xác định frame/document và chuỗi cha; không theo dõi điều hướng tab khác.
- Mỗi iframe được liên kết một lần với phần tử DOM qua nonce và phản hồi Chrome có đúng tab/frame/document. Không ghép bằng URL hoặc thứ tự iframe. Probe sau chỉ đọc video và các iframe đã liên kết qua extension messaging.
- Quảng cáo đổi kích thước/thứ tự không đổi fingerprint nếu player thật không đổi. Nếu quảng cáo làm player dịch chuyển thật, capture vẫn tạm dừng để tránh vùng lệch.
- ROI neo theo hình video render, loại letterbox và ánh xạ qua iframe/border/scale/clip. Vẫn lưu ROI tab-normalized cho helper. Player/viewport/clip đổi thì phải chọn lại vùng, không tự resume hoặc bỏ guard.
- Fullscreen nằm trong top layer: dừng duyệt ancestor tại fullscreen root, không áp clipping/transform của trang phía sau. Root HTML overflow được áp theo viewport, không theo bounding box của HTML đã scroll. Preview có viền xanh nét đứt của khung video đã liên kết để đối chiếu.
- Xoay/phối cảnh, video crop `cover` hoặc vị trí letterbox không căn giữa chưa hỗ trợ: fail closed. Iframe thay phần tử/document không tái dùng liên kết cũ. Cần quyền script trên mọi miền của chuỗi iframe.
- Đọc viewport và DPR trước lấy stream ID. Min/max width/height bằng kích thước cùng tỷ lệ viewport, tối đa 1920×1080, thay vì chỉ đặt hai trần độc lập. Guard tỷ lệ vẫn giữ; lỗi có cả kích thước ảnh và viewport để chẩn đoán.
- Content script có version/dispose/liveness để không dùng hook thuộc context Reload đã vô hiệu hóa. Service worker restart dựng lại liên kết capture còn sống; thất bại thì pause, không tạo capture mới.
- Recorder tách **Chờ mẫu đầu tiên**, **Chờ phim phát/tải đệm**, **Đang thu**, **Đã tạm dừng thu**, **Không nhận mẫu mới**, **Chờ lưu** và finalize ACK. Phiên hiện tại trong lịch sử dùng live view, không đợi poll IndexedDB.
- Watchdog mục tiêu 3 giây: video ready/đang phát nhưng không có mẫu mới thì probe lại rồi pause; pause/buffering/seeking không phải stall. Probe timeout 500 ms. Timer không là deadline cứng khi event loop/đĩa bận.

## Quyền và cập nhật

Manifest thêm **optional** `webNavigation`, không mở rộng host permissions. Video top-level không cần quyền này; iframe cần bấm **Liên kết iframe**. Người dùng đã xác nhận và đã cấp được quyền trên Chrome. Không thêm cloud/Python/HTTP helper.

1. Kết thúc phiên đang thu và chờ lưu trước Reload.
2. Đóng recorder cũ, Reload đúng ID `odjbedbbdecdfnngkibilbjnpfnmaein`, dùng `browser-extension/dist` của checkout này. Không gỡ extension để xử lý lỗi cập nhật; giữ IndexedDB/ID.
3. Mở lại từ tab phim. Dòng dưới Long Tieng phải hiện **Thu subtitle · v0.3.3**, lấy từ manifest Chrome đang nạp, không từ helper.
4. Cấp Liên kết iframe nếu chưa có quyền. Lỗi `Only permissions specified in the manifest may be requested` cần kiểm tra manifest/phiên bản đang nạp. UI mới preflight/hướng dẫn rõ, không tự mở rộng quyền hoặc đăng ký lại host.
5. Ổn định scroll/fullscreen/kích thước, pause 0:00, chọn video/vùng đủ hai dòng. Thử **Lưu mọi ảnh** 60 giây trước; kiểm tra cả trạng thái và số ảnh. Video play không chứng minh capture đang ghi.
6. Kết thúc/chờ finalize ACK rồi kiểm tra journal/PNG. Sau bài ngắn ổn định mới tăng 5 phút/cả tập. Không đổi mặc định Smart hoặc tuyên bố tối ưu đạt từ bản sửa này.

Không cần đăng ký lại helper. Native installation và bộ OCR/TTS không thay trong lượt sửa này.

## Kiểm thử

- `npm run extension:test`: **36 pass**. Test chuỗi/document/parent đổi, chu trình/thiếu cha, sender nonce, quảng cáo, hình học lồng/border/scale/letterbox/clip, ROI ngoài video, race chọn player, quyền thiếu/denied/manifest cũ và trạng thái/watchdog. Có regression fullscreen/HTML scroll theo tọa độ phim thật. Test migration/outbox/smart cũ vẫn chạy.
- `npm run extension:build`: thành công, dist manifest 0.3.3 có optional permission đúng. `npm run build` thành công; Rust app **45 pass, 1 ignored**. Không sửa Rust trong bản này.
- Chrome fixture `tests/capture-geometry.html`: production content/DOM/geometry, **transport mock**, iframe localhost khác origin root 127.0.0.1. Qua liên kết/border/letterbox, quảng cáo đổi kích thước/thứ tự không đổi layout, dịch chuyển player được phát hiện, xoay bị chặn và thay iframe bị từ chối. Không gọi đây là nghiệm thu Native Messaging/tabCapture thật.
- Recorder fixture `tests/recorder-smart.html`: start/pause, warning khi video còn 1×; `?navigation=missing|denied|undeclared` kiểm tra quyền mock. Không gửi dữ liệu tới helper thật.
- Chrome phim thật đã xác nhận cấp được quyền. Chọn ROI trên 0.3.1 bị guard tỷ lệ chặn; 0.3.2 sửa constraint stream nhưng ROI vẫn bị chặn. Đọc DOM thật xác nhận iframe `:fullscreen`, viewport 1219×1143; HTML đã scroll 344 px có rect y=-344, bottom=799. Video contain 1280×720 hiển thị khoảng y=229..915. Clipping HTML sai làm cắt vùng sub ở đáy, không phải bằng chứng người dùng chọn sai. 0.3.3 sửa top-layer/root-viewport handling theo dữ liệu này.
- Fixture có nút test native fullscreen, nhưng browser automation trả `TypeError: not granted`; **test native fullscreen fixture chưa đạt**, không bypass browser permission. Pure regression kiểm tra dữ liệu clip/ROI thật đã pass. Các ca geometry fixture thường đã chạy trước refactor clipping cuối; chưa chạy lại toàn bộ ca browser fixture sau refactor. Bài phim thật fullscreen dưới đây xác nhận đường capture chính, không thay mọi ca fixture.

## Bài thu thật 0.3.3

Người dùng xác nhận chọn được vùng và thực hiện thu **Lưu mọi ảnh** trên URL Tokyo Revengers chính. Kiểm tra dữ liệu sau khi phiên đã finalize, không sửa bản thu:

- Session `194fe099-34a9-4ac0-b5fd-9ea942fb1cc5`, schema 1. Trạng thái native `captured`, capture `stopped`; operation cuối `finalize`, ACK 1127.
- **1117 PNG**, **151089463 bytes (144.09 MiB)**; timestamp ảnh đầu 150 ms, cuối 176144 ms, khoảng thu **175.994 giây**. Tổng dung lượng vượt 128 MiB không làm dừng phiên; đây không phải phép đo đỉnh outbox chưa ACK.
- Kiểm tra bằng `scripts/inspect-native.mjs`: sequence message/ảnh liên tục, checksum protocol và SHA-256 từng PNG khớp, số ảnh/dung lượng/journal/ACK khớp manifest, không thiếu ảnh hoặc có PNG dư theo số lượng. Journal kết thúc đầy đủ.
- Mẫu đều thuộc segment 4 sau hai lần tua về 0 trước khi phát. Không có event pause/interrupted/layout-change trong đoạn thu này. Timestamp tăng; khoảng cách mẫu median **190 ms**, p95 **211 ms**, max **218 ms**, **0 khoảng >750 ms**. Tốc độ trung bình **6.34 Hz**, chưa đạt mục tiêu quan sát 10 Hz.
- Clock probe round-trip median **1.5 ms**, p95 **50.10 ms**; đây không phải sai lệch cue hoặc độ trễ durable ACK. Capture stream 1150×1080, viewport geometry 1219×1143, hình video y=229..915 trong fullscreen; ROI giữ nguyên cả phiên.
- Đã xem PNG đầu/giữa/cuối: ảnh đầu đen ở thời điểm mở phim, ảnh cuối không có sub; ảnh giữa hiển thị nguyên dòng phụ đề tiếng Việt đúng vùng. Đây là kiểm tra crop chọn mẫu, chưa phải ground truth mọi câu/timing hoặc xác nhận đủ hai dòng trong mọi cảnh.
- Evidence được sao chép ra `/private/tmp/long-tieng-capture-stability-033/` gồm summary và ba crop; dữ liệu gốc vẫn nằm trong app data `com.basushi.longtieng/ocr-sessions/<UUID>/`.

Một lần thử trước (`fc1817ad-7f76-4ba6-8bc5-27509eeb7f49`, 49 PNG) bị pause khi viewport thật đổi 1219×1143 → 1152×1080 rồi bị gián đoạn do Reload. Giữ sự kiện này trong dữ liệu; không coi là bài thu ổn định hoặc bỏ guard khi viewport đổi.

**Kết luận:** bài thu ngắn yêu cầu 60 giây đã đạt, thực tế gần 3 phút và finalize hợp lệ. Chưa nghiệm thu lại 5 phút/cả tập với 0.3.3, mất ACK/reconnect trên bản này hoặc timing tuyệt đối 200 ms. Chưa đạt mục tiêu tối ưu Smart 70% PNG/50% OCR; raw tiếp tục là mặc định. Bước tiếp theo là bài 5 phút với viewport/ROI ổn định, sau đó mới cả tập; không cần đăng ký lại helper.

Nguồn API: [Chrome webNavigation](https://developer.chrome.com/docs/extensions/reference/api/webNavigation), [permissions](https://developer.chrome.com/docs/extensions/reference/api/permissions), [tabCapture](https://developer.chrome.com/docs/extensions/reference/api/tabCapture). Gọi từ extension, chỉ định tab/frame/document, không nhận lệnh filesystem từ trang phim.
