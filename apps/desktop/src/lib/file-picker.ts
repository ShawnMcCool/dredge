import { open } from "@tauri-apps/plugin-dialog";

// Video containers (mp4/mov) are accepted for their audio track only — never
// played back. Symphonia decodes the audio and ignores the video.
const AUDIO_EXTENSIONS = ["mp3", "flac", "ogg", "wav", "m4a", "mp4", "mov"];

/** Native open dialog for a single audio or video file. Returns the chosen
 *  path, or null if the user cancelled. Video files are loaded for their audio
 *  track only. Keeps the Tauri dialog plugin out of components. */
export async function pickAudioFile(): Promise<string | null> {
  const path = await open({
    multiple: false,
    filters: [{ name: "audio / video", extensions: AUDIO_EXTENSIONS }],
  });
  return typeof path === "string" ? path : null;
}
