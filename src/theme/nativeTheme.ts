import { getCurrentWindow } from "@tauri-apps/api/window";

import { nativeThemeFor, type ThemePreference } from "./theme";

export type NativeTheme = "light" | "dark" | null;
export type SetNativeTheme = (theme: NativeTheme) => Promise<void>;

export interface NativeThemeSyncResult {
  generation: number;
  applied: boolean;
  current: boolean;
}

const noop = (): void => undefined;

export class NativeThemeQueue {
  private generation = 0;
  private tail: Promise<void> = Promise.resolve();

  constructor(private readonly setTheme: SetNativeTheme) {}

  request(preference: ThemePreference): Promise<NativeThemeSyncResult> {
    const generation = ++this.generation;
    const run = this.tail.catch(noop).then(async (): Promise<NativeThemeSyncResult> => {
      if (generation !== this.generation) {
        return { generation, applied: false, current: false };
      }

      try {
        await this.setTheme(nativeThemeFor(preference));
        return { generation, applied: true, current: generation === this.generation };
      } catch {
        return { generation, applied: false, current: generation === this.generation };
      }
    });

    this.tail = run.then(noop);
    return run;
  }
}

export function createNativeThemeSync(
  setTheme: SetNativeTheme,
  log: (message: string) => void = noop,
): (preference: ThemePreference) => Promise<NativeThemeSyncResult> {
  const queue = new NativeThemeQueue(setTheme);
  let failureReported = false;

  return async (preference) => {
    const result = await queue.request(preference);

    if (!result.applied && result.current && !failureReported) {
      failureReported = true;
      log("Native window theme synchronization is unavailable.");
    }

    return result;
  };
}

export const syncNativeTheme = createNativeThemeSync(
  (theme) => getCurrentWindow().setTheme(theme),
  import.meta.env.DEV ? console.debug : noop,
);
