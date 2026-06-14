/** Pure musical interpretation of a frequency. A4 = 440 Hz, equal temperament. */

const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"] as const;

export interface NoteReading {
  note: string;
  octave: number;
  /** Signed cents from the nearest semitone, -50..+50. */
  cents: number;
}

export function hzToReading(hz: number): NoteReading {
  const midi = 69 + 12 * Math.log2(hz / 440);
  const rounded = Math.round(midi);
  const cents = Math.round((midi - rounded) * 100);
  const note = NOTE_NAMES[((rounded % 12) + 12) % 12];
  const octave = Math.floor(rounded / 12) - 1;
  return { note, octave, cents };
}
