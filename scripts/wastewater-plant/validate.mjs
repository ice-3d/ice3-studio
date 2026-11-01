export const WWTP_STATS_LIMITS = Object.freeze({
  minTriangles: 300000,
  maxTriangles: 500000,
  maxUniqueTriangles: 150000,
  maxExpandedNodes: 15000,
  maxInstances: 500,
});

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
