// Node check of the injected editor client, with a minimal window/document.
// Run: node crates/ravet-plugin/tests/client.mjs
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
import vm from "node:vm";

// Objects from the sandboxed context have their own prototypes; compare as JSON.
const plain = (value) => JSON.parse(JSON.stringify(value));
const posted = [];
const window = { webkit: { messageHandlers: { ravet: { postMessage: (m) => posted.push(m) } } } };
const document = { readyState: "complete", addEventListener() {} };
const source = readFileSync(new URL("../editor/client.js", import.meta.url), "utf8");
vm.runInNewContext(source, { window, document, console });

const plugin = window.ravetPlugin;
assert.deepEqual(plain(posted.shift()), { type: "ready", protocol: 1 });
assert.equal(plugin.info, null);

const seen = [];
plugin.on("info", (info) => seen.push(["info", info.product.name]));
plugin.on("values", ({ changed }) => seen.push(["values", JSON.stringify(changed)]));
plugin.on("product", (data) => seen.push(["product", data]));
plugin.on("values", () => { throw new Error("listener errors are contained"); });
const errors = [];
console.error = (error) => errors.push(error);

window.ravetUpdate({ type: "info", protocol: 1, product: { name: "Gain" },
  parameters: [{ key: "gain", minimum: 0, maximum: 4, scale: "linear" },
               { key: "cutoff", minimum: 20, maximum: 20000, scale: "logarithmic" }] });
window.ravetUpdate({ type: "snapshot", sequence: 2, parameters: { gain: 1, cutoff: 200 } });
window.ravetUpdate({ type: "snapshot", sequence: 1, parameters: { gain: 3 } }); // stale
window.ravetUpdate({ type: "changes", sequence: 3, parameters: { gain: 2 } });
window.ravetUpdate({ type: "product", data: { preset: "warm" } });
window.ravetUpdate(null);
window.ravetUpdate({ type: "unknown" });

assert.equal(plugin.value("gain"), 2);
assert.equal(plugin.normalized("gain"), 0.5);
assert.ok(Math.abs(plugin.normalized("cutoff") - 1 / 3) < 1e-12);
assert.equal(plugin.normalized("missing"), undefined);
assert.equal(errors.length, 2);
assert.deepEqual(plain(seen), [
  ["info", "Gain"],
  ["values", '{"gain":1,"cutoff":200}'],
  ["values", '{"gain":2}'],
  ["product", { preset: "warm" }],
]);
let late = null;
plugin.on("info", (info) => { late = info.product.name; });
assert.equal(late, "Gain", "late info listeners are called at once");
assert.throws(() => plugin.on("other", () => {}));

plugin.set("gain", 1.5);
plugin.setNormalized("gain", 0.25);
plugin.begin("gain");
plugin.update("gain", 3);
plugin.updateNormalized("gain", 1);
plugin.end("gain");
plugin.send({ hello: true });
assert.deepEqual(plain(posted), [
  { type: "commit", key: "gain", value: 1.5 },
  { type: "commit", key: "gain", normalized: 0.25 },
  { type: "begin", key: "gain" },
  { type: "set", key: "gain", value: 3 },
  { type: "set", key: "gain", normalized: 1 },
  { type: "end", key: "gain" },
  { type: "product", data: { hello: true } },
]);
console.log("RAVET editor client: protocol checks passed.");
