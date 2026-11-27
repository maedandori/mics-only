# MICS-only contributor checks

- Use feature branches and passing relevant PR checks before merging or releasing.
- Web-only changes in `local-vue/**` (except the shared `local-vue/package.json`) or `.github/workflows/web-release.yml` require the independent Web CI: HTML/CSS/JS build, native Chromium checks, regression tests and package validation. Do not require Windows EXE, Launcher or Tauri builds for these changes. Bump `local-vue/web-release-version.txt` to publish a new independent Web release.
- Changes outside the Web-only scope (except contributor guidance), including mixed Web/Core changes, the shared package version, Rust, Launcher and Tauri, require passing Windows PR CI. Changes to the Windows workflow itself require Windows and Web PR checks; merging workflow-only changes does not republish an unchanged Core release. Manual Windows workflow dispatch remains available.
- Preserve Firebird read-only operation, lease protocol, MICS forecast and sync cadence when editing UI.
- Japanese user-facing labels and messages; preserve technical identifiers, product data and SQL syntax.
- Use literal replacement callbacks, validate edited files, retain all existing regression tests, and verify release assets before reporting success.
