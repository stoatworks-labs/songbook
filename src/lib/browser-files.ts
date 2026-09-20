/**
 * Browser file pickers for Songbook Lite. The rest of the UI thinks in
 * paths, so a pick returns a token (`browser-file:<n>`) that stands for the
 * chosen `File` objects until the api consumes them with `takeFiles`.
 */
const picked = new Map<string, File[]>();
let counter = 0;

/** Hand files in without a picker (tests, drag-and-drop); returns the token. */
export function registerFiles(files: File[]): string {
  counter += 1;
  const token = `browser-file:${counter}`;
  picked.set(token, files);
  return token;
}

export function pickBrowserFiles(opts: { multiple?: boolean; directory?: boolean; accept?: string }): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.multiple = Boolean(opts.multiple || opts.directory);
    if (opts.accept) input.accept = opts.accept;
    if (opts.directory) input.setAttribute('webkitdirectory', '');
    input.style.display = 'none';
    document.body.appendChild(input);
    let settled = false;
    const finish = (files: File[]) => {
      if (settled) return;
      settled = true;
      input.remove();
      resolve(files.length === 0 ? null : registerFiles(files));
    };
    input.onchange = () => finish(Array.from(input.files ?? []));
    // Cancel: focus comes back with no change event.
    window.addEventListener(
      'focus',
      () => {
        setTimeout(() => finish(Array.from(input.files ?? [])), 400);
      },
      { once: true },
    );
    input.click();
  });
}


/** The files behind a token; the token is spent. */
export function takeFiles(token: string): File[] {
  const files = picked.get(token) ?? [];
  picked.delete(token);
  return files;
}
