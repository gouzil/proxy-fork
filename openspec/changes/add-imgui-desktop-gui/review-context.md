# Whole-change review context

Base and current HEAD: `f7992b6f7df9ba1e633b4b57331d0691a9c3df7e`.
All implementation is uncommitted on `codex/imgui-desktop-gui`; inspect the working tree and new files, not an empty HEAD range.

Scope: all tracked modifications, `crates/proxy-fork-gui/`, new core config/dirs/service and tests, CLI lifecycle test, `scripts/package-gui.sh`, `scripts/gui-icon.swift`, GUI workflows, README and `docs/gui-macos-smoke.md`.
Existing unrelated untracked `.codegraph/`, `docs/superpowers/plans/2026-07-27-pr-5-simplification.md`, `proxy-fork copy.toml` and `test.py` must remain untouched.

Authority: proposal.md, design.md, tasks.md, and the three specs in this change. Ledger: progress.md.

Implemented: shared strict core configuration and graceful proxy service; CLI overlays; four-page ImGui GUI; bounded lifecycle logs; Chinese/English, dynamic CJK atlas, IME bridge; macOS tray, LaunchAgent, Keychain; fixed-origin GitHub updater with checksum/signature/notarization/team checks and transactional bundle replacement; unsigned local packaging and signed release CI.

Ruling: `replace_rules` is the shared name, and `save_and_replace_rules` prepares a full manager before disk write then performs an infallible swap under lifecycle lock. This avoids partial configuration changes.
Ruling: system trust/proxy changes and actual LaunchAgent registration are not automated smoke tests on the developer machine. Test injected restore settings and temporary bundles instead; these require explicit hardware verification before release.
Ruling: no commit or push was requested. Preserve all work and review uncommitted source directly.

Evidence logs in /private/tmp: proxy-fork-final-tests.log, proxy-fork-clippy.log, proxy-fork-final-msrv.log, proxy-fork-package-release.log. Passing logs may predate a small latest change; final reruns will follow review fixes.
Verified UI: packaged Metal window, all four pages, language switching, Chinese clipboard, draft preservation, no-CA/no-system-proxy listener start and stop on 127.0.0.1:18798. User system integration and signing credentials are deliberately untouched.
Not yet verified: real pinyin IME, mixed-DPI monitors, system proxy/Keychain/LaunchAgent, signed/notarized update on real bundles, Windows/Linux/Intel CI.

Review priorities: lifecycle/system proxy crash recovery and exit failures; save failure preserves disk and active rules; request handling compatibility; updater trust boundaries and rollback; event-driven rendering and font lifetime safety; platform build boundaries. Judge substantive issues by effect, not spec silence.
