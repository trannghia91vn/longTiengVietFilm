# Kế hoạch OCR phụ đề Việt và lồng tiếng trên trình duyệt

Trạng thái ngày 2026-10-14: mốc 2A đã đạt gate Chrome thật trên máy macOS này. Mốc 2B đã triển khai OCR worker/SRT và qua test fixture/recovery, nhưng Chrome bản 0.3 chưa nghiệm thu và phim thật còn nhiều cue vụn/nghi vấn; xem [báo cáo 2A](OCR_MILESTONE_2A.md), [báo cáo 2B](OCR_MILESTONE_2B.md). Mốc 3 đã có thư viện OCR/editor và review revision độc lập, theo yêu cầu triển khai của người dùng; xem [báo cáo mốc 3](OCR_MILESTONE_3.md). Người dùng đã chọn extension thu trước, không có video local, chấp nhận phát 1x và sửa câu OCR nghi vấn.

Nguồn chính để phát triển và nghiệm thu: [Tokyo Revengers tập 01 trên AnimeVietSub](https://animevietsub.nl/phim/tokyo-revengers-a4036/tap-01-75695.html). URL My Hero Academia cũ chỉ là nguồn thử phụ sau khi nguồn chính đạt yêu cầu. Luồng vẫn dành cho phụ đề Việt nằm sẵn trong hình. Lấy macOS của workspace hiện tại làm nền tảng desktop đầu tiên và Chrome làm browser của extension; nền tảng dùng thực tế chưa được người dùng xác nhận riêng.

## 1. Mục tiêu và phạm vi

Luồng mong muốn:

```text
Extension trên Chrome tại tab phim
  -> chọn vùng phụ đề ngay trong extension
  -> bấm thu, nhận ảnh vùng sub + thời gian video
  -> helper local OCR, ghép câu, checkpoint
  -> kết thúc thu, chờ xử lý hết, lưu SRT vào app data
  -> mở app Long Tieng -> tab OCR thấy bản ghi
  -> mở/xem thử/sửa/xử lý -> subtitles.vi.txt + subtitles.vi.srt
  -> dùng TTS hiện tại -> narration.vi.wav
  -> extension phát WAV đồng bộ với video
```

Thêm mục `OCR` độc lập ở sidebar. Tab này là thư viện kết quả thu, có tên phim/tập, thời gian thu, số cue, tên file SRT và trạng thái hoàn tất/một phần. Không yêu cầu người dùng tự tìm file trong Downloads rồi import lại. Cửa sổ app có thể đóng trong lúc thu; helper được cài cùng app nhận và lưu dữ liệu.

MVP hỗ trợ AnimeVietSub với Tokyo Revengers tập 01 trước, một video hữu hạn trong một phiên, phụ đề Việt một hoặc hai dòng, phát ở tốc độ 1x. Mở rộng website, thu nhanh, phụ đề karaoke và tự nhận dạng nhân vật là các bước sau. Ghi phim/mùa/tập trong metadata; mỗi tập có SRT/WAV riêng để tránh phát nhầm.

OCR chỉ biết những khung hình đã thu. Không thể khôi phục câu chưa từng xuất hiện trong ảnh. Xuất SRT đúng cú pháp và thu đủ lời của cả tập là hai tiêu chí riêng; app phải báo phần chưa thu và phần chưa được kiểm tra.

## 2. Những gì app đã có và cần bổ sung

| Phần | Hiện trạng trong mã nguồn | Việc cần làm |
| --- | --- | --- |
| Sidebar | `src/App.tsx`, `AppView` trong `src/types.ts` | Thêm view OCR, giữ phiên chạy khi chuyển view |
| Đọc/ghi SRT | `src-tauri/src/srt.rs` | Tái sử dụng parser/serializer; thêm kiểm tra kết quả OCR |
| Nhập SRT Việt | `project::import_srt` | Tái sử dụng quy tắc nhập vào `translated_text`, bỏ qua dịch |
| TTS | `project::synthesize`, cache WAV theo cue | Dùng lại giọng, cache, chỉnh thời lượng và sửa từng câu |
| Timeline narration | `build_narration` trong `project.rs` | Hiện ghi PCM thô để trộn video; bổ sung xuất WAV có header |
| Tạo dự án | `project::create` yêu cầu video local tồn tại | Thêm dự án chỉ có phụ đề, không dùng đường dẫn video giả |
| Xuất cuối | `project::render` probe video rồi xuất MP4/MKV | Thêm job xuất audio độc lập, không cần file phim |
| Job/log/model | Progress, checkpoint, diagnostics, Model Manager | Tái sử dụng cơ chế; trạng thái hủy phải theo từng job |
| Thu khi app đóng | Chưa có native host hoặc thư viện OCR | Helper local, storage contract dùng chung, thư viện SRT khi mở app |

TTS từng câu đã có, nhưng UI chưa có luồng SRT -> một WAV đầy đủ mà không cần video local. Đây là phần bắt buộc để đáp ứng mục tiêu phim đang phát trên website.

## 3. Kiến trúc thu đề xuất

### Phương án chính: extension + helper local + app desktop

Extension Chrome Manifest V3 làm nhiệm vụ lấy dữ liệu từ tab:

- Người dùng chọn tab/video và vùng phụ đề, thử OCR một ảnh rồi chủ động bật thu từ extension. Đây là điểm bắt đầu luồng; không phải chọn vùng trong app.
- Content script xác định video, đọc `currentTime`, `duration`, `playbackRate` và các sự kiện play/pause/seek/buffering/ended.
- Stream tab chạy trong offscreen document để việc thu không phụ thuộc popup còn mở.
- Thu video-only cho OCR; không cần lấy âm thanh của phim.
- Chọn hình chữ nhật vùng sub trên preview; lưu tọa độ chuẩn hóa cùng kích thước và phiên bản vùng chọn.
- Content script chạy trong frame chứa player, có host permission đúng miền iframe; không giả định script ở trang cha đọc được video khác origin.
- Crop trong extension, gửi ảnh vùng nhỏ cùng metadata về helper; không gửi toàn bộ video.
- Helper chạy OCR, ghép câu và lưu phiên/SRT vào cùng app data với app desktop. App đọc thư viện đã lưu để xem, chỉnh sửa, tạo giọng và xuất WAV.

Chrome yêu cầu thao tác chủ động của người dùng để bắt đầu `tabCapture`; không thể chỉ bấm trong Tauri rồi tự chiếm tab Chrome. Chrome hỗ trợ đường service worker -> offscreen document cho stream ID từ Chrome 116. Xem [tabCapture](https://developer.chrome.com/docs/extensions/reference/api/tabCapture) và [hướng dẫn thu tab](https://developer.chrome.com/docs/extensions/how-to/web-platform/screen-capture).

Với yêu cầu ghi xong rồi mới mở app, transport chính là Chrome Native Messaging. `connectNative` khởi động helper riêng được cài/đăng ký cùng app, không cần mở GUI và không cần dịch vụ luôn chạy từ lúc khởi động máy. Offscreen/content script gửi qua `runtime` đến service worker, rồi service worker chuyển đến host. Helper chỉ xử lý protocol phiên OCR, không nhận lệnh shell hoặc đường dẫn tùy ý. Xem [Native Messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging).

Spike phải chốt helper Rust, vị trí engine/model, app-data resolver dùng chung với Tauri, native host manifest, extension ID ổn định và installer/đăng ký host theo OS. Đóng gói helper cùng app; giữ source extension và source helper tích hợp trong `browser-extension/`. Logic OCR/storage dùng chung với desktop cần module không phụ thuộc cửa sổ hoặc `AppHandle`.

Giới hạn payload, version protocol, sequence/ACK và kiểm tra session/frame nguồn. `stdout` của helper chỉ chứa protocol có length prefix, log qua `stderr` hoặc diagnostics. Quyền host chỉ cấp cho extension ID được đăng ký. Helper giữ lock ghi theo phiên và đọc/ghi cùng schema với desktop. WebSocket khi GUI đang mở chỉ là lựa chọn thử phụ; không đáp ứng riêng trường hợp app đóng.

Chrome extension không tự ghi đường dẫn tùy ý vào app data. Luôn để helper sở hữu thao tác ghi. Khi helper chưa cài/model thiếu, báo lỗi trước khi bắt đầu. Khi helper mất kết nối giữa phiên, buffer bền vững có quota và báo đang chờ; không hứa vẫn có SRT hoàn chỉnh nếu chưa OCR/xử lý xong.

### Phương án đã loại khỏi MVP: app desktop thu vùng màn hình

Nếu chưa muốn cài extension, có thể nghiên cứu capture native theo OS và chọn vùng cửa sổ. Phải thử quyền screen recording, tình trạng cửa sổ bị che và thay đổi layout trên máy đích.

Chỉ hình ảnh + đồng hồ máy không cung cấp timeline phim đáng tin khi pause, buffering, tua hoặc đổi tốc độ. MVP kiểu này phải phát liên tục 1x, nhập mốc bắt đầu và chấp nhận hiệu chỉnh; muốn hỗ trợ tua vẫn cần nguồn thời gian từ player. Không giả định WebView Tauri hỗ trợ capture giống Chrome.

Người dùng đã xác nhận làm extension thu nhỏ trước, sau đó bổ sung phát WAV vào cùng extension. Không cần hỏi lại lựa chọn này.

## 4. Spike bắt buộc trên website thật

Khảo sát ngày 2026-10-14 trên URL chính: player nằm trong iframe khác origin, tại miền `storage.googleapiscdn.com` ở server đang chọn. Trang có các lựa chọn server DU, FB, HDX(ADS). Browser kiểm tra qua DOM ban đầu thấy player có nút Play và thời lượng 23:41, sau đó iframe hiện thông báo `Developer Tools đang mở`; không đọc được video clock trực tiếp trong lượt thử đó. Chrome thông thường không mở DevTools phát được phim, quan sát được sub Việt hai dòng và thử pause tại 04:27. Pause hiện quảng cáo trong player; click ban đầu hoặc đóng quảng cáo có thể mở tab ngoài, banner nổi có thể che player. Nhãn accessibility của timeline từng không cập nhật theo thời gian UI, nên không dùng nhãn đó làm clock. Xem [OCR_SITE_TEST.md](OCR_SITE_TEST.md) để phân biệt kiểm tra đã làm và ma trận còn phải nghiệm thu.

Không lấy kết quả video HTML5 trực tiếp của URL cũ để suy ra nguồn mới đã hỗ trợ. Cần test extension thật trên Chrome, video clock trong iframe với permission cho miền player, tab capture, crop và OCR. Không sửa cơ chế bảo vệ của website để làm test vượt qua.

Trước khi làm UI hoàn chỉnh, thử một đoạn phim 3-5 phút trên website được chọn:

1. Xác định video đích, iframe, phụ đề và khả năng truy cập thời gian phát.
2. Xác nhận vùng sub hardsub và player clock bên trong iframe trên Chrome thật; không dùng đồng hồ máy để thay thế khi clock chưa lấy được.
3. Kiểm tra tab capture có hiện đủ dấu Việt và vùng crop có đúng khi fullscreen/resize/scroll.
4. Thử pause, buffering, tua, fullscreen, resize, đổi tab và video trong iframe.
5. Đo độ lệch giữa ảnh captured và thời gian của video nguồn; thử OCR trên các câu có dấu, tên riêng và hai dòng.
6. Test mỗi server theo phạm vi đã chốt, iframe reload/đổi miền, popup quảng cáo, nút Skip OP/ED và tự chuyển tập. Mọi đoạn skip phải là gap chưa thu, không được báo toàn tập đầy đủ.
7. Ghi từ extension trong khi GUI app đóng, finalize và kiểm tra app mở lên thấy ngay bản ghi. Test helper chết, app/Chrome restart và retry không tạo bản trùng.

Kết quả spike là ma trận hỗ trợ của website, mẫu ảnh/timestamp và quyết định engine/cách thu. Nếu ảnh video/sub bị đen, không tìm được video clock, hoặc website dùng player đặc biệt, phải ghi giới hạn cụ thể; không hứa hỗ trợ mọi dịch vụ hoặc vượt cơ chế bảo vệ nội dung.

Đối với website khác trong tương lai, chỉ dùng `textTracks` khi xác minh có track phụ đề hợp lệ; xem [MDN textTracks](https://developer.mozilla.org/en-US/docs/Web/API/HTMLMediaElement/textTracks).

## 5. Timestamp và coverage

- SRT dùng thời gian của phim, với 0 là đầu timeline video; không dùng số giây từ lúc bấm thu.
- Lưu ảnh với timestamp tại lúc capture, số thứ tự, player ID, segment ID, vùng chọn và chất lượng ghép clock. Thời gian OCR hoàn thành không được dùng làm timestamp.
- Ưu tiên metadata frame từ video nguồn; `requestVideoFrameCallback().mediaTime` thuộc timeline của video mà callback được đăng ký.
- `mediaTime` của video preview chứa stream tab là clock của stream capture, không tự động là clock của phim. Cần ghép clock giữa các context, đo độ trễ và lưu sai số ước tính.
- Không chỉ dựa vào `timeupdate`. Dùng quan sát frame/clock và sự kiện player để phân biệt video đang tiến với buffering.
- Pause không sinh cue mới. Buffering không kéo dài cue bằng thời gian thực. Seek đóng segment, đặt lại trạng thái ghép câu; resume giữ timestamp gốc.
- Tua qua một đoạn phải đánh dấu chưa thu. Tua ngược tạo đoạn thu lại, rồi đối chiếu theo interval và phiên bản, không loại trùng theo text toàn phim.
- Ads, player đổi nguồn hoặc chuyển tập cần tạm dừng và xác nhận nguồn; duration/URL đơn lẻ không đủ xác định phim.
- Với nguồn chính, popup quảng cáo không được làm session chuyển sang active tab mới; luôn bám tab/frame/video đã chọn. Khi overlay/control che ROI, đánh dấu không chắc hoặc tạm ngưng xử lý chữ, không xuất text quảng cáo vào SRT.
- Lưu interval đã quan sát, interval mất frame/mất clock và interval chưa xử lý. Không đánh đồng khoảng không có lời với đoạn bị mất dữ liệu.
- Chỉ báo đã quét toàn tập khi timeline hữu hạn được bao phủ và không còn gap chưa xử lý. Đây vẫn là coverage quan sát, chưa chứng minh OCR bắt đủ mọi câu.

Xem [MDN currentTime](https://developer.mozilla.org/en-US/docs/Web/API/HTMLMediaElement/currentTime) và [MDN requestVideoFrameCallback](https://developer.mozilla.org/en-US/docs/Web/API/HTMLVideoElement/requestVideoFrameCallback).

## 6. Pipeline OCR và ghép câu

### Engine và preprocessing

Thử Tesseract native với model tiếng Việt `vie` trước; đây là đề xuất benchmark, không phải cam kết chất lượng. Model nhanh dùng cho lần đầu; model chất lượng cao thử lại câu khó nếu số liệu thực tế có lợi. Tesseract có model Việt và các bộ `tessdata_fast`/`tessdata_best`: [tài liệu model](https://tesseract-ocr.github.io/tessdoc/Data-Files.html).

Kết quả triển khai 2B: Tesseract-only chưa tốt trên nguồn chính. Baseline macOS đã thử dùng Apple Vision accurate có Vietnamese support của OS, với Tesseract `vie` fast fallback và raw candidates để review. Model best được benchmark vài crop nhưng chưa chọn/bundle. Worker xử lý sau durable finalize, không chạy OCR trên đường ACK ảnh; đây là lựa chọn để giữ capture 2A ổn định. Không suy ra chất lượng toàn tập từ 5 cue fixture sạch.

Bundle engine theo mô hình sidecar hiện có, đăng ký model/checksum/license trong Model Manager và scripts đóng gói. Không thêm Python vào runtime người dùng. Nếu dấu Việt/font nền phức tạp không đạt tiêu chí, benchmark một engine khác với cùng bộ mẫu trước khi chọn.

Các bước: crop -> kiểm tra độ nét -> phóng to hợp lý -> thử ảnh gốc và variant tương phản/mask chữ -> OCR -> chuẩn hóa Unicode NFC, khoảng trắng và ngắt dòng. Giữ dấu, tên riêng và bản OCR gốc; không tự viết lại thoại bằng LLM.

### Lấy mẫu và trạng thái cue

- Bắt đầu thử quan sát ROI khoảng 10 lần/giây ở tốc độ 1x; chỉ chạy OCR khi nghi có đổi chữ và khi cần xác nhận. Đây là cấu hình đo thử, không bảo đảm không bỏ sót câu ngắn.
- So sánh đặc trưng chữ, không chỉ pixel/hash toàn ROI vì cảnh phim phía sau thay đổi liên tục.
- State machine: không có sub -> câu đang xuất hiện -> câu ổn định -> câu đổi hoặc biến mất.
- Xác nhận bằng nhiều mẫu và consensus text/confidence, nhưng giữ mốc đầu tiên có bằng chứng câu xuất hiện, tránh dịch timestamp sang lúc xác nhận.
- Lưu khoảng bất định cho đầu/cuối cue theo mẫu trước/sau; mục tiêu sai số cần đo trên ground truth.
- Giữ buffer ảnh gần thời điểm đổi chữ để xử lý lại câu ngắn và chuyển cảnh. Không tự loại mọi câu tồn tại ngắn vì đó có thể là thoại thật.
- Cùng một câu hiện lặp lại ở hai thời điểm vẫn là hai cue. Hai câu gần giống nhau không được merge chỉ bằng fuzzy match.
- Khi dừng thu giữa câu, đánh dấu cue cuối chưa biết thời điểm kết thúc. Review hỗ trợ split/merge và sửa timing.

### Hàng đợi và lưu kết quả

Hàng đợi có giới hạn; extension có journal/buffer local trong IndexedDB với quota, sequence/ACK và retry. Chỉ xóa dữ liệu dự phòng sau ACK đã checkpoint/commit từ helper. Khi không theo kịp, báo lag và lưu frame theo khả năng. Nếu mất dữ liệu thì đánh dấu gap, không tiếp tục âm thầm và báo thành công. Tự pause player để giảm tải chỉ là tùy chọn được người dùng bật.

Lưu crop đại diện và crop nghi vấn thay vì mọi frame theo mặc định. Lưu đủ bằng chứng để sửa; có quota, dọn cache và chế độ giữ thêm ảnh để debug. Checkpoint theo batch nhỏ để khôi phục sau crash.

## 7. UI của mục OCR

Luồng chọn vùng và điều khiển thu nằm trong extension. Tab OCR của app ưu tiên thư viện kết quả và workspace review, theo phong cách công cụ hiện tại:

- Thanh nguồn: tên phim/tập, tab kết nối, video đích, duration, thời gian hiện tại, trạng thái clock và capture.
- Danh sách SRT tự đọc từ app data ngay khi mở tab: phim/tập, nguồn, ngày ghi, số câu, thời lượng, trạng thái và lần chỉnh sửa cuối.
- App đang mở thì refresh theo thông báo hoặc quét manifest có debounce; mở lại app phải đọc được dữ liệu đã ghi mà không cần Chrome đang mở.
- Mở bản ghi để xem SRT dạng bảng và dạng text; preview ảnh vùng sub đại diện theo cue. Xem/nghe thử TTS không yêu cầu tải phim local; xem đồng thời với phim dùng tab Chrome đã liên kết.
- Chọn vùng/test ảnh/start/pause/resume/stop là control của extension. App có thể hiển thị trạng thái mirror; chuyển sidebar không làm dừng thu.
- Bảng cue có start/end, text, ảnh gốc, confidence, cảnh báo và trạng thái đã kiểm tra.
- Lọc câu nghi vấn; chỉnh text/thời gian, split/merge, xóa nhiễu, hoàn tác, chạy OCR lại câu chọn.
- Thanh coverage cho đoạn đã thu, mất dữ liệu, chưa thu và các đoạn thu lại.
- Xuất TXT UTF-8, SRT UTF-8; tùy chọn TXT kèm timestamp. Ghi rõ kết quả đầy đủ hay một phần.
- Nút chuyển sang tạo giọng, giữ tên phim/tập, duration và nguồn OCR.

Những tình huống phải có UI: thiếu engine/model, chưa cấp quyền, chưa thấy video, nhiều video, mất tab, iframe không truy cập được, vùng chọn không còn đúng, không có chữ, frame bị đen, clock không tin cậy, disk đầy, job bị ngắt.

## 8. Dữ liệu, module và contract đề xuất

Tên file/command là đề xuất, chốt sau spike:

| Nơi | Trách nhiệm |
| --- | --- |
| `src/components/OcrWorkspace.tsx` | Workspace, preview, vùng chọn, review và xuất |
| `src/types.ts`, `src/lib/tauri.ts` | Types OCR và wrappers commands/events |
| `src-tauri/src/ocr.rs` | Phiên, crop, engine OCR và ghép cue |
| Module storage/protocol dùng chung | App-data resolver, schema, atomic write, lock, ACK/backpressure |
| `src-tauri/src/project.rs` | Dự án chỉ có phụ đề, dùng lại TTS, xuất WAV |
| `browser-extension/` | Toàn bộ source extension, cấu hình build/test, assets, source helper tích hợp và hướng dẫn cài |

Thư mục `browser-extension/` nằm ngay root repo; đã có extension capture và helper storage 2A. Layout hiện tại:

```text
browser-extension/
|-- README.md
|-- src/                # popup/recorder, vùng chọn, service worker, offscreen
|-- assets/             # icon và asset của extension
|-- native-host/        # source helper và manifest template
|-- tests/              # protocol, ROI, reconnect, integration Chrome
`-- dist/               # output Load unpacked sau build, không commit
```

Manifest và root build commands đã có; installer helper hiện là dev macOS, chưa bundle vào release app. Source nằm cùng repo; dữ liệu phim native ở app data, SRT/WAV sẽ bổ sung sau, không trong folder source.

`OcrSession`: ID, schema version, title, source identity, video duration, trạng thái, vùng chọn/version, engine/model/settings, segments, coverage, cues và checkpoint.

`OcrObservation`: sequence, segment/player ID, media timestamp, capture clock, clock uncertainty, ROI version, crop path, raw text, confidence và warnings.

`OcrCue`: ID, start/end, text được chọn, raw candidates, ảnh bằng chứng, timing uncertainty, confidence, reviewed flag và provenance.

App data dự kiến: `ocr-sessions/<id>/session.json`, journal quan sát, crop có quota, `subtitles.vi.srt`, `subtitles.vi.txt`. Giữ session JSON atomically; journal để resume và reprocess mà không phải phát lại toàn bộ phần đã thu.

Kết thúc thu: ngừng nhận frame -> drain queue -> kết thúc cue cuối/đánh dấu chưa rõ -> validate SRT -> ghi file tạm -> rename SRT -> commit manifest chứa revision/hash/status -> ACK hoàn tất cho extension. Status tách `recording`, `processing`, `ready`, `partial`, `interrupted`, `error`; `ready` không đồng nghĩa mọi cue đã review.

Thư viện app chỉ tin revision manifest đã commit; crash giữa ghi SRT và manifest phải phục hồi được từ journal. Dùng session UUID + revision + checksum để retry không tạo bản trùng. Helper đang ghi thì desktop không sửa cùng revision; mở bản snapshot chỉ đọc hoặc đợi finalize. Sau khi lưu edit, tạo revision mới, invalidate TTS theo cơ chế cache hiện có và giữ bản OCR gốc để đối chiếu.

Chỉ extension báo `Đã lưu vào app` khi nhận ACK kết quả đã ghi bền vững. Nếu chưa lưu được thì hiển thị pending/error và nút thử lại. SRT export ra Downloads là bản phụ tùy chọn, không phải đường truyền kết quả chính.

Commands dự kiến: create/load/list session, pair/start/stop capture, update region/cue, reprocess, export TXT/SRT, create audio-only project, export project WAV. Events: connection, capture status, OCR progress và cue updates. Scope event/cancel theo session/job ID.

## 9. SRT -> WAV đầy đủ

Triển khai 4A/4B dùng `AudioProject` độc lập từ snapshot review đã lưu trong `audio-projects/`, không migrate `DubbingProject` sang video giả. Giữ nguyên schema/invariant video local của dự án cũ. Toàn timeline có preflight mọi cue, duration/coverage, giọng riêng từng câu, batch/cache/checkpoint/quota và bộ WAV/SRT/metadata atomic; đọc [báo cáo 4B](OCR_MILESTONE_4B.md).

- Nhập SRT OCR như tiếng Việt; không chạy dịch và không học translation memory từ dữ liệu này.
- Dùng lại UI speakers, giọng, TTS cache, batch, resume/cancel và chỉnh từng cue.
- Job xuất WAV preflight toàn bộ cue bắt buộc, tạo silence từ mốc 0 và giữa câu, ghi sample vào vị trí timeline của phim.
- Không nối WAV từng câu liên tiếp vì sẽ làm mất khoảng lặng và lệch phim.
- Chốt policy cue overlap và câu đọc dài: mặc định cảnh báo để sửa; không silently cắt mất lời. Giới hạn tăng tốc và review câu không vừa slot.
- WAV đề xuất PCM 16-bit, mono, 48 kHz, chỉ chứa lời Việt. Clip không có lời cuối phim vẫn có silence nếu đã biết duration.
- Nếu chưa biết duration, có thể xuất đến cue cuối + tail với cảnh báo; không coi đó là track đầy đủ của phim.
- Ghi output tạm rồi rename khi thành công; hủy/lỗi không để file dở mang tên kết quả cuối.
- Xuất kèm `narration.vi.json`: schema version, source/episode identity, duration, timeline origin, sample rate, offset, SRT hash và audio hash để extension ghép đúng tập.

WAV 48 kHz mono 16-bit khoảng 346 MB/giờ theo số sample; với thời lượng UI 23:41 của nguồn mới, ước lượng khoảng 136 MB chưa tính header. Duration chính xác cần lấy từ video clock khi triển khai. Cần kiểm tra dung lượng. Phim dài có thể cần thêm bản nén phục vụ browser để tránh decode toàn bộ WAV vào RAM, nhưng vẫn giữ WAV làm output người dùng yêu cầu.

## 10. Extension phát WAV ở giai đoạn sau

Cho chọn file audio local và metadata, không bắt buộc app đang mở. Chọn đúng video và tập, kiểm tra duration/identity, thử sync và lưu offset theo video.

- Video là clock chính: pause/buffering dừng audio; seek đồng bộ lại vị trí; đổi tốc độ cập nhật playback rate; ended dừng cả track.
- `audioTime = videoTime + offset`; định nghĩa dấu offset rõ trong UI, clamp ngoài phạm vi và chờ khi chưa đến đầu track.
- Điều chỉnh drift bằng ngưỡng đo được, tránh seek audio liên tục gây tiếng giật. Test cả phiên dài và nhiều lần pause/seek.
- Chỉ phát sau thao tác người dùng; xử lý lỗi autoplay, đổi player, iframe và reload trang.
- Mức âm lượng phim/lời Việt riêng. Nếu giảm âm lượng phim, nhạc và hiệu ứng cũng giảm; không thể tách thoại gốc khỏi nhạc chỉ bằng một WAV lời Việt.
- Tách nền để giữ nhạc cần nguồn audio/video xử lý được, nên không thuộc MVP phim online.

## 11. Các giai đoạn và điều kiện hoàn thành

### Các mốc triển khai nhỏ

Để kiểm chứng rủi ro browser trước khi ghép native OCR, thực thi theo các mốc nhỏ dưới đây. Bảng giai đoạn sản phẩm bên dưới mô tả đích cuối, không phải phạm vi của mốc 1.

| Mốc | Phạm vi | Trạng thái 2026-10-14 |
| --- | --- | --- |
| 1. Capture + clock spike | Extension local, ROI, clock iframe, PNG + metadata, ZIP đối chiếu | Đã có implementation và chạy Chrome thật; xem [báo cáo](OCR_MILESTONE_1.md). Chưa có OCR/SRT/helper |
| 2A. Native capture storage | Helper local, ACK/outbox/recovery, lưu PNG/timestamp vào app data khi GUI đóng | Đã đạt gate trên Chrome/macOS hiện tại; [báo cáo nghiệm thu](OCR_MILESTONE_2A.md) |
| 2B. OCR + SRT | Engine Việt, ghép cue, checkpoint và SRT vào app data | Đã triển khai/test fixture; [báo cáo 2B](OCR_MILESTONE_2B.md). Chưa chốt Chrome mới/chất lượng phim thật |
| 3. OCR library + review | Mục OCR trong sidebar app, danh sách bản thu, editor/ảnh nghi vấn, retry và revision | Đã triển khai và test trên Tauri/macOS với bản sao phim thật; [báo cáo](OCR_MILESTONE_3.md). Không thay thế quality/Chrome gate còn mở của 2B |
| 4A. WAV thử | Snapshot review, TTS hiện có, đoạn 30/60 giây, WAV neo video 0, cache/hủy/xuất | Đã triển khai; test model thật với mẫu và 8 câu phim đã đối chiếu. [Báo cáo 4A](OCR_MILESTONE_4A.md) |
| 4B. WAV cả tập | Preflight toàn timeline, dung lượng, chất lượng thoại và export toàn tập | Đã triển khai và QA long track bằng model thật; [báo cáo](OCR_MILESTONE_4B.md). Phim nguồn chính vẫn thiếu coverage/review; không thay gate Chrome/OCR 2B |
| Tối ưu capture/OCR | Detector chữ, observation riêng, sparse PNG và recognition cache theo nhóm | Đã có thử nghiệm; [báo cáo](OCR_SMART_CAPTURE.md). Replay phim chưa đạt gate 70% PNG/50% OCR/100 ms bổ sung; raw vẫn mặc định, bài cả tập smart chưa đạt |
| 5. Playback | WAV trong extension, pause/seek/rate/offset/drift | Sau khi pipeline thu và review đạt |

Mốc 1 không báo "đã lưu vào app": dữ liệu tạm ở IndexedDB của extension. ZIP chỉ là bằng chứng kiểm thử, không thay flow SRT tự xuất hiện trong tab OCR đã chốt.

| Giai đoạn | Kết quả bàn giao | Điều kiện đi tiếp |
| --- | --- | --- |
| 0. Xác nhận + spike | URL Tokyo Revengers chính, iframe clock, native host và benchmark OCR | Capture/sub/clock hoạt động trong Chrome bình thường; helper chạy khi GUI đóng |
| 1. Thu và lưu phiên | Extension trong root repo, chọn vùng, helper, journal/coverage | Pause/seek/reconnect không phá timeline; ACK chỉ sau khi lưu |
| 2. OCR -> thư viện SRT -> review | Ghép cue, commit SRT, danh sách OCR và editor | Thu xong rồi mở app thấy SRT; Chrome đóng vẫn mở/sửa kết quả được |
| 3. SRT -> WAV | Dự án chỉ phụ đề, TTS hiện tại, WAV + metadata | Không cần video local, silence/timing đúng, không mất lời âm thầm |
| 4. Phát đồng bộ | Extension chọn audio, play/pause/seek/rate/offset | Test trên tập dài và website mục tiêu |

Làm bản end-to-end 3-5 phút qua SRT và WAV sớm, rồi mới mở rộng thu cả tập. Ước lượng công triển khai sau spike; khó nhất là website compatibility, clock mapping và font/dấu Việt, không phải thêm nút sidebar.

## 12. Kiểm thử và nghiệm thu

- Bộ mẫu có ground truth text và start/end: dấu Việt, tên riêng, hai dòng, nền chuyển động, câu ngắn, câu lặp, fade, logo/player controls.
- Đo character error rate, cue bị bỏ sót/merge nhầm/split nhầm, sai số đầu/cuối và tỷ lệ phải sửa tay. Confidence engine không thay cho những số đo này.
- Mục tiêu đề xuất để xác nhận: trên video mẫu sạch, ít nhất 95% cue có timing đầu/cuối lệch không quá 200 ms; đo riêng câu ngắn/transition. Không coi đây là kết quả đã đạt.
- Test pause/buffering/seek forward/backward, rate change, resize/fullscreen, đổi tab, nhiều video, iframe, quảng cáo, reload và capture bị dừng.
- Test crash/reconnect/engine lỗi/disk đầy/queue đầy, đảm bảo dữ liệu checkpoint giữ được và khoảng mất được đánh dấu.
- Test GUI app đóng khi start/stop, mở app sau khi helper finalize, Chrome đóng sau ACK, app không cần tự import Downloads. Retry cùng revision không tạo bản ghi trùng; edit không bị bản OCR cũ ghi đè.
- Test native host installer/extension ID, profile Chrome, quyền iframe đúng miền, đổi server và Skip OP/ED trên URL chính; báo cáo cả case chưa test hoặc bị chặn.
- SRT parse/write round-trip, cue index, thời gian hợp lệ, Unicode và overlap policy.
- Test audio bằng cue có tone chuẩn ở mốc biết trước: đầu 0, silence gap, sample rate, tail/duration, overlap, hủy và header WAV; test TTS riêng với model thật.
- Test migration dự án cũ và invariant video; OCR Việt không vào translation memory.
- Chạy `npm run build`, Rust tests theo module; sau khi triển khai UI, kiểm tra desktop bằng screenshot và thao tác thực tế, rồi test bundle engine trên máy đích.
- Playback extension: kiểm tra offset/drift với tone chuẩn, pause/seek/rate/buffering và cả tập. OCR timing bám sub hiển thị, chưa chắc trùng tuyệt đối điểm bắt đầu lời thoại gốc; cần nghe thử với TTS.

## 13. Quyết định đã chốt và còn mở

Đã chốt:

- AnimeVietSub, Tokyo Revengers tập 01 tại URL người dùng mới cung cấp là nguồn chính; URL cũ không còn là mục tiêu nghiệm thu chính.
- Extension thu phụ đề ngay giai đoạn đầu; OCR chạy local trong app.
- Source extension và helper tích hợp tập trung tại `browser-extension/` trong root dự án.
- Flow chính bắt đầu trong extension: chọn vùng -> thu -> lưu SRT vào app data -> mở tab OCR để xem, sửa và xử lý. GUI app không bắt buộc mở trong lúc thu; helper cài cùng app làm OCR/lưu file.
- Không có video local; dự án chỉ phụ đề và xuất WAV độc lập là bắt buộc.
- Phát cả tập ở 1x, kiểm tra/sửa câu nghi vấn.

Còn mở, có thể dùng mặc định để tiếp tục lập kế hoạch:

- macOS + Chrome là baseline theo môi trường hiện tại; xác nhận thêm nếu máy xem phim dùng nền tảng khác.
- Một giọng Việt cho bản đầu, cho phép gán giọng theo cue bằng chức năng hiện có; không tự suy luận nhân vật từ ảnh sub.
- Mục tiêu timing ban đầu 200 ms trên mẫu sạch; chốt chất lượng sau benchmark.
- Âm lượng khi xem: mặc định có thanh chỉnh phim/lời Việt; giảm phim sẽ giảm cả nhạc/hiệu ứng. Cần người dùng chọn ưu tiên khi triển khai playback.
- Opening/ending và chữ giới thiệu: mặc định giữ dữ liệu OCR, cho đánh dấu cue không đọc và chọn phạm vi, không tự xóa vì trông giống lời bài hát.

## 14. Những thứ cần chuẩn bị

- Chrome trên máy sẽ thu và app desktop đã cài cùng máy, cùng video/tập bản gốc khi xem lại. Native host phải được đăng ký lần đầu; GUI không cần chạy khi thu.
- Engine OCR native và model `vie` cùng license/checksum; engine/model TTS và FFmpeg hiện có.
- Dung lượng cho WAV, cache TTS và crop review; không cần lưu toàn bộ phim.
- Một đoạn 3-5 phút của tập 1 có hai dòng, dấu Việt và cảnh chuyển động để tạo ground truth thủ công; sau đó một tập trọn để kiểm tra coverage/drift.
- Bản đầu cài extension dạng development/unpacked; hướng dẫn cài và pairing trong tài liệu triển khai. Việc cài actual extension chỉ làm khi bắt đầu triển khai/test.
- Thêm hướng dẫn build extension ngay từ folder root, build/register helper và khắc phục lỗi host missing/ID mismatch; kiểm thử lại sau khi di chuyển app hoặc nâng cấp.

Không cần API cloud hay tải file phim để chạy luồng đề xuất. Tập 24 phút cần khoảng 24 phút phát để thu ở 1x, cộng thời gian xử lý hàng đợi, review và TTS; chưa có số đo tốc độ OCR/TTS trên máy đích.
