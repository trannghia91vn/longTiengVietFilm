# Capture và OCR thông minh

## Trạng thái

Cập nhật ngày 14/10/2026. Đã triển khai chế độ thử nghiệm từ extension đến helper và pipeline OCR của app. **Chưa đạt nghiệm thu tối ưu; Lưu mọi ảnh vẫn là mặc định.** Không coi kết quả fixture hoặc finalize ACK là bằng chứng đã thu đủ/chính xác cả tập. Không triển khai OCR song song lúc xem phim, không đổi hợp đồng SRT/WAV hoặc tự xóa bản thu cũ.

Bản local **0.3.4** được bàn giao theo yêu cầu dùng trước/sửa lỗi sau, không chặn bằng bài capture bổ sung. Raw vẫn là mặc định lần đầu, nhưng lựa chọn Smart của người dùng được nhớ. Cả hai chế độ có thêm nén PNG lossless; xem [bản local và số đo](OCR_LOCAL_RELEASE.md). Không coi thay đổi này là đã đạt các gate trong báo cáo này.

Nguồn Chrome chính: [Tokyo Revengers tập 01](https://animevietsub.nl/phim/tokyo-revengers-a4036/tap-01-75695.html). ID dev đã đăng ký: `odjbedbbdecdfnngkibilbjnpfnmaein`. Chỉ ID này được phép dùng host; installer vẫn cần xác nhận trước khi agent đăng ký/chạy bản mới.

## Cách dùng

1. Ổn định kích thước tab, scroll, fullscreen và player trước. Pause phim ở 0:00 nếu muốn thu cả tập.
2. Mở extension từ chính tab phim, chọn video/iframe và vùng sát phụ đề, vẫn đủ hai dòng và dấu tiếng Việt.
3. Trước khi Thu hoặc Lưu ảnh mẫu, chọn **Lưu mọi ảnh** hoặc **Thông minh · thử nghiệm**. Không đổi chế độ trong một phiên. Helper thiếu capability sẽ khóa Thông minh và báo cần cập nhật.
4. Bấm Thu rồi phát 1×. Quan sát trạng thái recorder: phim đang phát không có nghĩa capture đang chạy. Layout/clock/quota lỗi sẽ tạm dừng thu, không điều khiển phim.
5. Khi bị tạm dừng vì bố cục đổi, pause phim, chọn lại vùng và chủ động tiếp tục. Khoảng bỏ qua phải thu lại; không nối coverage qua gián đoạn.
6. Kết thúc và chờ **Đã lưu ảnh và timestamp** sau finalize ACK. Sau đó OCR chạy riêng; SRT vẫn cần review trong app. Muốn thu phiên mới, mở lại extension từ tab phim.

Overlay hướng dẫn trong app đã bổ sung cách chọn chế độ và giới hạn thử nghiệm.

## Kiến trúc

- `browser-extension/src/smart-capture.ts`: detector thuần dữ liệu, dùng chữ sáng có viền tối, lọc connected components/band và dấu rời. Không hash toàn ảnh chuyển động.
- So với anchor của nhóm, cần ba observation tương đương liên tiếp để xác nhận ổn định. Giữ ảnh đầu, chuyển trạng thái, ổn định, đuôi nhóm và định kỳ tối đa 2 giây. Không chắc/không hỗ trợ/không thấy mask thì lưu mọi mẫu, không suy ra silence.
- `offscreen.ts`: quan sát mục tiêu 10 Hz tách khỏi mã hóa PNG/IndexedDB. Giữ current/tail RGBA với giới hạn tổng ảnh 16 MiB; không giữ cả tập. PNG màu nguyên độ phân giải ROI, lossless.
- Pause/buffering/tua/rate/ROI/layout/mất capture/gap reset detector và flush. Start kiểm tra layout trước khi tạo phiên. Lỗi layout mới kèm snapshot hình học trước/sau có giới hạn độ dài để chẩn đoán, không nới điều kiện bảo vệ ROI.
- Service worker sở hữu Native Messaging. Offscreen chỉ capture và ghi outbox trước gửi; retry/ACK/finalize vẫn dùng flow 2A.

### Schema và durability

Protocol vẫn version 1. Hello thêm capability `smart-observations-v1`; phiên thông minh dùng capture schema 2, `captureMode: smart`, `observationCount`, `fallbackCount`. Bản thu schema 1 dùng pipeline cũ.

Ba sequence riêng: `messageSeq` cho protocol, `sequence` cho PNG, `observationSeq` cho timeline. `appendObservations` chứa 1..5 mẫu, flush theo batch hoặc timer mục tiêu 500 ms và các boundary. Event loop/đĩa bận có thể làm trễ timer, không cam kết deadline cứng.

Observation giữ clock, media time, segment, ROI, uncertainty, group/role/stability và `imageSequence`. PNG phải durable commit trước observation tham chiếu. Helper kiểm tra sequence, source/layout/size/ROI/group/timestamp và reject ảnh thiếu. ACK sau journal fsync/checkpoint, replay checksum cũ không nhân đôi dữ liệu. Journal phục hồi checkpoint lỗi/đuôi dang dở; lock/quota của 2A giữ nguyên.

IndexedDB v3 thêm store observations, migration bổ sung từ v1/v2. Không xóa archive/ZIP cũ. Outbox giới hạn 128 MiB chưa ACK; PNG ACK được giải phóng, metadata và preview vẫn giữ. Không cung cấp ZIP thiếu ảnh cho phiên native. Quota helper vẫn 2 GiB/phiên, 10 GiB/thư viện, tối thiểu 1 GiB ổ đĩa trống; không tự xóa dữ liệu để vượt quota.

### OCR và timeline

OCR chỉ chạy sau finalize. Nhóm ổn định nhận dạng ảnh stable/periodic; chỉ tái dùng khi text chính xác, không rỗng, confidence cao và các đại diện đồng thuận. Bất đồng/không chắc thì nhận dạng tất cả ảnh giữ lại, đánh dấu review, không fuzzy chọn câu.

Không dùng lại kết quả xuyên group/segment/ROI hoặc gộp lần xuất hiện riêng ở segment khác. Cache recognition theo ảnh trong work revision gắn source/engine/pipeline, checksum JSON bytes gốc, atomic fsync. Cache hỏng hoặc quá lớn fail closed. Mẫu manual giữ làm bằng chứng nhưng không bổ sung coverage quan sát.

Assembler schema 2 mở rộng PNG OCR thành chuỗi observation để tính biên/coverage. Không dùng khoảng cách PNG 750 ms để tạo gap. Group boundary có uncertainty/review flag; không dùng temporal-consensus cũ trên nhiều observation tái dùng cùng ảnh. Smart dùng identity `smart-1`, assembly version 3; raw giữ assembly 2 và cache cũ. Review revision đã lưu không bị thay bởi worker chạy lại.

## Số đo replay

Chạy detector trên bản sao dữ liệu cũ, cùng PNG nguồn/ROI cho baseline và smart. Baseline OCR có sẵn; đây không phải benchmark runtime A/B đồng thời. Một logical OCR call là một lần `Engine::recognize`, có thể gồm nhiều bước preprocess/engine nội bộ.

| Dữ liệu | PNG raw → smart | Giảm bytes PNG | OCR raw → smart | Giảm OCR | p95 chênh biên |
| --- | --- | --- | --- | --- | --- |
| Fixture 20 giây, 5 cue | 200 → 120 | 51.29% | 200 → 87 | 56.50% | 0 ms |
| Đoạn phim cũ khoảng 5 phút | 2,092 → 1,605 | 25.42% | 2,092 → 1,276 | 39.01% | 402 ms |

Fixture: 3,205,540 → 1,561,486 bytes; 200 observations, 60 fallback, worker 16.23 giây. Năm cue khớp baseline fixture đã đối chiếu ground truth, không thêm cue/mất cue trong fixture này. Chưa bao phủ mọi kiểu subtitle.

Phim: 266,479,366 → 198,730,258 bytes; 2,092 observations, 667 fallback, worker 270.91 giây. Baseline 329 cue, smart 324; 273 cue khớp exact text và khoảng giao nhau, 56 baseline và 51 smart không ghép exact được. **Không gọi những cue không ghép được là câu bị bỏ sót/thừa đã xác nhận**, vì raw OCR chưa phải human ground truth. p95 chỉ tính trên cặp đã ghép, không đo timing tuyệt đối.

Replay Node peak RSS: fixture 131,858,432 bytes; phim 172,097,536 bytes. Đây là script/decoder replay, không phải tổng RAM Chrome/helper/OCR. Giới hạn 16 MiB chỉ là buffer ảnh capture, không phải toàn bộ heap/canvas/engine.

Replay lại sau khi thêm giới hạn số connected components/glyphs cho detector (`fixture-bounded`, `film-bounded`) cho selection ảnh/observation giống hoàn toàn các lần đo trên. OCR fixture bằng helper bản cuối: 87 calls, 5/5 cue khớp, 0 ms, 17.98 giây. Không chạy lại toàn bộ OCR phim sau thay đổi giới hạn/cache-validation/manual cuối; số OCR phim trong bảng thuộc lần đo đã nêu.

Gate yêu cầu ≥70% PNG, ≥50% OCR, không thêm câu bỏ sót trên đoạn đã đối chiếu và p95 bổ sung ≤100 ms. **Phim đã đo chưa đạt cả hiệu quả và gate biên; quality recall phim chưa đủ dữ liệu đối chiếu. Không đổi mặc định.** Detector bảo thủ/fallback và ROI nhiều nền làm hiệu quả thấp. Chưa hứa timing cue 200 ms.

Artifacts QA tạm, không commit ảnh phim:

- `/private/tmp/long-tieng-smart-fixture-final/report.json`
- `/private/tmp/long-tieng-smart-film-components/report.json`
- `/private/tmp/long-tieng-smart-live-5m-audit.json`

## Chrome thật

Sau xác nhận của người dùng, helper được đăng ký và extension reload thủ công. Công cụ UI không được điều khiển trang nội bộ Chrome/extension; người dùng thực hiện thao tác recorder, agent chỉ kiểm tra dữ liệu local.

Phiên schema 2 `74b60180-792b-4035-aabd-e04b3da4e431` đã finalize: 2,222 PNG, 2,778 observations, 289 fallback, 790,227,161 bytes (753.62 MiB). Timeline 23,612..454,303 ms, dài 430.691 giây (7 phút 10.691 giây), interval quan sát lớn nhất 216 ms. Audit đã kiểm tra mọi hash PNG, journal/checksum/message sequence/refs/count/finalize ACK; không PNG dư. Đây là **đoạn hơn 5 phút**, không bắt đầu từ 0, không cả tập. Không suy ra tỷ lệ giảm so với raw từ số ảnh đơn lẻ.

Thông lượng trung bình đoạn này khoảng 6.45 observations/giây, chưa đạt quan sát đều 10 Hz trên máy/ROI này. Không dùng interval lớn nhất 216 ms để hứa timing cue. Cần đo riêng chi phí detector/PNG/durable outbox trên bài live bản cuối.

Các lần thử cả tập bị tạm dừng do layout guard. Lần mới `ca6ad238-941e-4518-a034-b30fcf28617c` lúc kiểm tra đang paused, 0 ảnh/0 observation; bắt đầu ở 157,673 ms rồi tua về 0 sau khi đã pause. Film play không tự resume capture. **Bài cả tập chưa đạt**. Không bỏ guard khi chưa biết phần hình học nào đổi.

Các chỉnh sửa timer 500 ms/giới hạn detector/chẩn đoán layout/cache/manual được build sau bài hơn 5 phút; bài live đó không chứng minh các thay đổi cuối đã chạy trên Chrome. Không reload/đổi helper khi người dùng đang capture. Các thư mục live đã không còn đầy đủ ở lần kiểm tra sau; báo cáo audit tạm ghi lại kết quả tại thời điểm kiểm tra, không cam kết chúng hiện vẫn tồn tại. Agent không xóa dữ liệu capture.

## Kiểm thử và lệnh QA

```bash
npm run extension:test
npm run extension:build
npm run native:test
npm run native:build
cargo clippy --manifest-path browser-extension/native-host/Cargo.toml --all-targets -- -D warnings
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run ocr:editor:test
npm run ocr:audio:test
npm run tauri build -- --debug --bundles app
```

Replay nguồn chỉ đọc journal/PNG, không mở storage phục hồi trên bản gốc. Import/worker chạy app data QA riêng:

```bash
cargo build --release --manifest-path browser-extension/native-host/Cargo.toml --examples
npm run ocr:smart:replay -- /absolute/source-session /private/tmp/new-smart-replay
node browser-extension/scripts/smart-qa.mjs /private/tmp/new-smart-replay
node browser-extension/scripts/smart-audit.mjs /absolute/finalized-smart-session /private/tmp/audit.json
```

Trên macOS sandbox có thể không cho Apple Vision nhận dạng; chạy QA với quyền phù hợp, không thay engine bằng kết quả mock rồi báo đạt. `smart-qa` dùng helper/model đã prepare; script không cài helper Chrome hoặc ghi vào thư viện thật. Không tái dùng output directory đang chứa kết quả đo cần giữ.

Đã chạy: extension 27 tests; helper 39 unit + stdio integration (raw và smart), 2 test có điều kiện ignored; app Rust 45 pass/1 ignored; editor 5/audio 7 pass. Có test migration v1/v2, outbox/ACK/quota/recovery, nhóm/stability/reset/dấu/nền động/unsupported, missing refs/segment/timestamp, journal commit/checkpoint crash/tail/replay/conflict, cache resume và bằng chứng manual. Test stdio dùng binary thật, reject version/origin/ảnh thiếu, restart/replay và không tạo PNG trùng.

Fixture OCR crash/reprocess trên bản sao: sau khi cắt journal QA ở record hoàn chỉnh rồi thêm tail dang dở, resume 120 derived observations với **0 OCR call mới**, revision/SRT/cues hash không đổi. Không sửa journal của dữ liệu nguồn.

UI fixture `tests/recorder-smart.html` mock transport local: chọn Smart, khóa chế độ khi thu, kết thúc mở khóa dù stream đã ngắt, helper cũ chặn Smart và hiện hướng dẫn. Đây không thay thế Chrome Native Messaging capture thật.

Đã kiểm tra UI desktop và viewport 390×844; không tràn ngang, label chế độ xuống dòng trong nút, reset viewport sau test. Build frontend/extension/helper, helper Clippy tất cả target `-D warnings` và Tauri debug app bundle thành công; bản debug được ad-hoc signed, chưa notarize/release. Build không tự cập nhật `/Applications` hay host Chrome đang chạy.

Chưa đạt/chưa chạy đầy đủ: cả tập 23 phút smart, human ground truth toàn đoạn phim, ≥70%/≥50%/≤100 ms, RAM tổng browser/worker, OS disk-full/power-loss thật và deadline timer trên Chrome bản cuối. Không tự tuyên bố nghiệm thu thành công từ unit tests.

## Hướng tiếp theo

Bản sửa ổn định capture tiếp theo ở [OCR_CAPTURE_STABILITY.md](OCR_CAPTURE_STABILITY.md): chỉ fingerprint player/chuỗi iframe chọn, ROI neo video và stream theo viewport. Không thay các số đo detector/quality gate trong báo cáo này.

Ưu tiên chẩn đoán layout trước/sau để hoàn tất bài thu dài; tiếp theo đối chiếu thủ công các cue phim không ghép được và nhóm chuyển trạng thái. Chỉ hiệu chỉnh detector/assembler dựa trên fixture có ground truth và replay cùng nguồn; không nới ngưỡng chỉ để đạt dung lượng. Sau khi tất cả gate đạt mới xem xét đưa Thông minh thành mặc định.
