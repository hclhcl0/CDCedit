# Nhập media từ liên kết

Trong thư viện Media, bấm **Từ liên kết**. Windows x64 có bộ tải tích hợp đầy đủ. Android có luồng tải trực tiếp file media Drive công khai, dùng HTTP native nên không chạy bộ EXE.

1. Bấm **Cài công cụ tải** lần đầu. CDCedit tải yt-dlp, Deno, rclone từ bản phát hành chính thức và FFmpeg theo bản đã ghim trong dự án. Các gói phải khớp SHA-256 trước khi được cài vào thư mục dữ liệu ứng dụng. **Cập nhật công cụ tải** dùng cùng cơ chế kiểm tra.
2. Dán các link YouTube, Facebook, TikTok hoặc Google Drive, mỗi dòng một link, rồi bấm **Đọc liên kết**.
3. Chọn file cần tải, chất lượng tối đa hoặc chỉ âm thanh. Các lựa chọn chất lượng áp dụng cho nền tảng video; Drive luôn tải file gốc.
4. Bấm **Tải và nhập**. File hoàn tất được lưu dưới `media/downloads/` của dự án, rồi được nhập bằng luồng media bình thường. Các mục lỗi vẫn được chọn để thử lại, các mục thành công được bỏ chọn.

Thẻ video có tên, thời lượng, các mức phân giải và thumbnail khi CDN cung cấp ảnh phù hợp. Các mức 1080p/720p/480p là giới hạn trên, không phải nâng độ phân giải nguồn.

## Google Drive

File media Drive công khai được đọc qua đường tải file gốc của Google. Với thư mục hoặc file riêng tư, bấm **Kết nối Google Drive**, hoàn thành đăng nhập trên trình duyệt rồi bật **Dùng tài khoản Drive**. CDCedit dùng rclone với phạm vi `drive.readonly`; không có lệnh sửa/xoá/tải lên Drive trong luồng nhập.

Link thư mục hiển thị file video, âm thanh và hình ảnh nằm trực tiếp trong thư mục đó để chọn. Không tự duyệt toàn bộ cây thư mục. File riêng tư đơn lẻ được nhận diện bằng ID trước khi tải; tên file gốc do Drive cung cấp khi tải. File đã bị chặn tải hoặc tài khoản không được cấp quyền sẽ báo lỗi.

Phiên Google được lưu ở thư mục cấu hình CDCedit, tách khỏi dự án. **Ngắt kết nối** xoá cấu hình phiên cục bộ; muốn thu hồi quyền OAuth trên Google, dùng phần quản lý ứng dụng đã kết nối của tài khoản Google. Không chia sẻ thư mục cấu hình ứng dụng.

## Phiên đăng nhập trang video

Nếu trang yêu cầu đăng nhập, có thể chọn file `cookies.txt` định dạng Netscape do bạn chủ động xuất. CDCedit không tự lấy cookie trình duyệt. Cookie được truyền cho công cụ tải tại máy và không được đưa vào file dự án hoặc log hoạt động.

## Huỷ và giới hạn

- Đóng hộp thoại không huỷ tác vụ; mở lại để xem tiến trình. **Huỷ** dừng tiến trình tải và các tiến trình con. Đóng hoặc chuyển dự án cũng yêu cầu huỷ tác vụ của dự án cũ.
- File `.part` không được nhập vào thư viện. Dữ liệu tải dở có thể còn trong thư mục tải riêng của lần thử, để người dùng kiểm tra hoặc dọn dẹp.
- Mỗi lần đọc tối đa 50 link và 1.000 file. Không tự tải playlist hoặc livestream.
- Tiến trình tải có thể bị giới hạn bởi đăng nhập, quota, vị trí mạng hoặc thay đổi của nền tảng. Cập nhật công cụ rồi thử lại khi trình trích xuất bị lỗi.
- Bộ công cụ tải được cài theo yêu cầu nên không cần đưa toàn bộ vào APK hoặc bộ cài chính. Android có nút nhập link trong bảng Media, hỗ trợ file Drive công khai. Bộ tải mạng xã hội, thư mục Drive và đăng nhập Drive chưa có runtime tương ứng trên Android; giao diện nêu rõ giới hạn này.

Nguồn: [yt-dlp](https://github.com/yt-dlp/yt-dlp), [rclone Drive](https://rclone.org/drive/), [cấu hình rclone](https://rclone.org/commands/rclone_config_create/).
