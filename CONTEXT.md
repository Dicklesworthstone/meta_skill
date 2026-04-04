# Context: Resource Tree Upgrade Initiative

## Goal

Chuẩn hóa và triển khai initiative nâng cấp `meta_skill` từ pipeline hiện tại, nơi `SKILL.md` là payload trung tâm, sang mô hình package root với `SKILL.md` làm entrypoint và companion resources được index, lưu trữ, và expose đúng qua MCP.

## Why Now

- Pain hiện tại đã được xác nhận bằng code review: các file cùng thư mục hoặc subtree của skill không đi qua pipeline index/storage/archive/load/MCP chính.
- Các skill thực tế như `bom-execute` và `visual-explainer` đã có cấu trúc nhiều file/thư mục, nên mô hình single-body hiện tại gây mất ngữ cảnh và làm MCP ít hữu ích.
- `docs/meta_skill_resource_tree_report.md` đã được cập nhật để phản ánh sát mã nguồn hiện tại và đủ chi tiết cho triển khai.

## Verified Current-State Findings

- Discovery chính trong `src/cli/commands/index.rs` chỉ tìm file tên `SKILL.md`.
- Reindex/hash hiện dựa trên `SkillSpec` parse từ `SKILL.md`.
- `skills.body` đang là canonical compiled view được nhiều call sites dùng cho search/show/load/browse.
- `assets_json` tồn tại nhưng hiện chỉ là placeholder/compat, chưa là resource manifest thật.
- `src/storage/tx.rs` đang serialize payload là `SkillSpec`, nên tx/recovery/sync chưa package-aware.
- `src/storage/git.rs` chỉ archive spec chuẩn hóa + `SKILL.md` render lại, chưa preserve full tree.
- MCP hiện chỉ advertise `tools`; `resources/list` đang stub rỗng; `resources/read` chưa tồn tại.
- Có MCP contract drift riêng ở `show`: implementation và E2E chưa đồng nhất.

## Important Nuances

- Toàn repo không hoàn toàn mù với folder roots:
  - `src/cli/commands/mod.rs::resolve_skill_markdown()` resolve directory thành `<dir>/SKILL.md`.
  - `src/cli/commands/bundle.rs::discover_skills_in_dir()` coi thư mục con chứa `SKILL.md` là skill.
- Tuy vậy, các behavior này chưa phải source-of-truth cho pipeline index/storage/archive/MCP.

## Constraints

- Giữ `SkillSpec` là canonical spec và `SKILL.md` là entrypoint trong phase đầu.
- Giữ `skills.body` như compiled canonical view trong giai đoạn chuyển tiếp để giảm blast radius.
- Không đổi nghĩa `content_hash` quá sớm; ưu tiên thêm `bundle_hash` song song.
- `assets_json` nên được giữ ngắn hạn như compat/summary layer.
- Cần siết symlink/path policy vì discovery hiện dùng `follow_links(true)`.

## Delivery Implications

- Đây là initiative nhiều milestone và có phụ thuộc rõ ràng, không phù hợp Track A direct-to-beads.
- Cần Track B `Phase/Wave/Epic -> Beads`.
- Trước khi tạo beads, cần có planning artifacts và khôi phục `br` vì `.beads/issues.jsonl` hiện có record invalid làm `br` preflight fail.

## Immediate Outputs Required In This Session

1. Cập nhật report kỹ thuật cho sát thực tế và đủ triển khai.
2. Tạo `CONTEXT.md` và `DELIVERY_MAP.md`.
3. Khắc phục blocker `br` để có thể tạo beads.
4. Thiết lập remote repo GitHub của người dùng làm nơi phát triển an toàn.

## Not In Scope Yet

- Chưa triển khai code resource-tree trong turn này.
- Chưa commit hoặc push nếu chưa hoàn tất planning baseline và xử lý blocker `br`.
