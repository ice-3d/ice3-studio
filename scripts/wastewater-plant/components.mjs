// 污水处理厂构筑物、设备与场地组件族。

import {
  TAU,
  rotateY,
  LIE_ALONG_X,
  box,
  cylinder,
  sphere,
  annularPrismMesh,
} from './model-kit.mjs';

// 圆形池：底板 + 环形池壁 + 水面 + 中心进水筒 + 周边出水堰 + 刮泥桥 + 栏杆 + 爬梯
// 圆形池：参数化定义。radius / depth / wall 全部走表达式，
// 沿圆周重复的构件由内核按 i / count 展开，改参数即可整体重算。
function circularBasin(
  b,
  prefix,
  { segments = 160, radius = 15, depth = 4.6, wall = 0.45, bridgeMode = 'full' },
) {
  const parts = [];
  const ring = 'i * tau / count';
  const bridgeLength = bridgeMode === 'half' ? 'radius + 0.6' : 'radius * 2 + 1.2';
  const bridgeX = bridgeMode === 'half' ? 'radius / 2' : '0';
  const ringPart = (id, outerRadius, innerRadius, bottom, top, role) => (
    b.part(id, {
      kind: 'custom_mesh',
      mesh: annularPrismMesh({
        outerRadius,
        innerRadius,
        bottom,
        top,
        segments,
      }),
    }, 0, 0, 0, role)
  );

  const radialSegments = Math.min(segments * 2, 256);
  parts.push(b.part(`${prefix}-base`, b.cylinder({ expr: 'radius' }, 0.5, radialSegments), 0, 0.25, 0, 'concrete-old'));

  parts.push(ringPart(`${prefix}-wall`, radius, radius - wall, 0.5, 0.5 + depth, 'concrete-new'));

  parts.push(b.part(`${prefix}-water`, b.cylinder({ expr: 'radius - wall' }, 0.06, radialSegments),
    0, { expr: '0.5 + depth - 0.55' }, 0, 'water-clarified'));
  parts.push(ringPart(
    `${prefix}-waterline`,
    radius + 0.025,
    radius - wall - 0.025,
    0.5 + depth - 0.42,
    0.5 + depth - 0.2,
    'concrete-wet',
  ));
  parts.push(ringPart(
    `${prefix}-scum-band`,
    radius - wall - 0.56,
    radius - wall - 0.86,
    0.5 + depth - 0.34,
    0.5 + depth - 0.22,
    'scum',
  ));
  parts.push(b.part(`${prefix}-inlet-well`, b.cylinder(1.5, { expr: 'depth + 0.9' }, 32),
    0, { expr: '0.5 + (depth + 0.9) / 2' }, 0, 'concrete-old'));
  parts.push(b.part(`${prefix}-inlet-sleeve`, b.cylinder(0.58, 1.3, 18),
    0, { expr: '0.5 + depth + 0.15' }, 0, 'stainless'));
  parts.push(b.part(`${prefix}-outlet-sleeve`, b.cylinder(0.5, 1.1, 18),
    { expr: 'radius' }, { expr: '0.5 + depth - 0.15' }, 0, 'stainless',
    { rotation: LIE_ALONG_X }));

  // 出水堰：贴内壁一圈，堰顶高于水面
  parts.push(ringPart(
    `${prefix}-weir`,
    radius - wall,
    radius - wall - 0.35,
    0.5 + depth - 0.45,
    0.5 + depth + 0.05,
    'concrete-new',
  ));

  // 池顶压顶：比墙宽的凸边
  parts.push(ringPart(
    `${prefix}-coping`,
    radius + 0.1,
    radius - wall - 0.24,
    0.5 + depth,
    0.5 + depth + 0.22,
    'concrete-new',
  ));

  // 池顶环形走道
  parts.push(ringPart(
    `${prefix}-deck`,
    radius - wall,
    radius - wall - 0.9,
    0.5 + depth,
    0.5 + depth + 0.1,
    'walkway',
  ));
  parts.push(ringPart(
    `${prefix}-rail`,
    radius + 0.03,
    radius - 0.03,
    0.5 + depth + 1.02,
    0.5 + depth + 1.1,
    'steel-dark',
  ));

  // 池顶栏杆：环形横杆由同一网格连续生成，立柱按固定角距布置。
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
      role: 'steel-dark',
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
      role: 'steel-dark',
    }));
  parts.push(b.part(`${prefix}-ladder-rail-a`, b.box(0.06, { expr: 'depth + 1.2' }, 0.06),
    { expr: 'radius + 0.25' }, { expr: '(depth + 1.2) / 2' }, -0.42, 'steel-dark'));
  parts.push(b.part(`${prefix}-ladder-rail-b`, b.box(0.06, { expr: 'depth + 1.2' }, 0.06),
    { expr: 'radius + 0.25' }, { expr: '(depth + 1.2) / 2' }, 0.42, 'steel-dark'));

  // 刮泥桥：初沉池使用半桥，二沉池使用贯穿池心的全桥。
  parts.push(b.part(`${prefix}-bridge-top`, b.box({ expr: bridgeLength }, 0.12, 0.16),
    { expr: bridgeX }, { expr: '0.5 + depth + 0.97' }, -0.7, 'steel-dark'));
  parts.push(b.part(`${prefix}-bridge-top-b`, b.box({ expr: bridgeLength }, 0.12, 0.16),
    { expr: bridgeX }, { expr: '0.5 + depth + 0.97' }, 0.7, 'steel-dark'));
  parts.push(b.part(`${prefix}-bridge-bottom`, b.box({ expr: bridgeLength }, 0.14, 1.5),
    { expr: bridgeX }, { expr: '0.5 + depth + 0.55' }, 0, 'walkway'));
  parts.push(b.repeatPart(`${prefix}-bridge-strut`, 9,
    b.box(0.1, 0.44, 0.12),
    {
      transform: {
        translation: {
          x: { expr: `(i / (count - 1) - 0.5) * (${bridgeLength}) + (${bridgeX})` },
          y: { expr: '0.5 + depth + 0.76' },
          z: -0.7,
        },
      },
      role: 'steel-dark',
    }));
  parts.push(
    b.part(`${prefix}-drive`, b.cylinder(0.55, 0.5, 20),
      0, { expr: '0.5 + depth + 1.25' }, 0, 'steel'),
    b.part(`${prefix}-drive-gearbox`, b.box(0.9, 0.32, 0.55),
      0.55, { expr: '0.5 + depth + 1.42' }, 0, 'steel-dark'),
    b.part(`${prefix}-drive-motor`, b.cylinder(0.28, 0.8, 16),
      1.15, { expr: '0.5 + depth + 1.42' }, 0, 'stainless',
      { rotation: LIE_ALONG_X }),
  );
  parts.push(b.part(`${prefix}-bridge-rail-a`, b.box({ expr: bridgeLength }, 0.08, 0.08),
    { expr: bridgeX }, { expr: '0.5 + depth + 1.55' }, -0.76, 'steel-dark'));
  parts.push(b.part(`${prefix}-bridge-rail-b`, b.box({ expr: bridgeLength }, 0.08, 0.08),
    { expr: bridgeX }, { expr: '0.5 + depth + 1.55' }, 0.76, 'steel-dark'));
  for (let index = 0; index < 8; index += 1) {
    parts.push(b.part(`${prefix}-stair-tread-${index + 1}`, b.box(0.36, 0.07, 1.1),
      { expr: 'radius + 0.5' }, 0.75 + index * 0.33, -1.1, 'walkway'));
  }
  parts.push(b.part(`${prefix}-stair-stringer-a`, b.box(0.1, { expr: 'depth' }, 0.1),
    { expr: 'radius + 0.32' }, { expr: '0.5 + depth / 2' }, -1.65, 'steel-dark'));
  parts.push(b.part(`${prefix}-stair-stringer-b`, b.box(0.1, { expr: 'depth' }, 0.1),
    { expr: 'radius + 0.32' }, { expr: '0.5 + depth / 2' }, -0.55, 'steel-dark'));

  return parts;
}

// 矩形曝气池：参数化。length / width / depth / wall 走表达式，
// 导流墙、曝气支管、走道立柱按索引展开。
function aerationBasin(
  b,
  prefix,
  { lanes = 3, airLines = 30, posts = 11, mode = 'aerobic' } = {},
) {
  const corridors = lanes;
  const parts = [];
  parts.push(b.part(`${prefix}-base`, b.box({ expr: 'length' }, 0.5, { expr: 'width' }), 0, 0.25, 0, 'concrete-old'));
  parts.push(b.part(`${prefix}-wall-n`, b.box({ expr: 'length' }, { expr: 'depth' }, { expr: 'wall' }),
    0, { expr: '0.5 + depth / 2' }, { expr: '-width / 2' }, 'concrete-new'));
  parts.push(b.part(`${prefix}-wall-s`, b.box({ expr: 'length' }, { expr: 'depth' }, { expr: 'wall' }),
    0, { expr: '0.5 + depth / 2' }, { expr: 'width / 2' }, 'concrete-new'));

  // 东西端墙：整面一块，用 subtract 挖出进水/出水洞，由内核拆成子盒。
  for (const [side, sign] of [['w', -1], ['e', 1]]) {
    parts.push(b.part(`${prefix}-wall-${side}`,
      b.box({ expr: 'wall' }, { expr: 'depth' }, { expr: 'width' }),
      { expr: `length / 2 * ${sign}` }, { expr: '0.5 + depth / 2' }, 0, 'concrete-new',
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
    0, { expr: '0.5 + depth - 0.5' }, 0, 'water-aerobic'));

  parts.push(b.repeatPart(`${prefix}-baffle-1`, corridors,
    b.box(0.35, { expr: 'depth - 0.15' }, { expr: 'width - 4' }),
    {
      transform: {
        translation: {
          x: { expr: '(i + 1) * length / (count + 1)' },
          y: { expr: '0.5 + (depth - 0.15) / 2' },
          z: 1.6,
        },
      },
      role: 'concrete-old',
    }));

  for (let lane = 0; lane < 3; lane += 1) {
    const z = { expr: `(${lane} - 1) * width / 3` };
    parts.push(b.part(`${prefix}-lane-wall-${lane + 1}`, b.box(0.3, { expr: 'depth - 0.2' }, 0.35),
      { expr: 'length / 2 - 1' }, { expr: '0.5 + depth / 2' }, z, 'concrete-old'));
    const foamId = lane === 0 ? `${prefix}-foam-band` : `${prefix}-foam-band-${lane + 1}`;
    parts.push(b.part(foamId, b.box({ expr: 'length / 4' }, 0.04, 0.7),
      { expr: '-length / 4' }, { expr: '0.5 + depth - 0.43' }, z, 'foam'));
  }
  if (mode === 'aerobic') {
    for (let index = 0; index < airLines; index += 1) {
      const x = { expr: `-length / 2 + 2 + ${index} * (length - 4) / ${airLines - 1}` };
      parts.push(b.part(`${prefix}-air-header-${index + 1}`, b.box(0.16, 0.16, { expr: 'width - 3' }),
        x, 0.9, 0, 'pipe-air'));
      for (let lane = 0; lane < corridors; lane += 1) {
        const z = { expr: `(${lane} - 1) * width / 3` };
        parts.push(b.part(`${prefix}-air-drop-${index + 1}-${lane + 1}`, b.cylinder(0.08, 2.8, 10),
          x, 1.9, z, 'pipe-air'));
        parts.push(b.part(`${prefix}-air-diffuser-${index + 1}-${lane + 1}`, b.cylinder(0.22, 0.12, 12),
          x, 0.72, z, 'pipe-air'));
      }
    }
  } else {
    for (const lane of [-1, 0, 1]) {
      parts.push(
        b.part(
          `${prefix}-mixer-column-${lane + 2}`,
          b.cylinder(0.22, 4.2, 16),
          { expr: 'length / 2 - 4' },
          2.4,
          { expr: `${lane} * width / 3` },
          'stainless',
        ),
        b.part(
          `${prefix}-mixer-propeller-${lane + 2}`,
          b.cylinder(0.85, 0.32, 18),
          { expr: 'length / 2 - 4' },
          0.9,
          { expr: `${lane} * width / 3` },
          'steel-dark',
        ),
      );
    }
  }

  parts.push(b.part(`${prefix}-walkway`, b.box({ expr: 'length' }, 0.14, 1.3),
    0, { expr: '0.5 + depth + 0.05' }, { expr: '-width / 2 - 0.7' }, 'walkway'));
  parts.push(b.part(`${prefix}-walkway-rail`, b.box({ expr: 'length' }, 0.07, 0.07),
    0, { expr: '0.5 + depth + 0.95' }, { expr: '-width / 2 - 1.3' }, 'steel-dark'));
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
      role: 'steel-dark',
    }));
  parts.push(b.part(`${prefix}-rail-n`, b.box({ expr: 'length' }, 0.07, 0.07),
    0, { expr: '0.5 + depth + 0.95' }, { expr: '-width / 2 - 0.1' }, 'steel-dark'));
  parts.push(b.part(`${prefix}-rail-s`, b.box({ expr: 'length' }, 0.07, 0.07),
    0, { expr: '0.5 + depth + 0.95' }, { expr: 'width / 2 + 0.1' }, 'steel-dark'));
  parts.push(b.part(`${prefix}-rail-toe-n`, b.box({ expr: 'length' }, 0.12, 0.05),
    0, { expr: '0.5 + depth + 0.1' }, { expr: '-width / 2 - 0.1' }, 'steel-dark'));
  parts.push(b.part(`${prefix}-rail-toe-s`, b.box({ expr: 'length' }, 0.12, 0.05),
    0, { expr: '0.5 + depth + 0.1' }, { expr: 'width / 2 + 0.1' }, 'steel-dark'));

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
    b.part(`${prefix}-crown-low`, sphere(2.3, 16, 10), 0, 5.0, 0, 'foliage'),
    b.part(`${prefix}-crown-mid`, sphere(2.0, 16, 10), 1.0, 6.1, -0.45, 'foliage'),
    b.part(`${prefix}-crown-high`, sphere(1.6, 16, 10), -0.75, 6.7, 0.55, 'foliage'),
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
function pipeRack(b, prefix, { bays = 38 } = {}) {
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

function pumpSkid(b, prefix, { length = 16, width = 8, count = 3 }) {
  const parts = [
    b.part(`${prefix}-plinth`, b.box(length, 0.3, width), 0, 0.15, 0, 'concrete'),
    b.part(`${prefix}-suction-header`, b.cylinder(0.45, length - 1.5, 16),
      0, 1.15, -width / 2 + 0.8, 'pipe-raw', { rotation: LIE_ALONG_X }),
    b.part(`${prefix}-discharge-header`, b.cylinder(0.38, length - 1.5, 16),
      0, 1.55, width / 2 - 0.8, 'pipe-process', { rotation: LIE_ALONG_X }),
  ];
  for (let index = 0; index < count; index += 1) {
    const x = -length / 2 + (index + 0.5) * (length / count);
    parts.push(
      b.part(`${prefix}-suction-branch-${index + 1}`, b.cylinder(0.2, 2.5, 14),
        x, 1.15, -1.95, 'pipe-raw',
        { rotation: [Math.SQRT1_2, 0, 0, Math.SQRT1_2] }),
      b.part(`${prefix}-discharge-branch-${index + 1}`, b.cylinder(0.16, 3.9, 14),
        x, 1.55, 1.25, 'pipe-process',
        { rotation: [Math.SQRT1_2, 0, 0, Math.SQRT1_2] }),
      b.part(`${prefix}-pump-body-${index + 1}`, b.cylinder(0.62, 1.55, 20),
        x, 1.15, -0.7, 'stainless', { rotation: LIE_ALONG_X }),
      b.part(`${prefix}-pump-motor-${index + 1}`, b.cylinder(0.48, 1.25, 18),
        x + 0.95, 1.15, -0.7, 'steel', { rotation: LIE_ALONG_X }),
      b.part(`${prefix}-pump-coupling-${index + 1}`, b.box(0.55, 0.42, 0.42),
        x + 0.22, 1.15, -0.7, 'steel'),
      b.part(`${prefix}-pump-valve-${index + 1}`, b.cylinder(0.3, 0.55, 14),
        x, 1.75, width / 2 - 0.8, 'pipe-process'),
      b.part(`${prefix}-pump-gauge-${index + 1}`, b.cylinder(0.16, 0.08, 14),
        x, 2.18, width / 2 - 0.8, 'stainless'),
    );
  }
  return parts;
}

function blowerSkid(b, prefix, { length = 8, width = 4.8, count = 2 }) {
  const parts = [b.part(`${prefix}-plinth`, b.box(length, 0.24, width), 0, 0.12, 0, 'concrete')];
  for (let index = 0; index < count; index += 1) {
    const x = -length / 2 + (index + 0.5) * (length / count);
    parts.push(
      b.part(`${prefix}-blower-volute-${index + 1}`, b.cylinder(0.78, 1.2, 22),
        x, 1.0, 0, 'stainless', { rotation: LIE_ALONG_X }),
      b.part(`${prefix}-blower-motor-${index + 1}`, b.box(1.25, 1.1, 1.2),
        x + 1.05, 0.83, 0, 'steel'),
      b.part(`${prefix}-blower-filter-${index + 1}`, b.box(1.0, 1.0, 1.0),
        x - 1.05, 0.78, 0, 'steel'),
      b.part(`${prefix}-blower-discharge-${index + 1}`, b.cylinder(0.26, 1.5, 14),
        x, 1.75, 0, 'pipe-air'),
    );
  }
  return parts;
}

function chemicalDosingSkid(b, prefix, { tanks = 2, mixers = 2 }) {
  const parts = [b.part(`${prefix}-containment`, b.box(10, 0.35, 5.5), 0, 0.175, 0, 'concrete')];
  for (let index = 0; index < tanks; index += 1) {
    const x = -2.4 + index * 4.8;
    parts.push(
      b.part(`${prefix}-dosing-tank-${index + 1}`, b.cylinder(1.25, 3.1, 24),
        x, 1.9, 0, 'steel'),
      b.part(`${prefix}-dosing-tank-cap-${index + 1}`, b.cylinder(1.35, 0.12, 24),
        x, 3.5, 0, 'stainless'),
    );
    if (index < mixers) {
      parts.push(b.part(`${prefix}-dosing-mixer-${index + 1}`, b.cylinder(0.12, 3.8, 12),
        x, 2.25, 0, 'steel'));
    }
  }
  parts.push(
    b.part(`${prefix}-metering-skid`, b.box(2.1, 0.7, 3.6), 2.9, 0.7, 1.0, 'stainless'),
    b.part(`${prefix}-dosing-pump-a`, b.box(0.7, 0.7, 0.65), 2.4, 1.25, 1.0, 'steel'),
    b.part(`${prefix}-dosing-pump-b`, b.box(0.7, 0.7, 0.65), 3.4, 1.25, 1.0, 'steel'),
  );
  return parts;
}

function dewateringSkid(b, prefix, { units = 2 }) {
  const parts = [b.part(`${prefix}-skid`, b.box(9, 0.35, 5.2), 0, 0.175, 0, 'steel')];
  for (let index = 0; index < units; index += 1) {
    const x = -2.2 + index * 4.4;
    parts.push(
      b.part(`${prefix}-centrifuge-bowl-${index + 1}`, b.cylinder(0.72, 3.4, 24),
        x, 1.4, 0, 'stainless', { rotation: LIE_ALONG_X }),
      b.part(`${prefix}-centrifuge-motor-${index + 1}`, b.box(1.2, 1.2, 1.1),
        x + 2.05, 1.05, 0, 'steel'),
      b.part(`${prefix}-centrifuge-feed-${index + 1}`, b.cylinder(0.22, 1.2, 12),
        x - 1.85, 1.1, 0, 'pipe-sludge'),
    );
  }
  parts.push(
    b.part(`${prefix}-polymer-skid`, b.box(2.2, 0.9, 1.8), -3.4, 0.8, 1.5, 'stainless'),
    b.part(`${prefix}-maintenance-platform`, b.box(9.4, 0.12, 1.1), 0, 2.1, 2.4, 'walkway'),
  );
  return parts;
}

function sludgeHeatExchanger(b, prefix, { length = 6, shellDiameter = 1.1 }) {
  return [
    b.part(`${prefix}-shell`, b.cylinder(shellDiameter / 2, length, 24),
      0, 1.25, 0, 'stainless', { rotation: LIE_ALONG_X }),
    b.part(`${prefix}-head-a`, b.cylinder(shellDiameter * 0.62, 0.35, 20),
      -length / 2, 1.25, 0, 'steel', { rotation: LIE_ALONG_X }),
    b.part(`${prefix}-head-b`, b.cylinder(shellDiameter * 0.62, 0.35, 20),
      length / 2, 1.25, 0, 'steel', { rotation: LIE_ALONG_X }),
    b.part(`${prefix}-support-a`, b.box(0.35, 1.0, 1.2), -length / 3, 0.5, 0, 'steel'),
    b.part(`${prefix}-support-b`, b.box(0.35, 1.0, 1.2), length / 3, 0.5, 0, 'steel'),
  ];
}

function valveManifold(b, prefix, { branches = 4, diameter = 0.4 }) {
  const parts = [
    b.part(`${prefix}-header`, b.cylinder(diameter / 2, branches * 1.8, 16),
      0, 1.0, 0, 'pipe-process', { rotation: LIE_ALONG_X }),
  ];
  for (let index = 0; index < branches; index += 1) {
    const x = -((branches - 1) * 1.8) / 2 + index * 1.8;
    parts.push(
      b.part(`${prefix}-branch-${index + 1}`, b.cylinder(diameter * 0.35, 1.6, 12),
        x, 0.75, 0, 'pipe-process'),
      b.part(`${prefix}-valve-${index + 1}`, b.cylinder(diameter * 0.62, 0.42, 16),
        x, 1.25, 0, 'steel'),
    );
  }
  return parts;
}

function serviceCanopy(b, prefix, { length = 10, width = 6, height = 5 }) {
  return [
    b.part(`${prefix}-roof`, b.box(length, 0.18, width), 0, height, 0, 'roof-light'),
    b.part(`${prefix}-post-a`, b.box(0.2, height, 0.2), -length / 2 + 0.4, height / 2, -width / 2 + 0.4, 'steel-dark'),
    b.part(`${prefix}-post-b`, b.box(0.2, height, 0.2), -length / 2 + 0.4, height / 2, width / 2 - 0.4, 'steel-dark'),
    b.part(`${prefix}-post-c`, b.box(0.2, height, 0.2), length / 2 - 0.4, height / 2, -width / 2 + 0.4, 'steel-dark'),
    b.part(`${prefix}-post-d`, b.box(0.2, height, 0.2), length / 2 - 0.4, height / 2, width / 2 - 0.4, 'steel-dark'),
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

export {
  circularBasin,
  aerationBasin,
  screeningChannel,
  pumpHouse,
  gritChamber,
  filterCell,
  disinfectionChannel,
  facilityBuilding,
  thickener,
  digester,
  dewateringHouse,
  gasHolder,
  tree,
  siteFence,
  entranceGate,
  gateHouse,
  signWall,
  pipeRack,
  hedgeRow,
  lampPost,
  parkingBay,
  pumpSkid,
  blowerSkid,
  chemicalDosingSkid,
  dewateringSkid,
  sludgeHeatExchanger,
  valveManifold,
  serviceCanopy,
  outfall,
  emergencyBasin,
  odorControl,
  wallWithOpening,
};
