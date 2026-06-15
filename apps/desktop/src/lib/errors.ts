/** Normalize a thrown value to a display string. */
export function errMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
