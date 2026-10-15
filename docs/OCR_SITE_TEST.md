# Khảo sát nguồn phim chính và kế hoạch nghiệm thu OCR

Ngày khảo sát: 2026-10-14, múi giờ Asia/Ho_Chi_Minh. Bảng dưới ghi khảo sát ban đầu. Native helper đã đạt gate 2A trên macOS 26.6.2/Chrome 155: thu 323 giây liên tục khi GUI đóng, helper crash/replay và persistence; xem [OCR_MILESTONE_2A.md](OCR_MILESTONE_2A.md). OCR/SRT 2B đã có code và test fixture/bản sao capture thật; chưa chốt Chrome 0.3 và chất lượng nguồn chính. Xem [OCR_MILESTONE_2B.md](OCR_MILESTONE_2B.md), không dùng kết quả fixture để đánh dấu ma trận sản phẩm dưới đây đã pass.

## Nguồn chính

[Tokyo Revengers tập 01 trên AnimeVietSub](https://animevietsub.nl/phim/tokyo-revengers-a4036/tap-01-75695.html).

Nguồn này thay URL My Hero Academia cũ trong toàn bộ nghiệm thu MVP. Không suy ra compatibility từ website cũ.

## Những gì đã kiểm tra trực tiếp

| Kiểm tra | Quan sát | Ý nghĩa với thiết kế |
| --- | --- | --- |
| Mở trang | Mở được trang tập 01, có các server DU, FB, HDX(ADS) | Ghi episode identity và server; không nhầm video quảng cáo |
| Player/frame | Player trong iframe khác miền, `storage.googleapiscdn.com` trong lần khảo sát | Cần content script và permission trong frame player, không chỉ ở trang cha |
| Browser có công cụ kiểm tra DOM | Ban đầu thấy Play và duration 23:41, sau đó iframe hiện thông báo `Developer Tools đang mở`; không đọc được video clock | Phải test Chrome bình thường và extension thật; không dùng kết quả DOM automation làm bằng chứng clock đã đạt |
| Chrome thông thường, không mở DevTools | Bấm Play phát được hình phim, duration hiển thị 23:41 | Nguồn có thể xem bằng đường thông thường; chưa chứng minh tabCapture |
| Phụ đề | Đã quan sát chữ Việt hai dòng ở đáy hình, font có viền trên nền chuyển động | ROI phải đủ cho hai dòng và dấu; giữ ảnh gốc khi OCR không chắc |
| Pause | Đã pause; UI hiện Phát và thời gian 04:27; có quảng cáo khi pause | Pause giữ timeline; quảng cáo không được biến thành cue thoại |
| Nhãn accessibility | Nhãn/slider từng vẫn ghi mốc 0 dù UI hiển thị 04:27 | Không lấy AX text hoặc OCR đồng hồ player làm clock chính; cần đọc video time trong frame |
| Popup/overlay | Click Play hoặc nút đóng quảng cáo có thể mở tab quảng cáo ngoài; banner nổi có thể che player | Session phải bám tab gốc, không chuyển theo active tab, phát hiện ROI bị che |

Ở khảo sát ban đầu chưa đọc được `currentTime` chính xác. Mốc 1 đã đọc clock trong iframe `storage.googleapiscdn.com`, thấy video 1280x720 và timestamp khớp pause của player. Capture được giới hạn 1920x1080, không phải độ phân giải phim. Không dùng duration/track metadata của URL cũ cho nguồn này; fullscreen/đổi server vẫn chưa nghiệm thu.

## Ma trận bắt buộc khi triển khai extension

Đây là ma trận nghiệm thu sản phẩm đầy đủ, không phải danh sách case đã pass. Chỉ capture/clock và một số lifecycle case của mốc 1 có bằng chứng ở báo cáo riêng.

| Nhóm | Kịch bản | Điều kiện đạt |
| --- | --- | --- |
| Setup | App đã cài, GUI đóng, start extension | Helper được Chrome khởi động; engine/model và thư mục app data được resolve đúng |
| Iframe | Video trong iframe khác origin | Clock đúng của phim, frame/player ID ổn định, permission đủ và không phụ thuộc DevTools |
| Capture | Thu đoạn 3-5 phút có sub | Tab stream hiện hình/chữ, vùng crop đúng, không lẫn quảng cáo ngoài vùng |
| ROI | Scroll, fullscreen, thoát fullscreen, phóng to player, resize | Vùng chọn đi theo video hoặc pause thu để chọn lại; không tiếp tục sai vùng âm thầm |
| Chữ Việt | Dấu, tên riêng, một/hai dòng, nền động và câu ngắn | Đo character error rate/cue recall; câu nghi vấn có ảnh để review |
| Timing | Đầu/cuối câu và đổi câu liên tiếp | So ground truth; ghi clock uncertainty và kiểm tra mục tiêu timing trong plan |
| Pause | Pause tại câu đang hiện, quảng cáo xuất hiện | Không sinh cue quảng cáo hoặc cộng thời gian thực vào timestamp |
| Buffering | Player chờ tải rồi chạy tiếp | Không nhầm với khoảng không có lời; đồng bộ lại clock |
| Seek | Tua tới, tua lui, tua lại cùng đoạn | Tách segment, báo gap và đối chiếu bản thu lại; không nhân bản cue sai |
| Skip OP/ED | Bấm nút skip tích hợp | Đoạn bỏ qua được đánh dấu chưa thu; không báo toàn tập đầy đủ |
| Tab/popup | Popup quảng cáo, đổi active tab, minimize và quay lại | Không chọn nhầm tab/clock; phát hiện frame drop hoặc throttling nếu có |
| Server | DU, FB và HDX(ADS), đổi server/reload iframe | Scope hỗ trợ được ghi rõ; reset player/clock khi cần, không trộn các timeline |
| Episode | Tự chuyển tập hoặc Tập tiếp | Finalize/đánh dấu tập cũ; tạo phiên mới theo tập, không nối nhầm |
| Lifecycle | Đóng popup extension, GUI đóng/mở, helper crash, Chrome restart | Checkpoint giữ được; resume capture cần thao tác người dùng; mất dữ liệu có gap |
| Storage | Disk đầy, queue đầy, duplicate message, crash lúc finalize | ACK sau commit, retry không trùng, không mất SRT cũ hoặc ghi đè edit |
| App library | Stop và chờ Đã lưu vào app, đóng Chrome, mở app/tab OCR | Thấy bản ghi tự động, mở SRT/ảnh cue, sửa và lưu không cần import Downloads |
| Audio | Chuyển SRT đã review sang TTS khi không có video local | Tạo WAV đúng timeline, mở lại SRT và revision không mất |
| Whole episode | Thu từ đầu đến cuối tập 01 ở 1x | Báo coverage/gap/review đúng; nghe thử WAV đầu/giữa/cuối |

## Cách ghi kết quả

Mỗi lượt ghi Chrome/OS/build extension/helper/model, URL phim và origin player, server, ROI, duration từ clock, phạm vi thu và các khoảng gián đoạn. Giữ crop đại diện và bộ cue ground truth ngắn để đo text/timing; không cần lưu toàn bộ phim.

Case đạt phải có dữ liệu và kết quả đo; case không đạt phải ghi lỗi/tác động/cách thu lại; case chưa chạy giữ nguyên trạng thái chưa test. Nghiệm thu URL chính trước, sau đó mới thêm URL cũ hoặc website khác.

Xem [OCR_PLAN.md](OCR_PLAN.md) cho pipeline, storage contract và lộ trình.
