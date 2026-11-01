import { mkdir, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { buildPlan } from './wastewater-plant/site-plan.mjs';

const DETAIL_MARKERS = [
  'window-frame',
  'door-frame',
  'roof-vent',
  'downpipe',
  'formwork',
  'shell-weld',
  'cable-tray',
  'local-control',
  'motor',
  'coupling',
];

export function buildWastewaterCapabilityReport(plan) {
  const definitions = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition }) => definition);
  const instances = plan.operations.filter(({ type }) => type === 'instance.create');
  const definitionById = new Map(definitions.map((definition) => [definition.id, definition]));
  const instanceCounts = new Map();
  for (const instance of instances) {
    const definitionId = instance.instance.definitionId;
    instanceCounts.set(definitionId, (instanceCounts.get(definitionId) ?? 0) + 1);
  }
  const repeatParts = definitions.flatMap((definition) => (
    definition.parts
      .filter((part) => Number(part.repeat) > 1)
      .map((part) => `${definition.id}/${part.id}`)
  ));
  const customMeshParts = definitions.flatMap((definition) => (
    definition.parts
      .filter((part) => part.content?.kind === 'custom_mesh')
      .map((part) => `${definition.id}/${part.id}`)
  ));
  const parameterizedComponents = definitions.filter(
    (definition) => definition.parameters.length > 0,
  );
  const closeUpDetailComponents = definitions.filter((definition) => (
    definition.parts.some((part) => DETAIL_MARKERS.some(
      (marker) => part.id.includes(marker),
    ))
  ));
  const sharedDefinitions = definitions.filter(
    (definition) => (instanceCounts.get(definition.id) ?? 0) > 1,
  );
  const missingInstanceDefinitions = instances
    .filter(({ instance }) => !definitionById.has(instance.definitionId))
    .map(({ name, instance }) => ({ name, definitionId: instance.definitionId }));

  return {
    schemaVersion: 1,
    componentDefinitions: definitions.length,
    componentInstances: instances.length,
    repeatPartDefinitions: repeatParts.length,
    repeatPartExamples: repeatParts.slice(0, 20),
    parameterizedComponents: parameterizedComponents.length,
    parameterizedExamples: parameterizedComponents
      .slice(0, 12)
      .map(({ id, name }) => ({ id, name })),
    sharedComponentDefinitions: sharedDefinitions.length,
    sharedExamples: sharedDefinitions
      .slice(0, 12)
      .map(({ id, name }) => ({ id, name, instances: instanceCounts.get(id) })),
    customMeshParts: customMeshParts.length,
    customMeshExamples: customMeshParts.slice(0, 12),
    closeUpDetailComponents: closeUpDetailComponents.length,
    closeUpDetailExamples: closeUpDetailComponents
      .slice(0, 16)
      .map(({ id, name }) => ({ id, name })),
    materialCount: plan.operations.filter(({ type }) => type === 'material.upsert').length,
    connectionCount: plan.operations.filter(({ type }) => type === 'connection.create').length,
    missingInstanceDefinitions,
  };
}

if (
  process.argv[1]
  && import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  const report = buildWastewaterCapabilityReport(buildPlan('wastewater-plant'));
  const scriptDir = dirname(fileURLToPath(import.meta.url));
  const outputPath = join(dirname(scriptDir), 'output/wastewater-asset-capabilities.json');
  await mkdir(dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ outputPath, ...report }, null, 2));
}
