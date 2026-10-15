# Agent Guide

Tài liệu này dành cho Codex hoặc lập trình viên sửa dự án Long Tieng. Hãy đọc cùng [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) trước khi đổi pipeline, model, schema dự án hoặc flow UI.

## Dự án là gì

Long Tieng là app desktop Tauri v2 + React/Vite để dịch SRT Anh/Nhật sang tiếng Việt và render audio lồng tiếng Việt cục bộ. Runtime native nằm trong `src-tauri/engines`; model lớn tải về app data qua Model Manager.

Mục tiêu sản phẩm là offline-first, dễ tiếp tục khi job dài bị lỗi, và không yêu cầu người dùng cài Ollama/Python.

## Cấu trúc quan trọng

- `src/App.tsx`: shell, navigation, trạng thái engine/model/memory.
- `src/components/ProjectEditor.tsx`: flow chính tạo dự án, nhập SRT, dịch, tạo giọng, test, export.
- `src/components/OcrWorkspace.tsx`: thư viện bản thu, review cue/ảnh, bản nháp và export SRT/TXT.
- `src/components/OcrAudioPreview.tsx`, `src-tauri/src/audio_project.rs` và `audio_project/full.rs`: snapshot review, WAV thử và toàn timeline, preflight/quota/export không cần video local.
- `src-tauri/src/ocr.rs`: adapter Tauri dùng crate native-host; không sao chép contract lưu trữ OCR.
- `src/components/ModelManager.tsx`: tải/xóa model, progress theo event.
- `src/components/Diagnostics.tsx`: log lỗi, báo cáo chẩn đoán.
- `src/lib/tauri.ts`: wrapper duy nhất cho Tauri commands/events ở frontend.
- `src/types.ts`: contract TypeScript mirror Rust serde camelCase.
- `src-tauri/src/lib.rs`: khai báo Tauri commands.
- `src-tauri/src/project.rs`: schema dự án, project lifecycle, translation memory, long job, TTS, render project.
- `src-tauri/src/pipeline.rs`: media inspection, preview proxy, Whisper, llama-server, dịch SRT, sync, render legacy.
- `src-tauri/src/models.rs`: model catalog, download/resume/checksum/remove.
- `src-tauri/src/srt.rs`: parser/serializer SRT.
- `scripts/prepare-engines-macos.sh`: build/copy native sidecars.
- `browser-extension/`: extension MV3 capture/clock, helper Rust storage 2A, OCR worker 2B và library review dùng chung với app; đọc các báo cáo mốc 1/2A/2B/3 trước khi đổi.

## Lệnh thường dùng

```bash
npm install
./scripts/prepare-engines-macos.sh
npm run build
cd src-tauri && cargo test
cd .. && npm run tauri dev
```

`npm run tauri dev` tự chạy Vite qua cấu hình Tauri. `npm run dev` chỉ chạy frontend preview và nhiều API sẽ dùng mock hoặc báo lỗi vì không có Tauri runtime.

## Invariant cần giữ

- Không đổi `video_path` của dự án đã tạo. `project::update` đang chặn việc này.
- Rust serde dùng `rename_all = "camelCase"`; khi thêm field Rust phải cập nhật `src/types.ts` tương ứng.
- Project lưu atomically qua `project.json.tmp` rồi rename. Đừng ghi trực tiếp project JSON từ frontend.
- Job dài phải checkpoint `last_job`, emit `project-progress`, và tôn trọng `cancel_job`.
- TTS cache phụ thuộc text đã chuẩn hóa đọc, voice, duration, speed, volume và audio settings. Khi đổi logic âm thanh, tăng `CURRENT_TTS_CACHE_VERSION` hoặc đổi cache key.
- SRT Việt nhập sẵn không được ghi vào translation memory. Chỉ lưu correction khi có `source_text`, `machine_translated_text` và người dùng sửa khác bản máy dịch.
- Model download phải giữ resume `.part`, checksum nếu có, lock tải trùng và trạng thái `missing|partial|installed|downloading|invalid`.
- App phải tiếp tục hoạt động khi Mel-Band RoFormer thiếu hoặc tách nền lỗi bằng cách fallback ducking.
- Frontend không gọi `invoke` trực tiếp ngoài `src/lib/tauri.ts` trừ khi có lý do rất rõ.

## Khi sửa UI

- Giữ giao diện là công cụ làm việc dày thông tin, không biến thành landing page.
- Dùng icon từ `lucide-react` cho action; nút có tooltip khi ý nghĩa không hiển nhiên.
- Các bước workflow chính vẫn là: nhập SRT, dịch nếu cần, tạo giọng/test, xuất video.
- Mọi job lâu cần có progress, trạng thái lỗi và đường thoát hủy nếu backend hỗ trợ.

## Khi sửa backend

- Ưu tiên thêm command ở `src-tauri/src/lib.rs`, implement ở module sở hữu domain, rồi expose qua `src/lib/tauri.ts`.
- Nếu command sinh event, thêm type vào `src/types.ts` và đặt tên event ổn định.
- Với process native, dùng helper quản lý timeout/cancel/log hiện có thay vì spawn rời rạc.
- Log lỗi đáng chẩn đoán bằng `diagnostics::append`, kèm source rõ như `tts.batch`, `model.download`, `render.mix`.
- Không đưa model lớn, binary build output, cache hoặc app data vào Git.

## Kiểm thử

Chạy ít nhất:

```bash
npm run build
cd src-tauri && cargo test
```

Mở rộng test Rust khi sửa:

- SRT parsing/serialization.
- Chia scene/utterance group/dịch và giữ ID/timestamp.
- Project migration hoặc schema default.
- Translation memory.
- TTS cache invalidation, resume/cancel/checkpoint.
- Model download state hoặc checksum.

Nếu không thể chạy test vì thiếu engine/model/network, ghi rõ trong kết quả cuối và nêu phần đã kiểm tra được.

Khi sửa extension, chạy thêm `npm run extension:build` và `npm run extension:test`. Kiểm thử capture bằng Chrome thật trên URL chính và fixture iframe; không suy ra browser compatibility chỉ từ unit test. PNG/ZIP kiểm thử không commit; timestamp chưa hiệu chỉnh compositor phải ghi là ước lượng. Giữ scope permission theo frame origin, không tự cấp toàn bộ website.

Khi sửa helper, chạy `npm run native:build` và `npm run native:test`. ACK chỉ sau durable commit; không xóa outbox trước ACK, không trộn sequence truyền với sequence ảnh. Giữ archive IndexedDB v1, UUID/path validation, locks/quota, stdout protocol riêng khỏi log. `native:register` cấp quyền native cho extension ID cụ thể và cần xác nhận người dùng trước khi agent đăng ký/chạy host mới trên Chrome.

Khi sửa OCR, giữ manifest/capture journal 2A tách khỏi `ocr.json`. Chỉ xử lý capture đã finalize, giữ raw candidates/PNG và uncertainty; không dùng LLM tự sửa lời thoại. Consumer dùng revision/hash đã commit, không suy ra hoàn tất từ alias SRT có trên đĩa. Checkpoint checksum trên bytes JSON gốc (`RawValue`), không round-trip float để verify. Chạy `npm run ocr:integration` sau prepare engine/fixture, ghi riêng chất lượng fixture và phim thật; không coi confidence hoặc SRT đúng cú pháp là đã review/thu đủ.

Review dùng `review.json` trỏ đến `review-revisions/<hash>/`, giữ `baseOcr` để worker chạy lại không thay bản sửa. Không sửa alias SRT của worker hoặc capture manifest. Save cần `.review.lock`, expected revision, fsync dữ liệu/thư mục và commit pointer sau cùng; conflict giữ nháp, không retry ghi đè. Engine recheck chỉ trả candidates để người dùng chọn. Đọc [báo cáo mốc 3](docs/OCR_MILESTONE_3.md), chạy `npm run ocr:editor:test` và test crate native khi sửa editor/revision. `LONG_TIENG_OCR_TEST_DATA` chỉ có trong debug và chỉ dành cho QA trên bản sao.

Audio OCR có schema riêng trong `audio-projects/`, không giả `videoPath` và không sửa `DubbingProject`. Chỉ dùng snapshot revision đã commit, kiểm tra review/coverage/biên/overlap ở cả UI và Rust. WAV neo video 0; không trim hoặc bỏ cue để vừa thời gian. Cache gắn text/voice/revision/timing/pipeline/engine-model hash. Job audio dùng UUID/cancel riêng và media gate chung với TTS/render video; không emit nhầm `project-progress`. Commit WAV/report trước manifest, giữ preview cũ khi lỗi/hủy. Chạy `npm run ocr:audio:test`, Rust tests và model thật trên dữ liệu QA; đọc [báo cáo 4A](docs/OCR_MILESTONE_4A.md). Không coi trial 30/60 giây là WAV cả tập hoặc gate timing OCR 200 ms.

4B giữ `TrialSettings.full`/`lastFull` additive và skip field mặc định để JSON/cache 4A cũ vẫn hợp lệ. Toàn timeline phải kiểm tra mọi cue, known duration đúng, coverage union (replay có thể bù gap), giọng override theo ID của snapshot và tất cả lỗi đọc quá dài. `allowGaps` không bỏ review/overlap; bản chưa rõ duration hoặc thiếu coverage phải có `partial: true`, không đổi `wholeEpisodeVerified` thành true. Batch tối đa 8, per-cue commit, `.quota.lock` toàn thư viện, dự trù dung lượng và resource watcher; không tự xóa orphan/cache. Job polling chỉ trả checkpoint, không băm lại WAV lớn. Export một thư mục immutable gồm WAV/SRT/JSON bằng rename atomically; verify source/revision/hash/format/coverage trước khi dùng/xuất. Đọc [báo cáo 4B](docs/OCR_MILESTONE_4B.md), phân biệt full-length synthetic QA với full-film nghiệm thu thật.

Capture thông minh dùng schema 2, capability `smart-observations-v1` và `appendObservations` trên protocol 1. Ảnh phải commit trước observation tham chiếu; sequence ảnh, observation và message là ba bộ đếm riêng. Coverage lấy từ observation, không từ khoảng cách PNG. Không tái sử dụng OCR xuyên group/segment hoặc coi detector trống là silence. Cache recognition gắn engine/pipeline/source hash và checksum bytes gốc. Giữ raw schema 1, review revision và SRT/WAV cũ; không tự xóa bản thu. Chế độ raw vẫn mặc định cho đến khi gate chất lượng/hiệu quả trong [báo cáo tối ưu](docs/OCR_SMART_CAPTURE.md) đạt. Replay chỉ đọc journal/PNG nguồn; dùng app data QA riêng cho importer/worker.

Capture ổn định 0.3.3 chỉ theo dõi video và ancestry đã chọn; không đưa mọi iframe vào fingerprint. Optional `webNavigation` chỉ truy vấn tab mục tiêu, phải được người dùng cấp. Giữ frame/document identity, kiểm tra clip/letterbox, aspect stream và ROI trước ghi; fullscreen không được clip bởi ancestor ngoài top layer/HTML đã scroll. Fail closed khi thiếu liên kết. Không nới guard để che lỗi hoặc gọi fixture transport mock là Chrome capture thật. Đọc [báo cáo ổn định](docs/OCR_CAPTURE_STABILITY.md) trước đổi geometry/permissions.

Bản dùng local 0.3.4 theo yêu cầu bàn giao sớm có PNG lossless repack bằng worker và nhớ chế độ người dùng. Không thay mặc định lần đầu hoặc diễn giải gate Smart chưa đạt thành đạt. PNG nén lỗi/lớn hơn phải giữ ảnh gốc; không thay pixel, hash/checksum phải theo bytes đã gửi. `npm run release:local` chỉ build, không cài/register/reload. Đọc [bản local](docs/OCR_LOCAL_RELEASE.md) khi sửa encoding/build release.
