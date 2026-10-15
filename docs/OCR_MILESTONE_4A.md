# Mốc 4A: OCR → WAV thử

Ngày nghiệm thu local: 2026-10-14. Implementation và bài thử model thật đã đạt phạm vi 4A trên macOS hiện tại. Không cập nhật app trong `/Applications`, đăng ký helper hoặc reload Chrome. Quality/Chrome gate 2B vẫn mở; chưa có WAV cả tập hoặc extension playback.

## Phạm vi

- OCR → mở bản thu → kiểm tra/sửa/lưu revision → tab `WAV thử` → chọn bắt đầu, 30/60 giây, một trong bảy giọng → tạo/nghe/xuất WAV.
- Không yêu cầu video local, không thêm model dịch, Python, HTTP server hoặc dịch vụ cloud.
- Audio có schema riêng, snapshot đã lưu; không dùng `videoPath` giả hoặc sửa schema dự án video.
- Chặn nháp chưa lưu, cue chưa review, vượt biên cửa sổ, overlap, ngoài vùng thu liên tục, quá 64 cue hoặc mốc cuối quá hai giờ. Không tự bỏ cue lỗi.
- PCM16 mono 48 kHz neo video 0: đoạn bắt đầu 85.650 giây có silence trước đó; output 115.650 giây nếu thử 30 giây. Không phải file 30 giây reset timestamp.
- Tái sử dụng managed audio.cpp/VieNeu/SEA-G2P/FFmpeg. Tempo tối đa 1.25×, có 80 ms headroom; audio vẫn vượt slot thì báo lỗi và không publish WAV bị cắt.

## Lưu và khôi phục

`app-data/audio-projects/<hash-session-revision>/` chứa `source.json`, atomic `project.json`, `cache/<key>/`, `previews/<key>/{preview.wav,report.json}` và workspace job. Capture/worker/review files không bị sửa bởi audio job.

Cache gồm checksum raw/processed và fingerprint pipeline/engine/model/voice/backend, text, source revision/hash và timing. Cue chỉ reusable sau fsync/commit metadata. Manifest chỉ trỏ WAV/report sau durable commit, và load kiểm tra lại checksum/format/duration. Hủy/lỗi giữ WAV trước đó; mở lại job chạy nhưng không còn writer lock thì đánh dấu gián đoạn. Retry dùng các cue cache đã hoàn tất, không tự chạy lại job khi mở app.

Media gate dùng chung với TTS/render video và voice preview. `.job.lock` bảo vệ theo dự án audio; `cancel.json` gắn UUID job, không dùng global cancel của video. UI vẫn mounted khi đổi sidebar và chặn thay nguồn khi job chạy. Model thiếu, engine thiếu, cache hỏng hoặc dung lượng không đủ đều báo lỗi; preflight giữ ít nhất 1 GiB trống ngoài ước lượng WAV/cache.

## Bằng chứng model thật

| Bài thử | Kết quả |
| --- | --- |
| Mẫu tổng hợp hai câu tiếng Việt, cửa sổ 30–60 giây | 2 cue mới, elapsed checkpoint 13 s; WAV 60.000 s, 5,760,044 bytes |
| Tạo lại mẫu | 0 mới / 2 cache; elapsed 11 s do fingerprint/preflight vẫn chạy; checksum WAV không đổi |
| Hủy job đổi giọng mẫu | Trạng thái cancelled, giữ nguyên preview Thùy Dung cũ, không publish preview dở |
| Nhận dạng độc lập mẫu bằng Whisper local | Nhận lại đúng hai câu mẫu; không dùng ASR timing làm chuẩn cue hoặc bằng chứng cả tập |
| Phim chính, bản sao capture 2B, đoạn 85.650–115.650 s | 8 cue review thủ công từ crop thật, 8 mới; elapsed checkpoint 18 s; WAV 115.650 s |
| PCM phim | 48 kHz mono PCM16, khoảng lặng đầu/gap/tail đúng; toàn bộ processed PCM của 8 cue nằm tại timestamp nguồn, không bị bỏ/truncate |
| Tempo phim | 6 cue 1.00×; 2 cue 1.09966× / 1.17812×; tất cả vừa slot |
| Native UI | Tạo/phát trong WebView; xuất qua Save dialog; quit/mở lại vẫn thấy cấu hình, metrics, WAV; checksum bản xuất trùng |
| Cache phim sau restart | 0 mới / 8 cache, elapsed 11 s; checksum WAV không đổi |
| UI responsive | Playwright screenshot desktop 1280×720, mobile 390×844; không overflow control/text; audio mẫu loaded, duration 60 s và seek đến 30 s |

Session phim: `6978a924-3a1e-4f1c-aee3-2d0ee943407e`. Review riêng trên bản sao: `d85793f23dadd0336951dbdb4b75c2a28ebf0aae9bdc998b049c6c486dc7c949`. Gộp cue vụn dựa vào crop, sửa dấu/tên theo ảnh, bỏ cue nhận nhầm gáy DVD; không viết lại lời bằng LLM và không sửa bản gốc. Đây là một đoạn đã review, không phải 329 cue gốc tự động đạt chất lượng.

WAV phim SHA-256: `d62ba03782b1f87773c4d98efbb7b39213f6c6a8f281e83f7b142cb64b2bc765`. Bản xuất QA: `/private/tmp/long-tieng-m4a-film-trial.wav`. Dữ liệu QA: `/private/tmp/long-tieng-m4a-film-F3VMQM`; evidence WAV/report dưới ignored `browser-extension/tests/results/ocr-audio/`, không commit ảnh/phim/model/audio vào Git.

## Verification

- App Rust: 39 passed, 1 optional ignored; optional prepare-review phim đã chạy riêng và passed.
- Helper: 32 unit + 1 stdio passed, 2 optional ignored. Extension: 20 passed. Editor: 5 passed. Audio preflight: 5 passed. Tổng default 102 passed.
- App frontend/Tauri debug `.app`, helper release, extension builds passed. App Clippy passed với allow ba nhóm lint baseline cũ (`too_many_arguments`, `unnecessary_map_or`, `needless_lifetimes`); không tuyên bố strict toàn repo sạch lint.
- Rust tests cover zero-anchor/gaps/full sample preservation, overflow/no publish, invalid format/overlap, review/coverage/biên, source snapshots/checksum/revision, locks/interrupted jobs, scoped cancel, export reload/corruption và cache identity.
- Một lỗi thực tế đã sửa: `crypto.randomUUID` thiếu trong WebView làm nút tạo WAV không chạy; UUID hiện được tạo qua command Rust. Launch sandbox ban đầu abort ở macOS `RegisterApplication`; QA native chạy bằng launch đã được phép ngoài sandbox.
- Skill YAML/metadata kiểm tra bằng Ruby Psych; `quick_validate.py` không chạy được do môi trường Python thiếu PyYAML. Không thêm dependency này vào runtime app.

## Chưa nghiệm thu

Chưa review toàn tập, benchmark cả bảy giọng/tên Nhật, đo ground-truth timing OCR 200 ms, chạy full-episode WAV, đồng bộ Chrome, hoặc fault-injection power loss/disk-full thật trong audio pipeline. Unit/atomic checkpoint tests không thay cho các bài này. Preview không tách nhạc/thoại gốc vì không có video local.

Đi tiếp 4B cần preflight toàn timeline và coverage, xử lý/sửa cue quá dài/chồng lấn, quyết định gap/end duration, quota/cache lâu dài, QA full-episode và benchmark memory/disk/time. Đồng thời hoàn tất Chrome 0.3/OCR quality gate 2B trước khi tuyên bố workflow end-to-end đã ổn định. Không cài/cập nhật native host hoặc quyền extension nếu chưa có xác nhận tại bước đó.
