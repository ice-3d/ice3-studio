const identity = () => ({
  translation: { x: 0, y: 0, z: 0 },
  rotation: [0, 0, 0, 1],
  scale: { x: 1, y: 1, z: 1 },
});

const vec = (x, y, z) => ({ x, y, z });
// 工程格式里字面量就是裸值；引用组件参数时才写 { param: 'name' }。
const literal = (value) => value;

function transform(x = 0, y = 0, z = 0, options = {}) {
  return {
    translation: literal(vec(x, y, z)),
    rotation: options.rotation ?? [0, 0, 0, 1],
    scale: literal(options.scale ?? vec(1, 1, 1)),
  };
}

const box = (width, height, depth) => ({
  kind: 'box',
  width: literal(width),
  height: literal(height),
  depth: literal(depth),
});

const cylinder = (radius, height, segments = 128) => ({
  kind: 'cylinder',
  radius: literal(radius),
  height: literal(height),
  segments,
});

const sphere = (radius, segments = 96, rings = 48) => ({
  kind: 'sphere',
  radius: literal(radius),
  segments,
  rings,
});

const rotateY = (angle) => [0, Math.sin(angle / 2), 0, Math.cos(angle / 2)];
const rotateZ90 = [0, 0, Math.SQRT1_2, Math.SQRT1_2];
const rotateX90 = [Math.SQRT1_2, 0, 0, Math.SQRT1_2];

function part(id, content, x, y, z, options = {}) {
  const value = {
    id,
    parentId: options.parentId ?? null,
    transform: transform(x, y, z, options),
    content,
    materialRole: null,
    metadata: options.metadata ?? {},
    visible: options.visible ?? true,
  };
  partMaterialKeys.set(value, options.role ?? null);
  return value;
}

function port(name, partId, x, y, z, direction, medium = 'liquid', diameter = 1.2) {
  return {
    name,
    partId,
    position: vec(x, y, z),
    direction,
    medium,
    diameter,
  };
}

function rectangularRail(prefix, width, depth, baseY, options = {}) {
  const spacing = options.spacing ?? 4;
  const height = options.height ?? 1.2;
  const bar = options.bar ?? 0.11;
  const parts = [
    part(`${prefix}-rail-north`, box(width, bar, bar), 0, baseY + height, -depth / 2, { role: 'safety' }),
    part(`${prefix}-rail-south`, box(width, bar, bar), 0, baseY + height, depth / 2, { role: 'safety' }),
    part(`${prefix}-rail-west`, box(bar, bar, depth), -width / 2, baseY + height, 0, { role: 'safety' }),
    part(`${prefix}-rail-east`, box(bar, bar, depth), width / 2, baseY + height, 0, { role: 'safety' }),
  ];
  let ordinal = 1;
  for (let x = -width / 2; x <= width / 2 + 0.01; x += spacing) {
    parts.push(
      part(`${prefix}-post-n-${ordinal}`, box(bar, height, bar), x, baseY + height / 2, -depth / 2, { role: 'steel' }),
      part(`${prefix}-post-s-${ordinal}`, box(bar, height, bar), x, baseY + height / 2, depth / 2, { role: 'steel' }),
    );
    ordinal += 1;
  }
  for (let z = -depth / 2 + spacing; z < depth / 2; z += spacing) {
    parts.push(
      part(`${prefix}-post-w-${ordinal}`, box(bar, height, bar), -width / 2, baseY + height / 2, z, { role: 'steel' }),
      part(`${prefix}-post-e-${ordinal}`, box(bar, height, bar), width / 2, baseY + height / 2, z, { role: 'steel' }),
    );
    ordinal += 1;
  }
  return parts;
}

function circularRail(prefix, radius, y, segments = 24) {
  const parts = [];
  const arcLength = (2 * Math.PI * radius) / segments;
  for (let index = 0; index < segments; index += 1) {
    const angle = (index / segments) * Math.PI * 2;
    parts.push(part(
      `${prefix}-ring-${index + 1}`,
      box(arcLength + 0.08, 0.1, 0.1),
      Math.cos(angle) * radius,
      y,
      Math.sin(angle) * radius,
      { rotation: rotateY(-angle), role: 'safety' },
    ));
    if (index % 2 === 0) {
      parts.push(part(
        `${prefix}-post-${index + 1}`,
        box(0.1, 1.15, 0.1),
        Math.cos(angle) * radius,
        y - 0.575,
        Math.sin(angle) * radius,
        { role: 'steel' },
      ));
    }
  }
  return parts;
}

function stair(prefix, x, y, z, steps = 12, direction = 1) {
  const parts = [];
  for (let index = 0; index < steps; index += 1) {
    parts.push(part(
      `${prefix}-step-${index + 1}`,
      box(1.8, 0.16, 0.62),
      x,
      y + (index + 1) * 0.18,
      z + direction * (steps - index) * 0.62,
      { role: 'steel' },
    ));
  }
  for (const side of [-0.92, 0.92]) {
    parts.push(part(
      `${prefix}-stringer-${side > 0 ? 'east' : 'west'}`,
      box(0.1, 0.12, 7.7),
      x + side,
      y + 1.15,
      z + direction * 3.9,
      { rotation: [-0.27 * direction, 0, 0, Math.sqrt(1 - 0.27 * 0.27)], role: 'safety' },
    ));
  }
  return parts;
}

function windowBand(prefix, width, y, z, count = 8) {
  return Array.from({ length: count }, (_, index) => part(
    `${prefix}-window-${index + 1}`,
    box(1.35, 1.7, 0.14),
    -width / 2 + 2 + index * ((width - 4) / Math.max(1, count - 1)),
    y,
    z,
    { role: 'glass' },
  ));
}

const materials = new Map();
const components = new Map();
const componentPartRoles = new Map();
let partMaterialKeys = new WeakMap();
const nodes = new Map();
const operations = [];
let nextMaterial = 1;
let nextComponent = 1;
let nextNode = 1;
let nextRelation = 1;

function material(key, name, color, edgeColor = null) {
  const id = `material-${String(nextMaterial++).padStart(6, '0')}`;
  materials.set(key, id);
  operations.push({
    type: 'material.upsert',
    material_id: id,
    name,
    base_color: color,
    opacity: color[3],
    edge_color: edgeColor,
  });
  return id;
}

function component(key, name, parts, ports = [], parameters = []) {
  const id = `component-${String(nextComponent++).padStart(6, '0')}`;
  const definition = {
    id,
    name,
    parameters,
    parts,
    ports,
    metadata: { source: 'fresh-agent-recreation' },
  };
  components.set(key, definition);
  componentPartRoles.set(
    id,
    new Map(parts.map((partDefinition) => [
      partDefinition.id,
      partMaterialKeys.get(partDefinition),
    ])),
  );
  return definition;
}

function group(key, name, parentId = null, metadata = {}) {
  const id = `node-${String(nextNode++).padStart(6, '0')}`;
  nodes.set(key, id);
  operations.push({
    type: 'node.create',
    node_id: id,
    parent_id: parentId,
    name,
    transform: identity(),
    content: { type: 'group' },
    material_id: null,
    metadata,
  });
  return id;
}

function siteBox(key, name, parentId, x, y, z, width, height, depth, materialKey, metadata = {}) {
  const id = `node-${String(nextNode++).padStart(6, '0')}`;
  nodes.set(key, id);
  operations.push({
    type: 'node.create',
    node_id: id,
    parent_id: parentId,
    name,
    transform: { ...identity(), translation: vec(x, y, z) },
    content: { type: 'primitive', kind: 'box', width, height, depth },
    material_id: materials.get(materialKey),
    metadata,
  });
  return id;
}

function overridesFor(definition) {
  const roleMaterials = {
    water: materials.get('water'),
    sludge: materials.get('sludge'),
    safety: materials.get('safety'),
    roof: materials.get('roof'),
    glass: materials.get('glass'),
    steel: materials.get('steel'),
    dark: materials.get('darkMetal'),
    chemical: materials.get('chemical'),
  };
  const overrides = {};
  const roles = componentPartRoles.get(definition.id);
  for (const partDefinition of definition.parts) {
    const materialId = roleMaterials[roles.get(partDefinition.id)];
    if (materialId) {
      overrides[partDefinition.id] = {
        materialId,
        visible: null,
        metadata: null,
        parameters: {},
      };
    }
  }
  return overrides;
}

function instance(definitionKey, key, name, parentId, x, y, z, materialKey, metadata = {}) {
  const definition = components.get(definitionKey);
  if (!definition) throw new Error(`unknown component ${definitionKey}`);
  const id = `node-${String(nextNode++).padStart(6, '0')}`;
  const instanceMetadata = {
    ...metadata,
    kind: {
      pump: 'lift-pump',
      screen: 'bar-screen',
      grit: 'grit-basin',
      equalization: 'equalization-tank',
      primaryClarifier: 'primary-structure',
      aeration: 'biological-reactor',
      blower: 'blower-equipment',
      secondaryClarifier: 'secondary-structure',
      filter: 'filter-cell',
      uv: 'disinfection-basin',
      thickener: 'thickener',
      digester: 'digester',
      dewatering: 'dewatering-unit',
      chemical: 'chemical-dosing',
      building: 'facility',
      analyzer: 'analyzer',
    }[definitionKey],
  };
  nodes.set(key, id);
  operations.push({
    type: 'instance.create',
    node_id: id,
    parent_id: parentId,
    name,
    transform: { ...identity(), translation: vec(x, y, z) },
    instance: {
      definitionId: definition.id,
      parameters: {},
      overrides: overridesFor(definition),
      metadata: instanceMetadata,
    },
    material_id: materials.get(materialKey),
    metadata: instanceMetadata,
  });
  return id;
}

function connect(fromKey, fromPort, toKey, toPort, name, medium, diameter, metadata = {}) {
  const fromNodeId = nodes.get(fromKey);
  const toNodeId = nodes.get(toKey);
  if (!fromNodeId || !toNodeId) throw new Error(`missing connection endpoint ${fromKey} -> ${toKey}`);
  const relationId = `relation-${String(nextRelation++).padStart(6, '0')}`;
  const connectionId = `node-${String(nextNode++).padStart(6, '0')}`;
  operations.push({
    type: 'relation.create',
    relation: {
      id: relationId,
      relationType: medium === 'air' ? 'utilities' : 'process',
      fromNodeId,
      toNodeId,
      metadata,
    },
  });
  operations.push({
    type: 'connection.create',
    node_id: connectionId,
    parent_id: null,
    name,
    transform: identity(),
    connection: {
      from: { kind: 'instance', nodeId: fromNodeId, port: fromPort },
      to: { kind: 'instance', nodeId: toNodeId, port: toPort },
      medium,
      diameter,
      routePolicy: medium === 'air'
        ? { kind: 'orthogonal' }
        : { kind: 'rack', elevation: 5.2 },
      relationId,
      metadata,
    },
    metadata,
  });
  operations.push({
    type: 'material.assign',
    node_id: connectionId,
    material_id: materials.get('steel'),
  });
}

async function rpc(method, params = {}, authority = 'agent') {
  requestId += 1;
  const headers = { 'content-type': 'application/json' };
  if (authority === 'agent') headers.authorization = `Bearer ${AGENT_TOKEN}`;
  if (authority === 'human') headers.cookie = `ice3_studio_approval=${APPROVAL_TOKEN}`;
  const response = await fetch(RPC_URL, {
    method: 'POST',
    headers,
    body: JSON.stringify({ jsonrpc: '2.0', id: requestId, method, params }),
  });
  const payload = await response.json();
  if (payload.error) throw new Error(`${method}: ${JSON.stringify(payload.error)}`);
  return payload.result;
}

function timed(label, start) {
  timings[label] = Math.round((performance.now() - start) * 10) / 10;
}

async function commitBatch(title, operations, idempotencyKey) {
  if (operations.length === 0) return null;
  const started = performance.now();
  const document = await rpc('document.get');
  const created = await rpc('changeset.create', {
    baseRevision: document.revision,
    title,
    idempotencyKey: `${idempotencyKey}-create`,
  });
  const added = await rpc('changeset.addOperations', {
    changeSetId: created.id,
    expectedVersion: created.version,
    idempotencyKey: `${idempotencyKey}-add`,
    entries: operations.map((operation) => ({
      source: { kind: 'agent', label: 'codex-large-wwtp-recreation' },
      operation,
    })),
  });
  const evaluated = await rpc('changeset.evaluate', {
    changeSetId: created.id,
    expectedVersion: added.version,
    includeCandidate: false,
  });
  if (evaluated.status !== 'ready' || !evaluated.candidateDigest) {
    throw new Error(`${title} is not ready: ${JSON.stringify(evaluated.diagnostics)}`);
  }
  const committed = await rpc('changeset.commit', {
    changeSetId: created.id,
    expectedVersion: evaluated.version,
    approvedDigest: evaluated.candidateDigest,
    idempotencyKey,
  });
  return {
    revision: committed.revision,
    operationCount: operations.length,
    elapsedMs: Math.round((performance.now() - started) * 10) / 10,
  };
}

function buildDesign() {
  material('ground', 'Site Aggregate', [0.34, 0.36, 0.33, 1], [0.18, 0.2, 0.18, 1]);
  material('concrete', 'Aged Concrete', [0.68, 0.7, 0.67, 1], [0.34, 0.36, 0.34, 1]);
  material('concreteDark', 'Wet Concrete', [0.46, 0.49, 0.48, 1], [0.2, 0.22, 0.21, 1]);
  material('water', 'Process Water', [0.025, 0.5, 0.66, 0.88], [0.01, 0.23, 0.3, 1]);
  material('sludge', 'Digested Sludge', [0.5, 0.35, 0.18, 1], [0.2, 0.13, 0.06, 1]);
  material('steel', 'Painted Steel', [0.58, 0.64, 0.69, 1], [0.24, 0.28, 0.31, 1]);
  material('darkMetal', 'Equipment Housing', [0.23, 0.28, 0.32, 1], [0.09, 0.11, 0.13, 1]);
  material('roof', 'Architectural Roof', [0.42, 0.48, 0.53, 1], [0.16, 0.2, 0.23, 1]);
  material('glass', 'Glazing', [0.08, 0.42, 0.58, 0.78], [0.02, 0.18, 0.25, 1]);
  material('road', 'Asphalt', [0.16, 0.18, 0.2, 1], [0.06, 0.07, 0.08, 1]);
  material('safety', 'Safety Yellow', [0.98, 0.7, 0.08, 1], [0.48, 0.32, 0.02, 1]);
  material('chemical', 'Chemical Feed', [0.42, 0.7, 0.25, 1], [0.16, 0.32, 0.08, 1]);

  const pumpParts = [
    part('base', box(5.8, 0.4, 3.2), 0, 0.2, 0, { role: 'dark' }),
    part('body', box(4.2, 2.1, 2.8), 0, 1.35, 0, { role: 'steel' }),
    part('volute', cylinder(1.25, 2.2, 160), 2.1, 1.45, 0, { rotation: rotateZ90, role: 'steel' }),
    part('motor', cylinder(1.2, 3.4, 160), -1.7, 2.5, 0, { rotation: rotateZ90, role: 'dark' }),
    part('coupling', cylinder(0.52, 1.1, 96), -3.55, 2.4, 0, { rotation: rotateZ90, role: 'safety' }),
    part('suction-pipe', cylinder(0.62, 2.8, 96), 2.8, 0.95, 0, { rotation: rotateZ90, role: 'steel' }),
    part('discharge-pipe', cylinder(0.55, 3.2, 96), 1.8, 3.3, 0, { role: 'steel' }),
    part('suction-valve', cylinder(0.78, 0.7, 96), 3.6, 0.95, 0, { rotation: rotateZ90, role: 'safety' }),
    part('discharge-valve', cylinder(0.72, 0.7, 96), 1.8, 4.1, 0, { role: 'safety' }),
    part('flange-a', cylinder(0.9, 0.18, 96), 4.0, 0.95, 0, { rotation: rotateZ90, role: 'dark' }),
    part('flange-b', cylinder(0.86, 0.18, 96), 1.8, 4.5, 0, { role: 'dark' }),
    part('control-box', box(1.15, 1.9, 0.75), -2.9, 1.2, 1.9, { role: 'dark' }),
    part('instrument', cylinder(0.12, 1.3, 48), -2.9, 2.6, 2.2, { role: 'safety' }),
    part('guard-west', box(0.1, 1.3, 2.6), -3.8, 1.15, 0, { role: 'safety' }),
    part('guard-east', box(0.1, 1.3, 2.6), 3.8, 1.15, 0, { role: 'safety' }),
    part('guard-north', box(7.6, 0.1, 0.1), 0, 1.78, -1.35, { role: 'safety' }),
    ...Array.from({ length: 4 }, (_, index) => part(
      `grout-bolt-${index + 1}`,
      cylinder(0.1, 0.24, 32),
      index < 2 ? -2.35 : 2.35,
      0.13,
      index % 2 === 0 ? -1.15 : 1.15,
      { role: 'dark' },
    )),
  ];
  component('pump', 'Process Pump', pumpParts, [
    port('inlet', 'body', -2.4, 1.15, 0, vec(-1, 0, 0), 'liquid', 1.2),
    port('outlet', 'body', 0, 3.0, 0, vec(0, 1, 0), 'liquid', 1.0),
    port('service', 'body', 2.1, 1.45, 0, vec(1, 0, 0), 'liquid', 0.8),
  ]);

  const screenParts = [
    part('channel', box(10, 2.5, 5.5), 0, 1.25, 0, { role: 'concrete' }),
    part('water', box(9.3, 1.2, 4.9), 0, 1.4, 0, { role: 'water' }),
    part('deck', box(10.8, 0.25, 6.2), 0, 2.65, 0, { role: 'steel' }),
    part('rake-frame', box(2.4, 3.8, 4.4), 0, 3.2, 0, { role: 'steel' }),
    part('drive', cylinder(0.78, 1.8, 128), 0, 5.2, 0, { rotation: rotateZ90, role: 'dark' }),
    part('gear', cylinder(0.55, 0.8, 96), 1.1, 5.2, 0, { rotation: rotateZ90, role: 'safety' }),
    part('chain-guard', box(4.8, 0.65, 1.9), -0.6, 4.65, 0, { role: 'dark' }),
    ...Array.from({ length: 18 }, (_, index) => part(
      `screen-bar-${index + 1}`,
      box(0.08, 2.7, 0.12),
      -1.36 + index * 0.16,
      1.7,
      0,
      { role: 'steel' },
    )),
    ...Array.from({ length: 8 }, (_, index) => part(
      `rake-tooth-${index + 1}`,
      box(0.16, 0.52, 0.16),
      -1.2 + index * 0.34,
      3.7,
      -1.7,
      { role: 'safety' },
    )),
    ...rectangularRail('screen-deck', 10.8, 6.2, 2.75, { spacing: 2.8 }),
  ];
  component('screen', 'Mechanical Bar Screen', screenParts, [
    port('inlet', 'channel', -5, 1.35, 0, vec(-1, 0, 0), 'liquid', 1.6),
    port('outlet', 'channel', 5, 1.35, 0, vec(1, 0, 0), 'liquid', 1.6),
  ]);

  const gritParts = [
    part('chamber', box(12, 4.2, 7), 0, 2.1, 0, { role: 'concrete' }),
    part('hopper', cylinder(2.8, 3.2, 160), 0, 0.6, 0, { role: 'dark' }),
    part('bridge', box(12.8, 0.42, 2.1), 0, 4.55, 0, { role: 'steel' }),
    part('screw-a', cylinder(0.38, 6, 128), -3.2, 1.2, 0, { rotation: rotateZ90, role: 'steel' }),
    part('screw-b', cylinder(0.38, 6, 128), 3.2, 1.2, 0, { rotation: rotateZ90, role: 'steel' }),
    part('drive-a', cylinder(0.76, 1.6, 128), -6.4, 1.65, 0, { rotation: rotateZ90, role: 'dark' }),
    part('drive-b', cylinder(0.76, 1.6, 128), 6.4, 1.65, 0, { rotation: rotateZ90, role: 'dark' }),
    part('classifier-a', box(3.2, 0.32, 1.6), -5.2, 3.7, 0, { role: 'steel' }),
    part('classifier-b', box(3.2, 0.32, 1.6), 5.2, 3.7, 0, { role: 'steel' }),
    part('water', box(11.2, 1.25, 6.2), 0, 2.4, 0, { role: 'water' }),
    ...rectangularRail('grit-bridge', 12.8, 7.2, 4.35, { spacing: 3 }),
    ...Array.from({ length: 6 }, (_, index) => part(
      `hopper-rib-${index + 1}`,
      box(0.14, 2.6, 0.14),
      -1.8 + index * 0.72,
      1.0,
      3.1,
      { rotation: [0, 0, index % 2 === 0 ? 0.25 : -0.25, 0.97], role: 'steel' },
    )),
  ];
  component('grit', 'Grit Removal Chamber', gritParts, [
    port('inlet', 'chamber', -6, 1.8, 0, vec(-1, 0, 0), 'liquid', 1.6),
    port('outlet', 'chamber', 6, 1.8, 0, vec(1, 0, 0), 'liquid', 1.6),
  ]);

  const equalizationParts = [
    part('tank', cylinder(12, 9, 224), 0, 4.5, 0, { role: 'concrete' }),
    part('water', cylinder(11.15, 6.7, 224), 0, 5.0, 0, { role: 'water' }),
    part('top-ring', cylinder(12.2, 0.3, 224), 0, 9.15, 0, { role: 'concrete' }),
    part('bridge', box(26, 0.45, 2.2), 0, 9.35, 0, { role: 'steel' }),
    part('mixer-drive', cylinder(1.05, 2.4, 160), 0, 10.2, 0, { role: 'dark' }),
    part('mixer-shaft', cylinder(0.2, 7.2, 64), 0, 6.9, 0, { role: 'steel' }),
    part('mixer-hub', cylinder(1.15, 0.5, 160), 0, 3.35, 0, { role: 'dark' }),
    part('inlet-pipe', cylinder(0.5, 5.2, 96), -10.2, 4.4, 0, { rotation: rotateZ90, role: 'steel' }),
    part('outlet-pipe', cylinder(0.48, 4.8, 96), 10.8, 4.2, 0, { rotation: rotateZ90, role: 'steel' }),
    ...Array.from({ length: 8 }, (_, index) => {
      const angle = (index / 8) * Math.PI * 2;
      return part(
        `mixer-blade-${index + 1}`,
        box(3.2, 0.12, 0.55),
        Math.cos(angle) * 1.7,
        3.35,
        Math.sin(angle) * 1.7,
        { rotation: rotateY(-angle), role: 'safety' },
      );
    }),
    ...circularRail('equalization-rail', 12.15, 10.2, 28),
    ...stair('equalization-stair', 10.6, 0, 3.2, 20, 1),
  ];
  component('equalization', 'Equalization Tank', equalizationParts, [
    port('inlet', 'tank', -12, 3, 0, vec(-1, 0, 0), 'liquid', 1.4),
    port('outlet', 'tank', 12, 3, 0, vec(1, 0, 0), 'liquid', 1.4),
  ]);

  function clarifierParts(prefix, detail) {
    const radius = detail.secondary ? 13.5 : 12;
    return [
      part('basin', cylinder(radius, 4.8, detail.secondary ? 240 : 224), 0, 2.4, 0, { role: 'concrete' }),
      part('water', cylinder(radius - 1.25, 2.2, detail.secondary ? 240 : 224), 0, 3.55, 0, { role: 'water' }),
      part('center-well', cylinder(2.8, 6.2, 160), 0, 3.1, 0, { role: 'concrete' }),
      part('center-drive', cylinder(1.2, 2.2, 128), 0, 6.8, 0, { role: 'dark' }),
      part('bridge', box(radius * 2 + 1.5, 0.45, 2), 0, 5.45, 0, { role: 'steel' }),
      part('walkway', box(radius * 1.85, 0.24, 1.15), 0, 5.82, 1.8, { role: 'safety' }),
      ...Array.from({ length: 14 }, (_, index) => {
        const angle = (index / 14) * Math.PI * 2;
        return part(
          `scraper-${index + 1}`,
          box(radius * 0.48, 0.16, 0.42),
          Math.cos(angle) * radius * 0.46,
          3.35,
          Math.sin(angle) * radius * 0.46,
          { rotation: rotateY(-angle), role: 'dark' },
        );
      }),
      ...Array.from({ length: 20 }, (_, index) => {
        const angle = (index / 20) * Math.PI * 2;
        return part(
          `weir-${index + 1}`,
          box(radius * 0.34, 0.24, 0.22),
          Math.cos(angle) * (radius - 1.2),
          4.75,
          Math.sin(angle) * (radius - 1.2),
          { rotation: rotateY(-angle), role: 'steel' },
        );
      }),
      ...circularRail(`${prefix}-rail`, radius + 0.35, 6.15, detail.secondary ? 30 : 26),
      ...stair(`${prefix}-stair`, radius - 0.4, 0, 3.2, detail.secondary ? 24 : 20, 1),
    ];
  }

  component('primaryClarifier', 'Primary Circular Clarifier', clarifierParts('primary', { secondary: false }), [
    port('inlet', 'basin', -12, 2.4, 0, vec(-1, 0, 0), 'liquid', 1.5),
    port('outlet', 'basin', 12, 2.4, 0, vec(1, 0, 0), 'liquid', 1.4),
    port('sludge', 'basin', 0, 2.4, -12, vec(0, 0, -1), 'liquid', 1.0),
  ]);

  const aerationParts = [
    part('basin', box(58, 5.4, 26), 0, 2.7, 0, { role: 'concrete' }),
    part('water', box(54, 3.4, 22), 0, 4.05, 0, { role: 'water' }),
    part('walkway', box(59, 0.42, 2.2), 0, 6.2, 0, { role: 'steel' }),
    part('blower-deck', box(11, 0.45, 7), 20, 5.8, 9, { role: 'steel' }),
    part('center-drive', cylinder(1.0, 2.2, 128), 0, 6.8, 0, { role: 'dark' }),
    ...Array.from({ length: 18 }, (_, index) => part(
      `diffuser-${index + 1}`,
      cylinder(0.17, 3.8, 64),
      -23 + index * 2.65,
      4.2,
      index % 2 === 0 ? -5.2 : 5.2,
      { role: 'steel' },
    )),
    ...Array.from({ length: 8 }, (_, index) => part(
      `air-drop-${index + 1}`,
      cylinder(0.25, 3.2, 80),
      -21 + index * 6,
      6.9,
      8,
      { role: 'safety' },
    )),
    ...Array.from({ length: 8 }, (_, index) => part(
      `air-header-${index + 1}`,
      box(6.2, 0.22, 0.22),
      -21 + index * 6,
      8.45,
      8,
      { role: 'steel' },
    )),
    ...rectangularRail('aeration-walkway', 59, 26, 6.35, { spacing: 4.6 }),
    ...rectangularRail('aeration-blowers', 11, 7, 5.8, { spacing: 2.5 }),
    ...stair('aeration-stair', 26, 0, 10, 22, 1),
  ];
  component('aeration', 'Aeration Basin', aerationParts, [
    port('inlet', 'basin', -29, 2.6, 0, vec(-1, 0, 0), 'liquid', 1.5),
    port('outlet', 'basin', 29, 2.6, 0, vec(1, 0, 0), 'liquid', 1.5),
    port('recycle', 'basin', 0, 2.6, -13, vec(0, 0, -1), 'liquid', 1.0),
    port('air', 'basin', 20, 5.8, 9, vec(0, 1, 0), 'air', 0.8),
    port('chemical', 'basin', 0, 2.6, 13, vec(0, 0, 1), 'liquid', 0.45),
  ]);

  const blowerParts = [
    part('skid', box(7, 1.05, 3.4), 0, 0.52, 0, { role: 'dark' }),
    part('volute', cylinder(1.75, 3.6, 192), 0, 2.75, 0, { rotation: rotateZ90, role: 'steel' }),
    part('motor', cylinder(1.15, 2.9, 160), 2.6, 2.35, 0, { rotation: rotateZ90, role: 'dark' }),
    part('coupling', cylinder(0.45, 1.0, 80), 4.1, 2.3, 0, { rotation: rotateZ90, role: 'safety' }),
    part('inlet-filter', box(2.7, 2.4, 2.4), -3.6, 2.2, 0, { role: 'steel' }),
    part('inlet-stack', cylinder(0.52, 4.2, 96), -3.6, 4.9, 0, { role: 'steel' }),
    part('outlet-pipe', cylinder(0.36, 5.6, 96), 0, 5, 0, { role: 'steel' }),
    part('silencer', cylinder(0.72, 2.2, 128), 0, 8, 0, { role: 'dark' }),
    part('control-panel', box(1.7, 2.5, 0.9), 4.2, 1.7, 1.2, { role: 'dark' }),
    ...Array.from({ length: 6 }, (_, index) => part(
      `filter-louver-${index + 1}`,
      box(2.2, 0.08, 0.12),
      -3.6,
      1.2 + index * 0.33,
      1.24,
      { role: 'safety' },
    )),
    ...Array.from({ length: 8 }, (_, index) => part(
      `base-bolt-${index + 1}`,
      cylinder(0.1, 0.28, 32),
      index < 4 ? -3.05 + index * 0.45 : 3.05 - (index - 4) * 0.45,
      0.13,
      index % 2 === 0 ? -1.35 : 1.35,
      { role: 'dark' },
    )),
  ];
  component('blower', 'Aeration Blower', blowerParts, [
    port('air', 'outlet-pipe', 0, 5.2, 0, vec(0, 1, 0), 'air', 0.8),
  ]);

  component('secondaryClarifier', 'Secondary Circular Clarifier', clarifierParts('secondary', { secondary: true }), [
    port('inlet', 'basin', -13.5, 2.4, 0, vec(-1, 0, 0), 'liquid', 1.5),
    port('outlet', 'basin', 13.5, 2.4, 0, vec(1, 0, 0), 'liquid', 1.4),
    port('return', 'basin', 0, 2.4, 13.5, vec(0, 0, 1), 'liquid', 1.0),
    port('waste', 'basin', 0, 2.4, -13.5, vec(0, 0, -1), 'liquid', 0.7),
  ]);

  const filterParts = [
    part('basin', box(19, 4, 12), 0, 2, 0, { role: 'concrete' }),
    part('media', box(16, 1.4, 9), 0, 2.25, 0, { role: 'sludge' }),
    part('water', box(15.2, 0.75, 8.2), 0, 3.35, 0, { role: 'water' }),
    part('bridge', box(19.8, 0.4, 1.8), 0, 4.4, 0, { role: 'steel' }),
    part('backwash-header', cylinder(0.43, 16, 128), 0, 4.0, -4.4, { rotation: rotateZ90, role: 'steel' }),
    part('backwash-valve', cylinder(0.52, 1.5, 96), 7.8, 3.4, -4.4, { role: 'safety' }),
    ...Array.from({ length: 20 }, (_, index) => part(
      `nozzle-${index + 1}`,
      cylinder(0.12, 0.72, 40),
      -7.4 + (index % 10) * 1.64,
      3.65,
      index < 10 ? -2.8 : 2.8,
      { role: 'dark' },
    )),
    ...rectangularRail('filter-rail', 19.8, 12.2, 4.4, { spacing: 3.1 }),
    ...stair('filter-stair', 8.5, 0, 4.4, 14, 1),
  ];
  component('filter', 'Tertiary Filter Cell', filterParts, [
    port('inlet', 'basin', -9.5, 1.9, 0, vec(-1, 0, 0), 'liquid', 1.3),
    port('outlet', 'basin', 9.5, 1.9, 0, vec(1, 0, 0), 'liquid', 1.2),
  ]);

  const uvParts = [
    part('channel', box(15, 2.7, 6.5), 0, 1.35, 0, { role: 'concrete' }),
    part('water', box(14.2, 1.4, 5.7), 0, 1.65, 0, { role: 'water' }),
    part('inlet-gate', box(0.4, 2.2, 4.8), -7.4, 1.6, 0, { role: 'safety' }),
    part('outlet-gate', box(0.4, 2.2, 4.8), 7.4, 1.6, 0, { role: 'safety' }),
    part('control-cabinet', box(1.6, 3.1, 1.1), 9, 1.55, 0, { role: 'dark' }),
    ...Array.from({ length: 12 }, (_, index) => part(
      `uv-bank-${index + 1}`,
      cylinder(0.16, 4.8, 64),
      -5.5 + (index % 6) * 2.2,
      3.1,
      index < 6 ? -1.8 : 1.8,
      { role: 'glass' },
    )),
    ...Array.from({ length: 18 }, (_, index) => part(
      `uv-lamp-${index + 1}`,
      cylinder(0.065, 4.1, 32),
      -5.8 + (index % 9) * 1.45,
      3.1,
      index < 9 ? -2.15 : 2.15,
      { role: 'safety' },
    )),
    ...rectangularRail('uv-rail', 15.2, 6.7, 2.7, { spacing: 2.6 }),
    ...stair('uv-stair', 6.2, 0, 3.4, 10, 1),
  ];
  component('uv', 'UV Disinfection Channel', uvParts, [
    port('inlet', 'channel', -7.5, 1.35, 0, vec(-1, 0, 0), 'liquid', 1.2),
    port('outlet', 'channel', 7.5, 1.35, 0, vec(1, 0, 0), 'liquid', 1.1),
  ]);

  const thickenerParts = [
    part('basin', cylinder(10, 5.4, 224), 0, 2.7, 0, { role: 'concrete' }),
    part('sludge', cylinder(8.8, 2.8, 224), 0, 3.1, 0, { role: 'sludge' }),
    part('bridge', box(21, 0.42, 1.8), 0, 5.7, 0, { role: 'steel' }),
    part('center-drive', cylinder(1.2, 2.5, 128), 0, 6.7, 0, { role: 'dark' }),
    ...Array.from({ length: 12 }, (_, index) => {
      const angle = (index / 12) * Math.PI * 2;
      return part(
        `rake-${index + 1}`,
        box(7.2, 0.16, 0.38),
        Math.cos(angle) * 3.8,
        3.8,
        Math.sin(angle) * 3.8,
        { rotation: rotateY(-angle), role: 'dark' },
      );
    }),
    ...circularRail('thickener-rail', 10.2, 6.2, 24),
    ...stair('thickener-stair', 8.8, 0, 3.2, 18, 1),
  ];
  component('thickener', 'Sludge Thickener', thickenerParts, [
    port('inlet', 'basin', -10, 2.7, 0, vec(-1, 0, 0), 'liquid', 0.9),
    port('outlet', 'basin', 10, 2.7, 0, vec(1, 0, 0), 'liquid', 0.8),
  ]);

  const digesterParts = [
    part('body', cylinder(12, 19, 240), 0, 9.5, 0, { role: 'concrete' }),
    part('dome', sphere(12, 128, 64), 0, 19, 0, { role: 'concrete' }),
    part('platform', box(27, 0.5, 4.2), 0, 13, 0, { role: 'steel' }),
    part('gas-header', cylinder(0.4, 24, 96), 0, 28, 0, { rotation: rotateZ90, role: 'steel' }),
    part('recirculation', cylinder(0.46, 17, 96), -6, 16, 7, { role: 'steel' }),
    ...Array.from({ length: 7 }, (_, index) => part(
      `band-${index + 1}`,
      cylinder(12.25, 0.25, 224),
      0,
      2.8 + index * 2.45,
      0,
      { role: 'dark' },
    )),
    ...Array.from({ length: 24 }, (_, index) => part(
      `ladder-rung-${index + 1}`,
      box(0.9, 0.08, 0.08),
      12.5,
      0.7 + index * 0.78,
      0,
      { role: 'safety' },
    )),
    ...circularRail('digester-platform', 13.4, 14, 28),
    ...stair('digester-stair', 11.2, 0, 4.2, 26, 1),
  ];
  component('digester', 'Anaerobic Digester', digesterParts, [
    port('inlet', 'body', -12, 5, 0, vec(-1, 0, 0), 'liquid', 0.9),
    port('outlet', 'body', 12, 5, 0, vec(1, 0, 0), 'liquid', 0.9),
  ]);

  const dewateringParts = [
    part('building', box(27, 10, 16), 0, 5, 0, { role: 'concrete' }),
    part('roof', box(29, 0.65, 18), 0, 10.3, 0, { role: 'roof' }),
    part('entrance', box(4, 3.4, 0.65), 0, 1.7, 8.3, { role: 'glass' }),
    part('centrifuge-a', cylinder(1.7, 7.5, 128), -7, 4.5, 0, { rotation: rotateZ90, role: 'steel' }),
    part('centrifuge-b', cylinder(1.7, 7.5, 128), 1, 4.5, 0, { rotation: rotateZ90, role: 'steel' }),
    part('polymer-header', cylinder(0.32, 20, 80), 0, 7.2, -4.6, { rotation: rotateZ90, role: 'chemical' }),
    part('cake-conveyor', box(19, 0.7, 1.5), 12, 2.0, 0, { role: 'dark' }),
    part('control-room', box(6.5, 3.5, 4.2), -11, 1.75, -7, { role: 'dark' }),
    ...windowBand('dewatering', 27, 5.5, 8.08, 9),
    ...Array.from({ length: 5 }, (_, index) => part(
      `roof-vent-${index + 1}`,
      cylinder(0.45, 1.4, 64),
      -10 + index * 5,
      11.2,
      0,
      { role: 'dark' },
    )),
  ];
  component('dewatering', 'Sludge Dewatering Building', dewateringParts, [
    port('inlet', 'building', -13.5, 3, 0, vec(-1, 0, 0), 'liquid', 0.8),
    port('outlet', 'building', 13.5, 2.2, 0, vec(1, 0, 0), 'liquid', 0.6),
  ]);

  const chemicalParts = [
    part('tank-a', cylinder(2.6, 6, 144), -2.1, 3, 0, { role: 'chemical' }),
    part('tank-b', cylinder(2.6, 6, 144), 2.1, 3, 0, { role: 'chemical' }),
    part('skid', box(8.5, 0.42, 5.4), 0, 0.21, 0, { role: 'dark' }),
    part('pump-a', box(1.5, 1.4, 1.2), -2.2, 0.9, 2.3, { role: 'steel' }),
    part('pump-b', box(1.5, 1.4, 1.2), 2.2, 0.9, 2.3, { role: 'steel' }),
    part('header', cylinder(0.22, 8, 64), 0, 3.3, 2.4, { rotation: rotateZ90, role: 'safety' }),
    part('control-box', box(1.2, 1.8, 0.8), 4.5, 1.1, -1.5, { role: 'dark' }),
    ...Array.from({ length: 4 }, (_, index) => part(
      `dosing-line-${index + 1}`,
      cylinder(0.08, 4.6, 32),
      -2.7 + index * 1.8,
      2.6,
      2.4,
      { role: 'chemical' },
    )),
    ...Array.from({ length: 4 }, (_, index) => part(
      `level-gauge-${index + 1}`,
      cylinder(0.1, 4.2, 24),
      index < 2 ? -4.35 : 4.35,
      3.2,
      index % 2 === 0 ? -2.1 : 2.1,
      { role: 'glass' },
    )),
  ];
  component('chemical', 'Chemical Dosing Station', chemicalParts, [
    port('outlet', 'pump-a', -2.2, 1.1, 3.0, vec(0, 0, 1), 'liquid', 0.45),
  ]);

  const buildingParts = [
    part('body', box(20, 8, 12), 0, 4, 0, { role: 'concrete' }),
    part('roof', box(21.5, 0.55, 13.5), 0, 8.25, 0, { role: 'roof' }),
    part('entrance', box(3.4, 3.2, 0.7), 0, 1.6, 6.3, { role: 'glass' }),
    part('canopy', box(5.2, 0.28, 2), 0, 3.4, 7, { role: 'steel' }),
    ...windowBand('service', 20, 4.6, 6.08, 7),
    ...Array.from({ length: 5 }, (_, index) => part(
      `roof-unit-${index + 1}`,
      box(1.6, 0.8, 1.2),
      -7 + index * 3.5,
      8.85,
      0,
      { role: 'dark' },
    )),
    ...Array.from({ length: 4 }, (_, index) => part(
      `side-window-${index + 1}`,
      box(0.14, 1.5, 1.6),
      10.08,
      4.4,
      -3.6 + index * 2.4,
      { role: 'glass' },
    )),
  ];
  component('building', 'Operations Building', buildingParts, []);

  const analyzerParts = [
    part('cabinet', box(1.3, 2.1, 0.9), 0, 1.05, 0, { role: 'dark' }),
    part('sample-pump', box(0.7, 0.6, 0.55), -1.1, 0.6, 0, { role: 'steel' }),
    part('sample-line', cylinder(0.06, 2.3, 24), 0, 1.5, 0.7, { role: 'safety' }),
    part('drain-line', cylinder(0.055, 1.5, 24), 0.55, 0.85, 0.55, { role: 'water' }),
    part('display', box(0.7, 0.45, 0.04), 0, 1.45, 0.47, { role: 'glass' }),
  ];
  component('analyzer', 'Process Analyzer', analyzerParts, []);
  const site = group('site', 'North River Water Resource Recovery Facility', null, { role: 'site' });
  const zones = new Map();
  zones.set('preliminary', group('preliminary', 'Influent and Preliminary Treatment', site, { area: 'preliminary' }));
  zones.set('primary', group('primary', 'Primary Treatment', site, { area: 'primary' }));
  zones.set('secondary', group('secondary', 'Biological and Secondary Treatment', site, { area: 'secondary' }));
  zones.set('tertiary', group('tertiary', 'Tertiary Treatment and Outfall', site, { area: 'tertiary' }));
  zones.set('sludge', group('sludge', 'Solids Handling and Energy Recovery', site, { area: 'solids' }));
  zones.set('utilities', group('utilities', 'Utilities and Administration', site, { area: 'utilities' }));

  siteBox('ground', 'Site Ground', site, 0, -0.28, 0, 1250, 0.55, 780, 'ground', { role: 'ground' });
  siteBox('process-yard', 'Process Yard', site, 0, -0.04, 0, 1080, 0.1, 390, 'concreteDark', { role: 'process-yard' });
  for (let index = 0; index < 7; index += 1) {
    siteBox(
      `road-h-${index + 1}`,
      `Service Road East-West ${index + 1}`,
      site,
      0,
      0.01,
      -330 + index * 110,
      1120,
      0.14,
      9,
      'road',
      { role: 'road' },
    );
  }
  for (let index = 0; index < 5; index += 1) {
    siteBox(
      `road-v-${index + 1}`,
      `Service Road North-South ${index + 1}`,
      site,
      -520 + index * 260,
      0.01,
      0,
      9,
      0.14,
      700,
      'road',
      { role: 'road' },
    );
  }

  const trainNodes = [];
  for (let train = 1; train <= 6; train += 1) {
    const z = -210 + (train - 1) * 82;
    const groupId = group(`train-${train}`, `Treatment Train ${train}`, zones.get('secondary'), { train: String(train) });
    const route = {};
    route.pump = instance('pump', `t${train}-pump`, `Train ${train} Influent Pump`, zones.get('preliminary'), -500, 0, z, 'steel', { train: String(train) });
    route.screen = instance('screen', `t${train}-screen`, `Train ${train} Bar Screen`, zones.get('preliminary'), -425, 0, z, 'concrete', { train: String(train) });
    route.grit = instance('grit', `t${train}-grit`, `Train ${train} Grit Chamber`, zones.get('preliminary'), -345, 0, z, 'concrete', { train: String(train) });
    route.eq = instance('equalization', `t${train}-eq`, `Train ${train} Equalization Tank`, zones.get('primary'), -245, 0, z, 'concrete', { train: String(train) });
    route.primary = instance('primaryClarifier', `t${train}-primary`, `Train ${train} Primary Clarifier`, zones.get('primary'), -125, 0, z, 'concrete', { train: String(train) });
    route.aeration = instance('aeration', `t${train}-aeration`, `Train ${train} Aeration Basin`, zones.get('secondary'), 30, 0, z, 'concrete', { train: String(train) });
    route.blower = instance('blower', `t${train}-blower`, `Train ${train} Aeration Blower`, zones.get('secondary'), 30, 0, z - 36, 'darkMetal', { train: String(train) });
    route.secondary = instance('secondaryClarifier', `t${train}-secondary`, `Train ${train} Secondary Clarifier`, zones.get('secondary'), 215, 0, z, 'concrete', { train: String(train) });
    route.ras = instance('pump', `t${train}-ras`, `Train ${train} RAS Pump`, zones.get('secondary'), 292, 0, z + 14, 'steel', { train: String(train) });
    route.was = instance('pump', `t${train}-was`, `Train ${train} WAS Pump`, zones.get('secondary'), 292, 0, z - 14, 'steel', { train: String(train) });
    route.filter = instance('filter', `t${train}-filter`, `Train ${train} Tertiary Filter`, zones.get('tertiary'), 390, 0, z, 'concrete', { train: String(train) });
    route.uv = instance('uv', `t${train}-uv`, `Train ${train} UV Channel`, zones.get('tertiary'), 500, 0, z, 'concrete', { train: String(train) });
    route.chemical = instance('chemical', `t${train}-chemical`, `Train ${train} Chemical Dosing`, zones.get('tertiary'), 500, 0, z + 24, 'chemical', { train: String(train) });
    route.analyzer = instance('analyzer', `t${train}-analyzer`, `Train ${train} Process Analyzer`, zones.get('secondary'), 350, 0, z + 22, 'darkMetal', { train: String(train) });
    route.thickener = instance('thickener', `t${train}-thickener`, `Train ${train} Sludge Thickener`, zones.get('sludge'), -180 + (train - 1) * 70, 0, 315, 'concrete', { train: String(train) });
    trainNodes.push(route);

    const extraEquipment = [
      ['pump', 'primary-sludge', `Train ${train} Primary Sludge Pump`, -175, z + 34, zones.get('primary'), 'steel'],
      ['pump', 'secondary-sludge', `Train ${train} Secondary Sludge Pump`, 275, z + 34, zones.get('secondary'), 'steel'],
      ['pump', 'recirculation-booster', `Train ${train} Recirculation Booster`, 330, z - 32, zones.get('secondary'), 'steel'],
      ['pump', 'filter-backwash', `Train ${train} Filter Backwash Pump`, 420, z + 32, zones.get('tertiary'), 'steel'],
      ['chemical', 'chemical-mixer', `Train ${train} Chemical Mixer`, 540, z - 32, zones.get('tertiary'), 'chemical'],
      ['analyzer', 'air-monitor', `Train ${train} Air Flow Monitor`, 45, z - 30, zones.get('secondary'), 'darkMetal'],
      ['analyzer', 'ras-monitor', `Train ${train} RAS Flow Monitor`, 255, z + 36, zones.get('secondary'), 'darkMetal'],
      ['analyzer', 'effluent-monitor', `Train ${train} Effluent Monitor`, 550, z + 34, zones.get('tertiary'), 'darkMetal'],
      ['pump', 'digester-feed', `Train ${train} Digester Feed Pump`, 95, 285 + (train - 1) * 10, zones.get('sludge'), 'steel'],
      ['pump', 'thickener-feed', `Train ${train} Thickener Feed Pump`, -220 + (train - 1) * 60, 300, zones.get('sludge'), 'steel'],
      ['blower', 'process-air-booster', `Train ${train} Process Air Booster`, 55, z + 34, zones.get('secondary'), 'darkMetal'],
      ['analyzer', 'sludge-density', `Train ${train} Sludge Density Meter`, -100 + (train - 1) * 65, 290, zones.get('sludge'), 'darkMetal'],
    ];
    for (const [definitionKey, keySuffix, name, x, extraZ, parentId, materialKey] of extraEquipment) {
      instance(
        definitionKey,
        `t${train}-${keySuffix}`,
        name,
        parentId,
        x,
        0,
        extraZ,
        materialKey,
        { train: String(train), role: 'ancillary-equipment' },
      );
    }
  }

  const outfallA = instance('pump', 'outfall-a', 'Outfall Pump A', zones.get('tertiary'), 605, 0, -35, 'steel', { role: 'outfall-pump' });
  const outfallB = instance('pump', 'outfall-b', 'Outfall Pump B', zones.get('tertiary'), 605, 0, 35, 'steel', { role: 'outfall-pump' });
  const digesters = [
    instance('digester', 'digester-1', 'Anaerobic Digester 1', zones.get('sludge'), 315, 0, 285, 'concrete', { role: 'digester' }),
    instance('digester', 'digester-2', 'Anaerobic Digester 2', zones.get('sludge'), 315, 0, 365, 'concrete', { role: 'digester' }),
    instance('digester', 'digester-3', 'Anaerobic Digester 3', zones.get('sludge'), 440, 0, 325, 'concrete', { role: 'digester' }),
  ];
  const dewatering = [
    instance('dewatering', 'dewatering-a', 'Dewatering Building A', zones.get('sludge'), 565, 0, 285, 'concrete', { role: 'dewatering' }),
    instance('dewatering', 'dewatering-b', 'Dewatering Building B', zones.get('sludge'), 565, 0, 365, 'concrete', { role: 'dewatering' }),
    instance('dewatering', 'dewatering-c', 'Dewatering Package C', zones.get('sludge'), 565, 0, 225, 'concrete', { role: 'dewatering' }),
    instance('dewatering', 'dewatering-d', 'Dewatering Package D', zones.get('sludge'), 565, 0, 165, 'concrete', { role: 'dewatering' }),
    instance('dewatering', 'dewatering-e', 'Dewatering Package E', zones.get('sludge'), 565, 0, 105, 'concrete', { role: 'dewatering' }),
    instance('dewatering', 'dewatering-f', 'Dewatering Package F', zones.get('sludge'), 565, 0, 45, 'concrete', { role: 'dewatering' }),
  ];

  const facilities = [
    ['facility-admin', 'Administration', -500, 320, 'building'],
    ['facility-ops', 'Operations Center', -380, 320, 'building'],
    ['facility-lab', 'Laboratory', -260, 320, 'building'],
    ['facility-maintenance', 'Maintenance Workshop', -140, 320, 'building'],
    ['facility-electrical', 'Electrical Building', -20, 320, 'building'],
    ['facility-blower-house', 'Blower Building', 100, 320, 'building'],
    ['facility-chemical-storage', 'Chemical Storage', 220, 320, 'building'],
    ['facility-residuals', 'Residuals Storage', 340, 320, 'building'],
    ['facility-gatehouse', 'Gatehouse', -560, 230, 'building'],
    ['facility-training', 'Training Center', 450, 230, 'building'],
  ];
  for (const [key, name, x, z, definition] of facilities) {
    instance(definition, key, name, zones.get('utilities'), x, 0, z, 'concrete', { role: 'facility' });
  }

  for (let index = 0; index < 10; index += 1) {
    instance(
      'pump',
      `utility-pump-${index + 1}`,
      `Utility Transfer Pump ${index + 1}`,
      zones.get('utilities'),
      -470 + index * 24,
      0,
      255,
      'steel',
      { role: 'utility-pump' },
    );
  }
  for (let index = 0; index < 5; index += 1) {
    instance(
      'chemical',
      `chemical-bulk-${index + 1}`,
      `Bulk Chemical Tank ${index + 1}`,
      zones.get('utilities'),
      -300 + index * 18,
      0,
      255,
      'chemical',
      { role: 'chemical-storage' },
    );
  }
  for (let index = 0; index < 4; index += 1) {
    instance(
      'building',
      `odor-control-${index + 1}`,
      `Odor Control Unit ${index + 1}`,
      zones.get('utilities'),
      100 + index * 26,
      0,
      235,
      'darkMetal',
      { role: 'odor-control' },
    );
  }
  for (let index = 0; index < 3; index += 1) {
    instance(
      'digester',
      `gas-holder-${index + 1}`,
      `Gas Holder ${index + 1}`,
      zones.get('sludge'),
      470 + index * 28,
      0,
      230,
      'steel',
      { role: 'gas-holder' },
    );
  }
  for (let index = 0; index < 3; index += 1) {
    instance(
      'filter',
      `heat-exchanger-${index + 1}`,
      `Digester Heat Exchanger ${index + 1}`,
      zones.get('sludge'),
      245,
      0,
      235 + index * 18,
      'steel',
      { role: 'heat-exchanger' },
    );
  }

  for (let index = 0; index < 6; index += 1) {
    const route = trainNodes[index];
    const suffix = `Train ${index + 1}`;
    const outfall = index % 2 === 0 ? 'outfall-a' : 'outfall-b';
    connect(`t${index + 1}-pump`, 'outlet', `t${index + 1}-screen`, 'inlet', `${suffix} Influent to Screen`, 'liquid', 1.5, { train: String(index + 1), sequence: '1' });
    connect(`t${index + 1}-screen`, 'outlet', `t${index + 1}-grit`, 'inlet', `${suffix} Screen to Grit`, 'liquid', 1.5, { train: String(index + 1), sequence: '2' });
    connect(`t${index + 1}-grit`, 'outlet', `t${index + 1}-eq`, 'inlet', `${suffix} Grit to Equalization`, 'liquid', 1.4, { train: String(index + 1), sequence: '3' });
    connect(`t${index + 1}-eq`, 'outlet', `t${index + 1}-primary`, 'inlet', `${suffix} Equalization to Primary`, 'liquid', 1.4, { train: String(index + 1), sequence: '4' });
    connect(`t${index + 1}-primary`, 'outlet', `t${index + 1}-aeration`, 'inlet', `${suffix} Primary to Aeration`, 'liquid', 1.5, { train: String(index + 1), sequence: '5' });
    connect(`t${index + 1}-aeration`, 'outlet', `t${index + 1}-secondary`, 'inlet', `${suffix} Aeration to Secondary`, 'liquid', 1.5, { train: String(index + 1), sequence: '6' });
    connect(`t${index + 1}-secondary`, 'outlet', `t${index + 1}-filter`, 'inlet', `${suffix} Secondary to Filter`, 'liquid', 1.3, { train: String(index + 1), sequence: '7' });
    connect(`t${index + 1}-filter`, 'outlet', `t${index + 1}-uv`, 'inlet', `${suffix} Filter to UV`, 'liquid', 1.2, { train: String(index + 1), sequence: '8' });
    connect(`t${index + 1}-uv`, 'outlet', outfall, 'inlet', `${suffix} UV to Outfall`, 'liquid', 1.1, { train: String(index + 1), sequence: '9' });
    connect(`t${index + 1}-blower`, 'air', `t${index + 1}-aeration`, 'air', `${suffix} Process Air`, 'air', 0.8, { train: String(index + 1), service: 'process-air' });
    connect(`t${index + 1}-secondary`, 'return', `t${index + 1}-ras`, 'inlet', `${suffix} Return Activated Sludge`, 'liquid', 1.0, { train: String(index + 1), service: 'ras' });
    connect(`t${index + 1}-ras`, 'outlet', `t${index + 1}-aeration`, 'recycle', `${suffix} RAS to Aeration`, 'liquid', 1.0, { train: String(index + 1), service: 'ras' });
    connect(`t${index + 1}-secondary`, 'waste', `t${index + 1}-was`, 'inlet', `${suffix} Waste Activated Sludge`, 'liquid', 0.7, { train: String(index + 1), service: 'was' });
    connect(`t${index + 1}-was`, 'outlet', `t${index + 1}-thickener`, 'inlet', `${suffix} WAS to Thickener`, 'liquid', 0.7, { train: String(index + 1), service: 'was' });
    connect(`t${index + 1}-thickener`, 'outlet', ['digester-1', 'digester-2', 'digester-3'][index % 3], 'inlet', `${suffix} Thickener to Digester`, 'liquid', 0.8, { train: String(index + 1), service: 'sludge' });
    connect(`t${index + 1}-chemical`, 'outlet', `t${index + 1}-aeration`, 'chemical', `${suffix} Chemical Feed`, 'liquid', 0.45, { train: String(index + 1), service: 'chemical' });
    void route;
  }
  connect('digester-1', 'outlet', 'dewatering-a', 'inlet', 'Digester 1 to Dewatering A', 'liquid', 0.8, { service: 'sludge' });
  connect('digester-2', 'outlet', 'dewatering-a', 'inlet', 'Digester 2 to Dewatering A', 'liquid', 0.8, { service: 'sludge' });
  connect('digester-3', 'outlet', 'dewatering-b', 'inlet', 'Digester 3 to Dewatering B', 'liquid', 0.8, { service: 'sludge' });

  const componentOperations = [...components.values()].map((definition) => ({
    type: 'component.define',
    definition,
  }));
  const instanceCount = operations.filter((operation) => operation.type === 'instance.create').length;
  const connectionCount = operations.filter((operation) => operation.type === 'connection.create').length;
  return {
    projectId: 'large-industrial-site',
    design: {
      processTrains: 6,
      sourceTheme: 'large municipal water resource recovery facility',
      componentDefinitions: components.size,
      plannedInstances: instanceCount,
      plannedRelations: nextRelation - 1,
      plannedConnections: connectionCount,
    },
    operations: [
      ...operations.filter((operation) => operation.type === 'material.upsert'),
      ...componentOperations,
      ...operations.filter((operation) => operation.type !== 'material.upsert'),
    ],
  };
}

export function buildDetailedIndustrialScenePlan() {
  resetDesignState();
  const plan = buildDesign();
  const operations = plan.operations;
  const materials = operations.filter(({ type }) => type === 'material.upsert');
  const definitions = operations.filter(({ type }) => type === 'component.define');
  const scene = operations.filter(({ type }) => (
    type === 'node.create' || type === 'instance.create'
  ));
  const relationships = operations.filter(({ type }) => (
    type === 'relation.create'
    || type === 'connection.create'
    || type === 'material.assign'
  ));
  const rawTransactions = [
    transaction('materials', materials),
    ...chunk(definitions, 4).map((operationsChunk, index) => (
      transaction(`definitions-${index + 1}`, operationsChunk)
    )),
    transaction('scene', scene),
    transaction('relationships', relationships),
  ];
  const transactions = rawTransactions.map((value, expectedRevision) => ({
    ...value,
    expectedRevision,
  }));
  const instances = scene.filter(({ type }) => type === 'instance.create');
  const connections = relationships.filter(({ type }) => type === 'connection.create');
  return {
    projectId: plan.projectId,
    processTrains: [1, 2, 3, 4, 5, 6],
    componentDefinitions: [...components.values()],
    transactions,
    expected: {
      primaryObjects: instances.length,
      facilityObjects: instances.filter(({ metadata }) => metadata?.role === 'facility').length,
      componentDefinitions: components.size,
      semanticParts: [...components.values()].reduce(
        (sum, definition) => sum + definition.parts.length,
        0,
      ),
      materialAssignments: relationships.filter(({ type }) => type === 'material.assign').length,
      minTriangles: 800_000,
      maxTriangles: 1_000_000,
      topology: {
        processTrains: plan.design.processTrains,
        danglingPorts: 0,
        invalidMedia: 0,
        connectionCount: connections.length,
      },
    },
  };
}

function resetDesignState() {
  materials.clear();
  components.clear();
  componentPartRoles.clear();
  partMaterialKeys = new WeakMap();
  nodes.clear();
  operations.length = 0;
  nextMaterial = 1;
  nextComponent = 1;
  nextNode = 1;
  nextRelation = 1;
}

function transaction(kind, values) {
  return {
    idempotencyKey: `large-industrial-${kind}`,
    operations: values,
  };
}

function chunk(values, size) {
  const chunks = [];
  for (let offset = 0; offset < values.length; offset += size) {
    chunks.push(values.slice(offset, offset + size));
  }
  return chunks;
}
