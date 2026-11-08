# Quản lý tài khoản Google Flow

Mở **Cài đặt → Google Flow** để thêm tài khoản và cấu hình kết nối.

1. Bấm **Thêm tài khoản**, đặt tên dễ nhận biết.
2. Chọn chế độ Chrome, nhập cổng riêng cho từng tài khoản.
3. Bấm **Lưu**, sau đó **Kết nối** và đăng nhập thủ công nếu được yêu cầu.
4. Dùng **Kiểm tra** để xem trạng thái phiên.

## Ba chế độ kết nối

- **Profile riêng do Concat quản lý:** Concat mở Chrome với thư mục dữ liệu
  riêng, giữ phiên đăng nhập cho những lần sau.
- **Chrome đang chạy qua CDP:** kết nối đến Chrome đã bật remote debugging
  trên máy này, theo cổng đã nhập. Chế độ này không tự mở Chrome thay thế.
- **Thư mục profile có sẵn:** chọn thư mục `user-data-dir` dành riêng và tên
  profile bên trong, ví dụ `Default` hoặc `Profile 1`. Đóng phiên đang sử dụng
  thư mục đó trước khi Concat mở nó.

Chrome thông thường chưa bật CDP không thể kết nối bằng cách nhập tên profile.
Không dùng thư mục `Google/Chrome/User Data` mặc định cho chế độ mở profile
riêng. Hai tài khoản chạy song song cần hai thư mục dữ liệu riêng và hai cổng
khác nhau; `Profile 1` và `Profile 2` trong cùng thư mục không đáp ứng điều này.

## Tài khoản đồng thời và tổng luồng

Hai lựa chọn đều từ 1 đến 4. Số tác vụ chạy thực tế tối đa bằng giá trị nhỏ
hơn; mỗi tài khoản chạy một tác vụ. Ví dụ, đặt 3 tài khoản và 2 luồng thì chạy
tối đa 2 tác vụ cùng lúc. Giới hạn áp dụng trong một phiên Concat đang chạy.

Trong **Flow Studio → Tạo video**, chọn tài khoản cố định cho từng cảnh hoặc
để tự động phân công. Có thể đặt URL dự án mặc định cho mỗi tài khoản trong
Cài đặt, hoặc nhập URL riêng cho cảnh. Nhân vật gốc nhập từ Flow cùng giọng
đã gắn luôn giữ đúng tài khoản và dự án nguồn.

Đang chạy hàng đợi thì không thay kết nối/profile. Nút dừng ngăn gửi cảnh
tiếp theo; video đã gửi vẫn có thể hoàn tất. Tác vụ lỗi hoặc chưa rõ kết quả
không được tự gửi lại sang tài khoản khác. Kiểm tra kết quả trên Flow trước
khi chủ động chạy lại.

Xóa mục tài khoản không xóa thư mục Chrome hoặc video đã tải. Concat không
sao chép mật khẩu/cookie từ profile khác.

## Tạo ảnh nhân vật và bối cảnh bằng Flow

1. Vào **Flow Studio → Nhân vật**, chọn tài khoản nguồn đã kết nối.
2. Nhập URL dự án Flow. Nếu tài khoản có URL mặc định trong Cài đặt,
   chức năng tạo ảnh ưu tiên URL mặc định đó.
3. Ở **Tạo ảnh nhân vật & bối cảnh**, chọn **Google Flow** và model ảnh.
4. Chọn tỷ lệ ảnh, bật **Cho phép dùng credit Flow**.
5. Bấm **Tạo ảnh bằng Flow** dưới nhân vật hoặc **Tạo ảnh bối cảnh bằng Flow**.

Ảnh tải về sẽ gắn vào hồ sơ tương ứng. Có thể dùng làm tham chiếu cho cảnh;
ảnh nhân vật cũng được lấy theo danh sách người xuất hiện đã chọn trong cảnh.
Không cần Gemini API key cho bước tạo ảnh này. Tạo ảnh có thể dùng credit
Flow; không tự chuyển sang Gemini API khi Flow báo lỗi.

Ảnh chân dung mới là ảnh tham chiếu cục bộ. Nó không tự tạo nhân vật có giọng
gắn sẵn trong thư viện Flow và không thay đổi hồ sơ/giọng gốc đã nhập từ Flow.
Các nút AI viết kịch bản, hồ sơ và mô tả bối cảnh vẫn dùng Gemini để tạo văn bản.
