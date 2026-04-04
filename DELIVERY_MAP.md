# Delivery Map: Resource Tree Upgrade Initiative

## Decision

Chọn **Track B: Phase/Wave/Epic -> Beads**.

## Why Track B

- Initiative này có nhiều milestone riêng, không phải một bounded fix đơn lẻ.
- Có dependency rõ ràng giữa report -> planning artifacts -> beads -> implementation waves.
- Có blocker hạ tầng thực tế ở `br`, nên không thể đi thẳng vào execution tracking.
- Có nhiều ràng buộc tương thích ngược: `tx`, `skills.body`, `content_hash`, `assets_json`, MCP contract.

## Initiative Shape

### Phase 0 — Planning Baseline

Mục tiêu:

- chuẩn hóa report kỹ thuật
- viết `CONTEXT.md`
- viết `DELIVERY_MAP.md`
- khôi phục khả năng dùng `br`
- chuyển remote phát triển sang repo GitHub của người dùng

Output:

- updated `docs/meta_skill_resource_tree_report.md`
- `CONTEXT.md`
- `DELIVERY_MAP.md`
- `br` usable again
- remote/repo baseline ready

### Phase 1 — Discovery and Package Model

Mục tiêu:

- phát hiện skill root thay vì file đơn
- recursive scan resources
- symlink/path hardening
- thêm `bundle_hash`
- manifest summary + `skill_resources`

Primary modules:

- `src/cli/commands/index.rs`
- `src/core/skill.rs`
- `src/storage/sqlite.rs`
- migrations

### Phase 2 — Archive and Transaction Bridge

Mục tiêu:

- preserve package trong archive
- nối package metadata/resources với tx flow mà không phá recovery/sync

Primary modules:

- `src/storage/git.rs`
- `src/storage/tx.rs`

### Phase 3 — MCP and Loading Surface

Mục tiêu:

- expose `resources/list`
- expose `resources/read`
- align `load/show` contract
- mở rộng disclosure/load bằng manifest/resources

Primary modules:

- `src/cli/commands/mcp.rs`
- `src/cli/commands/load.rs`
- `src/core/disclosure.rs`
- `tests/e2e/mcp_workflow.rs`

### Phase 4 — Search and UX Polish

Mục tiêu:

- tăng relevance từ filenames/excerpts/resource text
- relative reference resolver
- optional CLI UX improvements

Primary modules:

- `src/search/*`
- resolver/helper modules liên quan
- optional CLI surfaces

## Wave Strategy

- Mỗi phase có thể tách thành 1-2 waves tùy độ rộng thực tế.
- Các bead nên map vào thay đổi bounded theo module boundary.
- Review có thể batch theo wave cho low-risk slices, nhưng MCP/schema/tx changes cần review ngay.

## Beads Readiness Gate

Chỉ tạo beads sau khi:

1. `docs/meta_skill_resource_tree_report.md` được cập nhật xong.
2. `CONTEXT.md` và `DELIVERY_MAP.md` đã tồn tại.
3. `br` preflight hoạt động lại.

## First Recommended Beads

Khi `br` hoạt động lại, nên tạo:

1. Epic: `resource-tree-upgrade`
2. Task: `phase-1-discovery-and-package-model`
3. Task: `phase-2-archive-and-tx-bridge`
4. Task: `phase-3-mcp-and-load-surface`
5. Task: `phase-4-search-and-ux-polish`

Dependencies đề xuất:

- phase 2 depends on phase 1
- phase 3 depends on phase 1 and phase 2
- phase 4 depends on phase 3

## Remote Strategy

- Giữ repo hiện tại làm nguồn tham chiếu.
- Thêm remote repo GitHub của người dùng làm đích phát triển an toàn.
- Push branch làm việc sang repo người dùng sau khi baseline planning và beads setup đã ổn.
