// QA transport only; content/DOM geometry below are the production modules.
const handlers: ((message: unknown, sender: unknown, respond: (value: unknown) => void) => void)[] = [];
Object.assign(globalThis, { chrome: { runtime: { id: "geometry-qa", getManifest: () => ({manifest_version:3}), sendMessage: async (message: unknown) => { parent.postMessage({ qa: "runtime", message }, "*"); }, onMessage: { addListener: (handler: typeof handlers[number]) => handlers.push(handler) } } } });
window.addEventListener("message", event => {
  if (event.source !== parent || event.data?.qa !== "invoke") return;
  for (const handler of handlers) handler(event.data.message, { id: "geometry-qa" }, value => parent.postMessage({ qa: "response", id: event.data.id, value }, "*"));
});
await import("../browser-extension/src/content");
