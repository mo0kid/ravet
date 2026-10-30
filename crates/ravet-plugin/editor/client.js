// RAVET editor client, injected into every plug-in editor page.
// It implements the editor protocol (protocol 1) and exposes window.ravetPlugin.
(() => {
  "use strict";
  const listeners = { info: [], values: [], product: [] };
  let info = null;
  let sequence = -1;
  const values = {};
  const send = (message) => {
    const handler = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.ravet;
    if (handler) handler.postMessage(message);
  };
  const emit = (type, payload) => {
    for (const listener of listeners[type]) {
      try { listener(payload); } catch (error) { console.error(error); }
    }
  };
  // Called by the plug-in with each message from the plug-in to the editor.
  window.ravetUpdate = (message) => {
    if (!message || typeof message !== "object") return;
    switch (message.type) {
      case "info":
        info = message;
        emit("info", info);
        break;
      case "snapshot":
      case "changes":
        if (typeof message.sequence !== "number" || message.sequence <= sequence) return;
        sequence = message.sequence;
        Object.assign(values, message.parameters);
        emit("values", { values: { ...values }, changed: message.parameters,
          telemetry: message.telemetry, product: message.product });
        break;
      case "product":
        emit("product", message.data);
        break;
    }
  };
  const parameter = (key) => info && info.parameters.find((p) => p.key === key);
  window.ravetPlugin = {
    /** Product and parameter metadata, or null before the first `info`. */
    get info() { return info; },
    /** Current physical value of a parameter. */
    value(key) { return values[key]; },
    /** Current value of a parameter mapped to 0..1. */
    normalized(key) {
      const p = parameter(key);
      const v = values[key];
      if (!p || typeof v !== "number") return undefined;
      if (p.scale === "logarithmic") return Math.log(v / p.minimum) / Math.log(p.maximum / p.minimum);
      return (v - p.minimum) / (p.maximum - p.minimum);
    },
    /** Listen for "info", "values" or "product". */
    on(type, listener) {
      if (!listeners[type]) throw new Error(`unknown event ${type}`);
      listeners[type].push(listener);
      if (type === "info" && info) listener(info);
    },
    /** One complete edit, such as a click or typed value. */
    set(key, value) { send({ type: "commit", key, value }); },
    setNormalized(key, normalized) { send({ type: "commit", key, normalized }); },
    /** A continuous gesture: begin, any number of updates, end. */
    begin(key) { send({ type: "begin", key }); },
    update(key, value) { send({ type: "set", key, value }); },
    updateNormalized(key, normalized) { send({ type: "set", key, normalized }); },
    end(key) { send({ type: "end", key }); },
    /** A product-defined message to the plug-in. */
    send(data) { send({ type: "product", data }); },
  };
  const ready = () => send({ type: "ready", protocol: 1 });
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", ready, { once: true });
  else ready();
})();
