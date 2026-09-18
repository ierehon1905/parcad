/**
 * The browser's own small storage, for the few things this window remembers
 * between sessions.
 *
 * Nothing about the *part* is kept here — a part is a file, and the host owns
 * it. What is kept is how this window was last arranged: whether the code was
 * showing, and the key a tab opens itself to agents with.
 *
 * Both accessors swallow their failures. A private window and a browser with
 * site data blocked both throw on the property itself, and none of this is
 * worth a broken window.
 */

export function stored(name: string): string | null {
  try {
    return localStorage.getItem(name);
  } catch {
    return null;
  }
}

export function store(name: string, value: string | null) {
  try {
    if (value === null) localStorage.removeItem(name);
    else localStorage.setItem(name, value);
  } catch {
    // A private window: what this remembers lasts as long as the tab, which is
    // all it can.
  }
}
