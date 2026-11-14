// Parlato: optimistic save. Shows the new value right away and puts the old
// one back if the backend refuses it, so a switch never shows a setting that
// was not saved. `onFail` lets the caller show "Couldn't save that" (see
// hooks/useSaveFailure.tsx).

export async function persist<T>(
  next: T,
  prev: T,
  set: (v: T) => void,
  save: (v: T) => Promise<unknown>,
  onFail?: (e: unknown) => void,
): Promise<boolean> {
  set(next);
  try {
    await save(next);
    return true;
  } catch (e) {
    console.error(e);
    set(prev);
    onFail?.(e);
    return false;
  }
}
