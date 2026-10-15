let worker: Worker | null = null;
let busy = false;

export async function compactPng(original: Blob): Promise<Blob> {
  if (busy || original.size < 4096 || original.size > 16 * 1024 * 1024) return original;
  busy = true;
  try {
    worker ??= new Worker(new URL("./png-worker.ts", import.meta.url), { type: "module" });
    const active = worker;
    const bytes = await original.arrayBuffer();
    return await new Promise<Blob>(resolve => {
      const finish = (result: Blob, failed = false) => {
        clearTimeout(timer); active.onmessage = null; active.onerror = null;
        if (failed) { active.terminate(); worker = null; }
        resolve(result);
      };
      const timer = setTimeout(() => finish(original, true), 1500);
      active.onmessage = event => {
        const data = event.data;
        finish(data instanceof ArrayBuffer && data.byteLength > 0 && data.byteLength < original.size ? new Blob([data], { type: "image/png" }) : original);
      };
      active.onerror = event => { event.preventDefault(); finish(original, true); };
      try { active.postMessage(bytes, [bytes]); } catch { finish(original, true); }
    });
  } catch { return original; }
  finally { busy = false; }
}
