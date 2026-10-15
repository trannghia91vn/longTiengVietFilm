# Mốc 4B: WAV Toàn Timeline

Ngày triển khai/QA local: 2026-10-14. Tính năng 4B đã có trong app; QA kỹ thuật dùng model thật, source tổng hợp dài như một tập. **Chưa nghiệm thu một tập phim nguồn chính đủ phụ đề và review**, chưa phát đồng bộ trong extension. Không cập nhật `/Applications`, helper đăng ký hoặc quyền Chrome.

## Luồng và Giới Hạn

OCR → mở bản thu → kiểm tra/sửa/lưu → WAV → Toàn tập → chọn giọng mặc định/giọng từng câu → preflight → tạo/nghe → Xuất bộ WAV. Trial 30/60 giây vẫn còn và giữ artifact riêng.

- Snapshot immutable, không cần video local. Mọi cue phải reviewed, readable, timestamp hợp lệ, không overlap hoặc trùng ID; không bỏ cue ngoài cửa sổ như trial.
- Known duration phải đúng duration của video. Unknown duration cần nhập mốc cuối và xác nhận Một phần; không giả thời lượng cả tập.
- Coverage union có thể bù gap bằng đoạn thu lại; tolerance 250 ms ở mép lấy mẫu. Thiếu đoạn lớn cần xác nhận Một phần; silence ở đó không được coi là đã thu đủ thoại.
- Tối đa hai giờ và 10.000 cue. Batch tối đa tám câu, nhóm voice preset; per-cue fsync/checkpoint/cache. Không tự nhận diện nhân vật hoặc đổi lời thoại bằng LLM.
- PCM16 mono 48 kHz từ video 0 đến mốc cuối, silence trước/giữa/sau. Tempo tối đa 1.25×; full job báo mọi cue quá dài và không publish WAV nếu có câu không vừa slot.
- Quota audio riêng: 2 GiB/project, 10 GiB/library, chừa 1 GiB ổ trống. Dự trù trước, check quanh batch/timeline và resource watcher khoảng 500 ms giữa các lần scan; cue/workspace WAV giới hạn 16 MiB. Đây không phải OS hard quota, có thể overshoot tạm giữa scan. Không tự xóa file/cache/orphan; có nút mở thư mục audio.

## Contract và Phục Hồi

Schema audio project vẫn version 1 với `lastFull` và `TrialSettings.full` optional. Default field được skip khi serialize để settings/hash 4A cũ không đổi; không migrate schema dự án video. Cue override gắn ID trong revision, không tự áp sang revision mới sau reindex.

Trial/full cùng `.job.lock`, media gate và UUID cancellation; toàn thư viện audio có `.quota.lock` giữa process. Hủy/lỗi giữ artifact cũ. Crash khi chưa commit manifest để lại output tạm, không báo thành công; reopen nhận trạng thái interrupted rồi người dùng tiếp tục. Quota watcher đưa lỗi resource thành failed, không nhầm với người dùng hủy.

Polling `get_ocr_audio_job` chỉ đọc checkpoint/source. Load/open/export vẫn verify waveform và SRT/report; không băm lại WAV lớn ở mỗi poll. Source/engine/model/voice/timing hash giữ cache hợp lệ. Full scan tiếp tục thu các lỗi overrun, giữ cache để đối chiếu/sửa.

Full artifact nội bộ trong `previews/<key>/`. Export chọn thư mục cha và tạo atomic thư mục `narration-<key>/` với:

```text
narration.vi.wav
narration.vi.json
subtitles.vi.srt
```

JSON schema 2, kind `ocr-narration`: title/source URL/session/revision/hash, SRT/WAV hash, duration/duration source, anchor/offset 0, PCM format, missing ranges, partial/all-reviewed flags và metrics. `wholeEpisodeVerified` luôn false; đủ coverage và reviewed không chứng minh engine không bỏ sót thoại. File được sync rồi rename cả thư mục; lặp lại cùng output không ghi đè, phát hiện bộ xuất bị sửa sẽ từ chối.

## QA Model Thật

| Case | Kết quả |
| --- | --- |
| Synthetic 80 cue, duration 1.422.000 ms | Model VieNeu/SEA-G2P/FFmpeg thật; Thùy Dung mặc định, Đức Trí override câu 1 |
| Hủy giữa job | Cancelled sau 72 cue commit, elapsed checkpoint 62 s; không WAV cuối |
| Quit/reopen/resume | 72 cache + 8 mới, completed sau elapsed 39 s; đủ 80 cue |
| Build cuối, tạo lại toàn cache | 0 mới + 80 cache, elapsed 28 s; checksum WAV không đổi |
| Format/timeline | PCM16 mono 48 kHz, 23:42.000, 136.512.044 bytes; inspector đối chiếu toàn processed PCM của 80 cue và silence/gap/tail |
| Export native | Open dialog xuất WAV/SRT/JSON; WAV checksum trùng nội bộ; export lại cùng thư mục idempotent |
| Hai cue chỉ 180 ms | TTS thật tạo 1.555/0.971 s sau tempo 1.25×; báo cả hai lỗi, giữ cache, không WAV cuối; nút sửa mở đúng cue trong editor |
| Phim thật trên bản sao review 4A | 8/309 cue reviewed, còn một khoảng chưa thu; bị chặn trước TTS, kể cả bật consent Một phần |
| UI | Chrome Playwright desktop/mobile: scope, voice overrides, lỗi/edit, consent không bỏ review, player metadata; mobile 390×844 không overflow |
| Phát native | WAV 23:42 load/phát/pause, elapsed player tiến; export đang chạy khóa nguồn/control để tránh gửi lặp |

Synthetic data: `/var/folders/hd/pknm19x13t3bkjxl_xvbs40r0000gn/T/long-tieng-m4b-qa-6VWG8N`, session `f966717c-de35-4d87-94d3-8b460153b9a8`, revision `615fac3c8facbbb22fb2acb5cd8064f400287c76a70c90008348bd7ffbb30825`. WAV SHA-256: `bb2640f25d680488de7a07b245cc3bbd7700c293797e4e435e7a2ec78454a7e6`.

Export QA: `/private/tmp/long-tieng-m4b-export/narration-65dfc2bc68269140a26b07bbe6f1fb4a48e39e083a51a140e50a3b535d1454f0/`. Derived WAV/PCM report ở ignored `browser-extension/tests/results/ocr-audio-full/`. Source phim/ảnh/revision gốc không sửa; các bản QA không commit. RSS main process đo một thời điểm khi processed 41 khoảng 136 MiB, **không phải peak toàn app/WebView/engine**. Timeline writer stream sample, không giữ PCM cả tập trong RAM; inspector Node decode PCM đầy đủ chỉ dùng QA.

## Kiểm Thử và Gate Còn Mở

App Rust: 45 passed, 1 optional ignored. Helper: 32 unit + 1 stdio passed, 2 optional ignored. Extension: 20 passed; editor: 5; audio frontend: 7. Tổng mặc định 110 passed. Rust coverage mới: full validation/union/partial/voice/quota, resource stop với output sparse oversize, additive 4A compatibility, export idempotence/corruption và source/metadata validation. Không cài thêm dependency runtime.

Build frontend/Tauri debug app/helper release/extension và Clippy (với allow ba nhóm lint baseline cũ) được chạy. Từng có lỗi rustdoc do chạy Cargo test cùng Tauri build trên chung target; chạy lại serially đạt. Fixture IAB từng treo sau alert của mock; bỏ alert mock và kiểm thử UI bằng Chrome, không quy lỗi đó cho app native. Screenshot được xem trong tool, không tuyên bố có file screenshot lưu trong repo.

Chưa có full-film đã review đủ trên URL chính, chưa benchmark 10.000 cue/hai giờ/cả bảy giọng/tên Nhật hoặc peak RAM toàn hệ thống; chưa fault-inject mất điện/disk-full vật lý. Quota/file-error tests và checkpoint locks không thay các bài đó. Không tuyên bố timing OCR đạt 200 ms. Chrome/helper 0.3 và chất lượng OCR toàn tập 2B vẫn cần nghiệm thu riêng trước khi gọi workflow thu → review → WAV của phim thật là hoàn chỉnh. Mốc 5 playback chưa triển khai.

Cleanup: app QA và Vite đã dừng, viewport Chrome đã reset và tab Chrome QA đã đóng. Tab IAB tạm ID 8 không đóng được bằng API do timeout focus/CDP; không can thiệp UI Codex hoặc bypass giới hạn công cụ. Server của trang đó đã dừng.
