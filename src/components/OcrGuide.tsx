import { useEffect, useRef } from "react";
import { ScanText, X } from "lucide-react";

const chapters = [
  ["prepare", "Chuẩn bị"],
  ["capture", "Thu phụ đề"],
  ["recognize", "Chạy OCR"],
  ["review", "Sửa và lưu SRT"],
  ["trial", "Nghe WAV thử"],
  ["full", "Xuất WAV toàn tập"],
  ["troubleshoot", "Xử lý lỗi"],
  ["checklist", "Kiểm tra trước khi xong"],
] as const;

export function OcrGuide({ onClose }: { onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const modal = dialog.current;
    const overflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    modal?.showModal();
    closeButton.current?.focus();
    return () => {
      modal?.close();
      document.body.style.overflow = overflow;
      previous?.focus();
    };
  }, []);

  const jump = (id: string) => {
    const heading = dialog.current?.querySelector<HTMLElement>(`#ocr-guide-${id}`);
    heading?.scrollIntoView({ block: "start" });
    heading?.focus({ preventScroll: true });
  };

  return (
    <dialog ref={dialog} id="ocr-guide" className="ocr-guide" aria-modal="true" aria-labelledby="ocr-guide-title" aria-describedby="ocr-guide-intro" onKeyDown={event => {
      if (event.key !== "Tab") return;
      const targets = [...event.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), [tabindex="0"]')];
      const first = targets[0], last = targets[targets.length - 1];
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    }} onCancel={event => { event.preventDefault(); onClose(); }} onClick={event => {
      if (event.target !== event.currentTarget) return;
      const bounds = event.currentTarget.getBoundingClientRect();
      if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) onClose();
    }}>
      <header className="ocr-guide-header">
        <div><p className="eyebrow">HƯỚNG DẪN</p><h2 id="ocr-guide-title"><ScanText size={20} aria-hidden="true" /> OCR phụ đề tiếng Việt</h2></div>
        <button ref={closeButton} type="button" className="icon-button" aria-label="Đóng hướng dẫn OCR" title="Đóng hướng dẫn OCR" onClick={onClose}><X size={18} /></button>
      </header>
      <div className="ocr-guide-layout">
        <nav className="ocr-guide-nav" aria-label="Mục lục hướng dẫn OCR">
          {chapters.map(([id, label], index) => <button type="button" key={id} onClick={() => jump(id)}><span>{index + 1}</span>{label}</button>)}
        </nav>
        <div className="ocr-guide-content" tabIndex={0} aria-label="Nội dung hướng dẫn OCR">
          <p id="ocr-guide-intro" className="ocr-guide-intro">Phụ đề nằm sẵn trong hình video? Thu ảnh vùng phụ đề bằng extension Chrome, chuyển ảnh thành chữ trong app, kiểm tra và sửa SRT, rồi tạo giọng đọc tiếng Việt. Không cần tải video về máy.</p>
          <p className="ocr-guide-note"><strong>Luồng làm việc:</strong> Chrome thu ảnh + timestamp → app OCR → bạn kiểm tra và lưu phụ đề → WAV thử → WAV toàn tập. “AI dịch” là trạng thái engine dịch của app; phụ đề đã là tiếng Việt thì không cần bước dịch.</p>

          <section aria-labelledby="ocr-guide-prepare">
            <h3 id="ocr-guide-prepare" tabIndex={-1}>1. Chuẩn bị một lần</h3>
            <ol>
              <li><strong>Extension Chrome:</strong> mở <code>chrome://extensions</code>, bật Developer mode, chọn Load unpacked và thư mục <code>browser-extension/dist</code> trong dự án. Nếu đã nạp extension thì không cần nạp lần nữa. Thư mục này phải là bản đã build.</li>
              <li><strong>Helper local:</strong> helper là chương trình nhỏ nhận ảnh từ Chrome và lưu vào dữ liệu của app, kể cả khi app đóng. Helper phải được cài/đăng ký cho đúng ID extension; chỉ build app không tự cài helper. Trước khi thu, kiểm tra recorder hiện tên/phiên bản helper, không còn báo <strong>Chưa kết nối helper</strong>.</li>
              <li><strong>Engine và model:</strong> cần engine OCR tiếng Việt để đọc ảnh. Để tạo WAV, mở mục <strong>Model</strong> và chuẩn bị model <strong>VieNeu-TTS v3 Turbo Q8</strong> cùng dữ liệu giọng tương ứng. Thiếu engine/model thì làm theo thông báo lỗi trước khi chạy tiếp.</li>
              <li><strong>Phim:</strong> mở tập phim trong Chrome, chờ video phát được và phụ đề hiện rõ. URL chính đang dùng: <code>https://animevietsub.nl/phim/tokyo-revengers-a4036/tap-01-75695.html</code>.</li>
            </ol>
            <p>OCR đọc chữ trong ảnh, không lấy track phụ đề và không tự nhận dạng lời nói. Hãy chỉ thu nội dung bạn có quyền sử dụng; ảnh, phụ đề và WAV được xử lý/lưu cục bộ.</p>
          </section>

          <section aria-labelledby="ocr-guide-capture">
            <h3 id="ocr-guide-capture" tabIndex={-1}>2. Chọn vùng và thu phụ đề trong Chrome</h3>
            <ol>
              <li>Mở extension từ tab phim để vào recorder. Sau cập nhật, kiểm tra đúng phiên bản mới dưới tên Long Tieng. Với player trong iframe, bấm <strong>Liên kết iframe</strong> và cấp quyền tùy chọn nếu được yêu cầu, rồi chọn đúng mục <strong>Video</strong>. Quyền này giúp xác định khung chứa video; không cần đăng ký lại helper.</li>
              <li>Chọn <strong>Chế độ thu</strong> trước khi lưu ảnh mẫu hoặc Thu: <strong>Lưu mọi ảnh</strong> là mặc định lần đầu. <strong>Thông minh · thử nghiệm</strong> giữ ảnh đại diện khi nét chữ ổn định, vẫn theo dõi thời gian và lưu mọi mẫu khi không chắc. Extension nhớ lựa chọn cho lần mở sau nếu helper hỗ trợ. Cả hai chế độ tự nén PNG lossless, giữ màu, kích thước và dấu chữ; nếu nén lỗi vẫn lưu ảnh gốc. Mức tiết kiệm phụ thuộc phim, chưa được bảo đảm 70%. Kết thúc phiên trước khi đổi chế độ; mở lại extension từ tab phim nếu stream đã ngắt.</li>
              <li>Đợi một câu phụ đề hiện ra. Nhấn biểu tượng cắt vùng <strong>Chọn vùng subtitle</strong>, kéo bao quanh toàn bộ một hoặc hai dòng chữ. Chừa khoảng cho dấu tiếng Việt, đầu/cuối câu và dòng thứ hai; tránh logo, thanh điều khiển và phần hình không cần thiết.</li>
              <li>Kiểm tra vùng xem trước. Có thể dùng biểu tượng máy ảnh <strong>Lưu một ảnh mẫu</strong> để xem chữ có đầy đủ, rõ nét hay bị cắt. Nếu thay đổi kích thước player hoặc chế độ toàn màn hình, kiểm tra lại vùng trước khi tiếp tục.</li>
              <li>Để có cả tập, bắt đầu gần thời điểm <strong>0:00</strong>, nhấn <strong>Thu</strong> rồi phát phim ở tốc độ <strong>1×</strong> đến hết. Không bỏ qua opening/ending hoặc tua qua đoạn cần lồng tiếng. Giữ máy thức và tab phim hoạt động ổn định.</li>
              <li>Theo dõi cả trạng thái thu và số ảnh tăng. <strong>Chờ mẫu đầu tiên</strong> chưa có dữ liệu playback; <strong>Đã tạm dừng thu</strong> nghĩa là không thu thêm dù phim có thể vẫn phát. Kiểm tra helper, số ảnh chờ và bộ đệm. Khi xong, nhấn biểu tượng vuông <strong>Kết thúc phiên</strong>, chờ <strong>Đã lưu ảnh và timestamp</strong> rồi mới đóng tab/Chrome.</li>
            </ol>
            <p className="ocr-guide-note"><strong>“Chờ lưu” chưa phải hoàn tất.</strong> Extension còn phải gửi hết ảnh đang chờ và nhận xác nhận lưu từ helper. Ở bước này mới có ảnh và thời gian, chưa có SRT đã kiểm tra.</p>
            <p><strong>Thời gian lấy theo video:</strong> pause hoặc buffering không tính như thời gian phim đang chạy. Tua tạo đoạn thu riêng và có thể để lại khoảng chưa thu. Nếu bạn tạm dừng thu, phim không tự dừng; hãy tự pause phim để tránh mất câu.</p>
            <p><strong>Mất kết nối:</strong> extension thử kết nối lại và giữ tạm tối đa 128 MiB ảnh chưa được xác nhận lưu. Bộ đệm đầy thì dừng thu, không điều khiển phim. Sau khi kết nối phục hồi và gửi hết ảnh chờ, bạn chủ động nhấn Thu để tiếp tục; khoảng gián đoạn vẫn được ghi nhận. Khởi động lại Chrome không tự thu lại.</p>
          </section>

          <section aria-labelledby="ocr-guide-recognize">
            <h3 id="ocr-guide-recognize" tabIndex={-1}>3. Mở bản thu và chuyển ảnh thành chữ</h3>
            <ol>
              <li>Mở app <strong>Lồng tiếng</strong>, chọn <strong>OCR</strong> ở thanh bên trái. Các phiên đã lưu nằm trong danh sách <strong>Bản thu</strong>; không cần tìm SRT trong Downloads hay nhập ZIP.</li>
              <li>Chọn bản thu đúng tập phim. Nếu chưa thấy, nhấn biểu tượng <strong>Làm mới thư viện</strong> và kiểm tra recorder đã hoàn tất lưu.</li>
              <li>Nếu bản thu mới chỉ có ảnh, nhấn <strong>Chạy OCR</strong>. Đợi xử lý xong rồi mở danh sách câu. OCR chỉ xử lý phiên thu đã kết thúc và lưu bền vững; không cần phát phim lại trong lúc này.</li>
              <li>Nếu đã có kết quả, mở trực tiếp để kiểm tra. <strong>OCR lại</strong> chạy nhận dạng lại toàn bản thu và có bước xác nhận; các bản sửa đã lưu vẫn được giữ riêng, không tự bị ghi đè bằng kết quả mới.</li>
            </ol>
            <p>OCR có thể đọc sai dấu, tên riêng, chữ nhỏ hoặc chữ trên nền phức tạp. Độ tin cậy cao không đảm bảo câu đúng, và có SRT không đồng nghĩa đã thu đủ cả tập.</p>
          </section>

          <section aria-labelledby="ocr-guide-review">
            <h3 id="ocr-guide-review" tabIndex={-1}>4. Đối chiếu, sửa và lưu phụ đề SRT</h3>
            <ol>
              <li>Ở chế độ <strong>Câu</strong>, chọn từng câu để xem chữ, thời gian và ảnh đối chiếu. Phóng to ảnh hoặc xem ảnh lân cận khi cần. Dùng bộ lọc <strong>Nghi vấn</strong> để ưu tiên lỗi, rồi <strong>Chưa kiểm tra</strong> để không bỏ sót câu.</li>
              <li>Sửa đúng lời thoại nhìn thấy trong ảnh: dấu tiếng Việt, tên nhân vật, từ thiếu/thừa và xuống dòng. Đừng tự thêm lời vào đoạn không có bằng chứng.</li>
              <li>Kiểm tra thời gian bắt đầu/kết thúc theo video. Các ô sửa thời gian dùng <strong>mili giây (ms)</strong>: 12,5 giây = <strong>12500 ms</strong>. Thời gian kết thúc phải sau bắt đầu; tránh câu chồng lên nhau hoặc vượt cuối video.</li>
              <li>Dùng tách câu/gộp câu khi OCR chia không đúng, hoặc xóa câu rác nếu đó là logo/chữ không phải phụ đề. Gộp chỉ áp dụng với các câu phù hợp trong cùng đoạn thu; kiểm tra lại thời gian sau thao tác.</li>
              <li>Bấm câu đầu, giữ <strong>Shift</strong> rồi bấm câu cuối để chọn nhiều câu liên tiếp trong danh sách đang hiển thị. Hai nút xanh dương <strong>Xóa câu đã chọn</strong> và <strong>Gộp câu đã chọn</strong> xử lý cả dải; có thể <strong>Hoàn tác</strong> trước khi lưu. Gộp giữ khoảng thời gian tổng, các câu trùng chỉ giữ một lần, câu khác nội dung được nối theo thứ tự. Khi lọc làm dải bị ngắt hoặc khác đoạn thu, bỏ bộ lọc/chọn lại trước khi gộp.</li>
              <li>Nút hồng <strong>Rút gọn câu</strong> xử lý toàn bản thu, kể cả câu đang bị ẩn bởi tìm kiếm/bộ lọc: gộp các câu trùng hoặc gần giống liền nhau, cùng đoạn thu và cách nhau tối đa 1 giây. App giữ một nội dung, thời gian bắt đầu sớm nhất/kết thúc muộn nhất và toàn bộ ảnh đối chiếu. Câu đã gộp cần kiểm tra lại; có thể <strong>Hoàn tác</strong> trước khi <strong>Lưu</strong>.</li>
              <li>Đối chiếu xong, đánh dấu <strong>Đã kiểm tra</strong>. Nếu sửa chữ hoặc thời gian sau đó, câu cần được kiểm tra lại. Nhấn <strong>Lưu</strong> để tạo bản phụ đề đã lưu.</li>
              <li>Mở chế độ <strong>SRT</strong> để xem định dạng phụ đề; <strong>OCR gốc</strong> để xem kết quả nhận dạng ban đầu. Dùng biểu tượng <strong>Xuất SRT đã lưu</strong> hoặc xuất TXT khi cần file riêng. Nếu còn bản nháp, hãy lưu trước khi xuất/tạo WAV.</li>
            </ol>
            <p className="ocr-guide-note"><strong>Ví dụ một câu SRT:</strong></p>
            <pre aria-label="Ví dụ định dạng SRT">{"1\n00:00:12,500 --> 00:00:14,800\nChúng ta về nhà thôi."}</pre>
            <p>Bản nháp được giữ khi chuyển mục trong app. Nếu đổi bản thu có thay đổi chưa lưu, hãy đọc hộp xác nhận. Khi gặp xung đột bản lưu, giữ bản nháp và đối chiếu bản mới; không ghi đè vội.</p>
          </section>

          <section aria-labelledby="ocr-guide-trial">
            <h3 id="ocr-guide-trial" tabIndex={-1}>5. Tạo WAV thử trước khi chạy cả tập</h3>
            <ol>
              <li>Sau khi lưu phụ đề, mở chế độ <strong>WAV</strong>, chọn phạm vi <strong>Thử</strong>.</li>
              <li>Nhập <strong>Bắt đầu (giây)</strong>, chọn đoạn thử <strong>30 giây</strong> hoặc <strong>60 giây</strong> và <strong>Giọng mặc định</strong>. Khác với editor, ô này dùng giây: 85,65 giây tương ứng 85650 ms.</li>
              <li>Chọn đoạn nằm trong một khoảng thu liên tục, không cắt ngang đầu/cuối câu. Mọi câu trong đoạn phải đã kiểm tra và lưu, không chồng thời gian. Nếu có lỗi, sửa ở Câu, đánh dấu lại và Lưu.</li>
              <li>Nhấn <strong>Tạo WAV thử</strong>, nghe bằng player để kiểm tra phát âm và tốc độ. Dùng <strong>Xuất WAV</strong> nếu cần file thử.</li>
            </ol>
            <p className="ocr-guide-note"><strong>WAV luôn neo tại mốc 0 của video.</strong> Ví dụ thử 30 giây từ 85,65 giây: file dài 115,65 giây, có khoảng lặng đầu để giữ vị trí câu trên phim. Player thử tự chuyển đến đoạn đã chọn; đây không phải lỗi âm thanh.</p>
            <p>Nếu lời đọc dài hơn khoảng câu cho phép, app báo lỗi thay vì cắt mất lời. Hãy kiểm tra chữ/thời gian thực tế trước khi tạo lại.</p>
          </section>

          <section aria-labelledby="ocr-guide-full">
            <h3 id="ocr-guide-full" tabIndex={-1}>6. Tạo và xuất WAV toàn tập</h3>
            <ol>
              <li>Kiểm tra và lưu <strong>tất cả</strong> các câu, rồi vào <strong>WAV → Toàn tập</strong>. Đọc danh sách lỗi, số câu đã kiểm tra, thời lượng và các khoảng chưa thu trước khi chạy.</li>
              <li>Chọn giọng mặc định. Có thể gán giọng riêng cho từng câu trong danh sách giọng; app không tự đoán nhân vật. Nếu đã biết thời lượng video, app dùng thời lượng đó. Nếu chưa biết, bạn phải nhập thời lượng và chấp nhận bản Một phần.</li>
              <li>Nên thu đủ trước khi tạo cả tập. Chỉ bật <strong>“Tôi chấp nhận bản Một phần khi thiếu vùng thu hoặc chưa biết thời lượng”</strong> khi hiểu và chấp nhận thiếu dữ liệu. Đoạn chưa thu sẽ là khoảng lặng, không được tự khôi phục lời. Lựa chọn này không bỏ qua câu chưa kiểm tra, thời gian sai hay chồng câu.</li>
              <li>Nhấn <strong>Tạo WAV toàn tập</strong>. Theo dõi tiến độ; có thể <strong>Hủy</strong>. Nếu bị ngắt, mở lại và dùng <strong>Tiếp tục tạo WAV</strong>; các câu đã tạo hợp lệ được tái sử dụng từ cache. Khi báo câu quá dài, nhấn biểu tượng bút để quay về sửa đúng câu, kiểm tra và lưu rồi tạo lại.</li>
              <li>Sau khi tạo thành công, nghe kiểm tra rồi nhấn <strong>Xuất bộ WAV</strong>, chọn thư mục đích. App tạo thư mục riêng chứa <code>narration.vi.wav</code>, <code>narration.vi.json</code> và <code>subtitles.vi.srt</code>. Giữ cả ba file cùng nhau để giữ thông tin đồng bộ và bản phụ đề tương ứng.</li>
            </ol>
            <p>WAV chứa giọng đọc tiếng Việt, không kèm nhạc/âm thanh gốc của phim. Chức năng extension phát WAV đồng bộ với video Chrome là bước sau, chưa có trong flow hiện tại.</p>
            <p className="ocr-guide-note"><strong>“Đủ vùng thu” không bảo đảm chất lượng cả tập.</strong> Vẫn cần nghe/đối chiếu để phát hiện câu OCR bỏ sót hoặc thời gian chưa sát. Timestamp là ước lượng; chưa có bảo đảm sai số 200 ms.</p>
          </section>

          <section aria-labelledby="ocr-guide-troubleshoot">
            <h3 id="ocr-guide-troubleshoot" tabIndex={-1}>7. Lỗi thường gặp và cách xử lý</h3>
            <dl className="ocr-guide-faq">
              <dt>Recorder báo chưa kết nối helper hoặc “Chờ lưu” mãi</dt>
              <dd>Kiểm tra helper đã đăng ký đúng ID extension và còn đủ dung lượng ổ đĩa. Dùng nút Kết nối lại helper / thử lưu lại. Không đóng Chrome khi còn ảnh chờ; nếu bộ đệm đầy, tự pause phim trước rồi xử lý kết nối.</dd>
              <dt>Thu xong nhưng app không thấy bản thu</dt>
              <dd>Kiểm tra đã nhận “Đã lưu ảnh và timestamp”, mở OCR và làm mới thư viện. Chrome/helper và app phải chạy cùng tài khoản hệ điều hành và dùng cùng dữ liệu app. Có thể mở thư mục phiên từ recorder để kiểm tra; không cần nhập ZIP cho phiên native.</dd>
              <dt>Chạy OCR bị lỗi hoặc chữ sai nhiều</dt>
              <dd>Đọc lỗi engine OCR, kiểm tra bộ cài/helper có engine tương thích. Với chữ sai, xem ảnh mẫu: vùng có cắt dấu/dòng thứ hai không, player có đổi kích thước không? Sửa thủ công và kiểm tra từng câu; nếu mất chữ ngay trong ảnh thì cần thu lại đoạn đó.</dd>
              <dt>Nút tạo WAV bị khóa</dt>
              <dd>Lưu bản nháp trước, chuẩn bị model/giọng, rồi sửa các lỗi được liệt kê. Lọc Chưa kiểm tra để xử lý mọi câu còn thiếu; kiểm tra biên đoạn thử, chồng câu và khoảng chưa thu. Không bật Một phần chỉ để bỏ qua lỗi review.</dd>
              <dt>Lời đọc quá dài so với thời gian câu</dt>
              <dd>Dùng nút bút ở lỗi để mở câu. Đối chiếu chữ và timestamp, tách/gộp khi đúng với phụ đề thực tế, rồi kiểm tra và lưu lại. App không tự cắt lời hoặc bỏ câu để xuất được WAV.</dd>
              <dt>Hết dung lượng hoặc đạt giới hạn lưu trữ</dt>
              <dd>Dữ liệu thu mặc định tối đa 2 GiB/phiên, 10 GiB/thư viện; audio cũng có quota riêng. Hệ thống ngừng nhận/tạo thêm khi ổ trống quá thấp, cần ít nhất 1 GiB. Mở thư mục dữ liệu để sao lưu và chủ động quản lý; app không tự xóa dữ liệu của bạn.</dd>
            </dl>
          </section>

          <section aria-labelledby="ocr-guide-checklist">
            <h3 id="ocr-guide-checklist" tabIndex={-1}>8. Kiểm tra trước khi coi là xong</h3>
            <ul>
              <li>Recorder báo <strong>Đã lưu ảnh và timestamp</strong>, không còn ảnh chờ lưu.</li>
              <li>Bản thu đúng tập, không bỏ qua đoạn cần phụ đề; đã đọc các cảnh báo khoảng chưa thu.</li>
              <li>Đã đối chiếu chữ/thời gian, kiểm tra mọi câu và <strong>Lưu</strong> bản sửa.</li>
              <li>Đã nghe WAV thử, xử lý mọi lỗi và nghe kiểm tra WAV toàn timeline.</li>
              <li>Đã xuất đủ WAV + JSON + SRT; biết rõ bản xuất có phải <strong>Một phần</strong> không.</li>
            </ul>
            <p><strong>Ba trạng thái khác nhau:</strong> lưu ảnh xong ≠ OCR xong ≠ phụ đề cả tập đã kiểm tra. Chỉ việc đọc/đối chiếu của bạn mới xác nhận nội dung đúng và đủ.</p>
          </section>
        </div>
      </div>
      <footer className="ocr-guide-footer"><span>Đóng hướng dẫn không dừng tác vụ đang chạy.</span><button type="button" className="button secondary" onClick={onClose}>Đóng</button></footer>
    </dialog>
  );
}
