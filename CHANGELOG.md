# Changelog

All notable user-facing changes to dredge, newest first. Entries are written
at release time by `scripts/ship release`.

## v0.5.0 — 2026-06-20

### Improved

- **Renaming a song renames its folder.** Renaming a song's title or artist now renames its bundle folder on disk to match, so your library folders stay in sync with what the app shows. The rename is refused while stem separation or analysis is running for that song, and a playing song reloads only when its folder actually moves.

## v0.4.0 — 2026-06-17

### New

- **Per-section notes, with tablature.** Every song section now has its own notes — free text plus inline tab you type into and resize by dragging its edges. A clean display mode for reading while you play, an edit mode for changing things. Notes autosave and stay attached to the section by name (your "verse 2" notes follow verse 2), and typing in them never triggers playback or other shortcuts.

### Improved

- **Feels like a real desktop app.** The web-style right-click menu (Back, Reload, Inspect…) no longer pops up anywhere in the app.

### Fixed

- **Accent color changes apply instantly on the waveform.** Picking a new theme color now recolors the loop, section, and selection markings right away, instead of only after you next touch the waveform.
- **Controls stay clickable in fullscreen.** Going fullscreen could make some controls — like the right-hand tabs — unreliable to click; they're solid again.

## v0.3.0 — 2026-06-17

### Improved

- **Bass focus now lives with the stems, in one "Isolation" box.** They do the same job — making one part of the song stand out — so they're together now instead of split across the screen. Bass focus is still a single click and works on any song right away, even before you separate it into stems.

### Fixed

- **Resizing a loop no longer cuts the sound.** Dragging a loop's edge while it played used to drop the audio for about a second; now it stays smooth as long as the playhead is still inside the loop.
- **The loop button on a selection stays clickable** when a selection sits on top of an existing loop.

## v0.2.2 — 2026-06-16

### New

- **`dredge-doctor`** — a terminal command that lists the optional tools (ffmpeg, analysis, stem separation), shows which are installed, and prints the command to add anything that's missing. The desktop app shows the same under Settings → capabilities.

### Improved

- **MP3 export works out of the box** — the package now pulls in `ffmpeg` by default, so exporting to MP3 (and separating stems) no longer needs a manual install.

### Fixed

- **Export file names** — the file-name field now shows the extension dredge adds (`.wav` / `.mp3`), and typing one yourself no longer doubles it (no more `track.mp3.mp3`).

## v0.2.1 — 2026-06-16

### Fixed

- **Release packaging** — fixes the Linux release build so this version publishes correctly. This is the first downloadable build carrying the v0.2.0 changes (export, opening video files, the Settings capabilities panel, the tidier loop toolbar, and the export `~`-path fix — see v0.2.0 below). No app behavior changed since v0.2.0.

## v0.2.0 — 2026-06-16

### New

- **Export your practice** — render a loop or the whole song to WAV or MP3 at the tempo and pitch you've been drilling at, with your stem mix and bass-focus baked in. There's a new "export" tab in the right-hand panel.
- **Open video files** — load an mp4 or mov and dredge pulls the audio track straight out of it.
- **See what's installed** — Settings now has a capabilities panel showing whether stem separation, structure analysis, and MP3 export are ready on your machine, with a full-or-partial summary at a glance.

### Improved

- **Cleaner loop toolbar** — a clearer "save loop" icon, and the grid/snap controls now tuck into a corner button that slides open when you want them.
- **Export shows progress and can be cancelled** part-way through a render.
- **The guide explains the side panels** — click the corner icons (or press Ctrl+[ / Ctrl+]) to hide and show the library and panels.
- **More audio formats play** — a wider range of files decode via an ffmpeg fallback.

### Fixed

- **Export to a `~` path works** — typing `~/Music` now lands in your home folder instead of erroring or creating a stray folder; a relative path is rejected with a clear message.
- **Export checks the folder and file name up front**, so a bad path fails instantly instead of half-way through a render.
- **Some mp4/mov files decode correctly now** — dredge reads the audio track instead of the container's default track.

### Removed

- **System-audio capture and grab-back** have been removed.
