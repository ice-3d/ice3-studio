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

  const instance = (definitionKey, key, name, x, y, z, metadata = {}, parameters = {}, materialKey = null) => {
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
        parameters,
        overrides: entry.overrides,
        metadata,
      },
      material_id: materialKey ? state.materials.get(materialKey) : null,
      metadata,
    });
    return nodeId;
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
        // 真实厂区的主管比图面上的细线粗得多，这里统一放大一档。
        diameter: diameter * 1.8,
        routePolicy: { kind: 'orthogonal' },
        relationId,
        metadata: {},
      },
      material_id: state.materials.get(PIPE_MATERIAL[medium] ?? 'pipe-process'),
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

// 圆形池：底板 + 环形池壁 + 水面 + 中心进水筒 + 周边出水堰 + 刮泥桥 + 栏杆 + 爬梯
// 圆形池：参数化定义。radius / depth / wall 全部走表达式，
// 沿圆周重复的构件由内核按 i / count 展开，改参数即可整体重算。
function circularBasin(b, prefix, { segments = 48 }) {
  const parts = [];
  const ring = 'i * tau / count';

  parts.push(b.part(`${prefix}-base`, b.cylinder({ expr: 'radius' }, 0.5, segments * 2), 0, 0.25, 0, 'concrete'));

  // 池壁：段长等于弧长，位置与朝向按索引算
  parts.push(b.repeatPart(`${prefix}-wall`, segments,
    b.box({ expr: 'tau * (radius - wall / 2) / count' }, { expr: 'depth' }, { expr: 'wall' }),
    {
      transform: {
        translation: {
          x: { expr: `cos(${ring}) * (radius - wall / 2)` },
          y: { expr: '0.5 + depth / 2' },
          z: { expr: `sin(${ring}) * (radius - wall / 2)` },
        },
        yaw: { expr: '-i * 360 / count' },
      },
      role: 'concrete',
    }));

  parts.push(b.part(`${prefix}-water`, b.cylinder({ expr: 'radius - wall' }, 0.06, segments * 2),
    0, { expr: '0.5 + depth - 0.55' }, 0, 'water'));
  parts.push(b.part(`${prefix}-inlet-well`, b.cylinder(1.5, { expr: 'depth + 0.9' }, 32),
    0, { expr: '0.5 + (depth + 0.9) / 2' }, 0, 'concrete'));

  // 出水堰：贴内壁一圈，堰顶高于水面
  parts.push(b.repeatPart(`${prefix}-weir`, segments,
    b.box({ expr: 'tau * (radius - wall - 0.55) / count' }, 0.5, 0.35),
    {
      transform: {
        translation: {
          x: { expr: `cos(${ring}) * (radius - wall - 0.55)` },
          y: { expr: '0.5 + depth - 0.2' },
          z: { expr: `sin(${ring}) * (radius - wall - 0.55)` },
        },
        yaw: { expr: '-i * 360 / count' },
      },
      role: 'concrete',
    }));

  // 池顶压顶：比墙宽的凸边
  parts.push(b.repeatPart(`${prefix}-coping`, segments,
    b.box({ expr: 'tau * (radius + 0.1) / count' }, 0.22, { expr: 'wall + 0.34' }),
    {
      transform: {
        translation: {
          x: { expr: `cos(${ring}) * (radius - wall / 2 + 0.02)` },
          y: { expr: '0.5 + depth + 0.11' },
          z: { expr: `sin(${ring}) * (radius - wall / 2 + 0.02)` },
        },
        yaw: { expr: '-i * 360 / count' },
      },
      role: 'concrete',
    }));

  // 池顶环形走道
  parts.push(b.repeatPart(`${prefix}-deck`, segments,
    b.box({ expr: 'tau * (radius - wall - 0.5) / count' }, 0.1, 0.9),
    {
      transform: {
        translation: {
          x: { expr: `cos(${ring}) * (radius - wall - 0.45)` },
          y: { expr: '0.5 + depth + 0.05' },
          z: { expr: `sin(${ring}) * (radius - wall - 0.45)` },
        },
        yaw: { expr: '-i * 360 / count' },
      },
      role: 'walkway',
    }));

  // 池顶栏杆：横杆沿圆周，立柱每 3 段一根
  parts.push(b.repeatPart(`${prefix}-rail`, segments,
    b.box({ expr: 'tau * radius / count' }, 0.06, 0.06),
    {
      transform: {
        translation: {
          x: { expr: `cos(${ring}) * radius` },
          y: { expr: '0.5 + depth + 1.05' },
          z: { expr: `sin(${ring}) * radius` },
        },
        yaw: { expr: '-i * 360 / count' },
      },
      role: 'steel',
    }));
  parts.push(b.repeatPart(`${prefix}-post`, Math.floor(segments / 3),
    b.box(0.06, 1.1, 0.06),
    {
      transform: {
        translation: {
          x: { expr: 'cos(i * 3 * tau / count) * radius' },
          y: { expr: '0.5 + depth + 0.55' },
          z: { expr: 'sin(i * 3 * tau / count) * radius' },
        },
      },
      role: 'steel',
    }));

  // 外壁爬梯：9 级踏步 + 两根边轨
  parts.push(b.repeatPart(`${prefix}-ladder`, 9,
    b.box(0.9, 0.06, 0.3),
    {
      transform: {
        translation: {
          x: { expr: 'radius + 0.25' },
          y: { expr: '0.6 + i * 0.5' },
          z: 0,
        },
      },
      role: 'steel',
    }));
  parts.push(b.part(`${prefix}-ladder-rail-a`, b.box(0.06, { expr: 'depth + 1.2' }, 0.06),
    { expr: 'radius + 0.25' }, { expr: '(depth + 1.2) / 2' }, -0.42, 'steel'));
  parts.push(b.part(`${prefix}-ladder-rail-b`, b.box(0.06, { expr: 'depth + 1.2' }, 0.06),
    { expr: 'radius + 0.25' }, { expr: '(depth + 1.2) / 2' }, 0.42, 'steel'));

  // 刮泥桥：桁架，跨度随半径
  parts.push(b.part(`${prefix}-bridge-top`, b.box({ expr: 'radius * 2 + 1.2' }, 0.12, 0.16),
    0, { expr: '0.5 + depth + 0.97' }, -0.7, 'steel'));
  parts.push(b.part(`${prefix}-bridge-top-b`, b.box({ expr: 'radius * 2 + 1.2' }, 0.12, 0.16),
    0, { expr: '0.5 + depth + 0.97' }, 0.7, 'steel'));
  parts.push(b.part(`${prefix}-bridge-bottom`, b.box({ expr: 'radius * 2 + 1.2' }, 0.14, 1.5),
    0, { expr: '0.5 + depth + 0.55' }, 0, 'walkway'));
  parts.push(b.repeatPart(`${prefix}-bridge-strut`, 9,
    b.box(0.1, 0.44, 0.12),
    {
      transform: {
        translation: {
          x: { expr: '(i / (count - 1) - 0.5) * (radius * 2 + 1.2)' },
          y: { expr: '0.5 + depth + 0.76' },
          z: -0.7,
        },
      },
      role: 'steel',
    }));
  parts.push(b.part(`${prefix}-drive`, b.cylinder(1.1, 1.0, 24),
    0, { expr: '0.5 + depth + 1.25' }, 0, 'steel'));
  parts.push(b.part(`${prefix}-bridge-rail-a`, b.box({ expr: 'radius * 2 + 1.2' }, 0.08, 0.08),
    0, { expr: '0.5 + depth + 1.55' }, -0.76, 'steel'));
  parts.push(b.part(`${prefix}-bridge-rail-b`, b.box({ expr: 'radius * 2 + 1.2' }, 0.08, 0.08),
    0, { expr: '0.5 + depth + 1.55' }, 0.76, 'steel'));

  return parts;
}

// 矩形曝气池：参数化。length / width / depth / wall 走表达式，
// 导流墙、曝气支管、走道立柱按索引展开。
function aerationBasin(b, prefix, { corridors = 3, airLines = 10, posts = 11 }) {
  const parts = [];
  parts.push(b.part(`${prefix}-base`, b.box({ expr: 'length' }, 0.5, { expr: 'width' }), 0, 0.25, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-n`, b.box({ expr: 'length' }, { expr: 'depth' }, { expr: 'wall' }),
    0, { expr: '0.5 + depth / 2' }, { expr: '-width / 2' }, 'concrete'));
  parts.push(b.part(`${prefix}-wall-s`, b.box({ expr: 'length' }, { expr: 'depth' }, { expr: 'wall' }),
    0, { expr: '0.5 + depth / 2' }, { expr: 'width / 2' }, 'concrete'));

  // 东西端墙：整面一块，用 subtract 挖出进水/出水洞，由内核拆成子盒。
  for (const [side, sign] of [['w', -1], ['e', 1]]) {
    parts.push(b.part(`${prefix}-wall-${side}`,
      b.box({ expr: 'wall' }, { expr: 'depth' }, { expr: 'width' }),
      { expr: `length / 2 * ${sign}` }, { expr: '0.5 + depth / 2' }, 0, 'concrete',
      {
        subtract: [{
          width: { expr: 'wall + 0.4' },
          height: 1.1,
          depth: 1.8,
          x: 0,
          y: { expr: '4 - (0.5 + depth / 2)' },
          z: 0,
        }],
      }));
  }

  parts.push(b.part(`${prefix}-water`, b.box({ expr: 'length - wall * 2' }, 0.06, { expr: 'width - wall * 2' }),
    0, { expr: '0.5 + depth - 0.5' }, 0, 'water'));

  parts.push(b.repeatPart(`${prefix}-baffle`, corridors,
    b.box(0.35, { expr: 'depth - 0.15' }, { expr: 'width - 4' }),
    {
      transform: {
        translation: {
          x: { expr: '(i + 1) * length / (count + 1)' },
          y: { expr: '0.5 + (depth - 0.15) / 2' },
          z: 1.6,
        },
      },
      role: 'concrete',
    }));

  parts.push(b.repeatPart(`${prefix}-air`, airLines,
    b.box(0.18, 0.18, { expr: 'width - 3' }),
    {
      transform: {
        translation: {
          x: { expr: '-length / 2 + 2 + i * (length - 4) / (count - 1)' },
          y: 0.9,
          z: 0,
        },
      },
      role: 'air',
    }));

  parts.push(b.part(`${prefix}-walkway`, b.box({ expr: 'length' }, 0.14, 1.3),
    0, { expr: '0.5 + depth + 0.05' }, { expr: '-width / 2 - 0.7' }, 'walkway'));
  parts.push(b.part(`${prefix}-walkway-rail`, b.box({ expr: 'length' }, 0.07, 0.07),
    0, { expr: '0.5 + depth + 0.95' }, { expr: '-width / 2 - 1.3' }, 'steel'));
  parts.push(b.repeatPart(`${prefix}-walkway-post`, posts,
    b.box(0.06, 0.95, 0.06),
    {
      transform: {
        translation: {
          x: { expr: '-length / 2 + i * length / (count - 1)' },
          y: { expr: '0.5 + depth + 0.5' },
          z: { expr: '-width / 2 - 1.3' },
        },
      },
      role: 'steel',
    }));

  return parts;
}

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
// 滤池：参数化。长度/宽度/深度走表达式，分格与滤料按索引展开。
function filterCell(b, prefix, { cells = 4 }) {
  const parts = [];
  parts.push(b.part(`${prefix}-base`, b.box({ expr: 'length' }, 0.45, { expr: 'width' }), 0, 0.22, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-n`, b.box({ expr: 'length' }, { expr: 'depth' }, 0.45),
    0, { expr: '0.5 + depth / 2' }, { expr: '-width / 2' }, 'concrete'));
  parts.push(b.part(`${prefix}-wall-s`, b.box({ expr: 'length' }, { expr: 'depth' }, 0.45),
    0, { expr: '0.5 + depth / 2' }, { expr: 'width / 2' }, 'concrete'));
  parts.push(b.part(`${prefix}-wall-w`, b.box(0.45, { expr: 'depth' }, { expr: 'width' }),
    { expr: '-length / 2' }, { expr: '0.5 + depth / 2' }, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-e`, b.box(0.45, { expr: 'depth' }, { expr: 'width' }),
    { expr: 'length / 2' }, { expr: '0.5 + depth / 2' }, 0, 'concrete'));

  // 分格墙、滤料层、水面、反冲洗管随格数展开
  parts.push(b.repeatPart(`${prefix}-divider`, cells,
    b.box({ expr: 'length - 0.9' }, { expr: 'depth' }, 0.35),
    {
      transform: {
        translation: {
          x: 0,
          y: { expr: '0.5 + depth / 2' },
          z: { expr: '-width / 2 + i * width / count' },
        },
      },
      role: 'concrete',
      metadata: { skipFirst: 'true' },
    }));
  parts.push(b.repeatPart(`${prefix}-media`, cells,
    b.box({ expr: 'length - 1' }, 1.1, { expr: 'width / count - 0.9' }),
    {
      transform: {
        translation: {
          x: 0,
          y: 1.2,
          z: { expr: '-width / 2 + (i + 0.5) * width / count' },
        },
      },
      role: 'media',
    }));
  parts.push(b.repeatPart(`${prefix}-water`, cells,
    b.box({ expr: 'length - 1' }, 0.05, { expr: 'width / count - 0.9' }),
    {
      transform: {
        translation: {
          x: 0,
          y: { expr: '0.5 + depth - 0.6' },
          z: { expr: '-width / 2 + (i + 0.5) * width / count' },
        },
      },
      role: 'water',
    }));
  parts.push(b.repeatPart(`${prefix}-wash`, cells,
    b.box(0.22, 0.22, { expr: 'width / count - 1.2' }),
    {
      transform: {
        translation: {
          x: 0,
          y: 2.6,
          z: { expr: '-width / 2 + (i + 0.5) * width / count' },
        },
      },
      role: 'air',
    }));

  parts.push(b.part(`${prefix}-walkway`, b.box({ expr: 'length' }, 0.12, 1.2),
    0, { expr: '0.5 + depth + 0.1' }, { expr: '-width / 2 - 0.6' }, 'walkway'));
  parts.push(b.part(`${prefix}-walkway-rail`, b.box({ expr: 'length' }, 0.07, 0.07),
    0, { expr: '0.5 + depth + 0.95' }, { expr: '-width / 2 - 1.15' }, 'steel'));
  parts.push(b.repeatPart(`${prefix}-walkway-post`, 11,
    b.box(0.06, 0.9, 0.06),
    {
      transform: {
        translation: {
          x: { expr: '-length / 2 + i * length / (count - 1)' },
          y: { expr: '0.5 + depth + 0.5' },
          z: { expr: '-width / 2 - 1.15' },
        },
      },
      role: 'steel',
    }));
  return parts;
}
// 消毒渠：参数化。灯架与灯管按索引展开。
function disinfectionChannel(b, prefix, { banks = 3, lamps = 4 }) {
  const parts = [];
  parts.push(b.part(`${prefix}-base`, b.box({ expr: 'length' }, 0.4, { expr: 'width' }), 0, 0.2, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-n`, b.box({ expr: 'length' }, { expr: 'depth' }, 0.4),
    0, { expr: '0.5 + depth / 2' }, { expr: '-width / 2' }, 'concrete'));
  parts.push(b.part(`${prefix}-wall-s`, b.box({ expr: 'length' }, { expr: 'depth' }, 0.4),
    0, { expr: '0.5 + depth / 2' }, { expr: 'width / 2' }, 'concrete'));
  parts.push(b.part(`${prefix}-wall-w`, b.box(0.4, { expr: 'depth' }, { expr: 'width' }),
    { expr: '-length / 2' }, { expr: '0.5 + depth / 2' }, 0, 'concrete'));
  parts.push(b.part(`${prefix}-wall-e`, b.box(0.4, { expr: 'depth' }, { expr: 'width' }),
    { expr: 'length / 2' }, { expr: '0.5 + depth / 2' }, 0, 'concrete'));
  parts.push(b.part(`${prefix}-water`, b.box({ expr: 'length - 0.8' }, 0.05, { expr: 'width - 0.8' }),
    0, { expr: '0.5 + depth - 0.45' }, 0, 'water'));

  parts.push(b.repeatPart(`${prefix}-bank-beam`, banks,
    b.box(0.5, 0.3, { expr: 'width - 0.6' }),
    {
      transform: {
        translation: {
          x: { expr: '-length / 3 + i * length / 3' },
          y: { expr: '0.5 + depth - 0.25' },
          z: 0,
        },
      },
      role: 'steel',
    }));
  parts.push(b.repeatPart(`${prefix}-lamp`, banks * lamps,
    b.cylinder(0.09, { expr: 'depth * 0.5' }, 10),
    {
      transform: {
        translation: {
          x: { expr: '-length / 3 + floor(i / ' + lamps + ') * length / 3' },
          y: { expr: '0.5 + depth - depth * 0.25' },
          z: { expr: '-width / 2 + 0.9 + (i % ' + lamps + ') * (width - 1.8) / ' + (lamps - 1) },
        },
      },
      role: 'lamp',
    }));

  parts.push(b.part(`${prefix}-walkway`, b.box({ expr: 'length' }, 0.12, 1.1),
    0, { expr: '0.5 + depth + 0.1' }, { expr: '-width / 2 - 0.55' }, 'walkway'));
  parts.push(b.part(`${prefix}-rail`, b.box({ expr: 'length' }, 0.07, 0.07),
    0, { expr: '0.5 + depth + 0.95' }, { expr: '-width / 2 - 1.05' }, 'steel'));
  return parts;
}
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
// 污泥浓缩池：参数化。池壁与栏杆按索引展开。
function thickener(b, prefix, { segments = 32 }) {
  const parts = [];
  parts.push(b.part(`${prefix}-base`, b.cylinder({ expr: 'radius' }, 0.45, segments * 2), 0, 0.22, 0, 'concrete'));
  parts.push(b.repeatPart(`${prefix}-wall`, segments,
    b.box({ expr: 'tau * (radius - 0.2) / count' }, { expr: 'depth' }, 0.4),
    {
      transform: {
        translation: {
          x: { expr: 'cos(i * tau / count) * (radius - 0.2)' },
          y: { expr: '0.5 + depth / 2' },
          z: { expr: 'sin(i * tau / count) * (radius - 0.2)' },
        },
        yaw: { expr: '-i * 360 / count' },
      },
      role: 'concrete',
    }));
  parts.push(b.part(`${prefix}-water`, b.cylinder({ expr: 'radius - 0.4' }, 0.05, segments * 2),
    0, { expr: '0.5 + depth - 0.5' }, 0, 'sludge'));

  parts.push(b.part(`${prefix}-mixer-column`, b.cylinder(0.5, { expr: 'depth + 1.2' }, 16),
    0, { expr: '0.5 + (depth + 1.2) / 2' }, 0, 'steel'));
  parts.push(b.part(`${prefix}-mixer-drive`, b.cylinder(0.9, 0.8, 18),
    0, { expr: '0.5 + depth + 1.0' }, 0, 'steel'));
  parts.push(b.part(`${prefix}-mixer-arm-a`, b.box({ expr: 'radius * 1.6' }, 0.18, 0.3), 0, 1.6, 0, 'steel'));
  parts.push(b.part(`${prefix}-mixer-arm-b`, b.box(0.3, 0.18, { expr: 'radius * 1.6' }), 0, 1.6, 0, 'steel'));

  parts.push(b.repeatPart(`${prefix}-rail`, segments,
    b.box({ expr: 'tau * radius / count' }, 0.07, 0.07),
    {
      transform: {
        translation: {
          x: { expr: 'cos(i * tau / count) * radius' },
          y: { expr: '0.5 + depth + 0.6' },
          z: { expr: 'sin(i * tau / count) * radius' },
        },
        yaw: { expr: '-i * 360 / count' },
      },
      role: 'steel',
    }));
  parts.push(b.repeatPart(`${prefix}-post`, Math.floor(segments / 2),
    b.box(0.06, 1.05, 0.06),
    {
      transform: {
        translation: {
          x: { expr: 'cos(i * 2 * tau / count) * radius' },
          y: { expr: '0.5 + depth + 0.1' },
          z: { expr: 'sin(i * 2 * tau / count) * radius' },
        },
      },
      role: 'steel',
    }));
  return parts;
}
function digester(b, prefix, { domeLayers = 6, gallerySegments = 44, rungs = 16 }) {
  const parts = [];
  parts.push(b.part(`${prefix}-base`, b.cylinder({ expr: 'radius + 0.6' }, 0.8, 56), 0, 0.4, 0, 'concrete'));
  parts.push(b.part(`${prefix}-shell`, b.cylinder({ expr: 'radius' }, { expr: 'height' }, 56),
    0, { expr: '0.8 + height / 2' }, 0, 'concrete'));

  // 锥顶：逐层收径的圆柱近似
  parts.push(b.repeatPart(`${prefix}-dome`, domeLayers,
    b.cylinder({ expr: 'radius * (0.9 - i * 0.13)' }, { expr: 'height * 0.05' }, 56),
    {
      transform: {
        translation: {
          x: 0,
          y: { expr: '0.8 + height + height * 0.025 + i * height * 0.05' },
          z: 0,
        },
      },
      role: 'concrete',
    }));
  parts.push(b.part(`${prefix}-roof-cap`, b.cylinder(1.6, 0.6, 20),
    0, { expr: '0.8 + height + height * 0.32' }, 0, 'steel'));

  // 检修平台：沿池体外圈一圈走道与护栏
  parts.push(b.repeatPart(`${prefix}-gallery`, gallerySegments,
    b.box({ expr: 'tau * (radius + 0.7) / count' }, 0.16, 1.1),
    {
      transform: {
        translation: {
          x: { expr: 'cos(i * tau / count) * (radius + 0.2)' },
          y: { expr: '0.8 + height - 2.4' },
          z: { expr: 'sin(i * tau / count) * (radius + 0.2)' },
        },
        yaw: { expr: '-i * 360 / count' },
      },
      role: 'walkway',
    }));
  parts.push(b.repeatPart(`${prefix}-gallery-rail`, gallerySegments,
    b.box({ expr: 'tau * (radius + 0.7) / count' }, 0.07, 0.07),
    {
      transform: {
        translation: {
          x: { expr: 'cos(i * tau / count) * (radius + 0.7)' },
          y: { expr: '0.8 + height - 1.45' },
          z: { expr: 'sin(i * tau / count) * (radius + 0.7)' },
        },
        yaw: { expr: '-i * 360 / count' },
      },
      role: 'steel',
    }));

  // 外壁爬梯
  parts.push(b.repeatPart(`${prefix}-ladder`, rungs,
    b.box(0.42, 0.05, 0.85),
    {
      transform: {
        translation: {
          x: { expr: 'radius + 0.35' },
          y: { expr: '1.2 + i * height / count' },
          z: 0,
        },
      },
      role: 'steel',
    }));

  return parts;
}

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

// 带开孔的墙板：把一整面墙拆成四段，中间留出矩形洞。
// 这是"减一个盒子"的手工等价物，洞口的坐标由调用方给出。
function wallWithOpening(b, prefix, {
  width, depth, wall, along = 'z', centreY = 4, holeWidth = 1.6, holeHeight = 1.1,
}) {
  const lower = centreY - holeHeight / 2;
  const upper = centreY + holeHeight / 2;
  const parts = [];
  // 洞下方整条
  parts.push(b.part(`${prefix}-below`, b.box(along === 'z' ? 0.45 : width, lower, along === 'z' ? width : wall),
    0, lower / 2, 0, 'concrete'));
  // 洞上方整条
  parts.push(b.part(`${prefix}-above`, b.box(along === 'z' ? 0.45 : width, depth - upper, along === 'z' ? width : wall),
    0, upper + (depth - upper) / 2, 0, 'concrete'));
  // 洞两侧各一条
  const side = (width - holeWidth) / 2;
  for (const sign of [-1, 1]) {
    parts.push(b.part(
      `${prefix}-side-${sign > 0 ? 'a' : 'b'}`,
      b.box(along === 'z' ? 0.45 : side, holeHeight, along === 'z' ? side : wall),
      0,
      centreY,
      along === 'z' ? sign * (holeWidth / 2 + side / 2) : 0,
      'concrete',
    ));
  }
  return parts;
}

// 绿化乔木：一根树干加一个球形树冠，尺寸固定，靠位置与数量形成层次。
function tree(b, prefix) {
  return [
    b.part(`${prefix}-trunk`, b.cylinder(0.28, 3.4, 8), 0, 1.7, 0, 'trunk'),
    b.part(`${prefix}-crown`,
      { kind: 'sphere', radius: 2.7, segments: 14, rings: 10 },
      0, 5.4, 0, 'foliage'),
  ];
}

// 厂区围栏：沿边界每 3.5 米一根立柱，两道横杆。
function siteFence(b, prefix, { length = 620, posts = 178 }) {
  const parts = [];
  parts.push(b.repeatPart(`${prefix}-post`, posts,
    b.box(0.12, 2.2, 0.12),
    {
      transform: {
        translation: {
          x: { expr: '-length / 2 + i * length / (count - 1)' },
          y: 1.1,
          z: 0,
        },
      },
      role: 'steel',
    }));
  parts.push(b.part(`${prefix}-rail-high`, b.box({ expr: 'length' }, 0.08, 0.08), 0, 2.0, 0, 'steel'));
  parts.push(b.part(`${prefix}-rail-low`, b.box({ expr: 'length' }, 0.08, 0.08), 0, 1.0, 0, 'steel'));
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
  // 绿化与围栏。必须定义在组件声明之前，否则 define 时取不到材质。
  b.material('trunk', 'Tree trunk', [0.28, 0.2, 0.14, 1]);
  b.material('foliage', 'Foliage', [0.22, 0.38, 0.18, 1]);
  b.material('paving', 'Paving', [0.62, 0.61, 0.58, 1]);
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
  const primaryParts = circularBasin(b, 'pc', { segments: 48 });
  b.define('primary-clarifier', 'Primary clarifier', primaryParts, [
    b.port('inlet', 'pc-inlet-well', 0, 5.4, 0, vec(0, -1, 0), 'degritted-water', 1.0),
    b.port('outlet', 'pc-base', 12.4, 4.5, 0, vec(1, 0, 0), 'settled-water', 0.9),
  ], [
    { name: 'radius', valueType: { kind: 'number', minimum: null, maximum: null }, default: 13, unit: 'm' },
    { name: 'depth', valueType: { kind: 'number', minimum: null, maximum: null }, default: 4.2, unit: 'm' },
    { name: 'wall', valueType: { kind: 'number', minimum: null, maximum: null }, default: 0.45, unit: 'm' },
  ]);

  // 曝气池（推流式廊道）。首段接纳初沉出水，后续段接纳混合液，端口介质不同，
  // 因此按进水介质分成两种定义。
  b.define('aeration-feed', 'Aeration basin (feed)', aerationBasin(b, 'ae', {}), [
    b.port('inlet', 'ae-wall-w', -21, 4.0, 0, vec(1, 0, 0), 'settled-water', 0.9),
    b.port('outlet', 'ae-wall-e', 21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
  ], [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 42, unit: 'm' },
    { name: 'width', valueType: { kind: 'number', minimum: null, maximum: null }, default: 20, unit: 'm' },
    { name: 'depth', valueType: { kind: 'number', minimum: null, maximum: null }, default: 6, unit: 'm' },
    { name: 'wall', valueType: { kind: 'number', minimum: null, maximum: null }, default: 0.45, unit: 'm' },
  ]);
  b.define('aeration-mixed', 'Aeration basin (mixed liquor)', aerationBasin(b, 'ae', {}), [
    b.port('inlet', 'ae-wall-w', -21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
    b.port('outlet', 'ae-wall-e', 21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
  ], [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 42, unit: 'm' },
    { name: 'width', valueType: { kind: 'number', minimum: null, maximum: null }, default: 20, unit: 'm' },
    { name: 'depth', valueType: { kind: 'number', minimum: null, maximum: null }, default: 6, unit: 'm' },
    { name: 'wall', valueType: { kind: 'number', minimum: null, maximum: null }, default: 0.45, unit: 'm' },
  ]);

  // 二沉池（与初沉池同构，尺寸与用途不同）
  const secondaryParts = circularBasin(b, 'sc', { segments: 52 });
  b.define('secondary-clarifier', 'Secondary clarifier', secondaryParts, [
    b.port('inlet', 'sc-inlet-well', 0, 5.6, 0, vec(0, -1, 0), 'mixed-liquor', 0.9),
    b.port('outlet', 'sc-base', 14.4, 4.9, 0, vec(1, 0, 0), 'secondary-effluent', 0.8),
    b.port('sludge', 'sc-base', 0, 0.3, 0, vec(0, -1, 0), 'waste-sludge', 0.35),
  ], [
    { name: 'radius', valueType: { kind: 'number', minimum: null, maximum: null }, default: 15, unit: 'm' },
    { name: 'depth', valueType: { kind: 'number', minimum: null, maximum: null }, default: 4.6, unit: 'm' },
    { name: 'wall', valueType: { kind: 'number', minimum: null, maximum: null }, default: 0.45, unit: 'm' },
  ]);

  b.define('filter-cell', 'Filtration cell', filterCell(b, 'fl', {}), [
    b.port('inlet', 'fl-wall-w', -16, 2.4, 0, vec(1, 0, 0), 'secondary-effluent', 0.8),
    b.port('outlet', 'fl-wall-e', 16, 2.4, 0, vec(1, 0, 0), 'filtered-water', 0.8),
  ], [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 32, unit: 'm' },
    { name: 'width', valueType: { kind: 'number', minimum: null, maximum: null }, default: 26, unit: 'm' },
    { name: 'depth', valueType: { kind: 'number', minimum: null, maximum: null }, default: 3.6, unit: 'm' },
  ]);
  b.define('disinfection-channel', 'UV disinfection', disinfectionChannel(b, 'uv', {}), [
    b.port('inlet', 'uv-wall-w', -13, 2.2, 0, vec(1, 0, 0), 'filtered-water', 0.8),
    b.port('outlet', 'uv-wall-e', 13, 2.2, 0, vec(1, 0, 0), 'final-effluent', 0.8),
  ], [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 26, unit: 'm' },
    { name: 'width', valueType: { kind: 'number', minimum: null, maximum: null }, default: 5.5, unit: 'm' },
    { name: 'depth', valueType: { kind: 'number', minimum: null, maximum: null }, default: 3, unit: 'm' },
  ]);
  b.define('thickener', 'Sludge thickener', thickener(b, 'th', {}), [
    b.port('inlet', 'th-base', 8, 3.0, 0, vec(-1, 0, 0), 'waste-sludge', 0.35),
    b.port('outlet', 'th-mixer-column', 0, 3.6, 0, vec(0, -1, 0), 'thickened-sludge', 0.3),
  ], [
    { name: 'radius', valueType: { kind: 'number', minimum: null, maximum: null }, default: 8, unit: 'm' },
    { name: 'depth', valueType: { kind: 'number', minimum: null, maximum: null }, default: 4.4, unit: 'm' },
  ]);
  b.define('digester', 'Anaerobic digester', digester(b, 'dg', {}), [
    b.port('inlet', 'dg-shell', 11, 4.0, 0, vec(-1, 0, 0), 'thickened-sludge', 0.3),
    b.port('outlet', 'dg-shell', -11, 4.0, 0, vec(1, 0, 0), 'digested-sludge', 0.3),
  ], [
    { name: 'radius', valueType: { kind: 'number', minimum: null, maximum: null }, default: 11, unit: 'm' },
    { name: 'height', valueType: { kind: 'number', minimum: null, maximum: null }, default: 17, unit: 'm' },
  ]);
  b.define('gas-holder', 'Gas holder', gasHolder(b, 'gh', { radius: 8.5 }), []);
  b.define('dewatering-house', 'Dewatering house', dewateringHouse(b, 'dw', { length: 26, width: 15, height: 8 }), [
    b.port('inlet-a', 'dw-shell', -13, 2.0, -3, vec(1, 0, 0), 'digested-sludge', 0.3),
    b.port('inlet-b', 'dw-shell', -13, 2.0, 3, vec(1, 0, 0), 'digested-sludge', 0.3),
  ]);
  b.define('tree', 'Tree', tree(b, 'tr'), []);
  b.define('site-fence', 'Site fence', siteFence(b, 'sf', {}), [], [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 620, unit: 'm' },
  ]);
  b.define('blower-house', 'Blower house', facilityBuilding(b, 'bl', { length: 26, width: 14, floors: 1, floorHeight: 6 }), []);
  b.define('chemical-house', 'Chemical dosing', facilityBuilding(b, 'ch', { length: 20, width: 12, floors: 2, floorHeight: 3.8 }), []);
  b.define('control-building', 'Control building', facilityBuilding(b, 'cb', { length: 30, width: 16, floors: 3, floorHeight: 3.8 }), []);

  // 场地：按功能分区铺草坪，四周留出环形主干道。
  b.ground('场地（草坪）', -10, 610, -185, 185, 'lawn', -0.08);
  // 环形主干道：南北两条沿厂界，东西两端各一条。
  b.ground('道路（北）', -10, 620, -180, -168, 'asphalt', -0.03);
  b.ground('道路（南）', -10, 620, 168, 180, 'asphalt', -0.03);
  b.ground('道路（西）', -10, 2, -185, 185, 'asphalt', -0.03);
  b.ground('道路（东）', 600, 612, -185, 185, 'asphalt', -0.03);
  // 横向支路：分隔预处理 / 生化 / 二沉三个区块。
  b.ground('道路（支路一）', 120, 130, -160, 160, 'asphalt', -0.03);
  b.ground('道路（支路二）', 355, 365, -160, 160, 'asphalt', -0.03);
  // 道路标线：两侧各一条白线。
  b.material('marking', 'Road marking', [0.86, 0.86, 0.84, 1]);
  // 接触阴影：贴在构筑物脚下的深色薄片，替代真正的阴影贴图。
  b.material('shadow', 'Contact shadow', [0.04, 0.05, 0.06, 0.30]);
  // 管道按介质分色：原水灰、工艺水黄褐、清水绿、污泥深褐、空气蓝。
  b.material('pipe-raw', 'Pipe / raw water', [0.52, 0.53, 0.55, 1]);
  b.material('pipe-process', 'Pipe / process water', [0.66, 0.50, 0.26, 1]);
  b.material('pipe-clean', 'Pipe / clean water', [0.28, 0.55, 0.36, 1]);
  b.material('pipe-sludge', 'Pipe / sludge', [0.36, 0.24, 0.16, 1]);
  b.material('pipe-air', 'Pipe / air', [0.26, 0.48, 0.76, 1]);

  for (const edgeZ of [-179, -169, 169, 179]) {
    b.ground(`标线 z=${edgeZ}`, -10, 610, edgeZ - 0.15, edgeZ + 0.15, 'marking', -0.018);
  }

  // 总平面：按功能分区，成组紧凑布置。
  //
  // 对照真实厂区航拍：预处理在西端，初沉/生化/二沉沿流向自西向东成组排列，
  // 两条并联系列南北相对，污泥区独立在北侧，办公与辅助靠南侧入口，
  // 各区块之间用环形主干道与绿化带分隔。
  //
  // 预处理区（x 30–100）
  b.instance('screening-channel', 'gr-1', 'GR-101 进水格栅渠', 30, 0, 0, { stage: 'intake', tag: 'GR-101' });
  b.instance('pump-house', 'ps-1', 'PS-101 提升泵房', 62, 0, 0, { stage: 'intake', tag: 'PS-101' });
  b.instance('grit-chamber', 'gc-1', 'GC-101 曝气沉砂池', 95, 0, 0, { stage: 'intake', tag: 'GC-101' });

  // 两条系列刻意不完全对称：z 间距不同、x 略有偏移，像分期建设留下的错位。
  const trains = [
    { key: 'a', series: '1', z: 40, dx: 0 },
    { key: 'b', series: '2', z: -46, dx: 6 },
  ];

  // 初沉区（x 150）：两座并排，横向间距 80 米（一个池径加通道）
  for (const train of trains) {
    const { key, series, z, dx } = train;
    b.instance('primary-clarifier', `pc-${key}`, `PC-${series}01 初沉池`, 150 + dx, 0, z, {
      stage: 'primary', tag: `PC-${series}01`, train: `Train${series}`,
    }, { radius: 13, depth: 4.2, wall: 0.45 });

    // 生化区（x 215–340）：三段推流廊道首尾相接，段间留 13 米管廊
    b.instance('aeration-feed', `ae-${key}1`, `AE-${series}01 曝气池（厌氧段）`, 215 + dx, 0, z, {
      stage: 'biological', tag: `AE-${series}01`, train: `Train${series}`,
    }, { length: 42, width: 20, depth: 6, wall: 0.45 });
    b.instance('aeration-mixed', `ae-${key}2`, `AE-${series}02 曝气池（缺氧段）`, 270 + dx, 0, z, {
      stage: 'biological', tag: `AE-${series}02`, train: `Train${series}`,
    }, { length: 42, width: 20, depth: 6, wall: 0.45 });
    b.instance('aeration-mixed', `ae-${key}3`, `AE-${series}03 曝气池（好氧段）`, 325 + dx, 0, z, {
      stage: 'biological', tag: `AE-${series}03`, train: `Train${series}`,
    }, { length: 42, width: 20, depth: 6, wall: 0.45 });

    // 二沉区（x 380–470）：三座一排，间距 45 米（直径 30 加通道）
    for (let index = 1; index <= 3; index += 1) {
      b.instance('secondary-clarifier', `sc-${key}${index}`,
        `SC-${series}0${index} 二沉池`, 375 + dx + (index - 1) * 45, 0, z, {
          stage: 'secondary', tag: `SC-${series}0${index}`, train: `Train${series}`,
        }, { radius: 15, depth: 4.6, wall: 0.45 });
    }
  }

  // 深度处理区（东端）
  b.instance('filter-cell', 'fl-1', 'FL-101 滤池', 520, 0, 0, { stage: 'tertiary', tag: 'FL-101' },
    { length: 32, width: 26, depth: 3.6 });
  b.instance('disinfection-channel', 'uv-1', 'UV-101 紫外消毒渠', 560, 0, 0, { stage: 'tertiary', tag: 'UV-101' },
    { length: 26, width: 5.5, depth: 3 });

  // 污泥区（北侧独立成角，与生产区隔一条绿化带）
  b.instance('thickener', 'th-1', 'TH-101 污泥浓缩池', 230, 0, -145, { stage: 'sludge', tag: 'TH-101' },
    { radius: 8, depth: 4.4 });
  b.instance('thickener', 'th-2', 'TH-102 污泥浓缩池', 262, 0, -145, { stage: 'sludge', tag: 'TH-102' },
    { radius: 8, depth: 4.4 });
  b.instance('digester', 'dg-1', 'DG-101 消化池', 320, 0, -145, { stage: 'sludge', tag: 'DG-101' },
    { radius: 11, height: 17 });
  b.instance('digester', 'dg-2', 'DG-102 消化池', 320, 0, -105, { stage: 'sludge', tag: 'DG-102' },
    { radius: 11, height: 17 });
  b.instance('gas-holder', 'gh-1', 'GH-101 沼气柜', 372, 0, -125, { stage: 'sludge', tag: 'GH-101' });
  b.instance('dewatering-house', 'dw-1', 'DW-101 脱水机房', 430, 0, -145, { stage: 'sludge', tag: 'DW-101' });

  // 辅助区（南侧靠入口）
  b.instance('control-building', 'cb-1', 'CTRL-101 中控楼', 55, 0, 150, { stage: 'auxiliary', tag: 'CTRL-101' });
  b.instance('blower-house', 'bl-1', 'BLW-101 鼓风机房', 270, 0, 150, { stage: 'auxiliary', tag: 'BLW-101' });
  b.instance('chemical-house', 'ch-1', 'CHEM-101 加药间', 440, 0, 150, { stage: 'auxiliary', tag: 'CHEM-101' });

  // 水系连接
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

  // 污泥线
  b.connect('sc-a1', 'sludge', 'th-1', 'inlet', 'SC-101 → TH-101 剩余污泥', 'waste-sludge', 0.35);
  b.connect('sc-b1', 'sludge', 'th-2', 'inlet', 'SC-201 → TH-102 剩余污泥', 'waste-sludge', 0.35);
  b.connect('th-1', 'outlet', 'dg-1', 'inlet', 'TH-101 → DG-101 浓缩污泥', 'thickened-sludge', 0.3);
  b.connect('th-2', 'outlet', 'dg-2', 'inlet', 'TH-102 → DG-102 浓缩污泥', 'thickened-sludge', 0.3);
  b.connect('dg-1', 'outlet', 'dw-1', 'inlet-a', 'DG-101 → DW-101 消化污泥', 'digested-sludge', 0.3);
  b.connect('dg-2', 'outlet', 'dw-1', 'inlet-b', 'DG-102 → DW-101 消化污泥', 'digested-sludge', 0.3);


  // 接触阴影：跟随构筑物位置（含系列偏移）
  for (const train of trains) {
    const { series, z, dx } = train;
    b.shadowPatch(`SH-PC${series}`, 150 + dx + 0.9, z + 0.8, 29, 29, true);
    for (let index = 1; index <= 3; index += 1) {
      b.shadowPatch(`SH-AE${series}0${index}`, 215 + dx + (index - 1) * 55 + 0.9, z + 0.8, 45, 23);
      b.shadowPatch(`SH-SC${series}0${index}`, 375 + dx + (index - 1) * 45 + 0.9, z + 0.8, 33, 33, true);
    }
  }
  b.shadowPatch('SH-GR', 30, 0, 23, 9);
  b.shadowPatch('SH-PS', 62, 0, 25, 16);
  b.shadowPatch('SH-GC', 95, 0, 29, 14);
  b.shadowPatch('SH-FL', 520, 0, 36, 30);
  b.shadowPatch('SH-UV', 560, 0, 29, 9);
  b.shadowPatch('SH-TH1', 230, -145, 19, 19, true);
  b.shadowPatch('SH-TH2', 262, -145, 19, 19, true);
  b.shadowPatch('SH-DG1', 320, -145, 26, 26, true);
  b.shadowPatch('SH-DG2', 320, -105, 26, 26, true);
  b.shadowPatch('SH-DW', 430, -145, 29, 18);
  b.shadowPatch('SH-GH', 372, -125, 20, 20, true);
  b.shadowPatch('SH-CB', 55, 150, 33, 19);
  b.shadowPatch('SH-BLW', 270, 150, 29, 17);
  b.shadowPatch('SH-CH', 440, 150, 23, 15);

  // 跨池栈桥
  for (const train of trains) {
    const { series, z, dx } = train;
    b.walkway(`ST-${series}01 初沉至生化栈桥`, [172 + dx, z], [189 + dx, z], { y: 5.0 });
    b.walkway(`ST-${series}02 生化一至二栈桥`, [238 + dx, z], [246 + dx, z], { y: 5.6 });
    b.walkway(`ST-${series}03 生化二至三栈桥`, [293 + dx, z], [301 + dx, z], { y: 5.6 });
    b.walkway(`ST-${series}04 生化至二沉栈桥`, [348 + dx, z], [358 + dx, z], { y: 4.6 });
    b.walkway(`ST-${series}05 二沉间栈桥`, [394 + dx, z], [401 + dx, z], { y: 4.4 });
    b.walkway(`ST-${series}06 二沉间栈桥`, [439 + dx, z], [446 + dx, z], { y: 4.4 });
  }

  // 围栏：南北沿厂界
  b.instance('site-fence', 'fence-n', '围栏 / 北界', 300, 0, -176, { role: 'landscape' }, { length: 620 });
  b.instance('site-fence', 'fence-s', '围栏 / 南界', 300, 0, 176, { role: 'landscape' }, { length: 620 });

  // 绿化：成排乔木，位置带确定性抖动
  const jitter = (index, salt) => {
    const raw = Math.abs(Math.sin(index * 12.9898 + salt) * 43758.5453) % 1;
    return raw - 0.5;
  };
  for (let index = 0; index < 26; index += 1) {
    const x = 12 + index * 23 + jitter(index, 0.7) * 5;
    b.instance('tree', `tree-s-a-${index}`, `绿化 / 南排 ${index + 1}`, x, 0, 156 + jitter(index, 2.1) * 5, { role: 'landscape' }, {}, 'foliage');
    b.instance('tree', `tree-s-b-${index}`, `绿化 / 南外排 ${index + 1}`, x + 8, 0, 172 + jitter(index, 3.3) * 4, { role: 'landscape' }, {}, 'foliage');
    b.instance('tree', `tree-n-a-${index}`, `绿化 / 北排 ${index + 1}`, x + 4, 0, -158 + jitter(index, 4.7) * 5, { role: 'landscape' }, {}, 'foliage');
  }
  for (let index = 0; index < 8; index += 1) {
    const z = -120 + index * 32 + jitter(index, 5.9) * 6;
    b.instance('tree', `tree-mid-${index}`, `绿化 / 中带 ${index + 1}`, 122 + jitter(index, 6.1) * 6, 0, z, { role: 'landscape' }, {}, 'foliage');
    b.instance('tree', `tree-mid2-${index}`, `绿化 / 中带二 ${index + 1}`, 358 + jitter(index, 7.3) * 6, 0, z, { role: 'landscape' }, {}, 'foliage');
  }

  // 池边设备
  const equipmentNode = (name, x, z) => {
    b.equipment('control-box', name, x, 0, z, { tag: name.split(' ')[0] });
  };
  for (const train of trains) {
    for (let index = 1; index <= 3; index += 1) {
      equipmentNode(`EQ-SC${train.series}0${index} 控制箱`, 375 + train.dx + (index - 1) * 45 + 16, train.z - 18);
    }
    equipmentNode(`EQ-AE${train.series} 控制箱`, 270 + train.dx, train.z - 16);
    equipmentNode(`EQ-PC${train.series} 控制箱`, 150 + train.dx, train.z + 16);
  }
  equipmentNode('EQ-FL 控制箱', 520, -20);
  equipmentNode('EQ-GC 控制箱', 95, -14);


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

    // 同名工程已存在时先移除，避免重复生成时卡在 project already exists。
    if (explicitWorkspace) {
      await rm(join(workspace, plan.projectId), { recursive: true, force: true }).catch(() => {});
    }
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
