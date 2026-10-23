# Rule and Certificate Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the expanded Rules form and certificate text stack with the approved Clash Verge-inspired compact list and grouped dialogs.

**Architecture:** Keep existing App drafts, stable rule IDs, action dispatch and validation. A selected rule ID opens a grouped editor; closing retains its draft, while Save & apply remains the only configuration write. Reuse ImGui controls and the existing theme/primary button; bound centered dialogs to the viewport with scrollable bodies and a reachable Close footer.

**Tech Stack:** Rust, imgui 0.12, winit 0.30, wgpu 29, rust-i18n.

**Spec:** `openspec/changes/add-imgui-desktop-gui/design.md`, plus the user's approved list/editor and grouped-certificate direction in this conversation.

## Global Constraints

- Keep user-level GUI config loading and CLI behavior unchanged; no new dependencies.
- Preserve rule order, stable IDs, all nine editable fields, draft retention and save validation.
- Preserve explicit certificate confirmation, fingerprint ownership, macOS guards and busy-state disabling.
- Do not modify real Keychain, LaunchAgent, system proxy, appearance or daily configuration during tests.
- Work in the existing approved feature checkout; preserve unrelated dirty files. Do not commit, push or create another worktree.
- Close retains draft edits (not Cancel); make this explicit in both languages.

## Review Focus

- Long hosts, regex patterns and certificate paths: summaries must not cover Edit/More controls; full data stays available.
- Reorder/delete while editing: identify drafts by ID, never a stale vector index.
- Busy or unsupported platform: no certificate side effect or rule mutation may be queued by disabled controls.
- 640×480 and the native 18px font: dialog bodies scroll while Close stays reachable.
- Language changes, empty/default CA and stale ownership: no state loss, accidental authorization or implicit generation.

### Task 1: Compact Rules with grouped draft editor

**Files:** `src/app.rs`, `src/view/rules.rs`, `src/view/mod.rs`, `locales/app.yml` within `crates/proxy-fork-gui`; `crates/proxy-fork-gui/tests/view_test.rs`.

**Interfaces:** Consume `App::add_rule`, `RuleDraft`, `Action::Save`; produce `App::rule_editor: Option<u64>` and `rules::editor(ui, app)` called after the page shell.

- [x] Extend the real, single-context ImGui regression: two list rows remain reachable without expanded fields; Edit opens the matching ID, editing/closing preserves draft without Save, Add opens the new draft, busy Add is inert, More reorders/deletes by stable ID; test en/zh-CN and narrow viewport.
- [x] Run `cargo test -p proxy-fork-gui --test view_test --locked`; expect the compact-row interaction assertion to fail against the old form.
- [x] Implement compact two-line summaries, Edit/More controls, centered grouped editor and centered empty state. Show source, target and path-rewrite groups; no automatic save or new validation policy.
- [x] Run the same command; expect PASS. Record the result, without a commit.

### Task 2: Grouped certificate dialog and delivery verification

**Files:** `crates/proxy-fork-gui/src/view/dashboard.rs`, shared dialog placement in `src/view/mod.rs`, locale/test files above; append design/progress/QA/smoke documentation.

**Interfaces:** Consume the shared centered dialog and existing CA state/actions; no handler or config changes.

- [x] Add real UI regressions for centered bounds and reachable close, generation/refresh, confirmation-required Trust/Remove, recorded-fingerprint mismatch, busy and unsupported guards. Dispatch actions only, never run OS handlers.
- [x] Run the view test; expect the new centered-dialog action interaction to fail before implementation.
- [x] Implement status, certificate information, trust management, collapsed danger zone and fixed Close footer. Install trust is primary; replacement/removal remain separately labeled and explicitly authorized.
- [x] Run focused tests, `cargo test --workspace --locked`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, all-target check, Rust 1.90 check and ARM release build; expect zero failures.
- [x] Request one scoped fresh review of these changes, address Important/Critical findings with RED→GREEN; preserve unrelated changes.
- [x] Attempt isolated native cert/rule screenshots; report native blockers honestly. Refresh the ARM bundle/archive/checksum and test the actual updater archive; append documentation with exact evidence and remaining limits. Leave changes uncommitted.
