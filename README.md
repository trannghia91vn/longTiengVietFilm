# Lồng tiếng

Studio lồng tiếng phim chạy local bằng Tauri v2, dành cho nguồn tiếng Nhật hoặc tiếng Anh và đầu ra tiếng Việt.

## Project Editor

- Tạo và tự động khôi phục dự án trong app data (`projects/<id>/project.json`).
- Nhập SRT chuẩn hoặc phân tích 5 phút/toàn bộ phim bằng `whisper.cpp`.
- Mặc định mọi cue dùng một nhân vật; người dùng có thể thêm nhân vật, chọn giọng và gán lại từng cue thủ công.
- Với SRT Anh/Nhật, nút `Dịch sang Việt` tự đọc toàn bộ kịch bản, sau đó Hy-MT2 dịch sát nghĩa và Qwen3 14B biên tập khẩu ngữ, mạch đối đáp cùng xưng hô.
- Với SRT Việt, app bỏ qua bước dịch và đi thẳng sang Test 5 phút hoặc tạo giọng.
- Bộ nhớ văn phong local học tối đa 2.000 câu do người dùng sửa trên bản máy dịch, có thể tắt hoặc xóa trong Settings. SRT Việt nhập sẵn không được dùng để học.
- Timeline cho phép sửa text, timestamp, speaker, tốc độ và âm lượng; tạo lại một câu mà không render lại toàn phim.
- VieNeu-TTS v3 Turbo chạy qua `audio.cpp`, hỗ trợ bảy preset giọng nam/nữ và cache theo nội dung, giọng cùng thông số cue. Câu chỉ được viết gọn sau khi đo thời lượng audio thật và thử tạo lại tối đa hai lần.
- Mel-Band RoFormer tách vocal khỏi nền. Khi model không có hoặc tách lỗi, app tự dùng sidechain ducking trên audio gốc.
- Test 5 phút trong app; xuất MP4 lồng tiếng hoặc MKV gồm audio Việt, audio gốc và SRT Việt.
- Model Manager tải model ở lần đầu, resume file `.part`, kiểm dung lượng/checksum và chạy offline sau khi cài xong.
- Nhật ký lỗi lưu tối đa 300 sự kiện để chẩn đoán download, model và pipeline.

Không cần Ollama hoặc Python. Các runtime native được đóng gói trong app; model lớn nằm ngoài installer.

## Model mặc định

- ASR: Whisper large-v3-turbo Q5 + Silero VAD.
- Dịch nghĩa: Hy-MT2 7B Q4_K_M.
- Biên tập: Qwen3 14B Q4_K_M.
- TTS: VieNeu-TTS v3 Turbo Q8, 48 kHz.
- Source separation: Mel-Band RoFormer Q8.

## Development

```bash
npm install
./scripts/prepare-engines-macos.sh
npm run tauri dev
```

Script engine đã ghim phiên bản và build/copy `whisper-cli`, `llama-cli`, `llama-server`, `audiocpp_cli`, FFmpeg cùng FFprobe vào Tauri resources. `audio.cpp` ưu tiên Metal khi máy có Metal compiler đầy đủ và tự build CPU fallback khi chỉ có Command Line Tools.

## Kiểm tra và đóng gói

```bash
npm run build
cd src-tauri && cargo test
cd .. && npm run tauri build
```

Unit test bao phủ SRT, chia cảnh theo ngữ nghĩa, nhóm câu nhiều cue, giữ ID và timestamp, schema dự án cũ, bộ nhớ dịch, gán giọng mặc định và cache invalidation. Bản macOS local dùng ad-hoc signing; phát hành cho máy khác vẫn cần Developer ID và notarization.
# longTiengVietFilm
