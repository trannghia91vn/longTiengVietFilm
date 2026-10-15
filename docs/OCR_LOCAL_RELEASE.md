# Bản Local Tối Ưu 0.3.4

Cập nhật 14/10/2026. Theo yêu cầu người dùng, bàn giao bản dùng local để sử dụng và sửa lỗi qua phản hồi thực tế, không chặn bàn giao bằng thêm bài thu 5 phút/cả tập. Các mục tiêu 70% PNG/50% OCR và chất lượng biên cũ chưa được xác nhận; không đổi số đo lịch sử thành đạt.

## Dùng Ngay

```bash
npm run release:local
```

Build extension, helper và app macOS. App nằm tại `src-tauri/target/release/bundle/macos/Lồng tiếng.app`, extension tại `browser-extension/dist`. Không tự cài helper, reload Chrome, tải model hay khởi chạy ứng dụng. Xem [release](RELEASE.md) nếu cần chuẩn bị engine lần đầu.

1. Kết thúc phiên đang thu và chờ lưu xong trước khi cập nhật extension.
2. Reload đúng extension dev hiện tại, mở recorder từ tab phim; kiểm tra phiên bản **0.3.4**. Không gỡ extension hay đổi thư mục/ID đã đăng ký.
3. Chọn **Thông minh · thử nghiệm** nếu muốn bỏ ảnh lặp và giảm OCR; lựa chọn được nhớ cho lần mở sau khi helper hỗ trợ. Nếu helper không kết nối/thiếu capability, recorder mở ở **Lưu mọi ảnh**. Không tự bắt đầu thu hoặc nối phiên cũ khi Chrome restart.
4. Chọn vùng đủ hai dòng phụ đề, thu phim, kết thúc và đợi lưu. Mở `.app` → OCR để xử lý, kiểm tra/sửa SRT rồi tạo WAV như trước.
5. Nếu Smart có kết quả không phù hợp, kết thúc phiên và chọn **Lưu mọi ảnh** cho phiên mới. Hai chế độ đều hưởng nén PNG lossless mới.

Không cần đăng ký lại helper cho bản này: Rust, protocol, capability, quota và bộ OCR giữ nguyên. Chỉ extension thay đổi đường encoding/polling và nhớ lựa chọn. Không xóa hay nén lại dữ liệu người dùng đã thu.

## Tối Ưu Mới

- PNG canvas được repack phần IDAT bằng fflate level 6 trong worker riêng. Giữ nguyên toàn bộ bytes scanline đã lọc, header, metadata, màu/alpha, kích thước và độ phân giải. Không đổi sang JPEG/WebP, resize hoặc nhị phân hóa ảnh lưu.
- Chỉ thay PNG khi nhỏ hơn. Ảnh dưới 4 KiB bỏ qua; ảnh quá 16 MiB/PNG không hỗ trợ, lỗi worker hoặc quá 1.5 giây đều dùng PNG canvas gốc, không bỏ mẫu. Chỉ một tác vụ encoding chờ tại một thời điểm, không thêm hàng đợi toàn tập.
- Hash/checksum và dung lượng outbox tính trên PNG thực sự được lưu/gửi. ACK/durable commit và giới hạn 128 MiB chưa ACK giữ nguyên. Không sửa schema hay pipeline/cache của dữ liệu cũ.
- Poll capture mỗi 25 ms nhưng vẫn giữ guard khoảng lấy mẫu 100 ms và không chạy đồng thời hai record. Tick bận không còn phải đợi thêm một chu kỳ poll 100 ms đầy đủ. Chưa có số đo Chrome live cho nhịp mới; không cam kết đạt 10 Hz trên mọi ROI/máy.
- Chế độ lựa chọn chỉ được nhớ sau khi command thành công; lần mở đầu vẫn raw. Không biến Smart thành chế độ đã qua gate chất lượng. Detector và chiến lược OCR theo nhóm không nới tiêu chí so chữ để đổi chất lượng lấy tiết kiệm.

## Số Đo

Replay read-only toàn bộ phiên raw thật `194fe099-34a9-4ac0-b5fd-9ea942fb1cc5` (1117 PNG, gần 3 phút) trên Node:

| Chỉ số | Kết quả |
| --- | --- |
| PNG trước | 151089463 bytes / 144.09 MiB |
| PNG sau repack | 118391052 bytes / 112.91 MiB |
| Giảm dung lượng | **21.64%** |
| Nén median / p95 | **17.00 / 24.86 ms** mỗi ảnh |
| Tổng thời gian nén | **19.17 giây** |
| Bytes pixel đã lọc và header | Tất cả giống hệt nguồn |
| Dữ liệu nguồn | Chỉ đọc, không thay đổi |

Đây là lợi ích nén lossless bổ sung, **không phải tỷ lệ giảm Smart**. Repack PNG không giảm số lượt OCR. Tỷ lệ Smart phụ thuộc nét chữ/ROI và có thể fallback; số đo cũ trong [báo cáo Smart](OCR_SMART_CAPTURE.md) vẫn chưa đạt gate. Thời gian trên Node không phải FPS/RAM/latency đo trên Chrome; compression có thêm chi phí CPU mặc dù chạy trong worker.

Chạy lại phép đo mà không sửa nguồn:

```bash
node browser-extension/scripts/png-benchmark.mjs /absolute/finalized-session
```

## Kiểm Tra Bản Build

- Extension: 39 tests pass, gồm kiểm tra lossless RGB/RGBA/metadata/CRC, không tăng bytes, dữ liệu sai/truncated/oversize và fallback khi worker lỗi.
- Extension build tạo file worker riêng dưới `dist/assets/`; không cần quyền Chrome mới.
- `npm run release:local` thành công: extension 0.3.4, helper release và Tauri `.app`; frontend build thành công. `codesign --verify --deep --strict` xác nhận app hợp lệ trên đĩa. App ký ad-hoc local, chưa notarize/phát hành cho máy khác; chưa khởi chạy app đóng gói trong lượt này.
- Editor **5 pass**, audio **7 pass**, `git diff --check` sạch. Không sửa Rust/protocol nên không chạy lại toàn bộ Rust tests trong lượt này; app Rust đã có 45 pass/1 ignored ở lượt sửa 0.3.3. Không yêu cầu người dùng chạy thêm bài capture trước khi dùng bản local.
- Bundle có Tesseract/Vision/AudioCPP/SEA-G2P/FFmpeg/FFprobe và license tương ứng. **Chưa có whisper-cli, llama-cli, llama-server** trong nguồn/bundle hiện tại; bản build phục vụ flow OCR phụ đề Việt → WAV, chưa đủ tính năng nhận dạng âm thanh/dịch AI độc lập. `release:local` kiểm tra engine OCR/WAV bắt buộc và cảnh báo ba engine này; muốn các chức năng đó cần chuẩn bị engine theo [release](RELEASE.md). Các model lớn vẫn tải trong Model Manager, không kèm `.app`.
- Chưa thử worker mới bên trong extension Chrome thật, restore lựa chọn qua Chrome restart hoặc bài thu trọn tập 0.3.4. Khi có lỗi thực tế, thu thập trạng thái recorder/session và sửa tiếp; không gọi các case chưa chạy là đã đạt.
