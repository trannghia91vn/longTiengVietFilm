# Long Tieng

Ứng dụng desktop chạy local để dịch phụ đề và lồng tiếng Việt cho phim nguồn tiếng Nhật hoặc tiếng Anh. App dùng Tauri v2, React/Vite và các engine native đóng gói trong `src-tauri/engines`; model lớn được tải riêng trong Model Manager và lưu ngoài installer.

## Tính năng chính

- Tạo dự án từ video, lưu tự động trong app data dưới dạng `projects/<id>/project.json`.
- Nhập SRT tiếng Việt để bỏ qua dịch, hoặc nhập SRT tiếng Anh/Nhật rồi dịch sang tiếng Việt.
- Chuẩn bị ngữ cảnh toàn phim bằng Qwen3 trước khi dịch: tóm tắt cảnh, sắc thái, quan hệ nhân vật và quy tắc xưng hô.
- Dịch nghĩa bằng Hy-MT2, sau đó biên tập lời thoại bằng Qwen3 theo phong cách dự án, glossary, nhân vật và bộ nhớ văn phong.
- Chỉnh cue trên timeline: text, timestamp, nhân vật, tốc độ và âm lượng; tạo lại audio cho một cue mà không render lại toàn phim.
- Tạo giọng Việt bằng VieNeu-TTS v3 Turbo qua `audio.cpp`, gồm bảy preset nam/nữ và cache theo nội dung, giọng, timestamp, tốc độ, âm lượng.
- Test 5 phút trong app trước khi render toàn bộ.
- Xuất MP4 lồng tiếng hoặc MKV nhiều track, có fallback ducking khi tách nền bằng Mel-Band RoFormer không khả dụng.
- Model Manager tải model có resume `.part`, kiểm checksum và chạy offline sau khi cài xong.
- Nhật ký chẩn đoán lưu tối đa 300 sự kiện và có thể xuất báo cáo TXT.
- Mục OCR tự đọc bản thu native trong app data: xem/sửa câu và timestamp, đối chiếu ảnh, lọc nghi vấn, tách/gộp/xóa, hoàn tác, lưu revision riêng và xuất SRT/TXT. Không cần import từ Downloads.

Không cần Ollama hoặc Python để chạy app. Các runtime native được đóng gói cùng ứng dụng; model AI tải lần đầu qua Model Manager.

## Quy trình sử dụng

1. Mở tab Model và tải các model cần dùng.
2. Tạo dự án mới từ video tiếng Nhật hoặc tiếng Anh.
3. Nhập SRT đầy đủ. Nếu SRT đã là tiếng Việt, app chuyển thẳng sang tạo giọng.
4. Với SRT Anh/Nhật, bấm dịch sang Việt để app đọc toàn bộ kịch bản, dịch nghĩa và biên tập lời thoại.
5. Gán nhân vật, giọng preset, glossary và sửa lời thoại nếu cần.
6. Chạy Test 5 phút, nghe lại rồi tạo giọng cho toàn bộ cue.
7. Xuất video MP4 hoặc MKV.

## Model mặc định

| Vai trò | Model |
| --- | --- |
| ASR | Whisper large-v3-turbo Q5 + Silero VAD |
| Dịch nghĩa | Hy-MT2 7B Q4_K_M |
| Biên tập lời thoại | Qwen3 14B Q4_K_M |
| TTS | VieNeu-TTS v3 Turbo Q8, 48 kHz |
| Tách vocal | Mel-Band RoFormer Q8 |

## Phát triển

Yêu cầu chính: Node.js, npm, Rust stable, Tauri prerequisites, CMake, toolchain build native macOS và Xcode/Command Line Tools.

```bash
npm install
./scripts/prepare-engines-macos.sh
npm run tauri dev
```

Kiểm tra trước khi đóng gói:

```bash
npm run build
cd src-tauri && cargo test
cd .. && npm run tauri build
```

`scripts/prepare-engines-macos.sh` build/copy `whisper-cli`, `llama-cli`, `llama-server`, `audiocpp_cli`, FFmpeg, FFprobe và SEA-G2P vào `src-tauri/engines`. Script ưu tiên Metal khi có compiler đầy đủ và tự fallback CPU cho `audio.cpp` khi máy chỉ có Command Line Tools.

## Tài liệu

- [AGENT.md](AGENT.md): hướng dẫn cho agent hoặc lập trình viên khi sửa repo.
- [SKILL.md](SKILL.md): skill entrypoint để Codex hiểu cách làm việc với dự án này.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): kiến trúc frontend/backend, app data và pipeline.
- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md): setup, lệnh kiểm tra, quy ước sửa code và xử lý lỗi thường gặp.
- [docs/RELEASE.md](docs/RELEASE.md): chuẩn bị engine, model, đóng gói và checklist phát hành.
- [docs/OCR_PLAN.md](docs/OCR_PLAN.md): kế hoạch extension thu sub, thư viện OCR/SRT và WAV không cần video local; capture/storage 2A đã đạt gate, OCR 2B đang kiểm chứng chất lượng.
- [docs/OCR_MILESTONE_1.md](docs/OCR_MILESTONE_1.md): kết quả thu ảnh/timestamp bằng extension thật và các giới hạn chưa nghiệm thu.
- [docs/OCR_MILESTONE_2A.md](docs/OCR_MILESTONE_2A.md): helper/outbox lưu vào app data, test recovery và gate nghiệm thu Chrome trước OCR.
- [docs/OCR_MILESTONE_2B.md](docs/OCR_MILESTONE_2B.md): worker OCR, SRT/revision/checkpoint, số đo fixture/phim thật và phần chưa nghiệm thu.
- [docs/OCR_MILESTONE_3.md](docs/OCR_MILESTONE_3.md): thư viện OCR/editor trong app, bản sửa bền vững và kết quả nghiệm thu.
- [docs/OCR_MILESTONE_4A.md](docs/OCR_MILESTONE_4A.md): tab OCR → WAV thử 30/60 giây từ revision đã kiểm tra, không cần video local; cache, hủy, nghe và xuất WAV. WAV cả tập thuộc 4B.
- [docs/OCR_MILESTONE_4B.md](docs/OCR_MILESTONE_4B.md): OCR → WAV → Toàn tập, preflight mọi câu/coverage/dung lượng, giọng riêng, resume và bộ WAV/SRT/metadata. Đã có QA track dài bằng model thật; chưa xác nhận đủ phụ đề toàn tập nguồn chính.
- [docs/OCR_SITE_TEST.md](docs/OCR_SITE_TEST.md): khảo sát và ma trận kiểm thử website phim chính.
- [docs/OCR_SMART_CAPTURE.md](docs/OCR_SMART_CAPTURE.md): capture/OCR thông minh thử nghiệm, schema observation riêng, số đo replay và các gate chưa đạt. Chế độ lưu mọi ảnh vẫn mặc định lần đầu.
- [docs/OCR_LOCAL_RELEASE.md](docs/OCR_LOCAL_RELEASE.md): bản dùng local 0.3.4, nén PNG lossless, nhớ chế độ và build app/extension/helper bằng `npm run release:local`.
- [docs/OCR_CAPTURE_STABILITY.md](docs/OCR_CAPTURE_STABILITY.md): extension 0.3.3 liên kết đúng chuỗi iframe, ROI/fullscreen, stream theo viewport và trạng thái/watchdog thu; hướng dẫn cập nhật và gate Chrome còn mở.
- [browser-extension/README.md](browser-extension/README.md): nơi tập trung source extension và helper tích hợp ngay trong root repo.
