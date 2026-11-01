// Studio 原生参数化模型 Builder 与工程细节工具。

// 用 Studio 自身能力重建污水处理厂示例：几何、端口、管道全部由内核参数化生成。
// 本模块承载厂区布局；命令入口保留在 scripts/build-wastewater-model.mjs。
// 不引用任何外部资产。本文件属于案例层，工艺词汇只在这里出现。
//
// 构件依据真实构筑物组织：圆形池带中心进水筒、周边出水堰、刮泥桥与防护栏杆；
// 矩形池带导流墙、曝气支管、检修走道。端口声明介质与管径，管道由连接意图路由。

const TAU = Math.PI * 2;
// 材质在工程里存的是线性空间的量，渲染器输出时才做 sRGB 编码。
// 人挑配色是在 sRGB 空间挑的（#b9b3a6 这种），所以统一从十六进制反解，
// 不要手写线性小数——手写很容易把整片厂区调到发灰发暗，而且完全不可读。
const srgbChannel = (value) => (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
function srgb(hex, alpha = 1) {
  const text = hex.replace('#', '');
  const r = Number.parseInt(text.slice(0, 2), 16) / 255;
  const g = Number.parseInt(text.slice(2, 4), 16) / 255;
  const b = Number.parseInt(text.slice(4, 6), 16) / 255;
  return [srgbChannel(r), srgbChannel(g), srgbChannel(b), alpha];
}
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

// yaw 为绕 Y 轴的角度（弧度）。默认 0 时四元数就是单位元，
// 因此原有只传平移的调用点不需要改动。
function nodeTransform(x = 0, y = 0, z = 0, yaw = 0) {
  return {
    translation: vec(x, y, z),
    rotation: yaw === 0 ? [0, 0, 0, 1] : [0, Math.sin(yaw / 2), 0, Math.cos(yaw / 2)],
    scale: vec(1, 1, 1),
  };
}

const rotateY = (angle) => [0, Math.sin(angle / 2), 0, Math.cos(angle / 2)];
// 圆柱默认沿 Y 轴，绕 Z 轴转 -90° 让它躺平成沿 X 走向的管道。
const LIE_ALONG_X = [0, 0, -Math.SQRT1_2, Math.SQRT1_2];
const box = (w, h, d) => ({ kind: 'box', width: w, height: h, depth: d });
const cylinder = (r, h, segments = 40) => ({
  kind: 'cylinder',
  radius: r,
  height: h,
  segments,
});
// 球体用于树冠。段数刻意压低：一棵树三团冠，全厂九十多棵，
// 每团多两级细分就是几万个三角形，远景看不出差别。
const sphere = (radius, segments = 10, rings = 6) => ({ kind: 'sphere', radius, segments, rings });

// 构件登记表：几何与材质角色分离，材质在实例上覆盖。
function createBuilder() {
  const state = {
    ops: [],
    materials: new Map(),
    next: 1,
    components: new Map(),
    nodes: new Map(),
    instances: new Map(),
    lineSequence: 0,
  };
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

  // 重复部件：由内核按 i/count 展开，位置与朝向用表达式给出。
  const repeatPart = (partId, count, content, options = {}) => ({
    id: partId,
    repeat: count,
    transform: options.transform ?? {},
    content,
    metadata: options.metadata,
    role: options.role,
  });

  const part = (partId, content, x, y, z, role, options = {}) => ({
    id: partId,
    parentId: options.parentId,
    transform: partTransform(x, y, z, options),
    content,
    subtract: options.subtract,
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

  const define = (key, name, parts, ports, parameters = []) => {
    const definitionId = id('component');
    const overrides = {};
    for (const item of parts) {
      if (item.role) {
        if (!state.materials.has(item.role)) {
          throw new Error(`component ${name} references unknown material role ${item.role}`);
        }
        overrides[item.id] = {
          materialId: state.materials.get(item.role),
          parameters: {},
        };
      }
    }
    const definition = {
      id: definitionId,
      name,
      parameters,
      parts: parts.map(({ role, ...rest }) => rest),
      ports,
      metadata: { category: 'process-structure' },
    };
    state.ops.push({ type: 'component.define', definition });
    state.components.set(key, { definition, overrides });
    return definitionId;
  };

  const instance = (definitionKey, key, name, x, y, z, metadata = {}, parameters = {}, yaw = 0) => {
    const entry = state.components.get(definitionKey);
    if (!entry) throw new Error(`unknown component ${definitionKey}`);
    const nodeId = id('node');
    state.nodes.set(key, nodeId);
    const stage = metadata.stage ?? (metadata.role === 'landscape' ? 'landscape' : 'auxiliary');
    const parametricExtent = Number.isFinite(parameters.radius)
      ? parameters.radius
      : Math.max(
        ...['length', 'width', 'depth']
          .map((name) => parameters[name])
          .filter(Number.isFinite),
        0,
      ) / 2;
    const requestedClearanceRadius = Number(metadata.clearanceRadius);
    const clearanceRadius = Number.isFinite(requestedClearanceRadius)
      ? requestedClearanceRadius
      : (['auxiliary', 'landscape'].includes(stage) ||
        ['equipment', 'landscape', 'piping', 'site'].includes(metadata.role)
        ? 0
        : parametricExtent + 3);
    const normalizedMetadata = {
      ...metadata,
      stage,
      clearanceRadius: String(clearanceRadius),
    };
    state.ops.push({
      type: 'instance.create',
      node_id: nodeId,
      parent_id: null,
      name,
      transform: nodeTransform(x, y, z, yaw),
      instance: {
        definitionId: entry.definition.id,
        parameters,
        overrides: entry.overrides,
        metadata: normalizedMetadata,
      },
      metadata: normalizedMetadata,
    });
    state.instances.set(key, {
      nodeId,
      definition: entry.definition,
      transform: nodeTransform(x, y, z, yaw),
      metadata: normalizedMetadata,
    });
    return nodeId;
  };

  const rotateVector = (value, rotation) => {
    const [qx, qy, qz, qw] = rotation;
    const ix = qw * value.x + qy * value.z - qz * value.y;
    const iy = qw * value.y + qz * value.x - qx * value.z;
    const iz = qw * value.z + qx * value.y - qy * value.x;
    const iw = -qx * value.x - qy * value.y - qz * value.z;
    return vec(
      ix * qw + iw * -qx + iy * -qz - iz * -qy,
      iy * qw + iw * -qy + iz * -qx - ix * -qz,
      iz * qw + iw * -qz + ix * -qy - iy * -qx,
    );
  };

  const resolveWorldPort = (instanceKey, portName) => {
    const instanceEntry = state.instances.get(instanceKey);
    if (!instanceEntry) throw new Error(`missing instance ${instanceKey}`);
    const port = instanceEntry.definition.ports.find(({ name }) => name === portName);
    if (!port) throw new Error(`missing port ${instanceKey}/${portName}`);
    const rotated = rotateVector(port.position, instanceEntry.transform.rotation);
    const direction = rotateVector(port.direction, instanceEntry.transform.rotation);
    return {
      port,
      position: vec(
        instanceEntry.transform.translation.x + rotated.x,
        instanceEntry.transform.translation.y + rotated.y,
        instanceEntry.transform.translation.z + rotated.z,
      ),
      direction,
      metadata: instanceEntry.metadata,
    };
  };

  // 管道按介质分色，和真实厂区一样用颜色编码工艺流向。
  const PIPE_MATERIAL = {
    'raw-water': 'pipe-raw',
    'screened-water': 'pipe-raw',
    'lifted-water': 'pipe-raw',
    'degritted-water': 'pipe-raw',
    'settled-water': 'pipe-process',
    'mixed-liquor': 'pipe-process',
    'secondary-effluent': 'pipe-clean',
    'filtered-water': 'pipe-clean',
    'final-effluent': 'pipe-clean',
    'waste-sludge': 'pipe-sludge',
    'thickened-sludge': 'pipe-sludge',
    'digested-sludge': 'pipe-sludge',
    'air': 'pipe-air',
  };

  // 走线策略按介质分工，这决定了图面上能看见什么：
  // 同排构筑物之间的工艺水走地上、看得见，跨区输送的污泥与二沉出水全部埋地。
  // 真实厂区本来就把大部分厂际管线埋在冻土线以下，地上只留管廊那几根主管；
  // 如果所有连线都走 Orthogonal，每根管子都会在地上绕一个 L 形横跨别人的池子。
  const ROUTE_POLICY = {
    // 混合液去二沉池中心进水筒：真实做法是埋地进水管从池底穿进来，
    // 走地上就会出现「一根管从隔壁二沉池顶上横过去」的画面。
    'mixed-liquor': { kind: 'buried', depth: 2.2 },
    'waste-sludge': { kind: 'buried', depth: 1.6 },
    'thickened-sludge': { kind: 'buried', depth: 1.5 },
    'digested-sludge': { kind: 'buried', depth: 1.5 },
    // 二沉出水要横跨整排池子去滤池，同样埋地。
    'secondary-effluent': { kind: 'buried', depth: 1.8 },
    'air': { kind: 'rack', elevation: 3.6 },
  };

  const addPipeAccessory = (name, content, position, materialKey, metadata) => {
    state.ops.push({
      type: 'node.create',
      node_id: id('node'),
      parent_id: null,
      name,
      transform: nodeTransform(position.x, position.y, position.z),
      content: { type: 'primitive', ...content },
      material_id: state.materials.get(materialKey),
      metadata,
    });
  };

  const connect = (
    fromKey,
    fromPort,
    toKey,
    toPort,
    name,
    medium,
    diameter,
    options = {},
  ) => {
    const from = state.nodes.get(fromKey);
    const to = state.nodes.get(toKey);
    if (!from || !to) throw new Error(`missing endpoint ${fromKey} -> ${toKey}`);
    const fromEndpoint = resolveWorldPort(fromKey, fromPort);
    const toEndpoint = resolveWorldPort(toKey, toPort);
    const lineId = options.lineId ?? `L-${String(++state.lineSequence).padStart(3, '0')}`;
    const flow = options.flow ?? 'forward';
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
        // 真实厂区的主管比图面上的细线粗得多，这里统一放大一档。
        diameter: diameter * 1.8,
        routePolicy: ROUTE_POLICY[medium] ?? { kind: 'orthogonal' },
        relationId,
        metadata: {
          lineId,
          flow,
          service: name,
          medium,
        },
      },
      material_id: state.materials.get(PIPE_MATERIAL[medium] ?? 'pipe-process'),
    });
    if (options.fittings !== false) {
      addPipeAccessory(
        `${name} 起点法兰`,
        box(diameter * 2.1, 0.18, diameter * 2.1),
        fromEndpoint.position,
        'steel',
        { role: 'pipe-flange', lineId, stage: fromEndpoint.metadata.stage },
      );
      addPipeAccessory(
        `${name} 终点法兰`,
        box(diameter * 2.1, 0.18, diameter * 2.1),
        toEndpoint.position,
        'steel',
        { role: 'pipe-flange', lineId, stage: toEndpoint.metadata.stage },
      );
      if (medium === 'air' || /泵|提升|回流/.test(name)) {
        addPipeAccessory(
          `${name} 隔离阀`,
          box(diameter * 1.5, diameter * 1.25, diameter * 1.5),
          fromEndpoint.position,
          'steel',
          { role: 'pipe-valve', lineId, stage: fromEndpoint.metadata.stage },
        );
      }
    }
    const midpoint = vec(
      (fromEndpoint.position.x + toEndpoint.position.x) / 2,
      fromEndpoint.position.y,
      (fromEndpoint.position.z + toEndpoint.position.z) / 2,
    );
    if (options.supports !== false) {
      if (ROUTE_POLICY[medium]?.kind === 'buried') {
        addPipeAccessory(
          `${name} 检查井`,
          cylinder(0.52, 0.08, 18),
          vec(midpoint.x, 0.04, midpoint.z),
          'steel',
          { role: 'pipe-cover', lineId, stage: fromEndpoint.metadata.stage },
        );
      } else {
        addPipeAccessory(
          `${name} 管道支架`,
          box(0.24, 3.6, diameter * 2.4),
          vec(midpoint.x, 1.8, midpoint.z),
          'steel',
          { role: 'pipe-support', lineId, stage: fromEndpoint.metadata.stage },
        );
      }
    }
    if (state.lineSequence <= 5) {
      addPipeAccessory(
        `${lineId} 管线标牌`,
        box(2.3, 0.32, 0.08),
        vec(midpoint.x, Math.max(fromEndpoint.position.y, 2.8), midpoint.z),
        'sign-panel',
        { role: 'pipe-label', lineId, stage: fromEndpoint.metadata.stage },
      );
    }
  };

  // 场地：一块很薄的板，略低于池底，不参与工艺连接。
  const ground = (name, minX, maxX, minZ, maxZ, materialKey, level = -0.06) => {
    const nodeId = id('node');
    state.ops.push({
      type: 'node.create',
      node_id: nodeId,
      parent_id: null,
      name,
      transform: nodeTransform((minX + maxX) / 2, level, (minZ + maxZ) / 2),
      content: {
        type: 'primitive',
        kind: 'box',
        width: maxX - minX,
        height: 0.12,
        depth: maxZ - minZ,
      },
      material_id: state.materials.get(materialKey),
      metadata: { role: 'site', bounds: 'ignore' },
    });
  };

  // 架空栈桥：两点之间的走道板 + 两侧栏杆，用于连接相邻构筑物。
  const walkway = (name, from, to, { y = 5.2, width = 1.4, materialKey = 'walkway' } = {}) => {
    const dx = to[0] - from[0];
    const dz = to[1] - from[1];
    const length = Math.hypot(dx, dz);
    const angle = Math.atan2(dz, dx);
    const nodeId = id('node');
    // 绕 Y 轴旋转，使长边对齐两点连线。
    const rotation = [0, Math.sin(-angle / 2), 0, Math.cos(-angle / 2)];
    state.ops.push({
      type: 'node.create',
      node_id: nodeId,
      parent_id: null,
      name,
      transform: {
        translation: vec((from[0] + to[0]) / 2, y, (from[1] + to[1]) / 2),
        rotation,
        scale: vec(1, 1, 1),
      },
      content: {
        type: 'primitive',
        kind: 'box',
        width: length,
        height: 0.16,
        depth: width,
      },
      material_id: state.materials.get(materialKey),
      metadata: { role: 'walkway' },
    });
    // 两侧栏杆：沿栈桥法线方向各偏移半个桥宽。
    const normalX = -Math.sin(angle);
    const normalZ = Math.cos(angle);
    for (const side of [-1, 1]) {
      const railId = id('node');
      state.ops.push({
        type: 'node.create',
        node_id: railId,
        parent_id: null,
        name: `${name} 栏杆`,
        transform: {
          translation: vec(
            (from[0] + to[0]) / 2 + normalX * side * (width / 2),
            y + 0.9,
            (from[1] + to[1]) / 2 + normalZ * side * (width / 2),
          ),
          rotation,
          scale: vec(1, 1, 1),
        },
        content: {
          type: 'primitive',
          kind: 'box',
          width: length,
          height: 0.07,
          depth: 0.07,
        },
        material_id: state.materials.get('steel'),
        metadata: { role: 'railing' },
      });
    }
  };

  // 接触阴影：贴在构筑物正下方的一层深色薄片，压掉「悬浮感」。
  const shadowPatch = (name, x, z, sizeX, sizeZ, round = false) => {
    const nodeId = id('node');
    state.ops.push({
      type: 'node.create',
      node_id: nodeId,
      parent_id: null,
      name,
      transform: nodeTransform(x, 0.012, z),
      content: round
        ? { type: 'primitive', kind: 'cylinder', radius: Math.max(sizeX, sizeZ) * 0.575, height: 0.02, segments: 32 }
        : { type: 'primitive', kind: 'box', width: sizeX * 1.15, height: 0.02, depth: sizeZ * 1.15 },
      material_id: state.materials.get('shadow'),
      metadata: { role: 'shadow' },
    });
  };

  // 小型设备：一个控制箱加一台立式泵，作为独立节点放在构筑物旁。
  const equipment = (kind, name, x, y, z, metadata = {}) => {
    const nodeId = id('node');
    state.ops.push({
      type: 'node.create',
      node_id: nodeId,
      parent_id: null,
      name,
      transform: nodeTransform(x, y, z),
      content: { type: 'primitive', kind: 'box', width: 1.2, height: 1.6, depth: 0.8 },
      material_id: state.materials.get('facade'),
      metadata: { role: 'equipment', kind, ...metadata },
    });
    const pumpId = id('node');
    state.ops.push({
      type: 'node.create',
      node_id: pumpId,
      parent_id: null,
      name: `${name} 泵`,
      transform: nodeTransform(x + 1.8, y + 0.6, z),
      content: { type: 'primitive', kind: 'cylinder', radius: 0.45, height: 1.2, segments: 18 },
      material_id: state.materials.get('steel'),
      metadata: { role: 'equipment', kind: 'pump' },
    });
  };

  return { state, material, part, repeatPart, port, define, instance, connect, ground, walkway, equipment, shadowPatch, box, cylinder, rotateY };
}


const quaternionY = (yaw) => [0, Math.sin(yaw / 2), 0, Math.cos(yaw / 2)];

const offsetAlongRun = (x, z, offset, yaw) => ({
  x: x + Math.cos(yaw) * offset,
  z: z + Math.sin(yaw) * offset,
});

export function detailFlange(b, prefix, { diameter, position, rotation, material }) {
  return [
    b.part(`${prefix}-sleeve`, b.cylinder(diameter * 0.54, 0.18, 16),
      position.x, position.y, position.z, material, { rotation }),
    b.part(`${prefix}-plate`, b.cylinder(diameter * 0.75, 0.08, 16),
      position.x, position.y, position.z, material, { rotation }),
  ];
}

export function detailValve(b, prefix, { diameter, position, rotation, material }) {
  return [
    b.part(`${prefix}-body`, b.cylinder(diameter * 0.72, diameter * 1.2, 16),
      position.x, position.y, position.z, material, { rotation }),
    b.part(`${prefix}-bonnet`, b.box(diameter * 0.34, diameter * 0.5, diameter * 0.34),
      position.x, position.y + diameter * 0.72, position.z, material),
    b.part(`${prefix}-wheel`, b.cylinder(diameter * 0.55, 0.08, 18),
      position.x, position.y + diameter * 1.02, position.z, material),
  ];
}

export function detailStair(b, prefix, { x, y, z, width, height, yaw }) {
  const steps = 10;
  const run = Math.max(height * 0.78, 1.8);
  const rotation = quaternionY(yaw);
  const treadDepth = run / steps;
  const parts = [];
  for (let index = 0; index < steps; index += 1) {
    const offset = -run / 2 + (index + 0.5) * treadDepth;
    const position = offsetAlongRun(x, z, offset, yaw);
    parts.push(b.part(
      `${prefix}-tread-${index + 1}`,
      b.box(treadDepth * 0.88, 0.08, width),
      position.x,
      y + height * ((index + 1) / steps),
      position.z,
      'walkway',
      { rotation },
    ));
  }
  for (const side of [-1, 1]) {
    const sideOffset = offsetAlongRun(x, z, 0, yaw);
    const normalX = -Math.sin(yaw) * side * width * 0.48;
    const normalZ = Math.cos(yaw) * side * width * 0.48;
    parts.push(b.part(
      `${prefix}-stringer-${side > 0 ? 'a' : 'b'}`,
      b.box(run, 0.14, 0.08),
      sideOffset.x + normalX,
      y + height / 2,
      sideOffset.z + normalZ,
      'steel',
      { rotation },
    ));
  }
  return parts;
}

export function detailGuardrail(b, prefix, { x, y, z, length, yaw }) {
  const rotation = quaternionY(yaw);
  const parts = [];
  for (let index = 0; index < 3; index += 1) {
    const offset = -length / 2 + (index * length) / 2;
    const position = offsetAlongRun(x, z, offset, yaw);
    parts.push(b.part(
      `${prefix}-post-${index + 1}`,
      b.box(0.08, 1.1, 0.08),
      position.x,
      y + 0.55,
      position.z,
      'steel',
    ));
  }
  for (const [name, railY, height, depth] of [
    ['top', 1.05, 0.08, 0.08],
    ['mid', 0.55, 0.06, 0.06],
    ['toe', 0.08, 0.14, 0.06],
  ]) {
    parts.push(b.part(
      `${prefix}-${name}`,
      b.box(length, height, depth),
      x,
      y + railY,
      z,
      'steel',
      { rotation },
    ));
  }
  return parts;
}

export function detailSupport(b, prefix, { x, y, z, height, width }) {
  return [
    b.part(`${prefix}-column-a`, b.box(0.28, height, 0.28),
      x, y + height / 2, z - width / 2, 'steel'),
    b.part(`${prefix}-column-b`, b.box(0.28, height, 0.28),
      x, y + height / 2, z + width / 2, 'steel'),
    b.part(`${prefix}-beam`, b.box(0.3, 0.3, width + 0.6),
      x, y + height - 0.15, z, 'steel'),
  ];
}

export {
  TAU,
  srgb,
  lit,
  vec,
  partTransform,
  nodeTransform,
  rotateY,
  LIE_ALONG_X,
  box,
  cylinder,
  sphere,
  createBuilder,
};
