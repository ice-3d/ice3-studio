export const WWTP_STATS_LIMITS = Object.freeze({
  minTriangles: 300000,
  maxTriangles: 500000,
  maxUniqueTriangles: 150000,
  maxExpandedNodes: 15000,
  maxInstances: 500,
});

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
          message: `${endpoint.nodeId}/${endpoint.port}`,
        });
        continue;
      }
      if (port.medium !== operation.connection.medium) {
        issues.push({
          code: 'MEDIUM_MISMATCH',
          message: `${endpoint.nodeId}/${endpoint.port}`,
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
