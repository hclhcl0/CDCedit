# Chọn giọng lồng tiếng

Mở Chuyển văn bản thành giọng nói, chọn nhà cung cấp rồi chọn mô hình.

Gemini có thanh Temperature và ô nhập số từ 0 đến 2, mặc định 1. Lựa chọn được lưu cùng cài đặt TTS. Giá trị thấp ưu tiên sự ổn định, giá trị cao tăng tính đa dạng trong cách đọc. ElevenLabs và Local không hiện tham số Gemini này.

Gemini 2.5 Flash Preview TTS (`gemini-2.5-flash-preview-tts`) dùng Free Tier khi tài khoản có quyền và còn hạn mức. Gemini 2.5 Pro Preview TTS (`gemini-2.5-pro-preview-tts`) không có Free Tier theo bảng giá Google kiểm tra ngày 29/10/2026. Ứng dụng không tự đổi sang model trả phí khi hết quota. Hai model 2.5 dùng GenerateContent với âm thanh PCM 24 kHz được đóng gói thành WAV; các model 3.8 tiếp tục dùng Interactions. Với 2.5, chỉ dẫn phong cách đi trong prompt theo giao thức model; cần nghe trước để kiểm tra kết quả.

- **Local:** dùng danh sách giọng và giọng mẫu mà mô hình đã cài hỗ trợ. Không tự gán nhãn Bắc/Trung/Nam cho mô hình không cung cấp các giọng này.
- **Gemini:** chọn một trong 30 giọng studio, lọc nam/nữ khi có thông tin. Giọng chưa có nhãn giới tính nằm trong mục Tất cả. Chọn Bắc/Trung/Nam và phong cách đọc; các lựa chọn này được gửi trong metadata, không chèn vào văn bản đọc. Đây là chỉ dẫn cho mô hình, không bảo đảm giọng vùng miền riêng. Vẫn có thể nhập ID giọng tuỳ chỉnh.
- **ElevenLabs:** nhập API key dùng chung trong Cài đặt, bấm Tải / làm mới giọng của tài khoản, rồi chọn giọng. Mặc định chỉ hiện giọng có thông tin hỗ trợ người dùng miễn phí hoặc thuộc danh sách mặc định. Nếu API trả về quyền miễn phí bị từ chối, giọng đó bị loại dù thuộc danh sách mặc định. Giọng không rõ quyền không được coi là miễn phí. Vẫn trừ hạn mức ký tự và quyền truy cập API phụ thuộc tài khoản.

Bộ lọc vùng miền ElevenLabs chỉ dùng nhãn vùng miền rõ ràng từ nhà cung cấp; danh sách có thể trống. Tên giọng hiển thị kèm các nhãn ngôn ngữ, vùng giọng và phong cách do API cung cấp. Các phong cách có sẵn điều chỉnh tham số của mô hình hỗ trợ; model chỉ có bộ điều khiển cơ bản không hiện preset phong cách mở rộng. Muốn dùng ID tuỳ chỉnh chưa xác minh, bật hiện giọng trả phí hoặc chưa rõ quyền.

Lựa chọn giọng, mô hình, vùng miền và phong cách được lưu cùng cài đặt TTS. Tạo giọng, nghe trước và áp dụng vào timeline như trước. API key không được ghi vào dự án.

Tham khảo: [Gemini TTS](https://ai.google.dev/gemini-api/docs/speech-generation), [bảng giá Gemini](https://ai.google.dev/gemini-api/docs/pricing), [ElevenLabs voice catalog](https://elevenlabs.io/docs/api-reference/voices/search).
