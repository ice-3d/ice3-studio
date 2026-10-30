// tests/session-store.test.ts
import assert from "node:assert/strict";
import test from "node:test";

// src/shared/ExternalStore.ts
var ExternalStore = class {
  value;
  listeners = /* @__PURE__ */ new Set();
  constructor(initial) {
    this.value = initial;
  }
  get snapshot() {
    return this.value;
  }
  subscribe = (listener) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  update(next) {
    this.value = next;
    for (const listener of this.listeners) listener();
  }
};

// src/session/SessionStore.ts
var SessionStore = class extends ExternalStore {
  constructor(source) {
    super({
      projectId: "",
      revision: 0,
      document: null,
      diagnostics: [],
      selectionIds: [],
      connection: "connecting",
      previewUrl: null
    });
    this.source = source;
  }
  source;
  generation = 0;
  async refresh() {
    const generation = ++this.generation;
    try {
      const [document, diagnostics] = await Promise.all([
        this.source.document(),
        this.source.validation()
      ]);
      const previewUrl = this.source.preview ? (await this.source.preview(document.revision)).url : this.snapshot.previewUrl;
      if (generation !== this.generation) return;
      this.update({
        ...this.snapshot,
        projectId: document.projectId,
        revision: document.revision,
        document,
        diagnostics,
        connection: "connected",
        previewUrl
      });
    } catch {
      if (generation === this.generation) {
        this.update({ ...this.snapshot, connection: "error" });
      }
    }
  }
  select(selectionIds) {
    this.update({ ...this.snapshot, selectionIds: [...selectionIds] });
  }
};

// tests/session-store.test.ts
test("revision events refresh the latest document without changing selection", async () => {
  let revision = 3;
  const store = new SessionStore({
    document: async () => ({
      projectId: "generic-project",
      revision,
      scene: { nodes: [] }
    }),
    validation: async () => []
  });
  await store.refresh();
  store.select(["node-000001"]);
  revision = 4;
  await store.refresh();
  assert.equal(store.snapshot.revision, 4);
  assert.deepEqual(store.snapshot.selectionIds, ["node-000001"]);
});
//# sourceMappingURL=session-store.test.js.map
