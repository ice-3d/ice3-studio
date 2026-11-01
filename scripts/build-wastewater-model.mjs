// 用 Studio 自身能力重建污水处理厂示例：几何、端口、管道全部由内核参数化生成，
// 不引用任何外部资产。本文件属于案例层，工艺词汇只在这里出现。
//
// 构件依据真实构筑物组织：圆形池带中心进水筒、周边出水堰、刮泥桥与防护栏杆；
// 矩形池带导流墙、曝气支管、检修走道。端口声明介质与管径，管道由连接意图路由。

import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, rm } from 'node:fs/promises';
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

  return { state, material, part, port, define, instance, connect, ground, walkway, box, cylinder, rotateY };
}

// 圆形池：底板 + 环形池壁 + 水面 + 中心进水筒 + 周边出水堰 + 刮泥桥 + 栏杆 + 爬梯
function circularBasin(b, prefix, { radius = 12, depth = 4, wall = 0.45, segments = 28 }) {
  const parts = [];
  const wallR = radius - wall / 2;
  const arc = (TAU * wallR) / segments;
  // 段长与弧长一致，段间不重叠，避免相邻面互闪。

  parts.push(b.part(`${prefix}-base`, b.cylinder(radius, 0.5, segments * 2), 0, 0.25, 0, 'concrete'));
  for (let index = 0; index < segments; index += 1) {
    const angle = (index / segments) * TAU;
    parts.push(b.part(
      `${prefix}-wall-${index + 1}`,
      b.box(arc, depth, wall),
      Math.cos(angle) * wallR,
      0.5 + depth / 2,
      Math.sin(angle) * wallR,
      'concrete',
      { rotation: b.rotateY(-angle) },
    ));
  }

  // 池顶压顶：比墙体略宽的凸边，是真实圆池最显化的特征之一。
  const top = 0.5 + depth;
  const copingArc = (TAU * (radius + 0.1)) / segments;
  for (let index = 0; index < segments; index += 1) {
    const angle = (index / segments) * TAU;
    parts.push(b.part(
      `${prefix}-coping-${index + 1}`,
      b.box(copingArc, 0.22, wall + 0.34),
      Math.cos(angle) * (radius - wall / 2 + 0.02),
      top + 0.11,
      Math.sin(angle) * (radius - wall / 2 + 0.02),
      'concrete',
      { rotation: b.rotateY(-angle) },
    ));
  }
  // 池顶环形走道：贴内壁一圈，供检修人员站立。
  const deckArc = (TAU * (radius - wall - 0.5)) / segments;
  for (let index = 0; index < segments; index += 1) {
    const angle = (index / segments) * TAU;
    parts.push(b.part(
      `${prefix}-deck-${index + 1}`,
      b.box(deckArc, 0.1, 0.9),
      Math.cos(angle) * (radius - wall - 0.45),
      top + 0.05,
      Math.sin(angle) * (radius - wall - 0.45),
      'walkway',
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
  // 主桥做成桁架：两根弦杆 + 若干竖腹杆，比整块板更像照片里的结构。
  const bridgeSpan = radius * 2 + 1.2;
  parts.push(b.part(`${prefix}-bridge-top`, b.box(bridgeSpan, 0.12, 0.16), 0, bridgeY + 0.42, -0.7, 'steel'));
  parts.push(b.part(`${prefix}-bridge-top-b`, b.box(bridgeSpan, 0.12, 0.16), 0, bridgeY + 0.42, 0.7, 'steel'));
  parts.push(b.part(`${prefix}-bridge-bottom`, b.box(bridgeSpan, 0.14, 1.5), 0, bridgeY, 0, 'walkway'));
  for (let index = 0; index <= 8; index += 1) {
    const x = -bridgeSpan / 2 + index * (bridgeSpan / 8);
    parts.push(b.part(
      `${prefix}-bridge-strut-${index + 1}`,
      b.box(0.1, 0.44, 0.12),
      x,
      bridgeY + 0.21,
      -0.7,
      'steel',
    ));
    parts.push(b.part(
      `${prefix}-bridge-strut-b-${index + 1}`,
      b.box(0.1, 0.44, 0.12),
      x,
      bridgeY + 0.21,
      0.7,
      'steel',
    ));
  }
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
      b.box(railArc, 0.06, 0.06),
      Math.cos(angle) * radius,
      ringY + 0.5,
      Math.sin(angle) * radius,
      'steel',
      { rotation: b.rotateY(-angle) },
    ));
    if (index % 3 === 0) {
      parts.push(b.part(
        `${prefix}-post-${index + 1}`,
        b.box(0.06, 1.1, 0.06),
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
      b.box(0.35, depth - 0.15, width - 4),
      x,
      0.5 + (depth - 0.15) / 2,
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
      b.box(0.06, 0.95, 0.06),
      x,
      top + 0.5,
      -width / 2 - 1.3,
      'steel',
    ));
  }

  return parts;
}


// 进水格栅渠：矩形渠道，内设粗细两道格栅，渠道两侧留检修走道。
function screeningChannel(b, prefix, { length = 20, width = 6, depth = 2.6 }) {
  const parts = [];
  const top = 0.5 + depth;
  parts.push(b.part(`${prefix}-base`, b.box(length, 0.4, width), 0, 0.2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-n`, b.box(length, depth, 0.4), 0, 0.5 + depth / 2, -width / 2, 'concrete'));
  parts.push(b.part(`${prefix}-wall-s`, b.box(length, depth, 0.4), 0, 0.5 + depth / 2, width / 2, 'concrete'));
  parts.push(b.part(`${prefix}-wall-w`, b.box(0.4, depth, width), -length / 2, 0.5 + depth / 2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-e`, b.box(0.4, depth, width), length / 2, 0.5 + depth / 2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-water`, b.box(length - 0.8, 0.05, width - 0.8), 0, top - 0.6, 0, 'water'));

  // 两道格栅，栅条沿水流方向均匀排布。
  for (const [rack, station] of [[1, -4], [2, 3]]) {
    for (let index = 0; index < 13; index += 1) {
      const z = -width / 2 + 0.6 + index * ((width - 1.2) / 12);
      parts.push(b.part(
        `${prefix}-rack${rack}-bar-${index + 1}`,
        b.box(0.12, depth + 0.7, 0.07),
        station,
        0.5 + (depth + 0.7) / 2,
        z,
        'steel',
      ));
    }
    parts.push(b.part(`${prefix}-rack${rack}-frame`, b.box(0.3, 0.12, width - 0.8), station, top + 0.35, 0, 'steel'));
  }

  // 走道与栏杆
  parts.push(b.part(`${prefix}-walkway`, b.box(length, 0.12, 1.1), 0, top + 0.1, -width / 2 - 0.55, 'walkway'));
  parts.push(b.part(`${prefix}-walkway-rail`, b.box(length, 0.07, 0.07), 0, top + 0.95, -width / 2 - 1.05, 'steel'));
  for (let index = 0; index <= 8; index += 1) {
    parts.push(b.part(
      `${prefix}-walkway-post-${index + 1}`,
      b.box(0.06, 0.9, 0.06),
      -length / 2 + index * (length / 8),
      top + 0.5,
      -width / 2 - 1.05,
      'steel',
    ));
  }
  return parts;
}

// 提升泵房：单层厂房，内含泵组与集水池。
function pumpHouse(b, prefix, { length = 22, width = 13, height = 8 }) {
  const parts = [];
  parts.push(b.part(`${prefix}-slab`, b.box(length + 1, 0.6, width + 1), 0, 0.3, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-n`, b.box(length, height, 0.5), 0, 0.6 + height / 2, -width / 2, 'concrete'));
  parts.push(b.part(`${prefix}-wall-s`, b.box(length, height, 0.5), 0, 0.6 + height / 2, width / 2, 'concrete'));
  parts.push(b.part(`${prefix}-wall-w`, b.box(0.5, height, width), -length / 2, 0.6 + height / 2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-e`, b.box(0.5, height, width), length / 2, 0.6 + height / 2, 0, 'concrete'));
  // 坡屋顶：两块反向斜板拼成人字，比平顶更像厂区里的小泵房。
  const pitch = 0.30;
  const slope = [Math.sin(pitch / 2), 0, 0, Math.cos(pitch / 2)];
  const slopeNeg = [-Math.sin(pitch / 2), 0, 0, Math.cos(pitch / 2)];
  parts.push(b.part(
    `${prefix}-roof-a`,
    b.box(length + 1.4, 0.22, width * 0.64),
    0,
    0.6 + height + 0.85,
    -width * 0.26,
    'roof',
    { rotation: slope },
  ));
  parts.push(b.part(
    `${prefix}-roof-b`,
    b.box(length + 1.4, 0.22, width * 0.64),
    0,
    0.6 + height + 0.85,
    width * 0.26,
    'roof',
    { rotation: slopeNeg },
  ));
  parts.push(b.part(`${prefix}-ridge`, b.box(length + 1.5, 0.2, 0.35), 0, 0.6 + height + 1.12, 0, 'roof'));

  // 门洞与通风百叶
  parts.push(b.part(`${prefix}-door`, b.box(3.2, 4, 0.12), -length / 4, 0.6 + 2, width / 2 + 0.3, 'steel'));
  for (let index = 0; index < 5; index += 1) {
    parts.push(b.part(
      `${prefix}-louver-${index + 1}`,
      b.box(length * 0.5, 0.18, 0.1),
      length / 4,
      0.6 + 3.2 + index * 0.55,
      width / 2 + 0.3,
      'steel',
    ));
  }

  // 三台立式泵，含底座与电机
  for (let index = 0; index < 3; index += 1) {
    const x = -length / 2 + 3 + index * 6;
    parts.push(b.part(`${prefix}-pump${index + 1}-base`, b.box(2.2, 0.4, 2.2), x, 0.9, 0, 'steel'));
    parts.push(b.part(`${prefix}-pump${index + 1}-body`, b.cylinder(0.7, 2.4, 20), x, 2.3, 0, 'steel'));
    parts.push(b.part(`${prefix}-pump${index + 1}-motor`, b.cylinder(0.5, 1.1, 16), x, 3.9, 0, 'steel'));
    parts.push(b.part(`${prefix}-pump${index + 1}-riser`, b.cylinder(0.28, 4.2, 12), x, 2.6, 1.2, 'air'));
  }
  return parts;
}

// 曝气沉砂池：矩形池，池底设砂斗，池面设刮砂桥。
function gritChamber(b, prefix, { length = 26, width = 11, depth = 4.2 }) {
  const parts = [];
  const top = 0.5 + depth;
  parts.push(b.part(`${prefix}-base`, b.box(length, 0.5, width), 0, 0.25, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-n`, b.box(length, depth, 0.45), 0, 0.5 + depth / 2, -width / 2, 'concrete'));
  parts.push(b.part(`${prefix}-wall-s`, b.box(length, depth, 0.45), 0, 0.5 + depth / 2, width / 2, 'concrete'));
  parts.push(b.part(`${prefix}-wall-w`, b.box(0.45, depth, width), -length / 2, 0.5 + depth / 2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-e`, b.box(0.45, depth, width), length / 2, 0.5 + depth / 2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-water`, b.box(length - 0.9, 0.05, width - 0.9), 0, top - 0.5, 0, 'water'));

  // 砂斗：用递减的方块模拟倒锥，避免引入非轴对齐几何。
  for (let step = 0; step < 3; step += 1) {
    const scale = 1 - step * 0.28;
    parts.push(b.part(
      `${prefix}-hopper-${step + 1}`,
      b.box(length * 0.55 * scale, 0.9, width * 0.5 * scale),
      0,
      0.9 + step * 0.85,
      0,
      'concrete',
    ));
  }

  // 刮砂桥与驱动
  parts.push(b.part(`${prefix}-scraper-bridge`, b.box(length + 1, 0.36, 1.2), 0, top + 0.5, 0, 'walkway'));
  parts.push(b.part(`${prefix}-scraper-drive`, b.cylinder(0.7, 0.9, 18), 0, top + 1.1, 0, 'steel'));
  parts.push(b.part(`${prefix}-rail-n`, b.box(length, 0.08, 0.08), 0, top + 1.15, -width / 2, 'steel'));
  parts.push(b.part(`${prefix}-rail-s`, b.box(length, 0.08, 0.08), 0, top + 1.15, width / 2, 'steel'));

  // 曝气支管
  for (let index = 0; index < 8; index += 1) {
    const x = -length / 2 + 1.5 + index * ((length - 3) / 7);
    parts.push(b.part(`${prefix}-air-${index + 1}`, b.box(0.16, 0.16, width - 2), x, 1.2, 0, 'air'));
  }
  return parts;
}

// 滤池：分格矩形池，每格设滤料层与反冲洗配水。
function filterCell(b, prefix, { length = 32, width = 26, depth = 3.6, cells = 4 }) {
  const parts = [];
  const top = 0.5 + depth;
  const cellWidth = width / cells;
  parts.push(b.part(`${prefix}-base`, b.box(length, 0.45, width), 0, 0.22, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-n`, b.box(length, depth, 0.45), 0, 0.5 + depth / 2, -width / 2, 'concrete'));
  parts.push(b.part(`${prefix}-wall-s`, b.box(length, depth, 0.45), 0, 0.5 + depth / 2, width / 2, 'concrete'));
  parts.push(b.part(`${prefix}-wall-w`, b.box(0.45, depth, width), -length / 2, 0.5 + depth / 2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-e`, b.box(0.45, depth, width), length / 2, 0.5 + depth / 2, 0, 'concrete'));

  for (let index = 0; index < cells; index += 1) {
    const z = -width / 2 + cellWidth * (index + 0.5);
    // 分格墙
    if (index > 0) {
      parts.push(b.part(
        `${prefix}-divider-${index}`,
        b.box(length - 0.9, depth, 0.35),
        0,
        0.5 + depth / 2,
        -width / 2 + cellWidth * index,
        'concrete',
      ));
    }
    // 滤料层与承托层
    parts.push(b.part(`${prefix}-media-${index + 1}`, b.box(length - 1, 1.1, cellWidth - 0.9), 0, 1.2, z, 'media'));
    parts.push(b.part(`${prefix}-water-${index + 1}`, b.box(length - 1, 0.05, cellWidth - 0.9), 0, top - 0.6, z, 'water'));
    // 反冲洗配水管
    parts.push(b.part(`${prefix}-wash-${index + 1}`, b.box(0.22, 0.22, cellWidth - 1.2), 0, 2.6, z, 'air'));
  }

  // 操作走道与栏杆
  parts.push(b.part(`${prefix}-walkway`, b.box(length, 0.12, 1.2), 0, top + 0.1, -width / 2 - 0.6, 'walkway'));
  parts.push(b.part(`${prefix}-walkway-rail`, b.box(length, 0.07, 0.07), 0, top + 0.95, -width / 2 - 1.15, 'steel'));
  for (let index = 0; index <= 10; index += 1) {
    parts.push(b.part(
      `${prefix}-walkway-post-${index + 1}`,
      b.box(0.06, 0.9, 0.06),
      -length / 2 + index * (length / 10),
      top + 0.5,
      -width / 2 - 1.15,
      'steel',
    ));
  }
  return parts;
}

// 消毒渠：矩形渠道，槽内布置紫外灯组。
function disinfectionChannel(b, prefix, { length = 26, width = 5.5, depth = 3 }) {
  const parts = [];
  const top = 0.5 + depth;
  parts.push(b.part(`${prefix}-base`, b.box(length, 0.4, width), 0, 0.2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-n`, b.box(length, depth, 0.4), 0, 0.5 + depth / 2, -width / 2, 'concrete'));
  parts.push(b.part(`${prefix}-wall-s`, b.box(length, depth, 0.4), 0, 0.5 + depth / 2, width / 2, 'concrete'));
  parts.push(b.part(`${prefix}-wall-w`, b.box(0.4, depth, width), -length / 2, 0.5 + depth / 2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-e`, b.box(0.4, depth, width), length / 2, 0.5 + depth / 2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-water`, b.box(length - 0.8, 0.05, width - 0.8), 0, top - 0.45, 0, 'water'));

  // 紫外灯架：三组横梁，每组下挂灯管
  for (let bank = 0; bank < 3; bank += 1) {
    const x = -length / 3 + bank * (length / 3);
    parts.push(b.part(`${prefix}-bank${bank + 1}-beam`, b.box(0.5, 0.3, width - 0.6), x, top - 0.25, 0, 'steel'));
    for (let lamp = 0; lamp < 4; lamp += 1) {
      const z = -width / 2 + 0.9 + lamp * ((width - 1.8) / 3);
      parts.push(b.part(
        `${prefix}-bank${bank + 1}-lamp-${lamp + 1}`,
        b.cylinder(0.09, 1.6, 10),
        x,
        top - 1.1,
        z,
        'lamp',
      ));
    }
  }

  // 走道与盖板支撑
  parts.push(b.part(`${prefix}-walkway`, b.box(length, 0.12, 1.1), 0, top + 0.1, -width / 2 - 0.55, 'walkway'));
  parts.push(b.part(`${prefix}-rail`, b.box(length, 0.07, 0.07), 0, top + 0.95, -width / 2 - 1.05, 'steel'));
  return parts;
}

// 通用建筑：三层厂房，带窗带与入口雨棚。
function facilityBuilding(b, prefix, { length = 26, width = 14, floors = 3, floorHeight = 3.6 }) {
  const parts = [];
  const height = floors * floorHeight;
  parts.push(b.part(`${prefix}-slab`, b.box(length + 1, 0.6, width + 1), 0, 0.3, 0, 'concrete'));
  parts.push(b.part(`${prefix}-shell`, b.box(length, height, width), 0, 0.6 + height / 2, 0, 'facade'));
  parts.push(b.part(`${prefix}-roof`, b.box(length + 1.2, 0.45, width + 1.2), 0, 0.6 + height + 0.22, 0, 'roof'));

  // 每层窗带
  for (let floor = 0; floor < floors; floor += 1) {
    const y = 0.6 + floorHeight * floor + floorHeight * 0.55;
    parts.push(b.part(`${prefix}-window-s${floor + 1}`, b.box(length * 0.82, 1.3, 0.12), 0, y, width / 2 + 0.06, 'glass'));
    parts.push(b.part(`${prefix}-window-n${floor + 1}`, b.box(length * 0.82, 1.3, 0.12), 0, y, -width / 2 - 0.06, 'glass'));
  }

  // 入口雨棚与门
  parts.push(b.part(`${prefix}-canopy`, b.box(7, 0.3, 2.6), 0, 3.4, width / 2 + 1.2, 'steel'));
  parts.push(b.part(`${prefix}-entrance`, b.box(3.6, 2.8, 0.14), 0, 1.9, width / 2 + 0.08, 'glass'));
  return parts;
}


// 污泥浓缩池：小型圆池，带中心搅拌与周边出水堰。
function thickener(b, prefix, { radius = 8, depth = 4.4, segments = 24 }) {
  const parts = [];
  const top = 0.5 + depth;
  parts.push(b.part(`${prefix}-base`, b.cylinder(radius, 0.45, segments * 2), 0, 0.22, 0, 'concrete'));
  const wallR = radius - 0.2;
  const arc = (TAU * wallR) / segments;
  // 段长与弧长一致，段间不重叠，避免相邻面互闪。
  for (let index = 0; index < segments; index += 1) {
    const angle = (index / segments) * TAU;
    parts.push(b.part(
      `${prefix}-wall-${index + 1}`,
      b.box(arc, depth, 0.4),
      Math.cos(angle) * wallR,
      0.5 + depth / 2,
      Math.sin(angle) * wallR,
      'concrete',
      { rotation: b.rotateY(-angle) },
    ));
  }
  parts.push(b.part(`${prefix}-water`, b.cylinder(radius - 0.4, 0.05, segments * 2), 0, top - 0.5, 0, 'sludge'));

  // 中心搅拌器
  parts.push(b.part(`${prefix}-mixer-column`, b.cylinder(0.5, depth + 1.2, 16), 0, 0.5 + (depth + 1.2) / 2, 0, 'steel'));
  parts.push(b.part(`${prefix}-mixer-drive`, b.cylinder(0.9, 0.8, 18), 0, top + 1.0, 0, 'steel'));
  parts.push(b.part(`${prefix}-mixer-arm-a`, b.box(radius * 1.6, 0.18, 0.3), 0, 1.6, 0, 'steel'));
  parts.push(b.part(`${prefix}-mixer-arm-b`, b.box(0.3, 0.18, radius * 1.6), 0, 1.6, 0, 'steel'));

  // 池顶栏杆
  const railArc = (TAU * radius) / segments;
  for (let index = 0; index < segments; index += 1) {
    const angle = (index / segments) * TAU;
    parts.push(b.part(
      `${prefix}-rail-${index + 1}`,
      b.box(railArc, 0.05, 0.05),
      Math.cos(angle) * radius,
      top + 0.55,
      Math.sin(angle) * radius,
      'steel',
      { rotation: b.rotateY(-angle) },
    ));
    if (index % 3 === 0) {
      parts.push(b.part(
        `${prefix}-post-${index + 1}`,
        b.box(0.06, 1.05, 0.06),
        Math.cos(angle) * radius,
        top + 0.05,
        Math.sin(angle) * radius,
        'steel',
      ));
    }
  }
  return parts;
}

// 污泥消化池：圆柱池体，阶梯收顶，附搅拌与检修平台。
function digester(b, prefix, { radius = 11, height = 17, segments = 28 }) {
  const parts = [];
  parts.push(b.part(`${prefix}-base`, b.cylinder(radius + 0.6, 0.8, segments * 2), 0, 0.4, 0, 'concrete'));
  parts.push(b.part(`${prefix}-shell`, b.cylinder(radius, height, segments * 2), 0, 0.8 + height / 2, 0, 'concrete'));

  // 锥顶用三层递减圆柱近似
  const top = 0.8 + height;
  for (let step = 0; step < 6; step += 1) {
    const scale = 0.9 - step * 0.13;
    parts.push(b.part(
      `${prefix}-dome-${step + 1}`,
      b.cylinder(radius * scale, 0.85, segments * 2),
      0,
      top + 0.42 + step * 0.85,
      0,
      'concrete',
    ));
  }
  parts.push(b.part(`${prefix}-roof-cap`, b.cylinder(1.6, 0.6, 20), 0, top + 5.0, 0, 'steel'));

  // 检修平台与栏杆（沿池体中段）
  const galleryY = top - 2.4;
  const gallerySegments = segments;
  const arc = (TAU * (radius + 0.7)) / gallerySegments;
  for (let index = 0; index < gallerySegments; index += 1) {
    const angle = (index / gallerySegments) * TAU;
    parts.push(b.part(
      `${prefix}-gallery-${index + 1}`,
      b.box(arc + 0.1, 0.16, 1.1),
      Math.cos(angle) * (radius + 0.2),
      galleryY,
      Math.sin(angle) * (radius + 0.2),
      'walkway',
      { rotation: b.rotateY(-angle) },
    ));
    parts.push(b.part(
      `${prefix}-gallery-rail-${index + 1}`,
      b.box(arc + 0.1, 0.07, 0.07),
      Math.cos(angle) * (radius + 0.7),
      galleryY + 0.95,
      Math.sin(angle) * (radius + 0.7),
      'steel',
      { rotation: b.rotateY(-angle) },
    ));
  }

  // 外壁爬梯
  for (let index = 0; index < 16; index += 1) {
    parts.push(b.part(
      `${prefix}-ladder-${index + 1}`,
      b.box(0.42, 0.05, 0.85),
      radius + 0.35,
      1.2 + index * 1.05,
      0,
      'steel',
    ));
  }
  return parts;
}

// 脱水机房：厂房外加三台离心脱水机。
function dewateringHouse(b, prefix, { length = 26, width = 15, height = 8 }) {
  const parts = [];
  parts.push(b.part(`${prefix}-slab`, b.box(length + 1, 0.6, width + 1), 0, 0.3, 0, 'concrete'));
  parts.push(b.part(`${prefix}-shell`, b.box(length, height, width), 0, 0.6 + height / 2, 0, 'facade'));
  parts.push(b.part(`${prefix}-roof`, b.box(length + 1.2, 0.45, width + 1.2), 0, 0.6 + height + 0.22, 0, 'roof'));
  parts.push(b.part(`${prefix}-entrance`, b.box(4, 3.2, 0.14), -length / 4, 2.2, width / 2 + 0.08, 'glass'));
  for (let floor = 0; floor < 2; floor += 1) {
    parts.push(b.part(
      `${prefix}-window-${floor + 1}`,
      b.box(length * 0.6, 1.2, 0.12),
      length / 5,
      2.6 + floor * 2.4,
      width / 2 + 0.06,
      'glass',
    ));
  }

  // 三台卧式离心机：机座 + 转鼓 + 电机
  for (let index = 0; index < 3; index += 1) {
    const x = -length / 3 + index * (length / 3);
    parts.push(b.part(`${prefix}-centrifuge${index + 1}-base`, b.box(3.4, 0.9, 1.8), x, 1.05, 0, 'steel'));
    parts.push(b.part(`${prefix}-centrifuge${index + 1}-drum`, b.cylinder(0.85, 3.2, 18), x, 2.6, 0, 'steel', {
      rotation: [0, 0, Math.SQRT1_2, Math.SQRT1_2],
    }));
    parts.push(b.part(`${prefix}-centrifuge${index + 1}-motor`, b.box(1.2, 1.1, 1.2), x + 2.1, 2.0, 0, 'steel'));
  }
  return parts;
}

// 沼气柜：球形储气罐，坐在四根支柱上。
function gasHolder(b, prefix, { radius = 8.5 }) {
  const parts = [];
  const supportHeight = radius * 0.9;
  parts.push(b.part(`${prefix}-base`, b.cylinder(radius * 0.75, 0.4, 28), 0, 0.2, 0, 'concrete'));
  for (let index = 0; index < 4; index += 1) {
    const angle = (index / 4) * TAU + Math.PI / 4;
    parts.push(b.part(
      `${prefix}-leg-${index + 1}`,
      b.box(0.6, supportHeight, 0.6),
      Math.cos(angle) * radius * 0.55,
      supportHeight / 2,
      Math.sin(angle) * radius * 0.55,
      'steel',
    ));
  }
  parts.push(b.part(
    `${prefix}-sphere`,
    { kind: 'sphere', radius, segments: 28, rings: 18 },
    0,
    supportHeight + radius * 0.75,
    0,
    'roof',
  ));
  return parts;
}

function buildPlan(projectId) {
  const b = createBuilder();
  // 配色取自真实厂区照片：池壁浅米色、水面深蓝绿、屋面蓝灰。
  b.material('concrete', 'Reinforced concrete', [0.84, 0.81, 0.75, 1]);
  b.material('water', 'Process water', [0.12, 0.30, 0.36, 0.95]);
  b.material('steel', 'Galvanised steel', [0.52, 0.545, 0.565, 1]);
  b.material('walkway', 'Grating walkway', [0.42, 0.45, 0.47, 1]);
  b.material('air', 'Air header', [0.3, 0.52, 0.7, 1]);
  b.material('media', 'Filter media', [0.3, 0.28, 0.24, 1]);
  b.material('lamp', 'UV lamp', [0.62, 0.66, 0.86, 1]);
  b.material('facade', 'Facade panel', [0.8, 0.79, 0.75, 1]);
  b.material('roof', 'Metal roof', [0.33, 0.40, 0.48, 1]);
  b.material('lawn', 'Lawn', [0.33, 0.41, 0.24, 1]);
  b.material('asphalt', 'Asphalt road', [0.24, 0.25, 0.27, 1]);
  b.material('glass', 'Glazing', [0.42, 0.52, 0.58, 0.55]);
  b.material('sludge', 'Sludge', [0.28, 0.22, 0.16, 1]);

  // 进水段
  b.define('screening-channel', 'Screening channel', screeningChannel(b, 'gr', { length: 20, width: 6, depth: 2.6 }), [
    b.port('inlet', 'gr-wall-w', -10, 2.0, 0, vec(1, 0, 0), 'raw-water', 1.0),
    b.port('outlet', 'gr-wall-e', 10, 2.0, 0, vec(1, 0, 0), 'screened-water', 1.0),
  ]);
  b.define('pump-house', 'Lift pump house', pumpHouse(b, 'ps', { length: 22, width: 13, height: 8 }), [
    b.port('inlet', 'ps-wall-w', -11, 3.0, 0, vec(1, 0, 0), 'screened-water', 1.0),
    b.port('outlet', 'ps-wall-e', 11, 3.0, 0, vec(1, 0, 0), 'lifted-water', 1.0),
  ]);
  b.define('grit-chamber', 'Aerated grit chamber', gritChamber(b, 'gc', { length: 26, width: 11, depth: 4.2 }), [
    b.port('inlet', 'gc-wall-w', -13, 2.6, 0, vec(1, 0, 0), 'lifted-water', 1.0),
    b.port('outlet', 'gc-wall-e', 13, 2.6, 0, vec(1, 0, 0), 'degritted-water', 1.0),
  ]);

  // 初沉池
  const primaryParts = circularBasin(b, 'pc', { radius: 13, depth: 4.2, segments: 48 });
  b.define('primary-clarifier', 'Primary clarifier', primaryParts, [
    b.port('inlet', 'pc-inlet-well', 0, 5.4, 0, vec(0, -1, 0), 'degritted-water', 1.0),
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
  const secondaryParts = circularBasin(b, 'sc', { radius: 15, depth: 4.6, segments: 52 });
  b.define('secondary-clarifier', 'Secondary clarifier', secondaryParts, [
    b.port('inlet', 'sc-inlet-well', 0, 5.6, 0, vec(0, -1, 0), 'mixed-liquor', 0.9),
    b.port('outlet', 'sc-weir-1', 14.4, 4.9, 0, vec(1, 0, 0), 'secondary-effluent', 0.8),
    b.port('sludge', 'sc-base', 0, 0.3, 0, vec(0, -1, 0), 'waste-sludge', 0.35),
  ]);

  b.define('filter-cell', 'Filtration cell', filterCell(b, 'fl', { length: 32, width: 26, depth: 3.6, cells: 4 }), [
    b.port('inlet', 'fl-wall-w', -16, 2.4, 0, vec(1, 0, 0), 'secondary-effluent', 0.8),
    b.port('outlet', 'fl-wall-e', 16, 2.4, 0, vec(1, 0, 0), 'filtered-water', 0.8),
  ]);
  b.define('disinfection-channel', 'UV disinfection', disinfectionChannel(b, 'uv', { length: 26, width: 5.5, depth: 3 }), [
    b.port('inlet', 'uv-wall-w', -13, 2.2, 0, vec(1, 0, 0), 'filtered-water', 0.8),
    b.port('outlet', 'uv-wall-e', 13, 2.2, 0, vec(1, 0, 0), 'final-effluent', 0.8),
  ]);
  b.define('thickener', 'Sludge thickener', thickener(b, 'th', { radius: 8, depth: 4.4, segments: 40 }), [
    b.port('inlet', 'th-wall-1', 8, 3.0, 0, vec(-1, 0, 0), 'waste-sludge', 0.35),
    b.port('outlet', 'th-mixer-column', 0, 3.6, 0, vec(0, -1, 0), 'thickened-sludge', 0.3),
  ]);
  b.define('digester', 'Anaerobic digester', digester(b, 'dg', { radius: 11, height: 17, segments: 44 }), [
    b.port('inlet', 'dg-shell', 11, 4.0, 0, vec(-1, 0, 0), 'thickened-sludge', 0.3),
    b.port('outlet', 'dg-shell', -11, 4.0, 0, vec(1, 0, 0), 'digested-sludge', 0.3),
  ]);
  b.define('gas-holder', 'Gas holder', gasHolder(b, 'gh', { radius: 8.5 }), []);
  b.define('dewatering-house', 'Dewatering house', dewateringHouse(b, 'dw', { length: 26, width: 15, height: 8 }), [
    b.port('inlet-a', 'dw-shell', -13, 2.0, -3, vec(1, 0, 0), 'digested-sludge', 0.3),
    b.port('inlet-b', 'dw-shell', -13, 2.0, 3, vec(1, 0, 0), 'digested-sludge', 0.3),
  ]);
  b.define('blower-house', 'Blower house', facilityBuilding(b, 'bl', { length: 26, width: 14, floors: 1, floorHeight: 6 }), []);
  b.define('chemical-house', 'Chemical dosing', facilityBuilding(b, 'ch', { length: 20, width: 12, floors: 2, floorHeight: 3.8 }), []);
  b.define('control-building', 'Control building', facilityBuilding(b, 'cb', { length: 30, width: 16, floors: 3, floorHeight: 3.8 }), []);

  // 场地先铺，构筑物叠在上面。范围覆盖整条生产线与污泥区。
  b.ground('场地（草坪）', -150, 440, -110, 130, 'lawn', -0.08);
  // 主干道：沿生产线南北两侧各一条，外加通往污泥区的支路。
  b.ground('道路（北）', -150, 470, -72, -62, 'asphalt', -0.03);
  b.ground('道路（南）', -150, 470, 40, 50, 'asphalt', -0.03);
  b.ground('道路（污泥区支路）', 240, 250, 40, 130, 'asphalt', -0.03);

  // 总平面：两条并联系列共用进水段与深度处理，沿 x 轴自西向东。
  b.instance('screening-channel', 'gr-1', 'GR-101 进水格栅渠', -135, 0, 0, { stage: 'intake', tag: 'GR-101' });
  b.instance('pump-house', 'ps-1', 'PS-101 提升泵房', -105, 0, 0, { stage: 'intake', tag: 'PS-101' });
  b.instance('grit-chamber', 'gc-1', 'GC-101 曝气沉砂池', -70, 0, 0, { stage: 'intake', tag: 'GC-101' });

  const trains = [
    { key: 'a', series: '1', z: 20 },
    { key: 'b', series: '2', z: -20 },
  ];
  for (const train of trains) {
    const { key, series, z } = train;
    b.instance('primary-clarifier', `pc-${key}`, `PC-${series}01 初沉池`, 0, 0, z, {
      stage: 'primary', tag: `PC-${series}01`, train: `Train${series}`,
    });
    b.instance('aeration-feed', `ae-${key}1`, `AE-${series}01 曝气池（厌氧段）`, 45, 0, z, {
      stage: 'biological', tag: `AE-${series}01`, train: `Train${series}`,
    });
    b.instance('aeration-mixed', `ae-${key}2`, `AE-${series}02 曝气池（缺氧段）`, 92, 0, z, {
      stage: 'biological', tag: `AE-${series}02`, train: `Train${series}`,
    });
    b.instance('aeration-mixed', `ae-${key}3`, `AE-${series}03 曝气池（好氧段）`, 139, 0, z, {
      stage: 'biological', tag: `AE-${series}03`, train: `Train${series}`,
    });
    for (let index = 1; index <= 3; index += 1) {
      b.instance('secondary-clarifier', `sc-${key}${index}`,
        `SC-${series}0${index} 二沉池`, 190 + (index - 1) * 45, 0, z, {
          stage: 'secondary', tag: `SC-${series}0${index}`, train: `Train${series}`,
        });
    }
  }

  b.instance('filter-cell', 'fl-1', 'FL-101 滤池', 350, 0, 0, { stage: 'tertiary', tag: 'FL-101' });
  b.instance('disinfection-channel', 'uv-1', 'UV-101 紫外消毒渠', 395, 0, 0, { stage: 'tertiary', tag: 'UV-101' });
  b.instance('thickener', 'th-1', 'TH-101 污泥浓缩池', 250, 0, 62, { stage: 'sludge', tag: 'TH-101' });
  b.instance('thickener', 'th-2', 'TH-102 污泥浓缩池', 288, 0, 62, { stage: 'sludge', tag: 'TH-102' });
  b.instance('digester', 'dg-1', 'DG-101 消化池', 340, 0, 62, { stage: 'sludge', tag: 'DG-101' });
  b.instance('digester', 'dg-2', 'DG-102 消化池', 340, 0, 102, { stage: 'sludge', tag: 'DG-102' });
  b.instance('gas-holder', 'gh-1', 'GH-101 沼气柜', 380, 0, 118, { stage: 'sludge', tag: 'GH-101' });
  b.instance('dewatering-house', 'dw-1', 'DW-101 脱水机房', 405, 0, 78, { stage: 'sludge', tag: 'DW-101' });
  b.instance('control-building', 'cb-1', 'CTRL-101 中控楼', -135, 0, -62, { stage: 'auxiliary', tag: 'CTRL-101' });
  b.instance('blower-house', 'bl-1', 'BLW-101 鼓风机房', 92, 0, -62, { stage: 'auxiliary', tag: 'BLW-101' });
  b.instance('chemical-house', 'ch-1', 'CHEM-101 加药间', 350, 0, -55, { stage: 'auxiliary', tag: 'CHEM-101' });

  b.connect('gr-1', 'outlet', 'ps-1', 'inlet', 'GR-101 → PS-101 原水', 'screened-water', 1.0);
  b.connect('ps-1', 'outlet', 'gc-1', 'inlet', 'PS-101 → GC-101 提升水', 'lifted-water', 1.0);
  for (const train of trains) {
    const { key, series } = train;
    b.connect('gc-1', 'outlet', `pc-${key}`, 'inlet',
      `GC-101 → PC-${series}01 沉砂出水`, 'degritted-water', 1.0);
    b.connect(`pc-${key}`, 'outlet', `ae-${key}1`, 'inlet',
      `PC-${series}01 → AE-${series}01 进水`, 'settled-water', 0.9);
    b.connect(`ae-${key}1`, 'outlet', `ae-${key}2`, 'inlet',
      `AE-${series}01 → AE-${series}02 混合液`, 'mixed-liquor', 0.9);
    b.connect(`ae-${key}2`, 'outlet', `ae-${key}3`, 'inlet',
      `AE-${series}02 → AE-${series}03 混合液`, 'mixed-liquor', 0.9);
    for (let index = 1; index <= 3; index += 1) {
      b.connect(`ae-${key}3`, 'outlet', `sc-${key}${index}`, 'inlet',
        `AE-${series}03 → SC-${series}0${index} 混合液`, 'mixed-liquor', 0.9);
      b.connect(`sc-${key}${index}`, 'outlet', 'fl-1', 'inlet',
        `SC-${series}0${index} → FL-101 二沉出水`, 'secondary-effluent', 0.8);
    }
  }
  b.connect('fl-1', 'outlet', 'uv-1', 'inlet', 'FL-101 → UV-101 滤后水', 'filtered-water', 0.8);

  // 跨池栈桥：连接同系列相邻构筑物，兼作检修通道。
  for (const train of trains) {
    const { key, series, z } = train;
    b.walkway(`ST-${series}01 初沉至生化栈桥`, [-11, z], [-19, z], { y: 5.0 });
    b.walkway(`ST-${series}02 生化一至二栈桥`, [66, z], [71, z], { y: 5.6 });
    b.walkway(`ST-${series}03 生化二至三栈桥`, [113, z], [118, z], { y: 5.6 });
    b.walkway(`ST-${series}04 生化至二沉栈桥`, [160, z], [178, z], { y: 4.6 });
    b.walkway(`ST-${series}05 二沉间栈桥`, [212, z], [223, z], { y: 4.4 });
    b.walkway(`ST-${series}06 二沉间栈桥`, [257, z], [268, z], { y: 4.4 });
  }

  // 污泥线：剩余污泥 → 浓缩 → 消化 → 脱水
  b.connect('sc-a1', 'sludge', 'th-1', 'inlet', 'SC-101 → TH-101 剩余污泥', 'waste-sludge', 0.35);
  b.connect('sc-b1', 'sludge', 'th-2', 'inlet', 'SC-201 → TH-102 剩余污泥', 'waste-sludge', 0.35);
  b.connect('th-1', 'outlet', 'dg-1', 'inlet', 'TH-101 → DG-101 浓缩污泥', 'thickened-sludge', 0.3);
  b.connect('th-2', 'outlet', 'dg-2', 'inlet', 'TH-102 → DG-102 浓缩污泥', 'thickened-sludge', 0.3);
  b.connect('dg-1', 'outlet', 'dw-1', 'inlet-a', 'DG-101 → DW-101 消化污泥', 'digested-sludge', 0.3);
  b.connect('dg-2', 'outlet', 'dw-1', 'inlet-b', 'DG-102 → DW-101 消化污泥', 'digested-sludge', 0.3);

  return {
    projectId,
    operations: b.state.ops,
    summary: {
      components: b.state.components.size,
      instances: 22,
      connections: 24,
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
    ['serve', '--workspace', workspace, '--project', 'bootstrap', '--port', '0', '--write-mode', 'autonomous'],
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
    // 服务启动时会自动建一个占位项目，用完即删，避免污染工程目录。
    await rm(join(workspace, 'bootstrap'), { recursive: true, force: true }).catch(() => {});
  }
}

if (process.argv[1] && import.meta.url === `file://${process.argv[1]}`) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}

export { buildPlan };
