import { open } from "@tauri-apps/plugin-dialog";

const AUDIO_EXTENSIONS = ["mp3", "flac", "ogg", "wav", "m4a"];

/** Native open dialog for a single audio file. Returns the chosen path, or
 *  null if the user cancelled. Keeps the Tauri dialog plugin out of components. */
export async function pickAudioFile(): Promise<string | null> {
  const path = await open({
    multiple: false,
    filters: [{ name: "audio", extensions: AUDIO_EXTENSIONS }],
  });
  return typeof path === "string" ? path : null;
}
