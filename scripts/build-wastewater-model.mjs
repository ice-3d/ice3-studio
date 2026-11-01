// 用 Studio 自身能力重建污水处理厂示例：几何、端口、管道全部由内核参数化生成，
// 不引用任何外部资产。本文件属于案例层，工艺词汇只在这里出现。
//
// 构件依据真实构筑物组织：圆形池带中心进水筒、周边出水堰、刮泥桥与防护栏杆；
// 矩形池带导流墙、曝气支管、检修走道。端口声明介质与管径，管道由连接意图路由。

import { spawn } from 'node:child_process';
import { mkdir, mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';

const TAU = Math.PI * 2;
// 工程格式里字面量就是裸值，只有引用组件参数时才写 { param: 'name' }。
const lit = (value) => value;
const vec = (x, y, z) => ({ x, y, z });

function partTransform(x = 0, y = 0, z = 0, options = {}) {
  // 只写出与默认值不同的分量：默认平移为零、旋转为单位四元数、缩放为一。
  const value = {};
  if (x !== 0 || y !== 0 || z !== 0) value.translation = vec(x, y, z);
  if (options.rotation) value.rotation = options.rotation;
  return value;
}

function nodeTransform(x = 0, y = 0, z = 0) {
  return {
    translation: vec(x, y, z),
    rotation: [0, 0, 0, 1],
    scale: vec(1, 1, 1),
  };
}

const rotateY = (angle) => [0, Math.sin(angle / 2), 0, Math.cos(angle / 2)];
const box = (w, h, d) => ({ kind: 'box', width: w, height: h, depth: d });
const cylinder = (r, h, segments = 40) => ({
  kind: 'cylinder',
  radius: r,
  height: h,
  segments,
});

// 构件登记表：几何与材质角色分离，材质在实例上覆盖。
function createBuilder() {
  const state = { ops: [], materials: new Map(), next: 1, components: new Map(), nodes: new Map() };
  const id = (prefix) => `${prefix}-${String(state.next++).padStart(6, '0')}`;

  const material = (key, name, color, edgeColor = null) => {
    const materialId = id('material');
    state.materials.set(key, materialId);
    state.ops.push({
      type: 'material.upsert',
      material_id: materialId,
      name,
      base_color: color,
      opacity: color[3],
      edge_color: edgeColor,
    });
    return materialId;
  };

  const part = (partId, content, x, y, z, role, options = {}) => ({
    id: partId,
    parentId: options.parentId,
    transform: partTransform(x, y, z, options),
    content,
    metadata: options.metadata,
    role,
  });

  const port = (name, partId, x, y, z, direction, medium, diameter) => ({
    name,
    partId,
    position: vec(x, y, z),
    direction,
    medium,
    diameter,
  });

  const define = (key, name, parts, ports) => {
    const definitionId = id('component');
    const overrides = {};
    for (const item of parts) {
      if (item.role) {
        overrides[item.id] = {
          materialId: state.materials.get(item.role),
          parameters: {},
        };
      }
    }
    const definition = {
      id: definitionId,
      name,
      parameters: [],
      parts: parts.map(({ role, ...rest }) => rest),
      ports,
      metadata: { category: 'process-structure' },
    };
    state.ops.push({ type: 'component.define', definition });
    state.components.set(key, { definition, overrides });
    return definitionId;
  };

  const instance = (definitionKey, key, name, x, y, z, metadata = {}) => {
    const entry = state.components.get(definitionKey);
    if (!entry) throw new Error(`unknown component ${definitionKey}`);
    const nodeId = id('node');
    state.nodes.set(key, nodeId);
    state.ops.push({
      type: 'instance.create',
      node_id: nodeId,
      parent_id: null,
      name,
      transform: nodeTransform(x, y, z),
      instance: {
        definitionId: entry.definition.id,
        parameters: {},
        overrides: entry.overrides,
        metadata,
      },
      material_id: null,
      metadata,
    });
    return nodeId;
  };

  const connect = (fromKey, fromPort, toKey, toPort, name, medium, diameter) => {
    const from = state.nodes.get(fromKey);
    const to = state.nodes.get(toKey);
    if (!from || !to) throw new Error(`missing endpoint ${fromKey} -> ${toKey}`);
    const relationId = id('relation');
    state.ops.push({
      type: 'relation.create',
      relation: {
        id: relationId,
        relationType: medium === 'air' ? 'utilities' : 'process',
        fromNodeId: from,
        toNodeId: to,
        metadata: {},
      },
    });
    state.ops.push({
      type: 'connection.create',
      node_id: id('node'),
      parent_id: null,
      name,
      transform: nodeTransform(),
      connection: {
        from: { kind: 'instance', nodeId: from, port: fromPort },
        to: { kind: 'instance', nodeId: to, port: toPort },
        medium,
        diameter,
        routePolicy: { kind: 'orthogonal' },
        relationId,
        metadata: {},
      },
    });
  };

  return { state, material, part, port, define, instance, connect, box, cylinder, rotateY };
}

// 圆形池：底板 + 环形池壁 + 水面 + 中心进水筒 + 周边出水堰 + 刮泥桥 + 栏杆 + 爬梯
function circularBasin(b, prefix, { radius = 12, depth = 4, wall = 0.45, segments = 28 }) {
  const parts = [];
  const wallR = radius - wall / 2;
  const arc = (TAU * wallR) / segments;

  parts.push(b.part(`${prefix}-base`, b.cylinder(radius, 0.5, segments * 2), 0, 0.25, 0, 'concrete'));
  for (let index = 0; index < segments; index += 1) {
    const angle = (index / segments) * TAU;
    parts.push(b.part(
      `${prefix}-wall-${index + 1}`,
      b.box(arc + 0.1, depth, wall),
      Math.cos(angle) * wallR,
      0.5 + depth / 2,
      Math.sin(angle) * wallR,
      'concrete',
      { rotation: b.rotateY(-angle) },
    ));
  }

  // 水面略低于池顶，避免与池壁共面。
  parts.push(b.part(`${prefix}-water`, b.cylinder(radius - wall, 0.06, segments * 2), 0, 0.5 + depth - 0.55, 0, 'water'));
  parts.push(b.part(`${prefix}-inlet-well`, b.cylinder(1.5, depth + 0.9, 32), 0, 0.5 + (depth + 0.9) / 2, 0, 'concrete'));

  // 周边出水堰：贴内壁一圈，堰顶高出水面。
  const weirR = radius - wall - 0.55;
  const weirArc = (TAU * weirR) / segments;
  for (let index = 0; index < segments; index += 1) {
    const angle = (index / segments) * TAU;
    parts.push(b.part(
      `${prefix}-weir-${index + 1}`,
      b.box(weirArc + 0.06, 0.5, 0.35),
      Math.cos(angle) * weirR,
      0.5 + depth - 0.2,
      Math.sin(angle) * weirR,
      'concrete',
      { rotation: b.rotateY(-angle) },
    ));
  }

  // 刮泥桥：跨池直径，两端落在池壁走道上。
  const bridgeY = 0.5 + depth + 0.55;
  parts.push(b.part(`${prefix}-bridge`, b.box(radius * 2 + 1.2, 0.42, 1.6), 0, bridgeY, 0, 'walkway'));
  parts.push(b.part(`${prefix}-bridge-rail-a`, b.box(radius * 2 + 1.2, 0.08, 0.08), 0, bridgeY + 1.05, -0.76, 'steel'));
  parts.push(b.part(`${prefix}-bridge-rail-b`, b.box(radius * 2 + 1.2, 0.08, 0.08), 0, bridgeY + 1.05, 0.76, 'steel'));
  parts.push(b.part(`${prefix}-drive`, b.cylinder(1.1, 1.0, 24), 0, bridgeY + 0.7, 0, 'steel'));

  // 池顶防护栏杆
  const ringY = 0.5 + depth + 0.55;
  const railArc = (TAU * radius) / segments;
  for (let index = 0; index < segments; index += 1) {
    const angle = (index / segments) * TAU;
    parts.push(b.part(
      `${prefix}-rail-${index + 1}`,
      b.box(railArc + 0.1, 0.08, 0.08),
      Math.cos(angle) * radius,
      ringY + 0.5,
      Math.sin(angle) * radius,
      'steel',
      { rotation: b.rotateY(-angle) },
    ));
    if (index % 2 === 0) {
      parts.push(b.part(
        `${prefix}-post-${index + 1}`,
        b.box(0.1, 1.1, 0.1),
        Math.cos(angle) * radius,
        ringY,
        Math.sin(angle) * radius,
        'steel',
      ));
    }
  }

  // 检修爬梯：池外壁，从地面到池顶。
  for (let index = 0; index < 9; index += 1) {
    parts.push(b.part(
      `${prefix}-ladder-${index + 1}`,
      b.box(0.9, 0.06, 0.3),
      radius + 0.25,
      0.6 + index * 0.5,
      0,
      'steel',
    ));
  }
  parts.push(b.part(`${prefix}-ladder-rail-a`, b.box(0.06, depth + 1.2, 0.06), radius + 0.25, (depth + 1.2) / 2, -0.42, 'steel'));
  parts.push(b.part(`${prefix}-ladder-rail-b`, b.box(0.06, depth + 1.2, 0.06), radius + 0.25, (depth + 1.2) / 2, 0.42, 'steel'));

  return parts;
}

// 矩形曝气池：底板 + 四面池壁 + 导流墙 + 水面 + 曝气支管 + 检修走道 + 栏杆
function aerationBasin(b, prefix, { length = 40, width = 20, depth = 6, wall = 0.45 }) {
  const parts = [];
  const top = 0.5 + depth;
  parts.push(b.part(`${prefix}-base`, b.box(length, 0.5, width), 0, 0.25, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-n`, b.box(length, depth, wall), 0, 0.5 + depth / 2, -width / 2, 'concrete'));
  parts.push(b.part(`${prefix}-wall-s`, b.box(length, depth, wall), 0, 0.5 + depth / 2, width / 2, 'concrete'));
  parts.push(b.part(`${prefix}-wall-w`, b.box(wall, depth, width), -length / 2, 0.5 + depth / 2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-e`, b.box(wall, depth, width), length / 2, 0.5 + depth / 2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-water`, b.box(length - wall * 2, 0.06, width - wall * 2), 0, top - 0.5, 0, 'water'));

  // 导流墙：形成推流廊道，三段错位布置。
  for (let index = 0; index < 3; index += 1) {
    const x = -length / 2 + (index + 1) * (length / 4);
    parts.push(b.part(
      `${prefix}-baffle-${index + 1}`,
      b.box(0.35, depth - 0.6, width - 4),
      x,
      0.5 + (depth - 0.6) / 2,
      index % 2 === 0 ? 1.6 : -1.6,
      'concrete',
    ));
  }

  // 曝气支管：沿池长均布，接口朝上接空气干管。
  for (let index = 0; index < 10; index += 1) {
    const x = -length / 2 + 2 + index * ((length - 4) / 9);
    parts.push(b.part(
      `${prefix}-air-${index + 1}`,
      b.box(0.18, 0.18, width - 3),
      x,
      0.9,
      0,
      'air',
    ));
  }

  // 检修走道沿一侧布置，外侧加栏杆。
  parts.push(b.part(`${prefix}-walkway`, b.box(length, 0.14, 1.3), 0, top + 0.05, -width / 2 - 0.7, 'walkway'));
  parts.push(b.part(`${prefix}-walkway-rail`, b.box(length, 0.08, 0.08), 0, top + 0.95, -width / 2 - 1.3, 'steel'));
  for (let index = 0; index <= 10; index += 1) {
    const x = -length / 2 + index * (length / 10);
    parts.push(b.part(
      `${prefix}-walkway-post-${index + 1}`,
      b.box(0.09, 0.95, 0.09),
      x,
      top + 0.5,
      -width / 2 - 1.3,
      'steel',
    ));
  }

  return parts;
}

function buildPlan(projectId) {
  const b = createBuilder();
  b.material('concrete', 'Reinforced concrete', [0.62, 0.63, 0.61, 1]);
  b.material('water', 'Process water', [0.16, 0.36, 0.44, 0.92]);
  b.material('steel', 'Galvanised steel', [0.55, 0.58, 0.6, 1]);
  b.material('walkway', 'Grating walkway', [0.42, 0.45, 0.47, 1]);
  b.material('air', 'Air header', [0.3, 0.52, 0.7, 1]);

  // 初沉池
  const primaryParts = circularBasin(b, 'pc', { radius: 13, depth: 4.2, segments: 30 });
  b.define('primary-clarifier', 'Primary clarifier', primaryParts, [
    b.port('inlet', 'pc-inlet-well', 0, 5.4, 0, vec(0, -1, 0), 'raw-water', 1.0),
    b.port('outlet', 'pc-weir-1', 12.4, 4.5, 0, vec(1, 0, 0), 'settled-water', 0.9),
  ]);

  // 曝气池（推流式廊道）。首段接纳初沉出水，后续段接纳混合液，端口介质不同，
  // 因此按进水介质分成两种定义。
  b.define('aeration-feed', 'Aeration basin (feed)', aerationBasin(b, 'ae', { length: 42, width: 20, depth: 6 }), [
    b.port('inlet', 'ae-wall-w', -21, 4.0, 0, vec(1, 0, 0), 'settled-water', 0.9),
    b.port('outlet', 'ae-wall-e', 21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
  ]);
  b.define('aeration-mixed', 'Aeration basin (mixed liquor)', aerationBasin(b, 'ae', { length: 42, width: 20, depth: 6 }), [
    b.port('inlet', 'ae-wall-w', -21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
    b.port('outlet', 'ae-wall-e', 21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
  ]);

  // 二沉池（与初沉池同构，尺寸与用途不同）
  const secondaryParts = circularBasin(b, 'sc', { radius: 15, depth: 4.6, segments: 32 });
  b.define('secondary-clarifier', 'Secondary clarifier', secondaryParts, [
    b.port('inlet', 'sc-inlet-well', 0, 5.6, 0, vec(0, -1, 0), 'mixed-liquor', 0.9),
    b.port('outlet', 'sc-weir-1', 14.4, 4.9, 0, vec(1, 0, 0), 'final-effluent', 0.8),
  ]);

  // 总平面：沿工艺流向自西向东
  b.instance('primary-clarifier', 'pc-1', 'PC-101 初沉池', 0, 0, 0, { stage: 'primary', tag: 'PC-101' });
  b.instance('aeration-feed', 'ae-1', 'AE-101 曝气池（厌氧段）', 45, 0, 0, { stage: 'biological', tag: 'AE-101' });
  b.instance('aeration-mixed', 'ae-2', 'AE-102 曝气池（缺氧段）', 92, 0, 0, { stage: 'biological', tag: 'AE-102' });
  b.instance('aeration-mixed', 'ae-3', 'AE-103 曝气池（好氧段）', 139, 0, 0, { stage: 'biological', tag: 'AE-103' });
  b.instance('secondary-clarifier', 'sc-1', 'SC-101 二沉池', 200, 0, 0, { stage: 'secondary', tag: 'SC-101' });

  b.connect('pc-1', 'outlet', 'ae-1', 'inlet', 'PC-101 → AE-101 进水', 'settled-water', 0.9);
  b.connect('ae-1', 'outlet', 'ae-2', 'inlet', 'AE-101 → AE-102 混合液', 'mixed-liquor', 0.9);
  b.connect('ae-2', 'outlet', 'ae-3', 'inlet', 'AE-102 → AE-103 混合液', 'mixed-liquor', 0.9);
  b.connect('ae-3', 'outlet', 'sc-1', 'inlet', 'AE-103 → SC-101 混合液', 'mixed-liquor', 0.9);

  return {
    projectId,
    operations: b.state.ops,
    summary: {
      components: b.state.components.size,
      instances: 5,
      connections: 4,
      materials: b.state.materials.size,
    },
  };
}

function waitForServer(server) {
  // 服务启动时依次输出监听地址与会话 token 两行。
  return new Promise((resolvePromise, reject) => {
    const reader = createInterface({ input: server.stdout });
    const lines = [];
    const timer = setTimeout(() => reject(new Error('server did not start')), 120_000);
    reader.on('line', (line) => {
      const text = line.trim();
      if (!text) return;
      lines.push(text);
      if (lines.length === 2) {
        clearTimeout(timer);
        resolvePromise({ address: lines[0], token: lines[1] });
      }
    });
    server.on('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`server exited with ${code}`));
    });
  });
}

async function main() {
  const root = resolve('.');
  // 传入 --workspace 时写入持久工程目录，否则用一次性临时目录。
  const explicitWorkspace = process.argv.includes('--workspace')
    ? resolve(process.argv[process.argv.indexOf('--workspace') + 1])
    : null;
  const workspace = explicitWorkspace ?? await mkdtemp(join(tmpdir(), 'ice3-wwtp-built-'));
  if (explicitWorkspace) {
    await mkdir(explicitWorkspace, { recursive: true });
  }
  const plan = buildPlan('wastewater-plant');
  const server = spawn(
    resolve('target/release/studio-cli'),
    ['serve', '--workspace', workspace, '--project', 'wwtp-bootstrap', '--port', '0', '--write-mode', 'autonomous'],
    {
      cwd: root,
      env: { ...process.env, STUDIO_SESSION_TOKEN: 'agent-wastewater-plant', STUDIO_VIEWER_DIR: resolve('apps/viewer/dist') },
      stdio: ['ignore', 'pipe', 'inherit'],
    },
  );

  try {
    const { address, token } = await waitForServer(server);
    let requestId = 0;
    const rpc = async (method, params = {}) => {
      requestId += 1;
      const response = await fetch(`http://${address}/rpc`, {
        method: 'POST',
        headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: requestId, method, params }),
      });
      const envelope = await response.json();
      if (envelope.error) throw new Error(`${method}: ${JSON.stringify(envelope.error)}`);
      return envelope.result;
    };

    await rpc('project.create', { projectId: plan.projectId });
    const committed = await rpc('document.apply', {
      expectedRevision: 0,
      dryRun: false,
      idempotencyKey: 'wastewater-plant-1',
      operations: plan.operations,
    });
    const stats = await rpc('evaluation.stats');
    const exported = await rpc('export.sourceGlb');
    console.log(JSON.stringify({
      url: `http://${address}/?token=${token}`,
      revision: committed.revision,
      operations: plan.operations.length,
      stats,
      summary: plan.summary,
      sourceGlb: exported.sourcePath,
    }, null, 2));
    // Viewer 显示需要 toolchain 生成预览；未配置时保持服务以便检查文档与导出。
    const keepAliveMs = Number(process.env.KEEP_ALIVE_MS ?? 0);
    if (keepAliveMs > 0) {
      await new Promise((resolvePromise) => setTimeout(resolvePromise, keepAliveMs));
    }
  } finally {
    server.kill('SIGTERM');
  }
}

if (process.argv[1] && import.meta.url === `file://${process.argv[1]}`) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}

export { buildPlan };
