---
name: slint-ui-designer
description: Master skill for crafting modern, minimalist, responsive, and high-performance desktop application UIs using Slint and Rust in CDCedit. Use whenever designing, refactoring, styling, or creating UI components, layout structures, animations, design systems, dark modes, or AI tool studios in .slint files.
---

# Slint UI Designer: Minimalist & Modern Desktop Studio

Bộ kỹ năng chuyên sâu để thiết kế và hiện thực hóa giao diện ứng dụng desktop bằng **Slint UI** trong dự án CDCedit (Rust). Skill này giúp AI và lập trình viên luôn tạo ra các giao diện tinh gọn, thẩm mỹ cao (Dark Slate Minimalist), chuẩn công thái học và khai thác tối đa sức mạnh của Slint.

---

## 1. Triết lý Thiết kế: "Minimalist AI-First Studio"

Khi làm giao diện cho ứng dụng bản Lite hoặc AI Companion, luôn tuân thủ 4 nguyên tắc:

1. **Zero Clutter (Không thành phần thừa)**: 
   - Không lạm dụng quá nhiều đường kẻ (borders/dividers) dày đặc. Dùng khoảng cách (whitespace) và độ chênh lệch màu nền (subtle contrast) để phân tách khu vực.
   - Loại bỏ các thông số kỹ thuật khô cứng không phục vụ trực tiếp cho tác vụ chính của người dùng.
2. **Progressive Disclosure (Hiển thị theo ngữ cảnh)**:
   - Chỉ hiện bảng điều khiển chi tiết khi người dùng chọn đối tượng (ví dụ: click chọn clip trên Timeline thì mới hiện thanh chỉnh âm lượng/tốc độ).
   - Khi chưa có đối tượng chọn, hiển thị trạng thái chờ (Empty/Hero State) thân thiện, hướng dẫn trực quan.
3. **Thứ bậc thị giác rõ ràng (Visual Hierarchy)**:
   - **Hero Action**: Nút hành động cốt lõi (như "Tạo phụ đề tự động", "Lồng tiếng AI", "Mở trong CapCut") phải có màu accent, kích thước nổi bật và icon biểu cảm.
   - **Secondary Action**: Dạng ghost button, pill chip hoặc icon button tối giản.
4. **Không gian làm việc mở (Spacious Workspace)**:
   - Ưu tiên 60-70% diện tích màn hình cho khung xem trước (Preview) và không gian chế tác (AI Studio / Timeline).

---

## 2. Design System chuẩn của CDCedit (Dark Slate Theme)

### Bảng màu (Color Palette)
- **Background chính (Page)**: `#0a0b10` (Deep Dark) hoặc `Theme.page`
- **Surface / Card nền**: `#12151f` hoặc `Theme.panel`
- **Surface Hover**: `#1a1e2d` hoặc `Theme.field-hover`
- **Surface Active / Well**: `#151824` hoặc `Theme.well`
- **Đường viền tinh tế (Subtle Border)**: `Theme.line` hoặc màu accent với opacity `0.2 - 0.4`
- **Điểm nhấn chính (Primary Accent - AI Purple/Indigo)**: `#8b5cf6` / `#6366f1`
- **Điểm nhấn Gemini Studio (Teal Gradient)**: `#14b8a6` / `#2dd4bf` (Background: `#092020`)
- **Điểm nhấn CapCut & Dịch thuật (Blue/Cyan)**: `#3b82f6` / `#00c4b4`
- **Màu văn bản**:
  - Tiêu đề chính: `Theme.fg` hoặc `#ffffff`
  - Văn bản phụ / mô tả: `Theme.fg-muted` hoặc `#94a3b8`
  - Gợi ý / Metadata: `Theme.fg-dim` hoặc `#64748b`

### Bo góc (Border Radii)
- `Theme.r-xs`: 4px (Badge, icon button nhỏ)
- `Theme.r-sm`: 6px (Input field, button thường, FilterChip)
- `Theme.r`: 8px (Card thông thường, Modal)
- `Theme.r-lg`: 12px (Hero AI Card, Container lớn)

### Typography
- `Theme.fs-xs`: 10px (Badge đếm số, metadata)
- `Theme.fs-sm`: 11px (Nhãn phụ, chú thích)
- `Theme.fs`: 13px (Văn bản giao diện chuẩn, nút bấm)
- `Theme.fs-lg`: 15px - 16px (Tiêu đề nhóm, card heading)
- `Theme.weight-title`: Đậm (Semi-bold/Bold, 600 - 700)

---

## 3. Mẫu Component Slint UI Chuẩn (Copy-Paste Ready)

### A. FilterChip (Thẻ lọc nhanh dạng ngang)
Dùng để thay thế cho các thanh sidebar dọc cồng kềnh:
```slint
component FilterChip inherits Rectangle {
    in property <string> label;
    in property <int> count: -1;
    in property <bool> current: false;
    callback clicked();

    height: 24px;
    border-radius: Theme.r-sm;
    background: root.current ? Theme.accent.with-alpha(0.2) : (touch.has-hover ? Theme.field-hover : Theme.field);
    border-width: 1px;
    border-color: root.current ? Theme.accent : Colors.transparent;

    animate background { duration: 100ms; }
    animate border-color { duration: 100ms; }

    HorizontalLayout {
        padding-left: 8px;
        padding-right: 8px;
        spacing: 4px;
        alignment: center;

        Text {
            y: (parent.height - self.height) / 2;
            text: root.label;
            color: root.current ? Theme.fg : (touch.has-hover ? Theme.fg : Theme.fg-muted);
            font-size: Theme.fs-sm;
            font-weight: root.current ? Theme.weight-title : 400;
        }

        if root.count >= 0: Text {
            y: (parent.height - self.height) / 2;
            text: "\{root.count}";
            color: root.current ? Theme.accent : Theme.fg-dim;
            font-size: Theme.fs-xs;
            font-weight: Theme.weight-title;
        }
    }

    touch := TouchArea {
        mouse-cursor: pointer;
        clicked => { root.clicked(); }
    }
}
```

### B. Hero Action Card (Thẻ công cụ AI nổi bật)
Dùng cho tính năng chính như Phụ đề tự động hoặc Lồng tiếng:
```slint
Rectangle {
    border-radius: Theme.r;
    background: #171026;
    border-width: 1px;
    border-color: #8b5cf6.with-alpha(0.6);

    VerticalLayout {
        padding: 12px;
        spacing: 8px;

        HorizontalLayout {
            spacing: 8px;
            Icon {
                glyph: Glyph.sparkle;
                size: 20px;
                tint: #a78bfa;
            }
            Text {
                text: "Tạo Phụ đề tự động (PhoWhisper AI)";
                font-size: Theme.fs-lg;
                font-weight: Theme.weight-title;
                color: #f3e8ff;
                vertical-alignment: center;
            }
        }

        Text {
            text: "Tự động nhận diện giọng nói tiếng Việt chuẩn xác, tạo phụ đề khớp thời gian.";
            font-size: Theme.fs-sm;
            color: #c4b5fd;
            wrap: word-wrap;
        }

        HorizontalLayout {
            action-btn := Rectangle {
                height: 32px;
                border-radius: Theme.r-sm;
                background: btn-touch.has-hover ? #7c3aed : #6d28d9;
                animate background { duration: 100ms; }

                HorizontalLayout {
                    padding-left: 14px;
                    padding-right: 14px;
                    spacing: 6px;
                    alignment: center;

                    Icon {
                        y: (parent.height - self.height) / 2;
                        glyph: Glyph.sparkle;
                        size: 13px;
                        tint: Colors.white;
                    }
                    Text {
                        y: (parent.height - self.height) / 2;
                        text: "Bắt đầu tạo phụ đề";
                        font-size: Theme.fs;
                        font-weight: Theme.weight-title;
                        color: Colors.white;
                    }
                }

                btn-touch := TouchArea {
                    mouse-cursor: pointer;
                    clicked => { /* callback */ }
                }
            }
            Rectangle { horizontal-stretch: 1; }
        }
    }
}
```

### C. Modern Action Pill (Nút hành động cao cấp viền dạ quang)
Dùng cho nút "Mở trong CapCut" trên TitleBar hoặc Header:
```slint
capcut-button := Rectangle {
    width: label.width + icon.width + 26px;
    height: 25px;
    border-radius: Theme.r-sm;
    background: touch.has-hover ? #0e4c48 : #0c3835;
    border-width: 1px;
    border-color: touch.has-hover ? #2dd4bf : #14b8a6.with-alpha(0.8);

    animate background { duration: 100ms; easing: cubic-bezier(0.4, 0, 0.2, 1); }
    animate border-color { duration: 100ms; }

    icon := Icon {
        x: 9px;
        y: (parent.height - self.height) / 2;
        glyph: Glyph.sparkle;
        size: 13px;
        tint: #2dd4bf;
    }

    label := Text {
        x: icon.x + icon.width + 6px;
        y: (parent.height - self.height) / 2;
        text: "Mở trong CapCut";
        color: #ccfbf1;
        font-size: Theme.fs;
        font-weight: Theme.weight-title;
    }

    touch := TouchArea {
        mouse-cursor: pointer;
        clicked => { root.open-in-capcut(); }
    }
}
```

---

## 4. Các bẫy kỹ thuật (Pitfalls & Rules) cần tránh trong Slint

1. **Không có thuộc tính `horizontal-alignment` trong `Icon`**:
   - `Icon` trong Slint không nhận `horizontal-alignment: center`. Muốn căn giữa Icon, hãy bọc bằng `HorizontalLayout { alignment: center; Icon { ... } }` hoặc tính toán toạ độ `x: (parent.width - self.width) / 2`.
2. **Căn giữa chữ và Icon theo chiều dọc**:
   - Trong `HorizontalLayout`, luôn đặt `y: (parent.height - self.height) / 2` hoặc gán layout property `alignment: center`.
3. **Tránh vòng lặp ràng buộc (Circular Binding Loops)**:
   - Không đọc kích thước của phần tử anh em (`sibling.width`) từ bên trong layout tính toán kích thước của phần tử đó. Hãy dùng `horizontal-stretch` hoặc `parent.width`.
4. **Luôn kiểm tra biên dịch bằng `cargo check -p concat`**:
   - Slint sinh mã Rust lúc biên dịch thông qua build script (`build.rs`). Do đó, bất kỳ lỗi cú pháp `.slint` nào cũng sẽ được bắt ngay trong bước `cargo check -p concat`.
