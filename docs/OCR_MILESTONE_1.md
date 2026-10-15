# Mốc 1: extension capture và video clock

Ngày kiểm thử: 2026-10-14. Baseline macOS 26.6.2, Chrome thông thường, không mở DevTools trên website phim. Extension MV3 local 0.1.0 được người dùng cho phép nạp và kiểm thử.

## Kết quả bàn giao

- Source ở `browser-extension/`, build từ root bằng `npm run extension:build`.
- Mở extension trên tab phim, chọn video/ROI trên preview, snapshot, thu, pause/stop, xem lại lịch sử và xuất ZIP.
- Clock đọc từ video trong frame đã cấp quyền, bám tab/document/video ID ban đầu; không dùng đồng hồ UI hoặc thời gian thực làm timeline phim.
- Offscreen capture chạy độc lập với cửa sổ recorder. PNG và metadata cùng commit trong giao dịch IndexedDB; lỗi clock/layout/storage dừng hoặc pause, không tiếp tục sai âm thầm.
- Capture tối đa 1920x1080, mục tiêu lấy mẫu 10 Hz. Tốc độ thực đo khoảng 6,3 ảnh/giây ở đoạn phim này. Phiên thử giới hạn 128 MiB.
- ZIP gồm PNG, clock gốc, timestamp ước lượng, RTT, segment và event. Đây chưa phải SRT.

Chưa triển khai native helper, OCR, ghép cue/SRT, lưu app data hoặc sidebar OCR. GUI app chưa nhận những bản ghi này. ZIP không thay flow tự xuất hiện trong app đã chốt.

## Bằng chứng chạy thật

Nguồn chính: [Tokyo Revengers tập 01](https://animevietsub.nl/phim/tokyo-revengers-a4036/tap-01-75695.html). Frame player quan sát được là `https://storage.googleapiscdn.com`, video 1280x720, duration clock khoảng 1421,921 giây. Không suy ra hỗ trợ mọi server từ lần chạy này.

| Lượt | Bằng chứng | Kết quả |
| --- | --- | --- |
| Phim chính, bản đầu | Session `4975e8f8-acbb-480d-9b92-6782550ed21d` | 693 PNG, 63,98 MiB; quota bản cũ 64 MiB pause đúng. ROI ban đầu quá thấp, có câu bị cắt chân chữ |
| Fixture iframe khác origin | Session `f04a9e0b-746b-4103-8ae4-ddf179a0535d` | 233 PNG, 21,32 MiB; capture 1920x1080; tua về 2 giây, phát lại, mở tab khác nhưng capture vẫn bám tab gốc |
| Phim chính, ROI cao hơn | Session `a492e753-60b8-46f1-8a41-ed321ac2637f` | 835 PNG, 127,83 MiB; RTT median 1 ms, p95 3,20 ms; hai segment có ảnh, clock tăng trong từng segment |

Ở lượt cuối, timeline thu từ khoảng 06:28 tới 08:07, pause 44,562 giây, tua về 08:02 rồi thu tới 08:34. Đối chiếu metadata có **0 ảnh trong khoảng pause**, bỏ biên 200 ms để tránh race transition. Segment 0 có 634 ảnh; segment 2 có 201 ảnh. Khoảng 132 giây phát được lấy mẫu trước khi quota tự pause; thời gian thực của phiên dài hơn do pause. Không báo đoạn sau quota là đã thu.

Ảnh đầu lượt cuối còn overlay/controls khi player vừa chuyển từ pause sang play. Ảnh giữa cho thấy câu một dòng đầy đủ; vùng chọn giữ được hai dòng, nhưng việc chọn ROI không loại bỏ overlay tự động. Mốc OCR phải đánh dấu ảnh bị che/câu nghi vấn, không tự nhận đây là sub sạch.

Evidence trên máy phát triển:

- ZIP: `~/Downloads/long-tieng-capture-<session-id>.zip`.
- Báo cáo/crop: `browser-extension/tests/results/primary`, `fixture`, `primary-final` (ignored, không commit ảnh phim).
- Ground truth fixture: `browser-extension/tests/fixture.srt`, video 20 giây có timestamp/frame in sẵn, tạo bằng `scripts/fixture.mjs`.
- Kiểm tra archive: `scripts/inspect-capture.mjs` xác nhận đủ PNG, sequence liên tục, clock hữu hạn/tăng trong segment, video ID ổn định. Không đo được cue recall hoặc chất lượng OCR bằng script này.

## Timing

Đối chiếu thủ công ba crop fixture với clock in trong hình:

| Sequence | Clock trong hình (ms) | Timestamp metadata (ms) | Độ lệch (ms) |
| --- | --- | --- | --- |
| 1 | 100 | 162 | +62 |
| 116 | 18800 | 18864 | +64 |
| 233 | 19700 | 19851 | +151 |

Fixture 10 fps nên bản thân clock in sẵn có lượng tử 100 ms. Đây là ba mẫu, không phải đo p95 toàn bộ dataset hoặc timing đầu/cuối cue. `uncertaintyMs` chỉ là cận dưới theo clock/RTT/sample interval; compositor delay chưa được đo đầy đủ. Chưa thể cam kết 95% cue dưới 200 ms.

## Kiểm tra tự động

- `npm run extension:test`: 9 test pass (ROI/clock/discontinuity, atomic save, duplicate rollback, isolation/order, recovery).
- `npm run extension:build`: TypeScript strict và build MV3 pass.
- `npm run build`: app frontend pass.
- UI desktop Chrome và preview 390x844 đã kiểm tra bằng screenshot; preview nhỏ không tràn ngang. Đây không phải chứng nhận Chrome mobile hỗ trợ extension.
- `cargo test` trong `src-tauri`: 28 test pass; backend hiện tại không đổi.
- `npm audit`: 1 high ở dependency gián tiếp `source-map-js` hiện có. Chưa chạy audit fix vì ngoài phạm vi thay đổi milestone.

## Giới hạn và gate tiếp theo

Implementation capture/clock đã hoạt động và có archive kiểm chứng; nghiệm thu dài hạn chưa hoàn tất:

- **Chưa đạt bài thu liên tục 3-5 phút** với ROI cuối: PNG tăng nhanh và quota 128 MiB dừng khoảng 132 giây. Không tăng quota vô hạn; cần helper/backpressure và chọn ảnh bằng change detection trước khi thu cả tập.
- Chưa benchmark OCR/CER/cue recall hoặc compositor delay trên toàn bộ fixture.
- Chưa gây buffering bằng mạng chậm, disk-full, Chrome restart/crash, đổi server/tập, Skip OP/ED, fullscreen/minimize. Unit tests không thay những test browser này.
- Scroll/resize/fullscreen có cơ chế pause và yêu cầu chọn lại ROI; chưa coi toàn bộ ma trận geometry đã pass chỉ từ code.
- `requestVideoFrameCallback` trên video offscreen từng không phát callback: đã thêm fallback có kiểm tra decoded frame; mẫu PNG thực xác nhận không còn preview trắng.
- Reload extension từng làm content script cũ gửi message vào context hết hạn: đã thêm detach timer/listener/frame callback khi context không còn. Người dùng không cần mở DevTools trên trang phim.

Đi tiếp mốc 2 nên bắt đầu bằng helper tối thiểu nhận crop/clock có ACK sau commit, test GUI đóng, xử lý queue/quota rồi OCR đoạn ngắn. Giữ fixture và metadata hiện tại làm regression; chưa hứa thu cả tập/SRT chính xác trước khi benchmark.
