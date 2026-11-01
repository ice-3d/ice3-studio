export const WWTP_STATS_LIMITS = Object.freeze({
  minTriangles: 300000,
  maxTriangles: 500000,
  maxUniqueTriangles: 150000,
  maxExpandedNodes: 15000,
  maxInstances: 500,
});

const EXPRESSION_SCOPE = Object.freeze({
  abs: Math.abs,
  ceil: Math.ceil,
  cos: Math.cos,
  floor: Math.floor,
  max: Math.max,
  min: Math.min,
  pow: Math.pow,
  round: Math.round,
  sin: Math.sin,
  sqrt: Math.sqrt,
  tau: Math.PI * 2,
});

function resolveNumber(value, scope) {
  if (typeof value === 'number') return value;
  if (typeof value?.value === 'number') return value.value;
  if (typeof value?.expr !== 'string') return 0;
  const values = { ...EXPRESSION_SCOPE, ...scope };
  const names = Object.keys(values);
  return Function(...names, `"use strict"; return (${value.expr});`)(
    ...names.map((name) => values[name]),
  );
}

function rotateByQuaternion([x, y, z], [qx, qy, qz, qw]) {
  const ix = qw * x + qy * z - qz * y;
  const iy = qw * y + qz * x - qx * z;
  const iz = qw * z + qx * y - qy * x;
  const iw = -qx * x - qy * y - qz * z;
  return [
    ix * qw + iw * -qx + iy * -qz - iz * -qy,
    iy * qw + iw * -qy + iz * -qx - ix * -qz,
    iz * qw + iw * -qz + ix * -qy - iy * -qx,
  ];
}

function multiplyQuaternions([ax, ay, az, aw], [bx, by, bz, bw]) {
  return [
    aw * bx + ax * bw + ay * bz - az * by,
    aw * by - ax * bz + ay * bw + az * bx,
    aw * bz + ax * by - ay * bx + az * bw,
    aw * bw - ax * bx - ay * by - az * bz,
  ];
}

function boundsFromPoints(points) {
  if (points.length === 0) return null;
  return {
    minX: Math.min(...points.map(([x]) => x)),
    maxX: Math.max(...points.map(([x]) => x)),
    minY: Math.min(...points.map(([, y]) => y)),
    maxY: Math.max(...points.map(([, , y]) => y)),
    minZ: Math.min(...points.map(([, , z]) => z)),
    maxZ: Math.max(...points.map(([, , z]) => z)),
  };
}

function primitivePoints(content, scope) {
  const points = [];
  if (content.kind === 'box') {
    const width = resolveNumber(content.width, scope);
    const height = resolveNumber(content.height, scope);
    const depth = resolveNumber(content.depth, scope);
    for (const x of [-width / 2, width / 2]) {
      for (const y of [-height / 2, height / 2]) {
        for (const z of [-depth / 2, depth / 2]) points.push([x, y, z]);
      }
    }
  } else if (content.kind === 'cylinder') {
    const radius = resolveNumber(content.radius, scope);
    const height = resolveNumber(content.height, scope);
    for (const x of [-radius, radius]) {
      for (const y of [-height / 2, height / 2]) {
        for (const z of [-radius, radius]) points.push([x, y, z]);
      }
    }
  } else if (content.kind === 'sphere') {
    const radius = resolveNumber(content.radius, scope);
    for (const x of [-radius, radius]) {
      for (const y of [-radius, radius]) {
        for (const z of [-radius, radius]) points.push([x, y, z]);
      }
    }
  } else if (Array.isArray(content.positions ?? content.mesh?.positions)) {
    points.push(...(content.positions ?? content.mesh.positions));
  }
  return points;
}

function transformedPrimitiveBounds(content, transform, scope) {
  const points = primitivePoints(content, scope);
  if (points.length === 0) return null;
  const rotation = transform?.rotation ?? [0, 0, 0, 1];
  const translation = transform?.translation ?? { x: 0, y: 0, z: 0 };
  const center = [
    resolveNumber(translation.x, scope),
    resolveNumber(translation.y, scope),
    resolveNumber(translation.z, scope),
  ];
  return boundsFromPoints(points.map((point) => {
    const rotated = rotateByQuaternion(point, rotation);
    return [
      center[0] + rotated[0],
      center[1] + rotated[1],
      center[2] + rotated[2],
    ];
  }));
}

function componentInstanceBounds(operation, definition) {
  const parameters = Object.fromEntries(
    definition.parameters.map(({ name, default: value }) => [name, value]),
  );
  Object.assign(parameters, operation.instance.parameters);
  const instanceRotation = operation.transform.rotation ?? [0, 0, 0, 1];
  const instanceTranslation = operation.transform.translation;
  const points = [];
  for (const part of definition.parts) {
    if (!part.content) continue;
    const count = part.repeat ?? 1;
    for (let index = 0; index < count; index += 1) {
      const scope = { ...parameters, count, i: index };
      const localPoints = primitivePoints(part.content, scope);
      const localTransform = part.transform ?? {};
      const yaw = localTransform.yaw === undefined
        ? 0
        : resolveNumber(localTransform.yaw, scope) * Math.PI / 180;
      const localRotation = localTransform.rotation
        ?? (yaw === 0
          ? [0, 0, 0, 1]
          : [0, Math.sin(yaw / 2), 0, Math.cos(yaw / 2)]);
      const localTranslation = localTransform.translation ?? { x: 0, y: 0, z: 0 };
      const translated = [
        resolveNumber(localTranslation.x, scope),
        resolveNumber(localTranslation.y, scope),
        resolveNumber(localTranslation.z, scope),
      ];
      const rotatedTranslation = rotateByQuaternion(translated, instanceRotation);
      for (const point of localPoints) {
        const rotated = rotateByQuaternion(
          point,
          multiplyQuaternions(instanceRotation, localRotation),
        );
        points.push([
          instanceTranslation.x + rotatedTranslation[0] + rotated[0],
          instanceTranslation.y + rotatedTranslation[1] + rotated[1],
          instanceTranslation.z + rotatedTranslation[2] + rotated[2],
        ]);
      }
    }
  }
  return boundsFromPoints(points);
}

function intersectsRoad(bounds, road, clearance) {
  return bounds.maxX > road.minX - clearance
    && bounds.minX < road.maxX + clearance
    && bounds.maxZ > road.minZ - clearance
    && bounds.minZ < road.maxZ + clearance;
}

export function validateSiteRoadClearance(plan, clearance = 0.5) {
  const issues = [];
  const roads = plan.operations
    .filter(({ type, metadata }) => type === 'node.create' && metadata?.roadClass)
    .map((operation) => ({
      name: operation.name,
      minX: operation.transform.translation.x - operation.content.width / 2,
      maxX: operation.transform.translation.x + operation.content.width / 2,
      minZ: operation.transform.translation.z - operation.content.depth / 2,
      maxZ: operation.transform.translation.z + operation.content.depth / 2,
    }));
  const definitions = new Map(
    plan.operations
      .filter(({ type }) => type === 'component.define')
      .map(({ definition }) => [definition.id, definition]),
  );
  for (const operation of plan.operations) {
    if (operation.type === 'instance.create') {
      const definition = definitions.get(operation.instance.definitionId);
      const bounds = definition && componentInstanceBounds(operation, definition);
      for (const road of roads) {
        if (bounds && intersectsRoad(bounds, road, clearance)) {
          issues.push({
            code: 'ROAD_OBSTRUCTION',
            message: `${operation.name} overlaps ${road.name}`,
          });
        }
      }
      continue;
    }
    if (operation.type !== 'node.create') continue;
    if (operation.metadata?.role === 'site') continue;
    if (operation.content?.type !== 'primitive') continue;
    const bounds = transformedPrimitiveBounds(
      operation.content,
      operation.transform,
      {},
    );
    for (const road of roads) {
      if (bounds && intersectsRoad(bounds, road, clearance)) {
        issues.push({
          code: 'ROAD_OBSTRUCTION',
          message: `${operation.name} overlaps ${road.name}`,
        });
      }
    }
  }
  return { ok: issues.length === 0, issues };
}

function distanceToHorizontalRoute(point, route) {
  let distance = Infinity;
  for (let index = 1; index < route.length; index += 1) {
    const from = route[index - 1];
    const to = route[index];
    const dx = to.x - from.x;
    const dz = to.z - from.z;
    const denominator = dx * dx + dz * dz;
    if (denominator <= Number.EPSILON) continue;
    const ratio = Math.max(0, Math.min(1, (
      (point.x - from.x) * dx + (point.z - from.z) * dz
    ) / denominator));
    distance = Math.min(distance, Math.hypot(
      point.x - (from.x + ratio * dx),
      point.z - (from.z + ratio * dz),
    ));
  }
  return distance;
}

function coarsePipeRoute(connection, from, to) {
  const policy = connection.routePolicy;
  const elevated = policy.kind === 'buried'
    ? -policy.depth
    : policy.kind === 'rack'
      ? policy.elevation
      : policy.kind === 'orthogonal'
        ? Math.max(from.position.y, to.position.y) + 0.5
        : null;
  let startStub = {
    x: from.position.x + from.direction.x * 0.9,
    y: from.position.y + from.direction.y * 0.9,
    z: from.position.z + from.direction.z * 0.9,
  };
  let endStub = {
    x: to.position.x - to.direction.x * 0.9,
    y: to.position.y - to.direction.y * 0.9,
    z: to.position.z - to.direction.z * 0.9,
  };
  if (elevated !== null) {
    if ((startStub.y - from.position.y) * (elevated - from.position.y) < 0) {
      startStub = from.position;
    }
    if ((endStub.y - to.position.y) * (elevated - to.position.y) < 0) {
      endStub = to.position;
    }
  }
  return elevated === null
    ? [from.position, to.position]
    : [
      from.position,
      startStub,
      { x: startStub.x, y: elevated, z: startStub.z },
      { x: endStub.x, y: elevated, z: startStub.z },
      { x: endStub.x, y: elevated, z: endStub.z },
      { x: endStub.x, y: endStub.y, z: endStub.z },
      to.position,
    ];
}

export function validatePipeAccessoryAlignment(plan, tolerance = 1) {
  const issues = [];
  const definitions = new Map(
    plan.operations
      .filter(({ type }) => type === 'component.define')
      .map(({ definition }) => [definition.id, definition]),
  );
  const instances = new Map(
    plan.operations
      .filter(({ type }) => type === 'instance.create')
      .map((operation) => [operation.node_id, operation]),
  );
  const accessories = new Map();
  for (const operation of plan.operations) {
    if (operation.type !== 'node.create') continue;
    if (operation.metadata?.role !== 'pipe-support' || operation.metadata.supportPart !== 'beam') {
      continue;
    }
    const lineId = operation.metadata.lineId;
    accessories.set(lineId, operation);
  }
  const resolvePort = (reference) => {
    const node = instances.get(reference.nodeId);
    const definition = node && definitions.get(node.instance.definitionId);
    const port = definition?.ports.find(({ name }) => name === reference.port);
    if (!node || !port) return null;
    const rotation = node.transform.rotation;
    const position = rotateByQuaternion(
      [port.position.x, port.position.y, port.position.z],
      rotation,
    );
    const direction = rotateByQuaternion(
      [port.direction.x, port.direction.y, port.direction.z],
      rotation,
    );
    return {
      position: {
        x: node.transform.translation.x + position[0],
        y: node.transform.translation.y + position[1],
        z: node.transform.translation.z + position[2],
      },
      direction: { x: direction[0], y: direction[1], z: direction[2] },
    };
  };
  for (const operation of plan.operations) {
    if (operation.type !== 'connection.create') continue;
    const from = resolvePort(operation.connection.from);
    const to = resolvePort(operation.connection.to);
    const accessory = accessories.get(operation.connection.metadata.lineId);
    if (!from || !to || !accessory) continue;
    const route = coarsePipeRoute(operation.connection, from, to);
    const distance = distanceToHorizontalRoute(
      accessory.transform.translation,
      route,
    );
    if (!Number.isFinite(distance) || distance > tolerance) {
      issues.push({
        code: 'PIPE_ACCESSORY_OFF_ROUTE',
        message: `${accessory.name} is ${distance.toFixed(2)} m off ${operation.name}`,
      });
    }
  }
  return { ok: issues.length === 0, issues };
}

export function validateProcessGraph(plan, requirements) {
  const tagByNode = new Map();
  for (const operation of plan.operations) {
    if (operation.type !== 'instance.create') continue;
    const tag = operation.metadata?.tag;
    if (typeof tag === 'string') tagByNode.set(operation.node_id, tag);
  }
  const edges = plan.operations
    .filter(({ type }) => type === 'connection.create')
    .map(({ connection, name }) => ({
      from: connection.from.nodeId,
      to: connection.to.nodeId,
      medium: connection.medium,
      name,
    }));
  const outgoing = new Map();
  for (const edge of edges) {
    const group = outgoing.get(edge.from) ?? [];
    group.push(edge);
    outgoing.set(edge.from, group);
  }

  const issues = [];
  for (const requirement of requirements) {
    const starts = [...tagByNode]
      .filter(([, tag]) => requirement.fromTags.includes(tag))
      .map(([nodeId]) => nodeId);
    const reached = new Set(starts);
    const queue = [...starts];
    while (queue.length > 0) {
      const current = queue.shift();
      for (const edge of outgoing.get(current) ?? []) {
        if (!requirement.media.includes(edge.medium)) continue;
        if (reached.has(edge.to)) continue;
        reached.add(edge.to);
        queue.push(edge.to);
      }
    }
    const targets = [...tagByNode]
      .filter(([nodeId, tag]) => (
        requirement.toTags.includes(tag) && reached.has(nodeId)
      ))
      .map(([, tag]) => tag);
    const missing = requirement.toTags.filter((tag) => !targets.includes(tag));
    if (missing.length > 0) {
      issues.push({
        code: requirement.code,
        message: `${requirement.fromTags.join(',')} cannot reach ` +
          `${missing.join(',')} via ${requirement.media.join(',')}`,
      });
    }
  }
  return { ok: issues.length === 0, issues };
}

export function validateConnectionPorts(plan) {
  const issues = [];
  const usage = new Map();
  const definitions = new Map(
    plan.operations
      .filter(({ type }) => type === 'component.define')
      .map(({ definition }) => [definition.id, definition]),
  );
  const definitionByNode = new Map(
    plan.operations
      .filter(({ type }) => type === 'instance.create')
      .map((operation) => [
        operation.node_id,
        operation.instance.definitionId,
      ]),
  );
  for (const operation of plan.operations) {
    if (operation.type !== 'connection.create') continue;
    for (const endpoint of [operation.connection.from, operation.connection.to]) {
      const key = `${endpoint.nodeId}/${endpoint.port}`;
      usage.set(key, (usage.get(key) ?? 0) + 1);
    }
  }
  const headerPorts = new Set(
    [...definitions.values()]
      .flatMap((definition) => (
        definition.ports.map(({ name, metadata }) => (
          metadata?.allowMultiple
            ? `${definition.id}/${name}`
            : null
        ))
      ))
      .filter(Boolean),
  );
  for (const [key, count] of usage) {
    const separator = key.indexOf('/');
    const nodeId = key.slice(0, separator);
    const portName = key.slice(separator + 1);
    const definitionId = definitionByNode.get(nodeId);
    const portKey = `${definitionId}/${portName}`;
    if (count > 1 && !headerPorts.has(portKey)) {
      issues.push({
        code: 'PORT_REUSED',
        message: `${key} is used by ${count} connections`,
      });
    }
  }
  for (const operation of plan.operations) {
    if (operation.type !== 'connection.create') continue;
    const connection = operation.connection;
    for (const endpoint of [connection.from, connection.to]) {
      const definitionId = definitionByNode.get(endpoint.nodeId);
      const port = definitions.get(definitionId)?.ports
        .find(({ name }) => name === endpoint.port);
      if (!port) continue;
      const ratio = connection.diameter / port.diameter;
      if (ratio < 0.8 || ratio > 1.2) {
        issues.push({
          code: 'PIPE_DIAMETER_MISMATCH',
          message: `${operation.name}: ${endpoint.nodeId}/${endpoint.port} ` +
            `diameter ${connection.diameter} ` +
            `does not match port diameter ${port.diameter}`,
        });
      }
    }
  }
  return { ok: issues.length === 0, issues };
}

export function validateWastewaterPlan(plan) {
  const issues = [];
  const definitions = new Map();
  const instances = new Map();
  const nodeIds = new Set();
  const relationIds = new Set();

  for (const operation of plan.operations) {
    if (operation.type === 'component.define') {
      definitions.set(operation.definition.id, operation.definition);
    }
    if (operation.type === 'instance.create') {
      instances.set(operation.node_id, operation);
    }
    if (operation.type === 'relation.create') {
      relationIds.add(operation.relation.id);
    }
    const id = operation.node_id ?? operation.definition?.id ??
      operation.relation?.id ?? operation.material_id;
    if (id && nodeIds.has(id)) {
      issues.push({ code: 'DUPLICATE_ID', message: `duplicate id ${id}` });
    }
    if (id) nodeIds.add(id);
  }

  for (const operation of plan.operations) {
    if (operation.type !== 'connection.create') continue;
    for (const endpoint of [operation.connection.from, operation.connection.to]) {
      const instance = instances.get(endpoint.nodeId);
      const definition = instance && definitions.get(instance.instance.definitionId);
      const port = definition?.ports.find(({ name }) => name === endpoint.port);
      if (!port) {
        issues.push({
          code: 'UNKNOWN_PORT',
          message: `${operation.name}: ${instance?.name ?? endpoint.nodeId}/${endpoint.port}`,
        });
        continue;
      }
      if (port.medium !== operation.connection.medium) {
        issues.push({
          code: 'MEDIUM_MISMATCH',
          message: `${operation.name}: ${instance?.name ?? endpoint.nodeId}/${endpoint.port} ` +
            `expects ${operation.connection.medium}, found ${port.medium}`,
        });
      }
    }
    if (!relationIds.has(operation.connection.relationId)) {
      issues.push({
        code: 'MISSING_RELATION',
        message: operation.connection.relationId,
      });
    }
  }

  const processInstances = [...instances.values()].filter((operation) => (
    !['auxiliary', 'landscape'].includes(operation.metadata?.stage) &&
    Number.isFinite(Number(operation.metadata?.clearanceRadius)) &&
    Number(operation.metadata.clearanceRadius) > 0
  ));
  for (let leftIndex = 0; leftIndex < processInstances.length; leftIndex += 1) {
    const left = processInstances[leftIndex];
    for (let rightIndex = leftIndex + 1; rightIndex < processInstances.length; rightIndex += 1) {
      const right = processInstances[rightIndex];
      const dx = left.transform.translation.x - right.transform.translation.x;
      const dz = left.transform.translation.z - right.transform.translation.z;
      const minimum = Number(left.metadata.clearanceRadius) +
        Number(right.metadata.clearanceRadius);
      if (Math.hypot(dx, dz) < minimum) {
        issues.push({
          code: 'CLEARANCE_VIOLATION',
          message: `${left.name} overlaps ${right.name}`,
        });
      }
    }
  }

  const lineIds = new Set(
    plan.operations
      .filter(({ type }) => type === 'connection.create')
      .map(({ connection }) => connection.metadata?.lineId)
      .filter(Boolean),
  );
  for (const operation of plan.operations) {
    if (operation.type !== 'node.create') continue;
    if (!String(operation.metadata?.role ?? '').startsWith('pipe-')) continue;
    if (!lineIds.has(operation.metadata?.lineId)) {
      issues.push({
        code: 'UNKNOWN_LINE',
        message: String(operation.metadata?.lineId ?? 'missing'),
      });
    }
  }

  issues.push(...validateSiteRoadClearance(plan).issues);
  issues.push(...validatePipeAccessoryAlignment(plan).issues);

  return { ok: issues.length === 0, issues };
}

export function validateProjectStats(stats, limits) {
  const issues = [];
  const check = (condition, code, message) => {
    if (!condition) issues.push({ code, message });
  };
  check(
    Number.isInteger(stats.effectiveTriangles) &&
      stats.effectiveTriangles >= limits.minTriangles,
    'TRIANGLE_BUDGET_UNDERFLOW',
    `effective triangles ${stats.effectiveTriangles} below ${limits.minTriangles}`,
  );
  check(
    Number.isInteger(stats.effectiveTriangles) &&
      stats.effectiveTriangles <= limits.maxTriangles,
    'TRIANGLE_BUDGET_EXCEEDED',
    `effective triangles ${stats.effectiveTriangles} above ${limits.maxTriangles}`,
  );
  check(
    Number.isInteger(stats.uniqueTriangles) &&
      stats.uniqueTriangles <= limits.maxUniqueTriangles,
    'UNIQUE_TRIANGLE_BUDGET_EXCEEDED',
    `unique triangles ${stats.uniqueTriangles} above ${limits.maxUniqueTriangles}`,
  );
  check(
    Number.isInteger(stats.expandedNodes) &&
      stats.expandedNodes <= limits.maxExpandedNodes,
    'EXPANDED_NODE_BUDGET_EXCEEDED',
    `expanded nodes ${stats.expandedNodes} above ${limits.maxExpandedNodes}`,
  );
  check(
    Number.isInteger(stats.instances) && stats.instances <= limits.maxInstances,
    'INSTANCE_BUDGET_EXCEEDED',
    `instances ${stats.instances} above ${limits.maxInstances}`,
  );
  return { ok: issues.length === 0, issues };
}

export function assertProjectStats(stats, limits) {
  const result = validateProjectStats(stats, limits);
  if (!result.ok) {
    throw new Error(result.issues.map(({ code, message }) => `${code}: ${message}`).join('; '));
  }
  return result;
}
