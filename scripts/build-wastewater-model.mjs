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

  const instance = (definitionKey, key, name, x, y, z, metadata = {}, parameters = {}, yaw = 0) => {
    const entry = state.components.get(definitionKey);
    if (!entry) throw new Error(`unknown component ${definitionKey}`);
    const nodeId = id('node');
    state.nodes.set(key, nodeId);
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
        metadata,
      },
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
        routePolicy: ROUTE_POLICY[medium] ?? { kind: 'orthogonal' },
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

// 绿化乔木：树干 + 一根侧枝 + 三团错开的树冠。
// 单团球的轮廓太像棒棒糖，三团互相咬合才有真实树冠的不规则外形；
// 三团都是同一部件的三个副本，靠位移错开，不额外增加材质。
function tree(b, prefix) {
  return [
    b.part(`${prefix}-trunk`, b.cylinder(0.26, 3.8, 8), 0, 1.9, 0, 'trunk'),
    b.part(`${prefix}-branch`, b.cylinder(0.11, 1.5, 6), 0.55, 4.1, 0.25, 'trunk',
      { rotation: [0, 0, -0.5, 0.866] }),
    b.part(`${prefix}-crown-low`, sphere(2.3, 10, 6), 0, 5.0, 0, 'foliage'),
    b.part(`${prefix}-crown-mid`, sphere(2.0, 10, 6), 1.0, 6.1, -0.45, 'foliage'),
    b.part(`${prefix}-crown-high`, sphere(1.6, 10, 6), -0.75, 6.7, 0.55, 'foliage'),
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

// 电动伸缩门：两根贴石材的门柱 + 竖向格栅门扇 + 上下横杆。
// 门柱按真实做法做柱帽与柱础，门扇用 repeat 展开，改开口宽度即可重算。
function entranceGate(b, prefix) {
  const parts = [];
  for (const sign of [-1, 1]) {
    const tag = sign > 0 ? 'e' : 'w';
    parts.push(b.part(`${prefix}-plinth-${tag}`, b.box(1.55, 0.45, 1.55), sign * 8, 0.225, 0, 'concrete'));
    parts.push(b.part(`${prefix}-pier-${tag}`, b.box(1.2, 3.6, 1.2), sign * 8, 2.0, 0, 'concrete'));
    parts.push(b.part(`${prefix}-cap-${tag}`, b.box(1.5, 0.26, 1.5), sign * 8, 3.93, 0, 'concrete'));
  }
  parts.push(b.repeatPart(`${prefix}-bar`, 30, b.box(0.07, 2.0, 0.07), {
    transform: {
      translation: {
        x: { expr: '-6.6 + i * 13.2 / (count - 1)' },
        y: 1.4,
        z: 0,
      },
    },
    role: 'steel',
  }));
  parts.push(b.part(`${prefix}-rail-top`, b.box(13.4, 0.1, 0.1), 0, 2.42, 0, 'steel'));
  parts.push(b.part(`${prefix}-rail-mid`, b.box(13.4, 0.08, 0.08), 0, 1.4, 0, 'steel'));
  parts.push(b.part(`${prefix}-rail-bot`, b.box(13.4, 0.1, 0.1), 0, 0.42, 0, 'steel'));
  return parts;
}

// 门卫房：单层小房 + 雨篷 + 两面带窗 + 侧门。
function gateHouse(b, prefix) {
  return [
    b.part(`${prefix}-base`, b.box(4.8, 0.3, 4.0), 0, 0.15, 0, 'concrete'),
    b.part(`${prefix}-body`, b.box(4.4, 3.0, 3.6), 0, 1.8, 0, 'facade'),
    b.part(`${prefix}-roof`, b.box(5.1, 0.24, 4.3), 0, 3.42, 0, 'roof'),
    b.part(`${prefix}-canopy`, b.box(5.6, 0.18, 1.9), 0, 3.16, -2.7, 'roof'),
    b.part(`${prefix}-canopy-post-a`, b.cylinder(0.09, 3.1, 8), -2.4, 1.55, -3.5, 'steel'),
    b.part(`${prefix}-canopy-post-b`, b.cylinder(0.09, 3.1, 8), 2.4, 1.55, -3.5, 'steel'),
    // 玻璃略微凸出墙面 4 厘米，避免和墙体共面闪烁。
    b.part(`${prefix}-glass-a`, b.box(3.6, 1.2, 0.06), 0, 2.0, -1.82, 'glass'),
    b.part(`${prefix}-glass-b`, b.box(3.6, 1.2, 0.06), 0, 2.0, 1.82, 'glass'),
    b.part(`${prefix}-sill-a`, b.box(3.7, 0.1, 0.16), 0, 1.35, -1.86, 'concrete'),
    b.part(`${prefix}-sill-b`, b.box(3.7, 0.1, 0.16), 0, 1.35, 1.86, 'concrete'),
    b.part(`${prefix}-door`, b.box(0.06, 2.1, 1.0), 2.24, 1.35, 0, 'steel'),
  ];
}

// 厂区铭牌：矮墙 + 深色石板，实际项目里这里是厂名与标识。
function signWall(b, prefix) {
  return [
    b.part(`${prefix}-plinth`, b.box(9.4, 0.35, 1.0), 0, 0.175, 0, 'concrete'),
    b.part(`${prefix}-body`, b.box(9.0, 2.3, 0.7), 0, 1.35, 0, 'concrete'),
    b.part(`${prefix}-panel`, b.box(7.8, 1.5, 0.08), 0, 1.55, -0.36, 'sign-panel'),
    b.part(`${prefix}-coping`, b.box(9.3, 0.14, 0.9), 0, 2.56, 0, 'concrete'),
  ];
}

// 综合管廊：立柱 + 两层横梁 + 一根走道 + 五根按介质分色的主管。
// 主管高度用表达式挂在 height 参数上，改层高时管子跟着走。
function pipeRack(b, prefix, { bays = 38, pipes = true } = {}) {
  const parts = [];
  parts.push(b.repeatPart(`${prefix}-col`, bays + 1, b.box(0.32, { expr: 'height' }, 0.32), {
    transform: {
      translation: {
        x: { expr: '-length / 2 + i * length / (count - 1)' },
        y: { expr: 'height / 2' },
        z: 0,
      },
    },
    role: 'steel',
  }));
  parts.push(b.repeatPart(`${prefix}-beam-top`, bays + 1, b.box(0.2, 0.3, 3.8), {
    transform: {
      translation: {
        x: { expr: '-length / 2 + i * length / (count - 1)' },
        y: { expr: 'height - 0.15' },
        z: 0,
      },
    },
    role: 'steel',
  }));
  parts.push(b.repeatPart(`${prefix}-beam-low`, bays + 1, b.box(0.18, 0.24, 3.0), {
    transform: {
      translation: {
        x: { expr: '-length / 2 + i * length / (count - 1)' },
        y: { expr: 'height * 0.6' },
        z: 0,
      },
    },
    role: 'steel',
  }));
  // 纵梁：两根通长，托住上层管道。
  for (const sign of [-1, 1]) {
    parts.push(b.part(`${prefix}-stringer-${sign > 0 ? 'e' : 'w'}`,
      b.box({ expr: 'length' }, 0.22, 0.2), 0, { expr: 'height - 0.2' }, sign * 1.6, 'steel'));
  }
  // 检修走道与栏杆，挂在北侧。
  parts.push(b.part(`${prefix}-deck`, b.box({ expr: 'length' }, 0.08, 1.0), 0, { expr: 'height - 0.34' }, 2.75, 'walkway'));
  parts.push(b.part(`${prefix}-deck-rail`, b.box({ expr: 'length' }, 0.06, 0.06), 0, { expr: 'height + 0.66' }, 3.2, 'steel'));
  if (pipes) {
    // 管道：圆柱默认沿 Y，绕 Z 转 -90° 躺平到 X 方向。
    const rack = [
      ['raw', 0.30, 'pipe-raw', 'height + 0.18', -1.05],
      ['process', 0.26, 'pipe-process', 'height + 0.16', -0.1],
      ['air', 0.22, 'pipe-air', 'height + 0.15', 0.8],
      ['clean', 0.24, 'pipe-clean', 'height * 0.6 + 0.36', -0.55],
      ['sludge', 0.20, 'pipe-sludge', 'height * 0.6 + 0.34', 0.62],
    ];
    for (const [key, radius, role, y, z] of rack) {
      parts.push(b.part(`${prefix}-pipe-${key}`, b.cylinder(radius, { expr: 'length' }, 12),
        0, { expr: y }, z, role, { rotation: LIE_ALONG_X }));
    }
  }
  return parts;
}

// 修剪绿篱：一道矮墙加一条略窄的压顶，沿路布置形成连续的绿带。
function hedgeRow(b, prefix) {
  return [
    b.part(`${prefix}-body`, b.box({ expr: 'length' }, 0.95, 0.9), 0, 0.475, 0, 'hedge'),
    b.part(`${prefix}-top`, b.box({ expr: 'length' }, 0.18, 0.72), 0, 1.02, 0, 'hedge'),
  ];
}

// 路灯：柱础 + 灯杆 + 悬臂 + 灯头。臂长 1.6 米，实例用 yaw 摆向路心。
function lampPost(b, prefix) {
  return [
    b.part(`${prefix}-base`, b.cylinder(0.3, 0.55, 10), 0, 0.275, 0, 'concrete'),
    b.part(`${prefix}-pole`, b.cylinder(0.1, 8.2, 10), 0, 4.55, 0, 'steel'),
    b.part(`${prefix}-arm`, b.box(1.7, 0.09, 0.09), 0.8, 8.6, 0, 'steel'),
    b.part(`${prefix}-head`, b.box(0.72, 0.18, 0.34), 1.55, 8.5, 0, 'lamp'),
  ];
}

// 停车场：只画车位分隔线，铺装由场地层负责。行方向沿 X，线垂直于行。
function parkingBay(b, prefix, { stalls = 12, depth = 5.2 } = {}) {
  return [
    b.repeatPart(`${prefix}-line`, stalls + 1, b.box(0.12, 0.02, depth), {
      transform: {
        translation: {
          x: { expr: '-length / 2 + i * length / (count - 1)' },
          y: 0.005,
          z: 0,
        },
      },
      role: 'marking',
    }),
  ];
}

// 出水口：混凝土端墙 + 八字翼墙 + 出水明渠 + 水面。
function outfall(b, prefix) {
  return [
    b.part(`${prefix}-headwall`, b.box(7.0, 2.4, 1.0), 0, 1.2, 0, 'concrete'),
    b.part(`${prefix}-coping`, b.box(7.4, 0.2, 1.2), 0, 2.5, 0, 'concrete'),
    b.part(`${prefix}-floor`, b.box(7.0, 0.4, 9.0), 0, 0.2, 4.7, 'concrete'),
    b.part(`${prefix}-wing-n`, b.box(0.35, 1.9, 9.0), 3.5, 1.15, 4.7, 'concrete', { rotation: rotateY(0) }),
    b.part(`${prefix}-wing-s`, b.box(0.35, 1.9, 9.0), -3.5, 1.15, 4.7, 'concrete'),
    b.part(`${prefix}-water`, b.box(6.6, 0.06, 8.8), 0, 0.55, 4.7, 'water'),
    b.part(`${prefix}-grille-a`, b.cylinder(0.12, 1.8, 8), 2.6, 1.4, 0.55, 'steel'),
    b.part(`${prefix}-grille-b`, b.cylinder(0.12, 1.8, 8), -2.6, 1.4, 0.55, 'steel'),
  ];
}

// 事故池：矩形敞口池，只在来水超标或检修时蓄水。构造比工艺池简单，
// 但压顶、栏杆、爬梯一样不能少，否则在大场景里会显得像一块薄板。
function emergencyBasin(b, prefix, { length = 44, width = 26, depth = 5.2, wall = 0.5 }) {
  const parts = [];
  parts.push(b.part(`${prefix}-base`, b.box(length, 0.6, width), 0, 0.3, 0, 'concrete'));
  parts.push(b.part(`${prefix}-water`, b.box(length - wall * 2, 0.06, width - wall * 2),
    0, 0.6 + depth - 0.8, 0, 'water'));
  for (const [tag, sx, sz, w, d] of [
    ['n', 0, -(width / 2 - wall / 2), length, wall],
    ['s', 0, width / 2 - wall / 2, length, wall],
    ['w', -(length / 2 - wall / 2), 0, wall, width - wall * 2],
    ['e', length / 2 - wall / 2, 0, wall, width - wall * 2],
  ]) {
    parts.push(b.part(`${prefix}-wall-${tag}`, b.box(w, depth + 0.6, d),
      sx, 0.6 + (depth + 0.6) / 2, sz, 'concrete'));
  }
  // 压顶与栏杆：沿四条边各一圈。
  parts.push(b.repeatPart(`${prefix}-rail-n`, 22, b.box({ expr: 'length / 22' }, 0.06, 0.06), {
    transform: {
      translation: {
        x: { expr: '-length / 2 + i * length / 21 + length / 44' },
        y: { expr: '0.6 + depth + 1.65' },
        z: { expr: '-width / 2' },
      },
    },
    role: 'steel',
  }));
  parts.push(b.repeatPart(`${prefix}-rail-s`, 22, b.box({ expr: 'length / 22' }, 0.06, 0.06), {
    transform: {
      translation: {
        x: { expr: '-length / 2 + i * length / 21 + length / 44' },
        y: { expr: '0.6 + depth + 1.65' },
        z: { expr: 'width / 2' },
      },
    },
    role: 'steel',
  }));
  parts.push(b.part(`${prefix}-coping-n`, b.box({ expr: 'length' }, 0.22, 0.65), 0, { expr: '0.6 + depth + 0.71' }, { expr: '-width / 2' }, 'concrete'));
  parts.push(b.part(`${prefix}-coping-s`, b.box({ expr: 'length' }, 0.22, 0.65), 0, { expr: '0.6 + depth + 0.71' }, { expr: 'width / 2' }, 'concrete'));
  return parts;
}

// 生物除臭：一间矮厂房 + 两根排气筒 + 进风管，用来处理格栅间与污泥区的臭气。
function odorControl(b, prefix) {
  const parts = [
    b.part(`${prefix}-slab`, b.box(24, 0.5, 14), 0, 0.25, 0, 'concrete'),
    b.part(`${prefix}-shell`, b.box(23, 6.2, 13), 0, 3.6, 0, 'facade'),
    b.part(`${prefix}-roof`, b.box(24, 0.4, 14), 0, 6.9, 0, 'roof'),
    b.part(`${prefix}-door`, b.box(3.2, 3.4, 0.14), -6, 2.2, 6.55, 'steel'),
    b.part(`${prefix}-louver`, b.box(6, 2.2, 0.14), 6, 2.6, 6.55, 'glass'),
  ];
  for (const sign of [-1, 1]) {
    parts.push(b.part(`${prefix}-stack-${sign > 0 ? 'e' : 'w'}`,
      b.cylinder(0.9, 11, 16), sign * 8.5, 5.5, -4, 'steel'));
    parts.push(b.part(`${prefix}-stack-cap-${sign > 0 ? 'e' : 'w'}`,
      b.cylinder(1.15, 0.5, 16), sign * 8.5, 11.2, -4, 'roof'));
  }
  parts.push(b.part(`${prefix}-duct`, b.cylinder(0.7, 16, 12), 0, 7.9, 0, 'pipe-air',
    { rotation: [0, 0, -Math.SQRT1_2, Math.SQRT1_2] }));
  return parts;
}

function buildPlan(projectId) {
  const b = createBuilder();
  // 配色直接按照片里的观感用十六进制写，srgb() 负责转成线性分量。
  // 池壁是浅米灰的清水混凝土，不是纯白；水面偏青；屋面蓝灰。
  b.material('concrete', 'Reinforced concrete', srgb('#efebe4'));
  b.material('water', 'Process water', srgb('#6195a1'));
  b.material('steel', 'Galvanised steel', srgb('#c0c3c6'));
  b.material('walkway', 'Grating walkway', srgb('#afb4b8'));
  b.material('air', 'Air header', srgb('#95c0db'));
  // 绿化与围栏。必须定义在组件声明之前，否则 define 时取不到材质。
  b.material('trunk', 'Tree trunk', srgb('#907b69'));
  b.material('foliage', 'Foliage', srgb('#80a675'));
  b.material('paving', 'Paving', srgb('#ceccc8'));
  b.material('media', 'Filter media', srgb('#958f86'));
  b.material('lamp', 'UV lamp', srgb('#ced3ef'));
  b.material('facade', 'Facade panel', srgb('#e7e6e1'));
  b.material('roof', 'Metal roof', srgb('#9ba8b6'));
  b.material('lawn', 'Lawn', srgb('#9aab86'));
  b.material('asphalt', 'Asphalt road', srgb('#86898e'));
  b.material('glass', 'Glazing', srgb('#afc0c9', 0.55));
  b.material('sludge', 'Sludge', srgb('#90816f'));
  // 管道按介质分色：原水灰、工艺水黄褐、清水绿、污泥深褐、空气蓝。
  // 这些材质同时给「连接管线」和「管廊主管」用，必须在任何 define 之前声明：
  // 组件在 define 时就把材质 id 写进 overrides，声明晚了取到的就是 undefined。
  b.material('pipe-raw', 'Pipe / raw water', srgb('#c0c3c6'));
  b.material('pipe-process', 'Pipe / process water', srgb('#d3bb8c'));
  b.material('pipe-clean', 'Pipe / clean water', srgb('#90c3a1'));
  b.material('pipe-sludge', 'Pipe / sludge', srgb('#a1866f'));
  b.material('pipe-air', 'Pipe / air', srgb('#8cb7e2'));
  // 道路标线、接触阴影、绿篱、铭牌石板。
  b.material('marking', 'Road marking', srgb('#f0f0ee'));
  b.material('shadow', 'Contact shadow', srgb('#0e1113', 0.3));
  b.material('hedge', 'Clipped hedge', srgb('#869d79'));
  b.material('sign-panel', 'Sign panel', srgb('#696f75'));

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
    // 空气干管从鼓风机房接进来，落在池壁北侧。
    b.port('air-in', 'ae-wall-n', 0, 4.6, -10, vec(0, 0, -1), 'air', 0.8),
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
  b.define('site-fence', 'Site fence', siteFence(b, 'sf', { posts: 178 }), [], [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 620, unit: 'm' },
  ]);
  // 南界在大门处断开：立柱数量在 define 时写死，因此按段各定义一个构件。
  b.define('site-fence-gate-w', 'Site fence (west of gate)', siteFence(b, 'sfw', { posts: 10 }), [], [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 32, unit: 'm' },
  ]);
  b.define('site-fence-gate-e', 'Site fence (east of gate)', siteFence(b, 'sfe', { posts: 164 }), [], [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 572, unit: 'm' },
  ]);
  b.define('blower-house', 'Blower house', facilityBuilding(b, 'bl', { length: 26, width: 14, floors: 1, floorHeight: 6 }), [
    b.port('air-out', 'bl-shell', 13, 4.4, 0, vec(1, 0, 0), 'air', 0.8),
  ]);
  b.define('chemical-house', 'Chemical dosing', facilityBuilding(b, 'ch', { length: 20, width: 12, floors: 2, floorHeight: 3.8 }), []);
  b.define('control-building', 'Control building', facilityBuilding(b, 'cb', { length: 30, width: 16, floors: 3, floorHeight: 3.8 }), []);

  // 入口与场地设施
  b.define('entrance-gate', 'Entrance gate', entranceGate(b, 'eg'), []);
  b.define('gate-house', 'Gate house', gateHouse(b, 'gho'), []);
  b.define('sign-wall', 'Sign wall', signWall(b, 'sgw'), []);
  b.define('lamp-post', 'Lamp post', lampPost(b, 'lmp'), []);
  b.define('emergency-basin', 'Emergency basin', emergencyBasin(b, 'emg', {}), [], [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 44, unit: 'm' },
    { name: 'width', valueType: { kind: 'number', minimum: null, maximum: null }, default: 26, unit: 'm' },
    { name: 'depth', valueType: { kind: 'number', minimum: null, maximum: null }, default: 5.2, unit: 'm' },
  ]);
  b.define('odor-control', 'Odour control', odorControl(b, 'odr'), []);
  b.define('outfall', 'Outfall structure', outfall(b, 'ofs'), [
    b.port('inlet', 'ofs-headwall', 0, 1.1, -1.2, vec(0, 0, -1), 'final-effluent', 0.6),
  ]);
  b.define('parking-bay', 'Parking bay', parkingBay(b, 'pkb', { stalls: 12 }), [], [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 31.2, unit: 'm' },
  ]);
  b.define('hedge', 'Hedge', hedgeRow(b, 'hgr'), [], [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 40, unit: 'm' },
  ]);
  b.define('pipe-rack', 'Pipe rack', pipeRack(b, 'prk', { bays: 37 }), [], [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 222, unit: 'm' },
    { name: 'height', valueType: { kind: 'number', minimum: null, maximum: null }, default: 4.6, unit: 'm' },
  ]);

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
  // 入口引道：从南主干道穿进厂内，直抵中控楼前的广场。
  b.ground('道路（入口引道）', 23, 37, 140, 180, 'asphalt', -0.03);
  b.ground('广场（入口）', 6, 60, 140, 168, 'paving', -0.04);
  b.ground('停车场', 44, 92, 150, 166, 'asphalt', -0.03);

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
  // 鼓风机房必须贴着曝气池：风管走一百多米压损和造价都不现实。
  // 加药间同样就近设在深度处理区旁边，而不是全塞在南侧辅助区。
  b.instance('blower-house', 'bl-1', 'BLW-101 鼓风机房', 300, 0, 76, { stage: 'auxiliary', tag: 'BLW-101' });
  b.instance('chemical-house', 'ch-1', 'CHEM-101 加药间', 470, 0, 46, { stage: 'auxiliary', tag: 'CHEM-101' });

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


  // 接触阴影贴片已全部拆除：渲染器现在有真正的方向光阴影贴图，
  // 那些手贴的黑色薄片只会变成一圈圈假的黑边，还会把真实的投影盖住。

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

  // 围栏：南北沿厂界，南界中间给大门留出 16 米开口
  b.instance('site-fence', 'fence-n', '围栏 / 北界', 300, 0, -182, { role: 'landscape' }, { length: 620 });
  b.instance('site-fence-gate-w', 'fence-s-w', '围栏 / 南界西段', 6, 0, 182, { role: 'landscape' }, { length: 32 });
  b.instance('site-fence-gate-e', 'fence-s-e', '围栏 / 南界东段', 324, 0, 182, { role: 'landscape' }, { length: 572 });

  // ── 北侧预留区：事故池与除臭装置，把原本空着的北带补满 ──────
  b.instance('emergency-basin', 'emg-1', 'EMG-101 事故池', 150, 0, -122,
    { stage: 'auxiliary', tag: 'EMG-101' }, { length: 44, width: 26, depth: 5.2 });
  b.instance('odor-control', 'odr-1', 'ODR-101 生物除臭间', 470, 0, -122,
    { stage: 'auxiliary', tag: 'ODR-101' });

  // 绿化：成排乔木，位置带确定性抖动。入口广场一带留空，不种树。
  const jitter = (index, salt) => {
    const raw = Math.abs(Math.sin(index * 12.9898 + salt) * 43758.5453) % 1;
    return raw - 0.5;
  };
  const inEntranceZone = (x, z) => x > 4 && x < 64 && z > 138;
  for (let index = 0; index < 26; index += 1) {
    const x = 12 + index * 23 + jitter(index, 0.7) * 5;
    const zs = 156 + jitter(index, 2.1) * 5;
    const ze = 173 + jitter(index, 3.3) * 4;
    const zn = -160 + jitter(index, 4.7) * 5;
    if (!inEntranceZone(x, zs)) {
      b.instance('tree', `tree-s-a-${index}`, `绿化 / 南排 ${index + 1}`, x, 0, zs, { role: 'landscape' });
    }
    if (!inEntranceZone(x + 8, ze)) {
      b.instance('tree', `tree-s-b-${index}`, `绿化 / 南外排 ${index + 1}`, x + 8, 0, ze, { role: 'landscape' });
    }
    b.instance('tree', `tree-n-a-${index}`, `绿化 / 北排 ${index + 1}`, x + 4, 0, zn, { role: 'landscape' });
  }
  for (let index = 0; index < 8; index += 1) {
    const z = -120 + index * 32 + jitter(index, 5.9) * 6;
    b.instance('tree', `tree-mid-${index}`, `绿化 / 中带 ${index + 1}`, 122 + jitter(index, 6.1) * 6, 0, z, { role: 'landscape' });
    b.instance('tree', `tree-mid2-${index}`, `绿化 / 中带二 ${index + 1}`, 358 + jitter(index, 7.3) * 6, 0, z, { role: 'landscape' });
  }
  // 东西两端与污泥区补种，避免出现大片空草坪。
  for (let index = 0; index < 10; index += 1) {
    const z = -150 + index * 33 + jitter(index, 8.1) * 5;
    b.instance('tree', `tree-west-${index}`, `绿化 / 西端 ${index + 1}`, 14 + jitter(index, 9.4) * 4, 0, z, { role: 'landscape' });
    b.instance('tree', `tree-east-${index}`, `绿化 / 东端 ${index + 1}`, 592 + jitter(index, 10.2) * 4, 0, z, { role: 'landscape' });
  }
  for (let index = 0; index < 9; index += 1) {
    const x = 150 + index * 34 + jitter(index, 11.3) * 6;
    b.instance('tree', `tree-sludge-${index}`, `绿化 / 污泥区 ${index + 1}`, x, 0, -96 + jitter(index, 12.7) * 5, { role: 'landscape' });
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

  // ── 入口区 ─────────────────────────────────────────────
  // 大门开在南界正中偏西，正对引道；门卫房在门内侧偏东，雨篷朝大门。
  b.instance('entrance-gate', 'gate-1', 'GATE-101 入口大门', 30, 0, 182, { stage: 'auxiliary', tag: 'GATE-101' });
  b.instance('gate-house', 'gho-1', 'GTE-101 门卫房', 45, 0, 174, { stage: 'auxiliary', tag: 'GTE-101' }, {}, Math.PI);
  b.instance('sign-wall', 'sign-1', '厂区铭牌', 53, 0, 176, { stage: 'auxiliary', tag: 'SIGN-101' }, {}, Math.PI);
  b.instance('parking-bay', 'pkb-1', '停车场车位', 100, 0, 158, { stage: 'auxiliary', role: 'site' }, { length: 31.2 });
  // 出水口挂在紫外消毒渠东侧
  b.instance('outfall', 'ofs-1', 'OUT-101 出水口', 592, 0, 0, { stage: 'tertiary', tag: 'OUT-101' }, {}, Math.PI / 2);
  b.connect('uv-1', 'outlet', 'ofs-1', 'inlet', 'UV-101 → OUT-101 出水', 'final-effluent', 0.6);
  // 鼓风机房 → 两条系列的曝气干管
  // 只连最近的这条系列：第二系列的空气走管廊，不在草地上拉一根一百多米的长管。
  b.connect('bl-1', 'air-out', 'ae-a3', 'air-in', 'BLW-101 → AE-103 曝气干管', 'air', 0.8);

  // ── 综合管廊 ───────────────────────────────────────────
  // 三段主管廊把「进水泵房 → 生化 → 二沉 → 深度处理」串起来，
  // 主管按介质分色，与池边支管同色系，远看就是一张流量图。
  b.instance('pipe-rack', 'prk-1', '管廊 / 主工艺线', 241, 0, -3, { role: 'piping' }, { length: 222, height: 4.6 });
  b.instance('pipe-rack', 'prk-2', '管廊 / 污泥线', 333, 0, -125, { role: 'piping' }, { length: 194, height: 4.2 });
  b.instance('pipe-rack', 'prk-3', '管廊 / 深度处理线', 533, 0, -18, { role: 'piping' }, { length: 126, height: 4.0 });

  // ── 绿篱与路灯 ─────────────────────────────────────────
  // 绿篱沿主干道内侧连续布置，断口留在大门口与岔路口。
  b.instance('hedge', 'hedge-s1', '绿篱 / 南一', 110, 0, 166, { role: 'landscape' }, { length: 92 });
  b.instance('hedge', 'hedge-s2', '绿篱 / 南二', 300, 0, 166, { role: 'landscape' }, { length: 296 });
  b.instance('hedge', 'hedge-s3', '绿篱 / 南三', 500, 0, 166, { role: 'landscape' }, { length: 180 });
  b.instance('hedge', 'hedge-n1', '绿篱 / 北一', 300, 0, -166, { role: 'landscape' }, { length: 590 });
  // 路灯：主干道内侧每 42 米一盏，灯臂伸向路心。
  for (let index = 0; index < 13; index += 1) {
    const x = 66 + index * 42;
    b.instance('lamp-post', `lamp-s-${index}`, `路灯 / 南 ${index + 1}`, x, 0, 163, { role: 'landscape' }, {}, -Math.PI / 2);
    b.instance('lamp-post', `lamp-n-${index}`, `路灯 / 北 ${index + 1}`, x + 20, 0, -163, { role: 'landscape' }, {}, Math.PI / 2);
  }
  for (let index = 0; index < 3; index += 1) {
    b.instance('lamp-post', `lamp-e-${index}`, `路灯 / 引道 ${index + 1}`, 20, 0, 150 + index * 14, { role: 'landscape' }, {}, 0);
  }



  return {
    projectId,
    operations: b.state.ops,
    summary: {
      components: b.state.components.size,
      instances: b.state.ops.filter((op) => op.type === 'instance.create').length,
      connections: b.state.ops.filter((op) => op.type === 'connection.create').length,
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
