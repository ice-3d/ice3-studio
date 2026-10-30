import { StudioClient, type NodeSnapshot } from './api';
import {
  setDiagnostics,
  setError,
  setNodes,
  setReady,
  showSelection,
} from './panel';
import { SceneView } from './scene';

declare global {
  interface Window {
    __studioTest: {
      publishRevision(revision: number): void;
    };
  }
}

const client = new StudioClient();
const scene = new SceneView();
const stage = document.querySelector<HTMLElement>('[data-role="stage"]');
if (!stage) throw new Error('Studio stage is missing');
let currentNodes: NodeSnapshot[] = [];

function selectVisibleNode(ids: readonly string[]): void {
  const selectedId = ids[0];
  showSelection(currentNodes.find((node) => (
    node.id === selectedId
    || Object.values(node.metadata).includes(selectedId ?? '')
  )));
}

async function refresh(): Promise<void> {
  try {
    await client.health();
    const snapshot = await client.scene();
    const diagnostics = await client.validation();
    currentNodes = [...snapshot.scene.nodes];
    setReady(snapshot.projectId, snapshot.revision);
    setNodes(snapshot.scene.nodes);
    setDiagnostics(diagnostics);
    const preview = await client.preview(snapshot.revision);
    await scene.show(preview.url);
  } catch (error) {
    setError(error instanceof Error ? error.message : String(error));
  }
}

let refreshQueue = Promise.resolve();
function queueRefresh(): Promise<void> {
  refreshQueue = refreshQueue.then(refresh, refresh);
  return refreshQueue;
}

await scene.mount(stage);
scene.onSelection = selectVisibleNode;
window.__studioTest = {
  publishRevision(revision: number): void {
    setReady('wastewater-mini', revision);
  },
};
client.subscribe(() => {
  void queueRefresh();
});
await queueRefresh();
