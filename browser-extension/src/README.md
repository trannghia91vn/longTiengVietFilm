# Extension Source

Đây là source TypeScript/CSS của extension mốc 1.

Các module:

- `recorder.tsx`, `recorder.css`: chọn video/vùng, chụp mẫu, start/pause/stop, lịch sử và export.
- `content.ts`: video clock và player events trong frame được cấp quyền; hỗ trợ open shadow roots.
- `service-worker.ts`: bám tab gốc, scan frame, offscreen lifecycle và relay clock.
- `offscreen.ts`: stream video-only, preview/crop PNG, lấy mẫu và bảo vệ khi geometry/clock đổi.
- `core.ts`, `types.ts`: contract, crop normalized và ước lượng timestamp/segment.
- `storage.ts`: checkpoint session/PNG trong IndexedDB và khôi phục phiên ngắt khi kết nối lại.
- `export.ts`: ZIP ảnh + timing/provenance để kiểm tra mốc 1.

Mốc 1 không có ACK native; trạng thái ảnh đã lưu nghĩa là transaction IndexedDB đã hoàn tất. Mốc 2A đã thêm native bridge/outbox, mốc 2B thêm OCR status/start/cancel và preview SRT. Capture finalize không đồng nghĩa OCR đã commit. Playback WAV và app OCR editor/library thuộc mốc sau; xem báo cáo 2A/2B để phân biệt implementation và gate đã đạt.

Chọn vùng/thu bắt đầu trong extension. App desktop sở hữu thư viện SRT và editor. Không thêm code vào một repo extension riêng bên ngoài thư mục `browser-extension/` của dự án này.
