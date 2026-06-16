# Changelog

All notable user-facing changes to earworm, newest first. Entries are written
at release time by `scripts/ship release`.

## v0.2.0 — 2026-06-16

### New

- **Export your practice** — render a loop or the whole song to WAV or MP3 at the tempo and pitch you've been drilling at, with your stem mix and bass-focus baked in. There's a new "export" tab in the right-hand panel.
- **Open video files** — load an mp4 or mov and earworm pulls the audio track straight out of it.
- **See what's installed** — Settings now has a capabilities panel showing whether stem separation, structure analysis, and MP3 export are ready on your machine, with a full-or-partial summary at a glance.

### Improved

- **Cleaner loop toolbar** — a clearer "save loop" icon, and the grid/snap controls now tuck into a corner button that slides open when you want them.
- **Export shows progress and can be cancelled** part-way through a render.
- **The guide explains the side panels** — click the corner icons (or press Ctrl+[ / Ctrl+]) to hide and show the library and panels.
- **More audio formats play** — a wider range of files decode via an ffmpeg fallback.

### Fixed

- **Export to a `~` path works** — typing `~/Music` now lands in your home folder instead of erroring or creating a stray folder; a relative path is rejected with a clear message.
- **Export checks the folder and file name up front**, so a bad path fails instantly instead of half-way through a render.
- **Some mp4/mov files decode correctly now** — earworm reads the audio track instead of the container's default track.

### Removed

- **System-audio capture and grab-back** have been removed.
