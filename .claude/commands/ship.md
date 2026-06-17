---
description: Commit and push git changes — and optionally tag a release with a user-facing changelog and release-safety check
allowed-tools: Bash, AskUserQuestion, Read, Write, Edit
---

You are shipping dredge. All release mechanics are deterministic and live in `scripts/ship` (run it with no args for full usage) — your job is only the parts that need judgment: commit messages, the user-facing release notes, and reviewing any safety-check hunks the script flags.

## Arguments

- `/ship` — plain ship. Commit working change(s), push `main`. No tag.
- `/ship major|minor|patch` — ship AND release: bump version, changelog, release-safety gate, tag. Anything else: invalid, stop.

## Dredge specifics (read before pushing)

- **main is kept local and the SSH key is often locked.** Pushing is the explicit intent of `/ship`, so it's allowed here — but if a push fails, report it and surface the manual command (`scripts/ship` already prints it). **Never force-push.** Don't retry in a loop.
- **`just check` (tests + lint) is the prerequisite**, not part of ship. If it hasn't run green this session, run it first. `scripts/ship check` gates the *release path* (clean build, schema safety), not code quality.
- **Version source of truth is `apps/desktop/src-tauri/tauri.conf.json`.** The tag follows it exactly (release.yml triggers on `v*`); `scripts/ship release` owns the bump — never tag by hand.

## Plain ship (no version argument)

1. Skip if the tree is clean with no unpushed commits. Halt if behind `origin/main` ("pull/rebase first").
2. If the tree is dirty, read the diff and commit with conventional-prefix messages (`feat:`, `fix:`, `refactor:`, `docs:`, …) — split distinct work into separate commits, stage by named paths (never blind `git add -A`).
3. `git push origin main`. If it fails (locked key / offline), report and stop — don't force, don't loop.

## Release ship (`/ship major|minor|patch`)

1. **Commit pending work** as in plain ship (don't push yet — the release push carries it).
2. **`scripts/ship prepare <level>`** — prints `NEXT_VERSION`, `NOTES_FILE`, and the commits since the last tag. Read-only.
3. **Start the safety gate in the background**: `scripts/ship check` (runs `just artifacts` — a full release build, takes minutes). Write the release notes while it builds.
4. **Write the notes** to the `NOTES_FILE` path from step 2 — body only, no version header (the script adds `## v<version> — <date>`). Voice rules below. Don't pause for approval — best-effort and proceed.
5. **When the gate passes**: `scripts/ship release <level> --notes <NOTES_FILE>`. Inserts the changelog entry, commits, bumps `tauri.conf.json`, commits, pushes `main`, tags, pushes the tag. (If the push step fails on a locked key, the script tells the user how to finish — relay that.)
6. **`scripts/ship verify`** — polls the GitHub release until the `.deb` + tarball + `SHA256SUMS` are present (fails fast if the workflow run failed). Report the result.

### If `scripts/ship check` fails

Print the failure list verbatim. One check flags a diff for *judgment*:

- **`crates/practice/src/store.rs` touched since the last tag** — read the printed hunk. The schema rule is additive: a NEW `PRAGMA user_version` block, never an edit to a shipped one (editing a released block silently diverges existing users' databases). If the change is a new version block, re-run with `--allow-schema-change`. If it edits a shipped block, halt and fix.

Everything else (malformed changelog, `just artifacts` build failure) must be fixed, not overridden — a broken release is worse than a delayed one.

## Release-notes voice

dredge users are musicians drilling passages by ear, not engineers. Notes land on the GitHub release page (and CHANGELOG.md).

- **Translate jargon.** `fix(export): expand ~ and require absolute folder` → `Fixed: typing ~/Music in the export folder now lands in your home folder instead of erroring.`
- **Drop contributor-only items.** Refactors, test-only changes, CI tweaks, dependency bumps with no audible/visible effect → omit.
- **Group under `### New` / `### Improved` / `### Fixed`** — skip empty sections. Lead each bullet with a bold plain-language summary.
- **Voice:** present tense, active, second person where natural. No emoji, no hype.

## Important

- NEVER force-push `main`; never `git add -A` blindly.
- Create NEW commits rather than amending already-pushed ones.
- The tag follows `tauri.conf.json` exactly; `scripts/ship release` owns it — never tag by hand.
