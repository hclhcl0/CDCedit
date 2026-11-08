# Sesame Studio: phân tích và phương án build

Ngày khảo sát: 07/11/2026. Đối tượng: bản cài được tham chiếu bởi `C:\Users\Admin\Desktop\Sesame Studio.lnk`.

## 1. Kết luận

Sesame Studio là ứng dụng desktop điều phối quy trình làm video AI, kết hợp dịch vụ bên ngoài và xử lý media cục bộ. Có thể xây một ứng dụng tương tự theo từng giai đoạn. Chưa đủ dữ liệu để tái tạo chính xác bản build gốc: bản cài không cung cấp bộ cấu hình phát triển, dependency lock và pipeline đóng gói trong package.json đã kiểm tra.

Đề xuất mặc định: xây sản phẩm độc lập nếu chưa có source và quyền sửa đổi từ tác giả. Nếu mục tiêu là build chính Sesame Studio, cần bộ source được cấp quyền cùng cấu hình phát hành gốc trước khi chốt lệnh build.

## 2. Dữ liệu đã xác minh

| Hạng mục | Kết quả |
|---|---|
| Thư mục cài | `%LOCALAPPDATA%\Programs\Sesame Studio` |
| Phiên bản package | `1.0.0` |
| Nền tảng | Electron; có Chromium resources, LICENSE.electron.txt, executable desktop |
| Điểm vào khai báo | `resources/app/main.js` |
| Cấu trúc | `main.js`, `preload.js`, `engine/`, `ui/` |
| Giao diện | HTML/CSS/JavaScript; trang thực tế `ui/index.html` |
| Công cụ đóng kèm | FFmpeg, ffprobe, yt-dlp, Deno và DLL media |
| Dữ liệu theo README | `%APPDATA%\Sesame Studio`; đầu ra mặc định `Videos\Sesame Studio` |
| Build metadata | package.json chỉ có thông tin ứng dụng; không có scripts/dependencies/devDependencies |

Nguồn: metadata bản cài, README, LICENSE, danh sách file và giao diện đang chạy. Không đọc dữ liệu tài khoản, token hay hồ sơ Chrome.

## 3. Bản đồ chức năng

| Nhóm | Hành vi quan sát / tài liệu bản cài | Ưu tiên xây mới |
|---|---|---|
| Studio | Tạo ảnh/video, prompt, tối ưu prompt, ảnh tham chiếu, tỷ lệ khung hình, hàng đợi, chạy lại lỗi, ghép video | P0 |
| Tư liệu | Nhân vật tối đa 3 ảnh, mẫu prompt, người mẫu cố định, ảnh luân phiên; tham chiếu tên/@tên | P0 |
| Kịch bản / prompt đồng bộ | Có mục điều hướng và module riêng; cần khảo sát sâu đầu vào/đầu ra | P1 |
| Thư viện / dự án | Quản lý tài nguyên và dự án | P0 |
| Edit/Ghép / Cắt | README xác nhận dùng cục bộ và kèm FFmpeg | P0 |
| TTS | README nêu Edge, VieNeu, Google; Edge/VieNeu cần Python và gói riêng | P1 |
| Tài khoản / provider | README nêu Muse, Google Flow, Dola, Seedance qua Chrome | P1, từng provider |
| Đăng Reels | README nêu Facebook token và quyền tương ứng | P2 |

P0 là phạm vi MVP đề xuất; không phải mức độ đã kiểm chứng của ứng dụng gốc. Chưa chạy job AI, render, TTS hoặc đăng bài nên chưa kết luận các tính năng đó hoạt động đầu cuối.

## 4. Kiến trúc đề xuất cho sản phẩm độc lập

Luồng chính: UI → dịch vụ ứng dụng → hàng đợi → provider AI hoặc worker media → kho tài nguyên → editor → xuất video.

- UI desktop: Electron và TypeScript nếu làm ứng dụng riêng, để giữ hướng triển khai gần với nền tảng đã quan sát. Dùng preload và IPC có schema; chỉ cung cấp thao tác cần thiết cho renderer.
- Project service: quản lý Project, Character, Asset, Scene, GenerationJob, Timeline và ExportJob. SQLite lưu metadata; file media nằm ngoài database.
- Job service: trạng thái queued/running/succeeded/failed/cancelled, giới hạn đồng thời theo provider, retry có giới hạn, lưu tiến độ và khôi phục sau khi mở lại app.
- Provider adapter: giao diện chung submit/poll/cancel/download, khai báo capability và hạn mức; triển khai một provider trước. Ưu tiên API được cung cấp chính thức; nếu dùng luồng trình duyệt thì cần khảo sát riêng độ bền và điều kiện tích hợp.
- Media worker: gọi FFmpeg/ffprobe bằng danh sách đối số, theo dõi tiến độ, hủy tiến trình; chuẩn hóa fps, codec, kích thước và audio trước ghép.
- Credential service: lưu bí mật bằng cơ chế bảo vệ của hệ điều hành; loại bỏ khỏi log và gói build.

Nếu mục tiêu là bổ sung vào Concat hiện tại: dùng giao diện Slint/Rust của Concat và kiểm tra khả năng tái sử dụng các crate project/media/export/speech đang có. Không nhúng cả runtime Electron chỉ để dựng lại các màn hình. Tên crate chỉ là dấu hiệu cấu trúc, chưa phải xác nhận API có thể dùng ngay.

## 5. Hai đường build

### A. Build chính Sesame Studio từ source được cấp quyền

1. Thu thập repository gốc, commit/tag phát hành, lockfile, phiên bản Node/Electron, cấu hình installer, assets và manifest công cụ nhị phân.
2. Kiểm kê import/runtime dependency từ source được cấp quyền; khôi phục scripts dev/package/make và khóa phiên bản. Không đoán dependency chỉ từ tên module.
3. Chạy dev trong môi trường sạch; xác minh preload, đường dẫn resources và thư mục ghi dữ liệu.
4. Đóng gói app; đưa FFmpeg/ffprobe và DLL vào resources bên ngoài archive khi cần thực thi. Python/TTS phải có lựa chọn đóng kèm hoặc hướng dẫn thiết lập rõ ràng.
5. Tạo bộ cài Windows x64, kiểm tra nâng cấp/gỡ cài và bảo toàn dự án; thêm ký số khi phát hành.
6. Chạy bộ kiểm tra chấp nhận ở mục 7 trên máy sạch.

Electron khuyến nghị Electron Forge cho đóng gói/phân phối: https://www.electronjs.org/docs/latest/tutorial/application-distribution . Đây là lựa chọn đề xuất, chưa xác minh công cụ mà tác giả Sesame đã dùng.

Không có lệnh `npm run build` đáng tin cậy cho bản cài hiện tại vì package.json đã kiểm tra không định nghĩa script đó. Shortcut chỉ mở bản đã đóng gói, không cung cấp quy trình build.

### B. Xây sản phẩm độc lập — đề xuất khi chưa có bộ source được cấp quyền

| Mốc | Đầu ra có thể nghiệm thu | Ước lượng sơ bộ |
|---|---|---|
| 0 | Chốt app riêng hay tích hợp Concat, đặc tả dự án/job/provider, mẫu dữ liệu | 2–3 ngày |
| 1 | Tạo/mở/lưu dự án, nhập media, thư viện, cắt/ghép/xuất offline | 1–2 tuần |
| 2 | Một provider AI, hàng đợi bền vững, hủy/retry, tải kết quả vào thư viện | 1–2 tuần |
| 3 | Kịch bản nhiều cảnh, nhân vật tham chiếu, TTS và dựng bản nháp | 1–2 tuần |
| 4 | Installer, kiểm tra máy sạch, xử lý lỗi và hoàn thiện | Khoảng 1 tuần |

Ước lượng cho một lập trình viên có kinh nghiệm, không bao gồm chờ quyền truy cập dịch vụ. MVP một provider khoảng 4–7 tuần; phạm vi toàn bộ provider, editor nâng cao và đăng bài cần ước lượng lại sau prototype. Chi phí dịch vụ AI phụ thuộc provider, hạn mức và khối lượng, chưa tính vào đây.

## 6. Điểm cần giải quyết sớm

- Bản cài có nhiều file/module lớn: danh sách file không đủ đánh giá chất lượng nội bộ hoặc kết luận lỗi kiến trúc.
- Tích hợp dựa trên Chrome phụ thuộc giao diện và phiên đăng nhập; cần thử riêng từng provider và có đường phục hồi khi phiên hết hạn.
- FFmpeg đi kèm DLL: đóng gói thiếu thư viện sẽ lỗi trên máy sạch; không chỉ kiểm tra máy phát triển.
- TTS có phụ thuộc Python khác nhau; nên cho chạy worker riêng và hiển thị rõ tình trạng sẵn sàng.
- Render phải xử lý video khác codec/fps/kích thước, file tên tiếng Việt và hủy giữa chừng.
- LICENSE bản cài ghi hạn chế sửa đổi, đổi thương hiệu, phát hành bản sửa đổi và trích xuất mã; kế hoạch tái sử dụng cần làm rõ quyền với tác giả. Báo cáo này không sao chép implementation vào Concat.

## 7. Điều kiện nghiệm thu MVP

1. Cài và mở trên Windows sạch; đường dẫn có dấu/khoảng trắng hoạt động.
2. Tạo dự án, nhập media, đóng/mở lại không mất dữ liệu.
3. Cắt và ghép media mẫu khác thông số; MP4 đầu ra phát được và audio khớp.
4. Gửi một job AI với dữ liệu thử; lưu kết quả và liên kết đúng scene.
5. Đóng app giữa hàng đợi, mở lại không gửi trùng job đã hoàn thành; xử lý rõ job đang chạy chưa biết kết quả.
6. Hủy render/job, hết dung lượng, mất mạng và lỗi xác thực có thông báo có thể xử lý.
7. Bộ cài không chứa token, hồ sơ cá nhân hoặc đường dẫn tuyệt đối của máy phát triển.

## 8. Phạm vi khảo sát và bước tiếp theo

Đã mở ứng dụng và đọc màn hình Studio/Tư liệu, README, metadata và danh sách module. Chưa chỉnh sửa bản cài, chưa build source, chưa xác minh toàn bộ tab hoặc hiệu năng.

Bước triển khai đầu tiên đề xuất: MVP offline (dự án → thư viện → cắt/ghép → xuất), sau đó thêm một provider AI. Nếu chọn tích hợp Concat, cần một vòng kiểm tra API hiện có trước khi lập danh sách thay đổi cụ thể.
