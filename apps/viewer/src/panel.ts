import type { NodeSnapshot, ValidationReport } from './api';

const required = <T extends Element>(selector: string): T => {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`missing viewer element: ${selector}`);
  return element;
};

const status = required<HTMLOutputElement>('[role="status"]');
const projectName = required<HTMLElement>('[data-role="project-name"]');
const revision = required<HTMLElement>('[data-role="revision"]');
const nodeList = required<HTMLOListElement>('[data-role="node-list"]');
const diagnostics = required<HTMLElement>('[data-role="diagnostics"]');
const selection = required<HTMLElement>('[data-role="selection"]');
const selectionDetails = required<HTMLElement>('[data-role="selection-details"]');

export function setReady(projectId: string, nextRevision: number): void {
  projectName.textContent = projectId;
  revision.textContent = `Revision ${nextRevision}`;
  status.textContent = `Revision ${nextRevision}`;
  status.dataset.state = 'ready';
}

export function setError(message: string): void {
  status.textContent = message;
  status.dataset.state = 'error';
}

export function setNodes(nodes: readonly NodeSnapshot[]): void {
  nodeList.replaceChildren(...nodes.map((node) => {
    const item = document.createElement('li');
    item.textContent = node.name;
    item.dataset.nodeId = node.id;
    return item;
  }));
}

export function setDiagnostics(report: ValidationReport): void {
  diagnostics.textContent = report.length === 0
    ? 'No issues'
    : report.map((item) => `${item.code}: ${item.message}`).join('\n');
  diagnostics.dataset.state = report.length === 0 ? 'ready' : 'error';
}

export function showSelection(node: NodeSnapshot | undefined): void {
  if (!node) {
    selection.hidden = true;
    selectionDetails.replaceChildren();
    return;
  }
  selection.hidden = false;
  const rows: [string, string][] = [
    ['ID', node.id],
    ['Name', node.name],
    ...Object.entries(node.metadata).map(([key, value]) => [key, value] as [string, string]),
  ];
  selectionDetails.replaceChildren(...rows.flatMap(([term, description]) => {
    const dt = document.createElement('dt');
    dt.textContent = term;
    const dd = document.createElement('dd');
    dd.textContent = description;
    return [dt, dd];
  }));
}
