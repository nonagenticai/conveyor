-- S11, George 2026-11-09 — `kata_executor`, the conveyor's synthetic tenant-admin.
--
-- One vocabulary, one new value: a MACHINE principal (app_user.kind = 'agent' | 'service') that
-- holds every tenant scope an `admin` holds, so it can create/update the system's shared assets
-- (procedures today; workflows S13, knowledge S15) and run project work. It is never a human's
-- membership role: `members.set_role` refuses it for `kind = 'human'`.
--
-- The board-level role (`board_membership.role`) is a SEPARATE, display-only vocabulary
-- (crates/actions/src/board/boards.rs:23) and is deliberately untouched.
--
-- CONTRACT-CHANGE REQUEST 5: 02-DATA-ARCHITECTURE §2.1 `membership.role` gains 'kata_executor'.
ALTER TABLE membership DROP CONSTRAINT membership_role_check;
ALTER TABLE membership ADD CONSTRAINT membership_role_check
    CHECK (role IN ('owner', 'admin', 'member', 'viewer', 'kata_executor'));
