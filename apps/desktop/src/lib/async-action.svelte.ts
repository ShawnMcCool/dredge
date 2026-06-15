import { errMsg } from "./errors";

export interface AsyncAction {
  /** Last failure message, or null if the most recent run succeeded. */
  readonly error: string | null;
  /** True while a run is in flight. */
  readonly busy: boolean;
  /** Run an async fn, clearing error first and capturing any throw. */
  run(fn: () => Promise<unknown> | unknown): Promise<void>;
  /** Clear the current error. */
  clear(): void;
}

/**
 * Reactive wrapper for the repeated "clear error → set busy → try/catch →
 * unset busy" action scaffold. Returns a runes-backed object whose `error`
 * and `busy` are reactive when read in a component.
 */
export function asyncAction(): AsyncAction {
  let error = $state<string | null>(null);
  let busy = $state(false);
  return {
    get error() {
      return error;
    },
    get busy() {
      return busy;
    },
    clear() {
      error = null;
    },
    async run(fn) {
      error = null;
      busy = true;
      try {
        await fn();
      } catch (e) {
        error = errMsg(e);
      } finally {
        busy = false;
      }
    },
  };
}
