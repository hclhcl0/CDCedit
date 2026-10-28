<p align="center"><img src="../assets/icons/cdcedit_logo_256.png" alt="CDCedit" width="100" /></p>
<h1 align="center">CDCedit</h1>

CDCedit là bản tùy chỉnh từ [Concat](https://github.com/jub0t/Concat), giữ nguyên ghi nhận tác giả và giấy phép gốc. Hãy build mã nguồn bản này để sử dụng các chức năng CDCedit; những liên kết phát hành gốc bên dưới dẫn đến dự án Concat.

CDCedit là trình biên tập video miễn phí, mã nguồn mở và là lựa chọn thay thế CapCut cho macOS, Windows, Linux và Android. Nó làm được những việc mà người ta thực sự mở CapCut để làm: phụ đề tự động, chuyển văn bản thành giọng nói, xóa nền, hoạt ảnh keyframe, hiệu ứng và tiêu đề, cắt nhiều track, xuất 4K. Không có bất kỳ ràng buộc nào: không watermark, không tài khoản, không đăng ký, không tải lên.

Mọi thứ chạy cục bộ trên engine Rust gốc với trình tổng hợp GPU. Cài đặt, kéo thả footage, cắt. Các mô hình AI cho phụ đề, giọng nói và tách nền tải một lần từ Cài đặt và sau đó hoạt động ngoại tuyến. Footage của bạn không bao giờ rời khỏi ổ đĩa của bạn.

**Phù hợp cho:** TikTok, Reels và Shorts, video YouTube, hướng dẫn và ghi màn hình, trích đoạn podcast, meme.

**Cũng dành cho máy móc:** API JSON-RPC, để script và các tác nhân AI cũng có thể cắt video bằng nó.

## Điểm nổi bật

- 🚫 **Không watermark. Không tài khoản. Không tường phí.** Mãi mãi.
- 🔒 **100 % cục bộ.** Không tải lên gì cả. Hoạt động ngoại tuyến.
- 💬 **Phụ đề tự động.** Whisper cục bộ. Chọn kích thước mô hình, nhận phụ đề có kiểu dáng trên timeline.
- 🗣️ **Chuyển văn bản thành giọng nói + nhân bản giọng.** Giọng cục bộ miễn phí, hoặc bất kỳ giọng nào từ vài giây ghi âm.
- 🧍 **Xóa nền.** Người, vật thể, hoặc tự vẽ mặt nạ.
- 🎞️ **Keyframe.** Vị trí, tỷ lệ, xoay, độ mờ, âm lượng, tham số hiệu ứng. Trình chỉnh sửa đường cong tích hợp.
- ✨ **100 hiệu ứng, bộ lọc và chuyển cảnh.** Kết xuất bằng GPU, xem trực tiếp trong bản xem trước.
- ✂️ **Cắt nhanh.** Tách, cắt gọt, ripple, gộp, đóng băng khung hình, tốc độ. Timeline từ tính nếu bạn muốn.
- 🎚️ **Nhiều track, nhiều timeline.** Nhiều bản dựng trong một dự án. Chế độ hòa trộn, cắt khung, lật.
- 📝 **Tiêu đề.** Phông chữ, viền, bóng đổ, nền. Preset để bắt đầu.
- 🎙️ **Làm sạch giọng nói bằng một công tắc.** Khử ồn, tăng cường giọng, cân bằng âm lượng. Thêm giọng sóc, robot, điện thoại và các bạn.
- 📤 **Xuất.** H.264, HEVC, AV1. Lên đến 4K 60, màu 10-bit.
- 🦀 **Engine Rust gốc.** Trình tổng hợp GPU, proxy, giải mã phần cứng. Tua 4K mượt mà.
- 🤖 **Có thể lập trình.** API JSON-RPC để script và các tác nhân AI cắt video. Bản dựng từ mã nguồn có thêm gRPC và công cụ dòng lệnh.
- 🖥️ **macOS, Windows, Linux, Android.** 26 ngôn ngữ. Cùng một ứng dụng, cùng tệp dự án.

## Tải về

Hai cách:

1. **[Trang web](https://concatenate.pages.dev/#download)** đưa bạn đúng bản dựng cho máy của bạn. Bắt đầu từ đây.
2. **[GitHub Releases](https://github.com/jub0t/Concat/releases)** có mọi bản dựng cho mọi nền tảng, kèm trình cài đặt, gói và checksum. Khi bạn muốn tự chọn.

CDCedit đang ở giai đoạn **beta**: nó hoạt động, và vẫn còn vài góc cạnh. [Hãy cho biết](https://github.com/jub0t/Concat/issues) khi bạn gặp một lỗi.

**Nền tảng**

- ✅ **Windows** · x86_64 (bản dựng ARM đang tạm dừng, xem [#326](https://github.com/jub0t/Concat/issues/326)). Một bộ cài và một `.msi`. Nếu SmartScreen chặn bản dựng chưa ký: **More info** › **Run anyway**
- ✅ **macOS** · Intel và Apple Silicon. Nếu macOS từ chối mở bản dựng chưa ký: `xattr -dr com.apple.quarantine /Applications/CDCedit.app`
- ✅ **Linux** · x86_64 và ARM. `.deb`, `.rpm`, `.AppImage` và gói Arch. Ngoài ra còn Flatpak trên **[Flatpark](https://flatpark.org/apps/app.concat.editor/)**, một kho Flatpak cộng đồng đóng gói `.deb` x86_64 của mỗi bản phát hành và cập nhật theo nó
- ✅ **Android** · điện thoại và máy tính bảng
- ✅ **iOS / iPadOS** · iPhone và iPad, qua sideload

✅ Được hỗ trợ · 🚧 Đang thực hiện · 🧪 Cần kiểm thử

**Yêu cầu hệ thống** và kích thước các mô hình tùy chọn có trên [trang web](https://concatenate.pages.dev/guides/system-requirements).

**[Code signing policy](../CODE_SIGNING.md)**

## Cách đóng góp

> [!IMPORTANT]
> Cách đóng góp tốt nhất là lấy một bản dựng từ trang [Releases](https://github.com/jub0t/Concat/releases) và dùng nó: tìm chỗ nó hỏng, và nói chỗ nó có thể tốt hơn.
>
> Sẵn sàng viết mã? [CONTRIBUTING.md](../CONTRIBUTING.md) mô tả cách thiết lập, bố cục cây mã nguồn, các kiểm tra cần chạy và cách cấp phép cho đóng góp. Điều khiển CDCedit từ script, dịch vụ hay tác nhân? [Tài liệu dành cho nhà phát triển](https://concatenate.pages.dev/docs) mô tả API của CDCedit và các phương thức truyền tải: JSON-RPC và gRPC. [Thảo luận này](https://github.com/jub0t/Concat/discussions/3) là nơi dự án được công bố.
> 
> Người đóng góp có thể nhận vai trò `@Contributor` trên máy chủ Discord: chỉ cần hỏi.

## Lịch sử sao

<a href="https://www.star-history.com/?repos=jub0t%2Fconcat&type=date&releases=&legend=bottom-right">
 <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=jub0t/concat&type=date&legend=bottom-right" />
</a>

## Người đóng góp

<a href="https://github.com/jub0t/Concat/graphs/contributors">
  <img alt="Contributors" src="https://contrib.rocks/image?repo=jub0t/concat">
</a>

## DMCA

Nếu bạn cho rằng nội dung nào đó trong kho mã này, trong các bản phát hành của nó, hoặc trong một gói hay mô hình mà nó phân phối vi phạm bản quyền của bạn, hãy gửi email tới [jub0trd@gmail.com](mailto:jub0trd@gmail.com) kèm theo: mô tả tác phẩm thuộc sở hữu của bạn, liên kết tới nơi tài liệu vi phạm xuất hiện tại đây, thông tin liên hệ của bạn, một tuyên bố rằng bạn tin một cách thiện chí rằng việc sử dụng đó không được bạn, người đại diện của bạn hoặc pháp luật cho phép, và một tuyên bố, chịu hình phạt về tội khai man, rằng thông báo là chính xác và bạn là chủ sở hữu hoặc được ủy quyền hành động thay mặt chủ sở hữu, có chữ ký tay hoặc chữ ký điện tử của bạn. Các thông báo hợp lệ sẽ được xử lý kịp thời, và bất kỳ ai có tài liệu bị gỡ bỏ đều có thể gửi thông báo phản đối (counter-notice) tới cùng địa chỉ này.
