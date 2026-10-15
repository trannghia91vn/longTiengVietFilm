# Long Tieng Chrome Extension

Thư mục root này tập trung source extension Chrome và helper native tích hợp với app Long Tieng: capture/clock/outbox, worker OCR, sidebar/editor và WAV đã có. Capture thông minh đang thử nghiệm, chưa đạt gate hiệu quả/chất lượng; xem [báo cáo tối ưu](../docs/OCR_SMART_CAPTURE.md). Không coi có SRT/WAV là đã thu đủ phụ đề cả tập.

Nguồn phát triển và nghiệm thu chính: [Tokyo Revengers tập 01](https://animevietsub.nl/phim/tokyo-revengers-a4036/tap-01-75695.html).

## Luồng đã chốt

1. Mở phim trong Chrome, mở extension và chọn vùng phụ đề.
2. Thử nhận một ảnh, bắt đầu thu ở 1x; lấy timestamp từ video trong iframe.
3. Helper local ghi ảnh/timestamp vào app data, kể cả khi GUI app đóng.
4. Kết thúc thu, đợi **Đã lưu ảnh và timestamp** sau finalize ACK. OCR chạy riêng sau đó; **Đã lưu SRT · Chưa review** là trạng thái khác.
5. Mở app -> tab OCR thấy bản ghi; mở xem text/ảnh bằng chứng, chỉnh sửa và chuyển sang TTS.
6. Giai đoạn sau bổ sung chọn WAV và phát đồng bộ vào chính extension này.

## Source và output

Layout hiện tại và phần dành cho mốc tiếp theo:

```text
browser-extension/
|-- manifest.json
|-- src/              # recorder React, offscreen capture, content clock, storage/export
|-- scripts/          # build, test, tạo fixture và kiểm tra archive
|-- native-host/      # crate Rust protocol/storage, binary và test stdio
|-- tests/            # clock/ROI tests và fixture iframe khác origin
`-- dist/             # output build để Load unpacked, không commit
```

Chỉ source/cấu hình/tài liệu nằm ở đây. Ảnh/timestamp và kết quả OCR nằm trong app data; WAV nằm trong audio project của app. Không đưa dữ liệu người dùng vào Git. Engine/model OCR dùng chiến lược đóng gói/cache local, không tải JS chạy từ website phim.

## Contract tích hợp

- Mốc 1 lưu phiên/PNG/metadata trong IndexedDB của extension, xuất ZIP; chưa tạo SRT hoặc ghi vào app data.
- Mốc 2A dùng Chrome Native Messaging; helper có thể chạy khi GUI app đóng. Các phiên mốc 1 giữ nguyên và vẫn xuất ZIP.
- Helper ghi đúng app data của Long Tieng, sở hữu đường dẫn output và ACK sau fsync PNG/journal/checkpoint. Bản 2A chưa có SRT để báo đã lưu phụ đề.
- Extension giữ journal/buffer có quota; retry theo session ID, revision và checksum, không tạo bản ghi trùng.
- App đọc thư viện OCR từ dữ liệu đã commit, không cần người dùng tìm và import SRT từ Downloads.
- Script thu clock phải chạy trong frame player được cấp quyền; chỉ quyền trên `animevietsub.nl` chưa đủ cho iframe khác miền.

Kế hoạch đầy đủ ở [docs/OCR_PLAN.md](../docs/OCR_PLAN.md); khảo sát website và ma trận kiểm thử ở [docs/OCR_SITE_TEST.md](../docs/OCR_SITE_TEST.md). Báo cáo [mốc 1](../docs/OCR_MILESTONE_1.md) và [mốc 2A](../docs/OCR_MILESTONE_2A.md) ghi rõ những gì đã test, chưa test và gate đi tiếp.

## Build và sử dụng mốc 2A

Chạy tại root repo, dùng chung dependencies với app:

```bash
npm install
npm run extension:build
npm run extension:test
npm run native:build
npm run native:test
# Kiểm tra ID thực trong Chrome; đăng ký helper sau khi cho phép chạy binary local:
npm run ocr:prepare
npm run native:register -- odjbedbbdecdfnngkibilbjnpfnmaein
```

Trong Chrome, mở `chrome://extensions`, bật Developer mode, Load unpacked và chọn `browser-extension/dist`. Sau khi đổi code, build lại rồi Reload extension. Giữ nguyên đường dẫn để giữ ID dev; nếu di chuyển/clone repo, kiểm tra ID và đăng ký helper lại. Chỉ cấp quyền đúng extension ID. Source chỉ hỗ trợ installer dev macOS ở 2A, chưa đóng gói native host trong bản release app.

Extension 0.3.3 sửa liên kết iframe/ROI, stream theo viewport và fullscreen không bị clip theo trang đã scroll; xem [báo cáo ổn định capture](../docs/OCR_CAPTURE_STABILITY.md). Helper không đổi. Sau Reload, kiểm tra recorder hiển thị v0.3.3; nếu dùng iframe, bấm **Liên kết iframe** để cấp optional `webNavigation`. Không gỡ extension hoặc đăng ký lại helper chỉ vì cập nhật này. Bài thu thật bản mới còn phải nghiệm thu.

1. Mở URL phim chính, bấm extension Long Tieng trên chính tab đó.
2. Chọn video nếu có nhiều player; khi iframe dùng miền khác, nút Kết nối player yêu cầu quyền cho đúng miền đó.
3. Bấm icon crop rồi kéo vùng trên preview, chừa đủ chiều cao cho sub một và hai dòng. Icon camera lưu một ảnh để kiểm tra trước.
4. Đợi Helper đã kết nối, bấm Thu rồi trở lại phim và Play. Pause phim không sinh ảnh lặp; tua tạo segment/event riêng. Đóng recorder không dừng capture. Chưa cài helper thì không bắt đầu phiên mới.
5. Dùng Kết thúc phiên rồi đợi **Đã lưu ảnh và timestamp**. Trong lúc Chờ lưu, outbox vẫn còn dữ liệu chưa ACK. Mở thư mục cho phiên native; phiên mốc 1 vẫn dùng Xuất ZIP.
6. Với host 0.3 đã cập nhật, OCR chạy riêng sau finalize. Phần OCR / SRT báo progress/lỗi/tiếp tục và preview khi đã commit. `Đã lưu SRT · Chưa review` không có nghĩa đủ tập hoặc chữ đúng. Phiên native cũ có Chạy OCR; chưa cập nhật host thì không dùng lệnh OCR mới.

Phiên native lưu ở `~/Library/Application Support/com.basushi.longtieng/ocr-sessions/<UUID>/`. Xem [native-host README](native-host/README.md) cho framing, journal/recovery, cài đặt và chẩn đoán. Khi helper ngắt, outbox giữ dữ liệu rồi reconnect sau 1/2/4/8/tối đa 10 giây. Hết bộ đệm thì pause thu, không pause phim: cần xem lại khoảng bị bỏ qua trước khi tiếp tục. Sau Chrome restart, mở extension để gửi lại; capture chỉ bắt đầu do thao tác người dùng.

ZIP có `capture.json`, `frames/*.png` và mô tả timing. `estimatedVideoTimeMs` thuộc timeline phim; `clock` giữ time/rate/state gốc, `frameMediaTime` nếu có, RTT và epoch capture. Độ trễ compositor chưa hiệu chỉnh; không coi số RTT nhỏ là bằng chứng timing SRT đã đạt 200 ms.

Capture tối đa 1920x1080, lấy mẫu mục tiêu 10 Hz. Outbox toàn bộ phiên native giới hạn 128 MiB dữ liệu chưa ACK, trong đó dành 1 MiB cho checkpoint/finalize; ảnh đã ACK được giải phóng nên tổng phiên có thể vượt 128 MiB. Giữ một PNG preview cuối mỗi phiên. Helper giữ toàn bộ crop gốc ở 2A: quota 2 GiB/phiên, 10 GiB/thư viện và ít nhất 1 GiB free. Tốc độ/dung lượng thực phụ thuộc ROI/cảnh/máy. Không tự xóa ảnh cũ.

Nếu đổi scroll/fullscreen/kích thước, capture pause và yêu cầu chọn lại vùng. Quota/clock/disk lỗi giữ dữ liệu đã commit và báo lỗi. Chưa hứa thu cả tập hoặc timing dưới 200 ms trước nghiệm thu thực tế; gate 2A là bài thu ít nhất 5 phút và recovery.

## Capture thông minh thử nghiệm

Chọn **Thông minh · thử nghiệm** trước khi Thu hoặc Lưu ảnh mẫu. Helper phải có capability `smart-observations-v1`; helper cũ chỉ dùng **Lưu mọi ảnh**, cần cập nhật/đăng ký lại đúng ID dev để thử smart. Không đổi chế độ giữa phiên. Sau Kết thúc, mở lại extension từ tab phim để kết nối capture cho phiên mới.

Từ 0.3.4, extension nhớ chế độ người dùng đã chọn cho lần mở sau; nếu helper chưa kết nối/không hỗ trợ thì mở bằng Lưu mọi ảnh, không tự capture. Cả hai chế độ nén PNG lossless bằng worker trước khi ghi outbox, giữ nguyên pixel/màu/độ phân giải. Lỗi/timeout nén sẽ lưu PNG canvas gốc, không bỏ ảnh. Build cả app/extension/helper bằng `npm run release:local`; xem [bản local](../docs/OCR_LOCAL_RELEASE.md).

Smart vẫn quan sát mục tiêu 10 Hz, nhưng chỉ mã hóa PNG bằng chứng khi đổi chữ, ổn định, kết thúc nhóm và kiểm tra định kỳ mỗi 2 giây. Chữ ngoài kiểu hỗ trợ hoặc không chắc sẽ fallback lưu mọi mẫu. PNG vẫn màu, không giảm độ phân giải hoặc nén mất dữ liệu. Số lần quan sát, ảnh và fallback hiển thị riêng; bộ đệm ảnh RAM tối đa 16 MiB, outbox chưa ACK vẫn 128 MiB.

Schema 2 tách observation và PNG, vì vậy khoảng cách ảnh lớn không đồng nghĩa thiếu coverage. OCR chỉ chạy sau finalize, tái dùng kết quả trong nhóm đã xác nhận; bất đồng thì đọc toàn bộ ảnh giữ lại và đánh dấu kiểm tra. Bản thu/ZIP/review cũ giữ nguyên. Chưa đạt giảm 70% PNG trên dữ liệu phim đã đo, nên **Lưu mọi ảnh vẫn mặc định**. Lệnh replay, audit và số đo ở [báo cáo](../docs/OCR_SMART_CAPTURE.md).

## Fixture và kiểm tra gói mẫu

Fixture là video 20 giây có đồng hồ/frame number và sub Việt in sẵn. Script tạo fixture cần FFmpeg và `@napi-rs/canvas` ở môi trường dev; không phải dependency runtime của extension.

```bash
node browser-extension/scripts/fixture.mjs /path/to/node-runtime
npm run extension:dev
```

Mở `http://127.0.0.1:1431/tests/fixture.html`; iframe dùng `localhost` để thử khác origin. Chỉ cấp quyền localhost khi kiểm thử. Video có các nút tua, đổi tốc độ và mở tab khác. Có thể xem riêng UI tại `http://127.0.0.1:1431/recorder.html`; ngoài Chrome extension, UI hiển thị chưa kết nối.

```bash
node browser-extension/scripts/inspect-capture.mjs /path/to/capture.zip browser-extension/tests/results/run
```

Script kiểm tra đủ PNG, sequence/count và clock tăng trong từng segment, in thống kê RTT và xuất vài ảnh đối chiếu. Dữ liệu mẫu nằm trong thư mục ignored `tests/results`; không commit PNG phim hoặc ZIP vào Git.
