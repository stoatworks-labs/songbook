/**
 * Songbook Lite's bridge to the Rust core, compiled to WebAssembly.
 *
 * One exported call, bytes in and bytes out, framed as `songbook-wasm`
 * documents: `u32 json length | JSON | u32 blob count | (u32 length | bytes)*`
 * both ways, little-endian. No wasm-bindgen glue: the module imports nothing,
 * so instantiating it is a fetch and a call.
 */

interface Exports {
  memory: WebAssembly.Memory;
  sb_alloc: (len: number) => number;
  sb_free: (ptr: number, len: number) => void;
  sb_call: (ptr: number, len: number) => number;
  sb_set_clock: (secs: bigint) => void;
  sb_seed: (seed: bigint) => void;
}

let exportsPromise: Promise<Exports> | null = null;

function load(): Promise<Exports> {
  if (!exportsPromise) {
    exportsPromise = (async () => {
      const url = new URL('songbook.wasm', document.baseURI).toString();
      const res = await fetch(url);
      if (!res.ok) throw new Error(`songbook.wasm: HTTP ${res.status}`);
      const { instance } = 'instantiateStreaming' in WebAssembly && res.headers.get('content-type')?.includes('application/wasm')
        ? await WebAssembly.instantiateStreaming(res, {})
        : await WebAssembly.instantiate(await res.arrayBuffer(), {});
      const ex = instance.exports as unknown as Exports;
      const seed = new BigUint64Array(1);
      crypto.getRandomValues(seed);
      ex.sb_seed(seed[0]);
      return ex;
    })();
  }
  return exportsPromise;
}

export interface WasmResult<T = unknown> {
  ok: T;
  blobs: Uint8Array[];
}

const enc = new TextEncoder();
const dec = new TextDecoder();

/** Run one operation in the Rust core. Throws the core's error message. */
export async function call<T = unknown>(op: string, header: Record<string, unknown> = {}, blobs: Uint8Array[] = []): Promise<WasmResult<T>> {
  const ex = await load();
  ex.sb_set_clock(BigInt(Math.floor(Date.now() / 1000)));
  const json = enc.encode(JSON.stringify({ op, ...header }));
  const total = 4 + json.length + 4 + blobs.reduce((n, b) => n + 4 + b.length, 0);
  const ptr = ex.sb_alloc(total);
  if (!ptr) throw new Error('songbook.wasm: out of memory');
  const mem = new Uint8Array(ex.memory.buffer, ptr, total);
  const view = new DataView(ex.memory.buffer, ptr, total);
  let at = 0;
  view.setUint32(at, json.length, true);
  at += 4;
  mem.set(json, at);
  at += json.length;
  view.setUint32(at, blobs.length, true);
  at += 4;
  for (const b of blobs) {
    view.setUint32(at, b.length, true);
    at += 4;
    mem.set(b, at);
    at += b.length;
  }
  const out = ex.sb_call(ptr, total);
  // The buffer may have moved if memory grew: re-read through the live buffer.
  const head = new DataView(ex.memory.buffer, out, 8);
  const outTotal = head.getUint32(0, true);
  const jsonLen = head.getUint32(4, true);
  const body = new Uint8Array(ex.memory.buffer, out + 4, outTotal).slice();
  ex.sb_free(out, outTotal + 4);
  const bodyView = new DataView(body.buffer);
  const result = JSON.parse(dec.decode(body.subarray(4, 4 + jsonLen))) as { ok?: T; error?: string };
  let p = 4 + jsonLen;
  const n = bodyView.getUint32(p, true);
  p += 4;
  const outBlobs: Uint8Array[] = [];
  for (let i = 0; i < n; i += 1) {
    const l = bodyView.getUint32(p, true);
    p += 4;
    outBlobs.push(body.slice(p, p + l));
    p += l;
  }
  if (result.error !== undefined) throw new Error(result.error);
  return { ok: result.ok as T, blobs: outBlobs };
}
