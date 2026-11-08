---
name: flow-prompt-craft
description: "Chuyên gia kỹ thuật Prompt cho Google Flow (Veo 3.1 & Omni Flash). Tạo video prompt điện ảnh chuẩn xác, kiểm soát chuyển động camera chậm và ổn định, duy trì nhân vật nhất quán và không bị tràn ký tự."
status: stable
category: video-ai
subcategory: prompt-engineering
version: 1.0
tags: [google-flow, veo, omni-flash, video-prompt, camera-control, cinematic]
---

# Flow Prompt Craft (Kỹ thuật Prompt Video Google Flow)

## Mục đích của Skill
Bộ kỹ năng này chuyển đổi kịch bản cảnh quay hoặc ý tưởng phân cảnh thành **Video Prompt chuẩn sản xuất dành riêng cho Google Flow** (chạy trên các mô hình Veo 3.1 và Omni Flash 1.2).

Skill giải quyết triệt để các vấn đề phổ biến của video AI:
1. **Camera lia quá nhanh**: Ép mô hình AI giữ nhịp điệu camera chậm rãi, êm đềm kiểu điện ảnh hoặc khóa tĩnh khung hình.
2. **Nhân vật bị biến dạng hoặc đổi mặt**: Giữ vững các đặc điểm nhận diện thị giác cố định của người Việt (*Fixed visual identity*).
3. **Prompt quá dài gây tràn bộ nhớ / cắt cụt**: Loại bỏ toàn bộ tâm lý nội tâm thừa, giữ prompt cô đọng dưới 450 ký tự.
4. **Tránh chèn phụ đề rác**: Đảm bảo AI không tự ý in chữ, tiêu đề hay watermark lên video.

---

## Khi nào nên sử dụng Skill
- Khi nhấn nút **"AI viết prompt cảnh đã chọn"** trong CDCedit / Concat.
- Khi muốn chuyển đổi một cảnh kịch bản văn học sang prompt video thực tế cho Google Flow / Veo.
- Khi một cảnh video sinh ra có chuyển động camera quá chóng mặt và cần chỉnh lại prompt để chuyển động chậm, mượt.

---

## Công thức 5 Thành phần Vàng (The 5-Part Formula)

Mỗi prompt cho Google Flow được cấu trúc theo đúng thứ tự sau:

```text
[1. Góc máy & Tốc độ Camera] + [2. Chủ thể & Nhận diện] + [3. Hành động đơn duy nhất] + [4. Môi trường & Ánh sáng] + [5. Lời thoại & Ràng buộc]
```

### 1. Góc máy & Tốc độ Camera (Camera & Motion Pacing)
*Luôn sử dụng tiếng Anh cho chỉ dẫn camera để mô hình diffusion hiểu chính xác nhất:*
- **Máy tĩnh (Khuyên dùng khi có nhiều diễn xuất)**: `Static tripod shot, locked-off framing, calm pacing, no camera pan or zoom.`
- **Tiến gần chậm (Tạo cảm xúc)**: `Slow and steady cinematic push-in at calm 0.5x pacing, smooth dolly-in, stable gimbal.`
- **Lia ngang chậm (Khám phá bối cảnh)**: `Extremely slow horizontal pan, gentle camera tracking, smooth fluid motion, no rapid moves.`
- **Cầm tay nhẹ (Đời thường, tự nhiên)**: `Gentle subtle handheld camera, organic micro-movements, steady composition.`

### 2. Chủ thể & Nhận diện cố định (Subject & Appearance Anchor)
- **Đặc tả người Việt thực tế**: Luôn chỉ rõ là người Việt Nam với nét mặt tự nhiên, màu da tự nhiên và trang phục đặc trưng (`Vietnamese man/woman, natural facial features, wearing [signature clothes]`).
- **Loại bỏ hoàn toàn tâm lý nội tâm**: TUYỆT ĐỐI KHÔNG đưa vào: *Want & need, Flaw, suy nghĩ, hồi ức*. AI chỉ vẽ được cái mắt nhìn thấy được.

### 3. Hành động cốt lõi đơn nhất (One Core Action)
- Một clip Flow chỉ dài **5 đến 8 giây**.
- **Chỉ mô tả MỘT hành động liên tục**, không nhồi nhét chuỗi hành động (`vừa xách, vừa đổ, vừa chạy, vừa cười`).
- Ví dụ tốt: `He carefully tips over the old tire to pour out accumulated rainwater.`
- Ví dụ xấu (làm camera giật): `He runs into the yard, grabs five pots, cleans the tires, and Mai claps and dances around.`

### 4. Môi trường & Ánh sáng (Environment & Lighting)
- Bối cảnh chân thực: `In a bright Vietnamese suburban garden, lush green plants, sun-drenched stone path.`
- Ánh sáng: `Soft cinematic natural daylight, warm tones, diffuse shadows.`

### 5. Ràng buộc âm thanh, thoại & Không chữ (Constraints & Dialogue)
- **Lời thoại chính xác**: Giữ nguyên vẹn tiếng Việt gốc: `Exact dialogue: "[Câu thoại tiếng Việt]"`.
- **Ràng buộc âm bản (Negative rule)**: `No subtitles, no on-screen text, no watermarks, cinematic photorealism.`

---

## ⚡ Kỹ thuật Gắn thẻ Mention `@` trong Google Flow (Inline Mention Chips)

Google Flow hỗ trợ tính năng cực kỳ mạnh mẽ: **gõ phím `@` ngay trong ô Prompt** để mở danh mục nguyên liệu tự động (Mention Autocomplete):
- `@Characters`: Chọn nhân vật đã lưu (Saved Flow Characters với chân dung và mã giọng).
- `@Images`: Chọn ảnh tham chiếu đã tải lên trong dự án.
- `@Voices`: Chọn giọng đọc có sẵn của Google Flow.
- `@Videos`: Chọn video trước đó làm mồi tiếp nối.

### Tại sao gắn thẻ `@` là chìa khóa vàng của Flow?
1. **Gán cứng hành động cho đúng nhân vật**:
   - Thay vì viết: *"Người cha đổ nước, cô bé vỗ tay. Ảnh 1 là cha, ảnh 2 là con gái."* (AI rất dễ làm lẫn lộn ai là ai).
   - Hãy viết: **`"@Anh Hùng tips over the tire to pour water while @Bé Mai claps excitedly."`**
   - Khi đó, Flow gắn chip nguyên liệu trực tiếp vào đúng vị trí từ trong câu: mô hình Veo / Omni biết chính xác 100% người đang đổ nước mang khuôn mặt của `@Anh Hùng`, và người vỗ tay mang khuôn mặt của `@Bé Mai`!
2. **Không bao giờ bị tải ảnh trùng lặp**:
   - Khi gõ `@`, Flow lấy ngay ảnh và nhân vật đã có trong dự án. Không cần tải lên lại từ ổ cứng, giúp thư viện dự án luôn gọn gàng và không tốn thời gian upload.
3. **Thao tác nhanh**:
   - Trong giao diện Flow, chỉ cần gõ `@` và gõ vài ký tự đầu của tên nhân vật rồi bấm `Enter` là đối tượng được gắn chip lập tức vào văn bản.

---

## Bảng so sánh Trước & Sau (Before & After)

### ❌ Prompt cũ (Lỗi: quá dài, camera bị giật, nhồi nhét tâm lý)
```text
SÂN VƯỜN - BAN NGÀY. Hùng hăng hái mang chậu cây đi đổ sạch nước, lật úp chiếc lốp xe cũ và cọ rửa sạch sẽ các dụng cụ chứa nước quanh nhà. Mai đứng bên cạnh vỗ tay cổ vũ. Cảnh quay rộng cho thấy ngôi nhà sạch bóng nước đọng...
Character: Anh Hùng — Người cha. Want & need: Muốn nghỉ ngơi / Cần nhận thức nguy hiểm. Flaw & motivation: Chủ quan...
Character: Bé Mai. Want & need: Muốn bố dọn dẹp...
(Use attached images as visual ingredients for consistency... [BỊ CẮT CỤT])
```
*Hậu quả*: Prompt dài 1.850 ký tự, bị cắt cụt đuôi, camera lia cuống cuồng giữa Hùng và Mai, AI tạo chuyển động hỗn loạn.

### ✅ Prompt mới qua Skill (Chuẩn Flow: 350 ký tự, camera chậm êm)
```text
Cinematic static tripod shot, calm pacing. A 35-year-old Vietnamese father with neat short hair, wearing a dark navy polo shirt and shorts, bent down tipping over an old rubber tire to pour out stagnant water. A 10-year-old Vietnamese girl in a light t-shirt stands nearby watching attentively. Sunny Vietnamese suburban garden with potted plants, soft natural daylight, shallow depth of field. No subtitles, no on-screen text, photorealistic 4k.
```
*Kết quả*: Camera đứng yên vững chãi, bố cục ổn định, nhân vật giữ nguyên ngoại hình, chuyển động nước đổ và diễn xuất cực kỳ mượt mà.

---

## Tiêu chí Đánh giá Chất lượng (Quality Checklist)
- [ ] Độ dài prompt nằm trong khoảng 250 – 450 ký tự (không bị cắt cụt).
- [ ] Có từ khóa làm chậm camera (`calm pacing`, `static tripod`, `slow push-in`, `no rapid camera motion`).
- [ ] Chỉ chứa 1 nhịp hành động chính phù hợp thời lượng 5–8 giây.
- [ ] Nhân vật được neo đặc trưng Việt Nam thực tế, không có từ ngữ tâm lý trừu tượng.
- [ ] Có câu chặn chữ trên màn hình (`No subtitles, no on-screen text`).
