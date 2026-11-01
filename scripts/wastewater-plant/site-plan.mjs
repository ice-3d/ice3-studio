// 污水处理厂厂区布局、实例和连接。

import {
  createBuilder as createBuilderFromKit,
  srgb,
  vec,
} from './model-kit.mjs';
import {
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
} from './components.mjs';
import {
  cakeConveyor,
  distributionBox,
  gasCollector,
  gasFlare,
  rasPumpStation,
  sludgeCollectionWell,
} from './process-nodes.mjs';
import {
  validateProcessGraph,
  validateWastewaterPlan,
} from './validate.mjs';

export const requiredProcessGraph = [
  {
    code: 'MISSING_PRIMARY_SLUDGE',
    fromTags: ['PC-101', 'PC-201'],
    toTags: ['TH-101', 'TH-102'],
    media: ['primary-sludge'],
  },
  {
    code: 'MISSING_RAS',
    fromTags: ['SC-101', 'SC-102', 'SC-103', 'SC-201', 'SC-202', 'SC-203'],
    toTags: ['AE-102', 'AE-202'],
    media: ['return-sludge'],
  },
  {
    code: 'MISSING_GAS',
    fromTags: ['DG-101', 'DG-102'],
    toTags: ['GH-101'],
    media: ['biogas'],
  },
  {
    code: 'MISSING_DIGESTED_SLUDGE_1',
    fromTags: ['DG-101'],
    toTags: ['DW-101-C1'],
    media: ['digested-sludge'],
  },
  {
    code: 'MISSING_DIGESTED_SLUDGE_2',
    fromTags: ['DG-102'],
    toTags: ['DW-101-C2'],
    media: ['digested-sludge'],
  },
];

function buildPlan(projectId) {
  const b = createBuilderFromKit();
  // 配色直接按照片里的观感用十六进制写，srgb() 负责转成线性分量。
  // 池壁是浅米灰的清水混凝土，不是纯白；水面偏青；屋面蓝灰。
  b.material('concrete', 'Reinforced concrete', srgb('#efebe4'));
  b.material('water', 'Process water', srgb('#6195a1'));
  b.material('steel', 'Galvanised steel', srgb('#9aa5aa'));
  b.material('walkway', 'Grating walkway', srgb('#8e989c'));
  b.material('air', 'Air header', srgb('#95c0db'));
  // 绿化与围栏。必须定义在组件声明之前，否则 define 时取不到材质。
  b.material('trunk', 'Tree trunk', srgb('#907b69'));
  b.material('foliage', 'Foliage', srgb('#748f63'));
  b.material('paving', 'Paving', srgb('#ceccc8'));
  b.material('media', 'Filter media', srgb('#958f86'));
  b.material('lamp', 'UV lamp', srgb('#ced3ef'));
  b.material('facade', 'Facade panel', srgb('#e7e6e1'));
  b.material('roof', 'Metal roof', srgb('#9ba8b6'));
  b.material('lawn', 'Lawn', srgb('#94a97c'));
  b.material('asphalt', 'Asphalt road', srgb('#86898e'));
  b.material('glass', 'Glazing', srgb('#afc0c9', 0.55));
  b.material('sludge', 'Sludge', srgb('#90816f'));
  // 管道按介质分色：原水灰、工艺水黄褐、清水绿、污泥深褐、空气蓝。
  // 这些材质同时给「连接管线」和「管廊主管」用，必须在任何 define 之前声明：
  // 组件在 define 时就把材质 id 写进 overrides，声明晚了取到的就是 undefined。
  b.material('pipe-raw', 'Pipe / raw water', srgb('#aab3b7'));
  b.material('pipe-process', 'Pipe / process water', srgb('#b7aa8b'));
  b.material('pipe-clean', 'Pipe / clean water', srgb('#7f9f96'));
  b.material('pipe-sludge', 'Pipe / sludge', srgb('#876f5d'));
  b.material('pipe-air', 'Pipe / air', srgb('#7696ad'));
  // 道路标线、接触阴影、绿篱、铭牌石板。
  b.material('marking', 'Road marking', srgb('#f0f0ee'));
  b.material('shadow', 'Contact shadow', srgb('#0e1113', 0.3));
  b.material('hedge', 'Clipped hedge', srgb('#869d79'));
  b.material('sign-panel', 'Sign panel', srgb('#696f75'));
  b.material('concrete-new', 'New concrete', srgb('#ddd5c7'));
  b.material('concrete-old', 'Weathered concrete', srgb('#aaa398'));
  b.material('concrete-wet', 'Wet concrete', srgb('#77776f'));
  b.material('steel-dark', 'Dark steel', srgb('#62717a'));
  b.material('stainless', 'Stainless steel', srgb('#aeb7ba'));
  b.material('roof-light', 'Light metal roof', srgb('#c7ced1'));
  b.material('water-anoxic', 'Anoxic water', srgb('#29434c'));
  b.material('water-aerobic', 'Aerobic water', srgb('#386a73'));
  b.material('water-clarified', 'Clarified water', srgb('#4f7f89'));
  b.material('water-sludge', 'Sludge water', srgb('#625747'));
  b.material('foam', 'Aeration foam', srgb('#b9c8bd', 0.72));
  b.material('scum', 'Clarifier scum', srgb('#665b49'));
  b.material('asphalt-new', 'New asphalt', srgb('#62676b'));
  b.material('asphalt-worn', 'Worn asphalt', srgb('#6e7275'));
  b.material('curb', 'Curb', srgb('#c8c2b6'));
  b.material('drain', 'Drain', srgb('#555a5e'));

  // 进水段
  b.define('screening-channel', 'Screening channel', screeningChannel(b, 'gr', { length: 20, width: 6, depth: 2.6 }), [
    b.port('inlet', 'gr-wall-w', -10, 2.0, 0, vec(1, 0, 0), 'raw-water', 1.0),
    b.port('outlet', 'gr-wall-e', 10, 2.0, 0, vec(1, 0, 0), 'screened-water', 1.0),
  ]);
  b.define('pump-house', 'Lift pump house', pumpHouse(b, 'ps', { length: 22, width: 13, height: 8 }), [
    b.port('inlet', 'ps-wall-w', -11, 3.0, 0, vec(1, 0, 0), 'screened-water', 1.0),
    b.port('outlet', 'ps-wall-e', 11, 3.0, 0, vec(1, 0, 0), 'screened-water', 1.0),
  ]);
  b.define('grit-chamber', 'Aerated grit chamber', gritChamber(b, 'gc', { length: 26, width: 11, depth: 4.2 }), [
    b.port('inlet', 'gc-wall-w', -13, 2.6, 0, vec(1, 0, 0), 'lifted-water', 1.0),
    b.port('outlet', 'gc-wall-e', 13, 2.6, 0, vec(1, 0, 0), 'degritted-water', 1.0),
  ]);

  // 初沉池
  const primaryParts = circularBasin(b, 'pc', {
    segments: 160,
    radius: 13,
    depth: 4.2,
    wall: 0.45,
    bridgeMode: 'half',
  });
  b.define('primary-clarifier', 'Primary clarifier', primaryParts, [
    b.port('inlet', 'pc-inlet-well', 0, 5.4, 0, vec(0, -1, 0), 'degritted-water', 1.0),
    b.port('outlet', 'pc-base', 12.4, 4.5, 0, vec(1, 0, 0), 'settled-water', 0.9),
    b.port('primary-sludge', 'pc-base', 0, 0.3, 0, vec(0, -1, 0), 'primary-sludge', 0.3),
  ], [
    { name: 'radius', valueType: { kind: 'number', minimum: null, maximum: null }, default: 13, unit: 'm' },
    { name: 'depth', valueType: { kind: 'number', minimum: null, maximum: null }, default: 4.2, unit: 'm' },
    { name: 'wall', valueType: { kind: 'number', minimum: null, maximum: null }, default: 0.45, unit: 'm' },
  ]);

  // 推流式生化池按工艺段拆分。厌氧、缺氧段使用潜水搅拌器，只有好氧段安装曝气器。
  const basinParameters = [
    { name: 'length', valueType: { kind: 'number', minimum: null, maximum: null }, default: 42, unit: 'm' },
    { name: 'width', valueType: { kind: 'number', minimum: null, maximum: null }, default: 20, unit: 'm' },
    { name: 'depth', valueType: { kind: 'number', minimum: null, maximum: null }, default: 6, unit: 'm' },
    { name: 'wall', valueType: { kind: 'number', minimum: null, maximum: null }, default: 0.45, unit: 'm' },
  ];
  b.define('anaerobic-basin', 'Anaerobic basin',
    aerationBasin(b, 'ae', { mode: 'anaerobic' }), [
    b.port('inlet', 'ae-wall-w', -21, 4.0, 0, vec(1, 0, 0), 'settled-water', 0.9),
    b.port('outlet', 'ae-wall-e', 21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
  ], basinParameters);
  b.define('anoxic-basin', 'Anoxic basin',
    aerationBasin(b, 'ae', { mode: 'anoxic' }), [
    b.port('inlet', 'ae-wall-w', -21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
    b.port('return-inlet', 'ae-wall-n', 5, 4.0, -10, vec(0, 0, 1), 'return-sludge', 0.35),
    b.port('outlet', 'ae-wall-e', 21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
  ], basinParameters);
  b.define('aerobic-basin', 'Aerobic basin',
    aerationBasin(b, 'ae', { mode: 'aerobic' }), [
    b.port('inlet', 'ae-wall-w', -21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
    b.port('outlet', 'ae-wall-e', 21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
    b.port('air-in', 'ae-wall-n', 0, 4.6, -10, vec(0, 0, -1), 'air', 0.8),
  ], basinParameters);

  // 二沉池（与初沉池同构，尺寸与用途不同）
  const secondaryParts = circularBasin(b, 'sc', {
    segments: 160,
    radius: 15,
    depth: 4.6,
    wall: 0.45,
    bridgeMode: 'full',
  });
  b.define('secondary-clarifier', 'Secondary clarifier', secondaryParts, [
    b.port('inlet', 'sc-inlet-well', 0, 5.6, 0, vec(0, -1, 0), 'mixed-liquor', 0.9),
    b.port('outlet', 'sc-base', 14.4, 4.9, 0, vec(1, 0, 0), 'secondary-effluent', 0.8),
    b.port('return-sludge', 'sc-base', -3, 0.3, 0, vec(0, -1, 0), 'return-sludge', 0.3),
    b.port('waste-sludge', 'sc-base', 3, 0.3, 0, vec(0, -1, 0), 'waste-sludge', 0.35),
  ], [
    { name: 'radius', valueType: { kind: 'number', minimum: null, maximum: null }, default: 15, unit: 'm' },
    { name: 'depth', valueType: { kind: 'number', minimum: null, maximum: null }, default: 4.6, unit: 'm' },
    { name: 'wall', valueType: { kind: 'number', minimum: null, maximum: null }, default: 0.45, unit: 'm' },
  ]);

  b.define('filter-cell', 'Filtration cell', filterCell(b, 'fl', {}), [
    b.port('inlet', 'fl-wall-w', -16, 2.4, 0, vec(1, 0, 0), 'secondary-effluent', 0.8),
    b.port('outlet', 'fl-wall-e', 16, 2.4, 0, vec(1, 0, 0), 'filtered-water', 0.8),
    b.port('chemical-in', 'fl-wall-n', 0, 2.2, -13, vec(0, 0, 1), 'chemical', 0.15),
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
    b.port('primary-inlet', 'th-base', -8, 3.0, 0, vec(1, 0, 0), 'primary-sludge', 0.3),
    b.port('outlet', 'th-mixer-column', 0, 3.6, 0, vec(0, -1, 0), 'thickened-sludge', 0.3),
  ], [
    { name: 'radius', valueType: { kind: 'number', minimum: null, maximum: null }, default: 8, unit: 'm' },
    { name: 'depth', valueType: { kind: 'number', minimum: null, maximum: null }, default: 4.4, unit: 'm' },
  ]);
  b.define('digester', 'Anaerobic digester', digester(b, 'dg', {}), [
    b.port('inlet', 'dg-shell', 11, 4.0, 0, vec(-1, 0, 0), 'thickened-sludge', 0.3),
    b.port('outlet', 'dg-shell', -11, 4.0, 0, vec(1, 0, 0), 'digested-sludge', 0.3),
    b.port('gas-out', 'dg-dome', 0, 18.5, 0, vec(0, 1, 0), 'biogas', 0.35),
  ], [
    { name: 'radius', valueType: { kind: 'number', minimum: null, maximum: null }, default: 11, unit: 'm' },
    { name: 'height', valueType: { kind: 'number', minimum: null, maximum: null }, default: 17, unit: 'm' },
  ]);
  b.define('gas-holder', 'Gas holder', gasHolder(b, 'gh', { radius: 8.5 }), [
    b.port('gas-in', 'gh-sphere', -4, 5.5, 0, vec(-1, 0, 0), 'biogas', 0.35),
    b.port('gas-out', 'gh-sphere', 4, 5.5, 0, vec(1, 0, 0), 'biogas', 0.35),
  ]);
  b.define('grit-distribution', 'Grit distribution box',
    distributionBox(b, 'gdb', { length: 6, width: 5 }), [
      b.port('inlet', 'gdb-wall-w', -3, 2.5, 0, vec(1, 0, 0), 'degritted-water', 0.9),
      b.port('outlet-a', 'gdb-wall-e', 3, 2.5, -1.2, vec(1, 0, 0), 'degritted-water', 0.9),
      b.port('outlet-b', 'gdb-wall-e', 3, 2.5, 1.2, vec(1, 0, 0), 'degritted-water', 0.9),
    ]);
  b.define('clarifier-distribution', 'Clarifier distribution box',
    distributionBox(b, 'cdb', {}), [
      b.port('inlet', 'cdb-wall-w', -4, 2.5, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
      ...[1, 2, 3].map((index) => b.port(
        `outlet-${index}`,
        'cdb-wall-e',
        4,
        2.5,
        (index - 2) * 1.5,
        vec(1, 0, 0),
        'mixed-liquor',
        0.9,
      )),
    ]);
  b.define('filter-collection', 'Filter collection box',
    distributionBox(b, 'fcb', { length: 12, width: 8 }), [
      ...[1, 2, 3, 4, 5, 6].map((index) => b.port(
        `inlet-${index}`,
        'fcb-wall-w',
        -6,
        2.4,
        (index - 3.5) * 1.1,
        vec(1, 0, 0),
        'secondary-effluent',
        0.8,
      )),
      b.port('outlet', 'fcb-wall-e', 6, 2.4, 0, vec(1, 0, 0), 'secondary-effluent', 0.8),
    ]);
  b.define('sludge-collection-well', 'Sludge collection well',
    sludgeCollectionWell(b, 'scw', {}), [
      b.port('inlet-a', 'scw-shell', -1.2, 3.2, 0, vec(1, 0, 0), 'primary-sludge', 0.3),
      b.port('inlet-b', 'scw-shell', 1.2, 3.2, 0, vec(-1, 0, 0), 'primary-sludge', 0.3),
      b.port('outlet-a', 'scw-shell', -0.8, 3.2, 0.8, vec(0, 0, 1), 'primary-sludge', 0.3),
      b.port('outlet-b', 'scw-shell', 0.8, 3.2, 0.8, vec(0, 0, 1), 'primary-sludge', 0.3),
    ]);
  b.define('ras-pump-station', 'RAS pump station',
    rasPumpStation(b, 'rws', {}), [
      ...[1, 2, 3].flatMap((train) => (
        [1, 2, 3].map((index) => b.port(
          `inlet-${train}${index}`,
          'rws-slab',
          -6 + (index - 1) * 6,
          0.5,
          (train - 2) * 3,
          vec(0, 0, train === 1 ? -1 : 1),
          'return-sludge',
          0.3,
        ))
      )),
      ...[1, 2, 3].flatMap((train) => (
        [1, 2, 3].map((index) => b.port(
          `waste-in-${train}${index}`,
          'rws-slab',
          -6 + (index - 1) * 6,
          0.5,
          (train - 2) * 4,
          vec(0, 0, train === 1 ? -1 : 1),
          'waste-sludge',
          0.35,
        ))
      )),
      b.port('outlet-a', 'rws-slab', -7, 0.8, 3, vec(-1, 0, 0), 'return-sludge', 0.3),
      b.port('outlet-b', 'rws-slab', 7, 0.8, -3, vec(1, 0, 0), 'return-sludge', 0.3),
      b.port('waste-out-a', 'rws-slab', -6, 0.8, 4, vec(-1, 0, 0), 'waste-sludge', 0.35),
      b.port('waste-out-b', 'rws-slab', 6, 0.8, -4, vec(1, 0, 0), 'waste-sludge', 0.35),
    ]);
  b.define('gas-collector', 'Gas collector',
    gasCollector(b, 'gcl', {}), [
      b.port('inlet-a', 'gcl-vessel', -3.5, 3.0, 0, vec(-1, 0, 0), 'biogas', 0.35),
      b.port('inlet-b', 'gcl-vessel', -3.0, 3.0, 0, vec(-1, 0, 0), 'biogas', 0.35),
      b.port('outlet', 'gcl-vessel', 4, 3.0, 0, vec(1, 0, 0), 'biogas', 0.35),
    ]);
  b.define('gas-flare', 'Gas flare', gasFlare(b, 'gf', {}), [
    b.port('inlet', 'gf-base', -1, 0.8, 0, vec(-1, 0, 0), 'biogas', 0.35),
  ]);
  b.define('cake-conveyor', 'Cake conveyor',
    cakeConveyor(b, 'cvc', {}), []);
  b.define('dewatering-house', 'Dewatering house', dewateringHouse(b, 'dw', { length: 26, width: 15, height: 8 }), [
    b.port('inlet-a', 'dw-shell', -13, 2.0, -3, vec(1, 0, 0), 'digested-sludge', 0.3),
    b.port('inlet-b', 'dw-shell', -13, 2.0, 3, vec(1, 0, 0), 'digested-sludge', 0.3),
    b.port('cake-a', 'dw-shell', 13, 1.2, -3, vec(-1, 0, 0), 'dewatered-sludge', 0.25),
    b.port('cake-b', 'dw-shell', 13, 1.2, 3, vec(-1, 0, 0), 'dewatered-sludge', 0.25),
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
    b.port('air-out-b', 'bl-shell', 13, 4.4, 3, vec(1, 0, 0), 'air', 0.8),
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
  b.define('lift-pump-skid', 'Lift pump skid',
    pumpSkid(b, 'lps', { length: 16, width: 8, count: 3 }), [
      b.port('inlet', 'lps-suction-header', -7.25, 1.15, -3.2, vec(0, 0, 1), 'screened-water', 1.0),
      b.port('outlet', 'lps-discharge-header', 7.25, 1.55, 3.2, vec(0, 0, -1), 'lifted-water', 1.0),
    ]);
  b.define('blower-skid', 'Blower skid',
    blowerSkid(b, 'bls', { length: 8, width: 4.8, count: 2 }), [
      b.port('air-in', 'bls-blower-filter-1', -5.05, 1.0, 0, vec(-1, 0, 0), 'air', 0.8),
      b.port('air-out', 'bls-blower-discharge-2', 2.75, 2.5, 0, vec(0, 1, 0), 'air', 0.8),
    ]);
  b.define('chemical-dosing-skid', 'Chemical dosing skid',
    chemicalDosingSkid(b, 'cds', { tanks: 2, mixers: 2 }), [
      b.port('chemical-out', 'cds-dosing-tank-2', 2.9, 2.4, 0, vec(1, 0, 0), 'chemical', 0.15),
    ]);
  b.define('dewatering-skid', 'Dewatering skid',
    dewateringSkid(b, 'dws', { units: 2 }), [
      b.port('sludge-in', 'dws-centrifuge-feed-1', -6.85, 1.1, 0, vec(-1, 0, 0), 'digested-sludge', 0.35),
      b.port('cake-out', 'dws-skid', 4.5, 0.5, 0, vec(1, 0, 0), 'dewatered-sludge', 0.35),
    ]);
  b.define('sludge-heat-exchanger', 'Sludge heat exchanger',
    sludgeHeatExchanger(b, 'she', { length: 6, shellDiameter: 1.1 }), [
      b.port('sludge-in', 'she-head-a', -3.2, 1.25, 0, vec(-1, 0, 0), 'thickened-sludge', 0.3),
      b.port('sludge-out', 'she-head-b', 3.2, 1.25, 0, vec(1, 0, 0), 'thickened-sludge', 0.3),
    ]);
  b.define('valve-manifold', 'Valve manifold',
    valveManifold(b, 'vmf', { branches: 4, diameter: 0.4 }), [
      b.port('inlet', 'vmf-header', -3.6, 1.0, 0, vec(-1, 0, 0), 'settled-water', 0.4),
      b.port('outlet', 'vmf-header', 3.6, 1.0, 0, vec(1, 0, 0), 'settled-water', 0.4),
    ]);
  b.define('service-canopy', 'Service canopy',
    serviceCanopy(b, 'svc', { length: 10, width: 6, height: 5 }), []);

  // 场地：按功能分区铺草坪，四周留出环形主干道。
  b.ground('场地（草坪）', -10, 610, -185, 185, 'lawn', -0.08);
  // 环形主干道：南北两条沿厂界，东西两端各一条。
  b.ground('道路（北）', -10, 620, -180, -168, 'asphalt-new', -0.03);
  b.ground('道路（南）', -10, 620, 168, 180, 'asphalt-worn', -0.03);
  b.ground('道路（西）', -10, 2, -185, 185, 'asphalt-new', -0.03);
  b.ground('道路（东）', 600, 612, -185, 185, 'asphalt-worn', -0.03);
  // 横向支路：分隔预处理 / 生化 / 二沉三个区块。
  b.ground('道路（支路一）', 120, 130, -160, 160, 'asphalt-worn', -0.03);
  b.ground('道路（支路二）', 355, 365, -160, 160, 'asphalt-new', -0.03);
  // 入口引道：从南主干道穿进厂内，直抵中控楼前的广场。
  b.ground('道路（入口引道）', 23, 37, 140, 180, 'asphalt-new', -0.03);
  b.ground('广场（入口）', 6, 60, 140, 168, 'paving', -0.04);
  b.ground('停车场', 44, 92, 150, 166, 'asphalt-worn', -0.03);

  b.ground('北侧道路标线', -10, 610, -179.15, -178.85, 'marking', -0.018);
  b.ground('北侧道路内标线', -10, 610, -169.15, -168.85, 'marking', -0.018);
  b.ground('南侧道路标线', -10, 610, 168.85, 169.15, 'marking', -0.018);
  b.ground('南侧道路外标线', -10, 610, 178.85, 179.15, 'marking', -0.018);
  b.ground('生物池检修通道', 178, 365, 58, 64, 'paving', -0.04);
  b.ground('二沉池检修通道', 365, 528, 58, 64, 'paving', -0.04);
  b.ground('北侧设备装卸场地', 400, 477, -161, -132, 'paving', -0.04);
  b.ground('预处理设备装卸场地', 42, 72, 18, 30, 'paving', -0.04);
  b.ground('深度处理检修场地', 500, 578, 12, 32, 'paving', -0.04);
  b.ground('生化区检修环路西', 172, 178, -62, 62, 'asphalt-worn', -0.026);
  b.ground('生化区检修环路东', 350, 356, -62, 62, 'asphalt-new', -0.026);
  b.ground('南侧路缘', -10, 610, 167.7, 168.2, 'curb', -0.015);
  b.ground('北侧路缘', -10, 610, -168.2, -167.7, 'curb', -0.015);
  b.ground('南侧排水沟', -10, 610, 166.9, 167.35, 'drain', -0.012);
  b.ground('北侧排水沟', -10, 610, -167.35, -166.9, 'drain', -0.012);

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
    b.instance('anaerobic-basin', `ae-${key}1`, `AE-${series}01 厌氧池`, 215 + dx, 0, z, {
      stage: 'biological', tag: `AE-${series}01`, train: `Train${series}`,
    }, { length: 42, width: 20, depth: 6, wall: 0.45 });
    b.instance('anoxic-basin', `ae-${key}2`, `AE-${series}02 缺氧池`, 270 + dx, 0, z, {
      stage: 'biological', tag: `AE-${series}02`, train: `Train${series}`,
    }, { length: 42, width: 20, depth: 6, wall: 0.45 });
    b.instance('aerobic-basin', `ae-${key}3`, `AE-${series}03 好氧池`, 325 + dx, 0, z, {
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
  b.instance('lift-pump-skid', 'lps-1', 'PS-101 提升泵组', 62, 0, 10,
    { stage: 'intake', tag: 'PS-101-P', role: 'equipment' });
  b.instance('blower-skid', 'bls-1', 'BLW-101 鼓风机 1', 284, 0, 68,
    { stage: 'auxiliary', tag: 'BLW-101-A', role: 'equipment' });
  b.instance('blower-skid', 'bls-2', 'BLW-101 鼓风机 2', 314, 0, 68,
    { stage: 'auxiliary', tag: 'BLW-101-B', role: 'equipment' });
  b.instance('chemical-dosing-skid', 'cds-1', 'CHEM-101 加药装置', 470, 0, 30,
    { stage: 'auxiliary', tag: 'CHEM-101-D', role: 'equipment' });
  b.instance('sludge-heat-exchanger', 'she-1', 'HEX-101 污泥换热器', 290, 0, -145,
    { stage: 'sludge', tag: 'HEX-101', role: 'equipment' });
  b.instance('dewatering-skid', 'dws-1', 'DW-101 脱水机 1', 458, 0, -138,
    { stage: 'sludge', tag: 'DW-101-C1', role: 'equipment' });
  b.instance('dewatering-skid', 'dws-2', 'DW-101 脱水机 2', 458, 0, -156,
    { stage: 'sludge', tag: 'DW-101-C2', role: 'equipment' });
  b.instance('valve-manifold', 'vmf-1', '生化配水阀组', 177, 0, 5,
    { stage: 'biological', role: 'equipment' });
  b.instance('service-canopy', 'svc-1', '提升泵组检修棚', 62, 0, 10,
    { stage: 'intake', role: 'equipment' });

  // 分流、汇流和回流节点。每个方向使用独立端口，避免多根主管叠在同一点。
  b.instance('grit-distribution', 'dd-g', 'DD-001 沉砂配水井', 128, 0, 0,
    { stage: 'intake', tag: 'DD-001' });
  for (const train of trains) {
    const { key, series, z } = train;
    b.instance('clarifier-distribution', `dd-${key}`, `DD-${series}01 二沉分流井`,
      key === 'a' ? 353 : 359, 0, z, { stage: 'secondary', tag: `DD-${series}01` });
  }
  b.instance('filter-collection', 'fb-1', 'FB-101 二沉集水井', 495, 0, 0,
    { stage: 'secondary', tag: 'FB-101' });
  b.instance('sludge-collection-well', 'scw-1', 'SCW-101 初沉集泥井', 190, 0, 0,
    { stage: 'sludge', tag: 'SCW-101' });
  b.instance('ras-pump-station', 'rws-1', 'RWS-101 回流污泥泵站', 410, 0, 0,
    { stage: 'secondary', tag: 'RWS-101' });
  b.instance('gas-collector', 'gcl-1', 'GCL-101 沼气集气总管', 344, 0, -125,
    { stage: 'sludge', tag: 'GCL-101' });
  b.instance('gas-flare', 'gf-1', 'GF-101 沼气安全火炬', 395, 0, -100,
    { stage: 'sludge', tag: 'GF-101' });
  b.instance('cake-conveyor', 'cvc-1', 'CV-101 泥饼输送机 1', 444, 0, -138,
    { stage: 'sludge', tag: 'CV-101' });
  b.instance('cake-conveyor', 'cvc-2', 'CV-102 泥饼输送机 2', 444, 0, -156,
    { stage: 'sludge', tag: 'CV-102' });

  // 水系连接
  b.connect('gr-1', 'outlet', 'ps-1', 'inlet', 'GR-101 → PS-101 原水', 'screened-water', 1.0);
  b.connect('ps-1', 'outlet', 'lps-1', 'inlet', 'PS-101 → 提升泵组', 'screened-water', 1.0);
  b.connect('lps-1', 'outlet', 'gc-1', 'inlet', '提升泵组 → GC-101', 'lifted-water', 1.0);
  b.connect('gc-1', 'outlet', 'dd-g', 'inlet',
    'GC-101 → DD-001 沉砂出水', 'degritted-water', 1.0);
  let filterInlet = 1;
  for (const train of trains) {
    const { key, series, z } = train;
    b.connect('dd-g', `outlet-${key}`, `pc-${key}`, 'inlet',
      `DD-001 → PC-${series}01 沉砂出水`, 'degritted-water', 1.0);
    b.connect(`pc-${key}`, 'outlet', `ae-${key}1`, 'inlet',
      `PC-${series}01 → AE-${series}01 进水`, 'settled-water', 0.9);
    b.connect(`ae-${key}1`, 'outlet', `ae-${key}2`, 'inlet',
      `AE-${series}01 → AE-${series}02 混合液`, 'mixed-liquor', 0.9);
    b.connect(`ae-${key}2`, 'outlet', `ae-${key}3`, 'inlet',
      `AE-${series}02 → AE-${series}03 混合液`, 'mixed-liquor', 0.9);
    b.connect(`ae-${key}3`, 'outlet', `dd-${key}`, 'inlet',
      `AE-${series}03 → DD-${series}01 混合液`, 'mixed-liquor', 0.9);
    for (let index = 1; index <= 3; index += 1) {
      b.connect(`dd-${key}`, `outlet-${index}`, `sc-${key}${index}`, 'inlet',
        `DD-${series}01 → SC-${series}0${index} 混合液`, 'mixed-liquor', 0.9);
      b.connect(`sc-${key}${index}`, 'outlet', 'fb-1', `inlet-${filterInlet}`,
        `SC-${series}0${index} → FB-101 二沉出水`, 'secondary-effluent', 0.8);
      b.connect(`sc-${key}${index}`, 'return-sludge',
        'rws-1', `inlet-${series}${index}`,
        `SC-${series}0${index} → RWS-101 回流污泥`,
        'return-sludge', 0.3);
      b.connect(`sc-${key}${index}`, 'waste-sludge',
        'rws-1', `waste-in-${series}${index}`,
        `SC-${series}0${index} → RWS-101 剩余污泥`,
        'waste-sludge', 0.35);
      filterInlet += 1;
    }
    b.connect('rws-1', `outlet-${key}`, `ae-${key}2`, 'return-inlet',
      `RWS-101 → AE-${series}02 回流污泥`, 'return-sludge', 0.3);
  }
  b.connect('fb-1', 'outlet', 'fl-1', 'inlet',
    'FB-101 → FL-101 二沉出水', 'secondary-effluent', 0.8);
  b.connect('fl-1', 'outlet', 'uv-1', 'inlet', 'FL-101 → UV-101 滤后水', 'filtered-water', 0.8);

  // 污泥线
  b.connect('pc-a', 'primary-sludge', 'scw-1', 'inlet-a',
    'PC-101 → SCW-101 初沉污泥', 'primary-sludge', 0.3);
  b.connect('pc-b', 'primary-sludge', 'scw-1', 'inlet-b',
    'PC-201 → SCW-101 初沉污泥', 'primary-sludge', 0.3);
  b.connect('scw-1', 'outlet-a', 'th-1', 'primary-inlet',
    'SCW-101 → TH-101 初沉污泥', 'primary-sludge', 0.3);
  b.connect('scw-1', 'outlet-b', 'th-2', 'primary-inlet',
    'SCW-101 → TH-102 初沉污泥', 'primary-sludge', 0.3);
  b.connect('rws-1', 'waste-out-a', 'th-1', 'inlet',
    'RWS-101 → TH-101 剩余污泥', 'waste-sludge', 0.35);
  b.connect('rws-1', 'waste-out-b', 'th-2', 'inlet',
    'RWS-101 → TH-102 剩余污泥', 'waste-sludge', 0.35);
  b.connect('th-1', 'outlet', 'she-1', 'sludge-in', 'TH-101 → HEX-101', 'thickened-sludge', 0.3);
  b.connect('she-1', 'sludge-out', 'dg-1', 'inlet', 'HEX-101 → DG-101', 'thickened-sludge', 0.3);
  b.connect('th-2', 'outlet', 'dg-2', 'inlet', 'TH-102 → DG-102 浓缩污泥', 'thickened-sludge', 0.3);
  b.connect('dg-1', 'outlet', 'dws-1', 'sludge-in', 'DG-101 → DW-101 脱水机 1', 'digested-sludge', 0.3);
  b.connect('dg-2', 'outlet', 'dws-2', 'sludge-in', 'DG-102 → DW-101 脱水机 2', 'digested-sludge', 0.3);
  b.connect('cds-1', 'chemical-out', 'fl-1', 'chemical-in', 'CHEM-101 → FL-101', 'chemical', 0.15);

  // 沼气：两座消化池汇入集气总管，再进入气柜，末端接安全火炬。
  b.connect('dg-1', 'gas-out', 'gcl-1', 'inlet-a',
    'DG-101 → GCL-101 沼气', 'biogas', 0.35);
  b.connect('dg-2', 'gas-out', 'gcl-1', 'inlet-b',
    'DG-102 → GCL-101 沼气', 'biogas', 0.35);
  b.connect('gcl-1', 'outlet', 'gh-1', 'gas-in',
    'GCL-101 → GH-101 沼气', 'biogas', 0.35);
  b.connect('gh-1', 'gas-out', 'gf-1', 'inlet',
    'GH-101 → GF-101 安全放空', 'biogas', 0.35);

  // 泥饼由输送机搬运，不生成圆管扫掠。
  b.relate('dws-1', 'cvc-1', 'material-handling',
    { service: 'DW-101 脱水机 1 → CV-101', medium: 'dewatered-cake' });
  b.relate('cvc-1', 'dw-1', 'material-handling',
    { service: 'CV-101 → DW-101 泥饼', medium: 'dewatered-cake' });
  b.relate('dws-2', 'cvc-2', 'material-handling',
    { service: 'DW-101 脱水机 2 → CV-102', medium: 'dewatered-cake' });
  b.relate('cvc-2', 'dw-1', 'material-handling',
    { service: 'CV-102 → DW-101 泥饼', medium: 'dewatered-cake' });
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

  // ── 入口区 ─────────────────────────────────────────────
  // 大门开在南界正中偏西，正对引道；门卫房在门内侧偏东，雨篷朝大门。
  b.instance('entrance-gate', 'gate-1', 'GATE-101 入口大门', 30, 0, 182, { stage: 'auxiliary', tag: 'GATE-101' });
  b.instance('gate-house', 'gho-1', 'GTE-101 门卫房', 45, 0, 174, { stage: 'auxiliary', tag: 'GTE-101' }, {}, Math.PI);
  b.instance('sign-wall', 'sign-1', '厂区铭牌', 53, 0, 176, { stage: 'auxiliary', tag: 'SIGN-101' }, {}, Math.PI);
  b.instance('parking-bay', 'pkb-1', '停车场车位', 100, 0, 158, { stage: 'auxiliary', role: 'site' }, { length: 31.2 });
  // 出水口挂在紫外消毒渠东侧
  b.instance('outfall', 'ofs-1', 'OUT-101 出水口', 592, 0, 0, { stage: 'tertiary', tag: 'OUT-101' }, {}, Math.PI / 2);
  b.connect('uv-1', 'outlet', 'ofs-1', 'inlet', 'UV-101 → OUT-101 出水', 'final-effluent', 0.6);
  // 两台鼓风机分别向两组好氧池供气。
  b.connect('bl-1', 'air-out', 'bls-1', 'air-in', 'BLW-101 → 鼓风机 1', 'air', 0.8);
  b.connect('bls-1', 'air-out', 'ae-a3', 'air-in', '鼓风机 1 → AE-103', 'air', 0.8);
  b.connect('bl-1', 'air-out-b', 'bls-2', 'air-in', 'BLW-101 → 鼓风机 2', 'air', 0.8);
  b.connect('bls-2', 'air-out', 'ae-b3', 'air-in', '鼓风机 2 → AE-203', 'air', 0.8);

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



  const plan = {
    projectId,
    operations: b.state.ops,
    summary: {
      components: b.state.components.size,
      instances: b.state.ops.filter((op) => op.type === 'instance.create').length,
      connections: b.state.ops.filter((op) => op.type === 'connection.create').length,
      materials: b.state.materials.size,
    },
  };
  const validation = validateWastewaterPlan(plan);
  if (!validation.ok) {
    throw new Error(validation.issues.map(({ message }) => message).join('; '));
  }
  const processValidation = validateProcessGraph(plan, requiredProcessGraph);
  if (!processValidation.ok) {
    throw new Error(processValidation.issues
      .map(({ code, message }) => `${code}: ${message}`)
      .join('; '));
  }
  return plan;
}

export { buildPlan };
