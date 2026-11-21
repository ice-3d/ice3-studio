import { LIE_ALONG_X, cylinder, pipeMaterialForMedium } from './model-kit.mjs';

const number = (name, defaultValue) => ({
  name,
  valueType: { kind: 'number', minimum: 0.01, maximum: 100 },
  default: defaultValue,
  unit: 'm',
});

function teeParts(b, prefix) {
  return [
    b.part(
      `${prefix}-run`,
      b.cylinder({ expr: 'runDiameter / 2' }, { expr: 'runLength' }, 24),
      0, 0, 0, 'pipe-process', { rotation: LIE_ALONG_X },
    ),
    b.part(
      `${prefix}-branch`,
      b.cylinder({ expr: 'branchDiameter / 2' }, { expr: 'branchLength' }, 24),
      0, { expr: 'branchLength / 2' }, 0, 'pipe-process',
    ),
    b.part(
      `${prefix}-collar`,
      b.cylinder({ expr: 'branchDiameter * 0.72' }, 0.12, 24),
      0, { expr: 'branchLength * 0.82' }, 0, 'steel-dark',
    ),
    b.part(
      `${prefix}-flange`,
      b.cylinder({ expr: 'runDiameter * 0.68' }, 0.08, 24),
      { expr: '-runLength / 2' }, 0, 0, 'steel', { rotation: LIE_ALONG_X },
    ),
  ];
}

function reducerParts(b, prefix) {
  return [
    b.part(
      `${prefix}-large-flange`,
      b.cylinder({ expr: 'largeDiameter * 0.62' }, 0.10, 24),
      { expr: '-length / 2' }, 0, 0, 'steel', { rotation: LIE_ALONG_X },
    ),
    b.part(
      `${prefix}-shell`,
      b.cylinder({ expr: '(largeDiameter + smallDiameter) / 4' }, { expr: 'length' }, 24),
      0, 0, 0, 'pipe-process', { rotation: LIE_ALONG_X },
    ),
    b.part(
      `${prefix}-small-flange`,
      b.cylinder({ expr: 'smallDiameter * 0.72' }, 0.10, 24),
      { expr: 'length / 2' }, 0, 0, 'steel', { rotation: LIE_ALONG_X },
    ),
  ];
}

function sleeveParts(b, prefix) {
  const cut = {
    radius: { expr: 'pipeDiameter / 2' },
    height: { expr: 'wallThickness + extension * 2 + 0.02' },
    segments: 24,
    axis: 'y',
  };
  return [
    b.part(
      `${prefix}-wall-a`,
      b.cylinder({ expr: 'sleeveDiameter / 2' }, { expr: 'wallThickness / 2 + extension' }, 24),
      { expr: '-(wallThickness + extension * 2) / 4' }, 0, 0,
      'steel-dark', { rotation: LIE_ALONG_X, subtract: [cut] },
    ),
    b.part(
      `${prefix}-wall-b`,
      b.cylinder({ expr: 'sleeveDiameter / 2' }, { expr: 'wallThickness / 2 + extension' }, 24),
      { expr: '(wallThickness + extension * 2) / 4' }, 0, 0,
      'steel-dark', { rotation: LIE_ALONG_X, subtract: [cut] },
    ),
  ];
}

export function definePipeAttachmentComponents(b, media) {
  for (const [index, medium] of media.entries()) {
    const suffix = medium.replaceAll(/[^a-z0-9]+/g, '-');
    const pipeMaterial = pipeMaterialForMedium(medium);
    const runDirection = { x: -1, y: 0, z: 0 };
    b.define(
      `pipe-tee-${suffix}`,
      `Pipe tee (${medium})`,
      teeParts(b, `pte-${index}`).map((part) => ({
        ...part,
        role: part.role === 'pipe-process' ? pipeMaterial : part.role,
      })),
      [
        b.port(
          'run-in', `pte-${index}-run`,
          { expr: '-runLength / 2' }, 0, 0, runDirection, medium,
          { expr: 'runDiameter' },
        ),
        b.port(
          'run-out', `pte-${index}-run`,
          { expr: 'runLength / 2' }, 0, 0, { x: 1, y: 0, z: 0 }, medium,
          { expr: 'runDiameter' },
        ),
        b.port(
          'branch', `pte-${index}-branch`, 0,
          { expr: 'branchLength' }, 0, { x: 0, y: 1, z: 0 }, medium,
          { expr: 'branchDiameter' },
        ),
      ],
      [
        number('runDiameter', 0.4),
        number('branchDiameter', 0.3),
        number('runLength', 2.0),
        number('branchLength', 1.2),
      ],
    );
    b.define(
      `pipe-reducer-${suffix}`,
      `Pipe reducer (${medium})`,
      reducerParts(b, `prd-${index}`).map((part) => ({
        ...part,
        role: part.role === 'pipe-process' ? pipeMaterial : part.role,
      })),
      [
        b.port(
          'large', `prd-${index}-large-flange`,
          { expr: '-length / 2' }, 0, 0, runDirection, medium,
          { expr: 'largeDiameter' },
        ),
        b.port(
          'small', `prd-${index}-small-flange`,
          { expr: 'length / 2' }, 0, 0, { x: 1, y: 0, z: 0 }, medium,
          { expr: 'smallDiameter' },
        ),
      ],
      [
        number('largeDiameter', 0.4),
        number('smallDiameter', 0.3),
        number('length', 1.2),
      ],
    );
    b.define(
      `wall-sleeve-${suffix}`,
      `Wall sleeve (${medium})`,
      sleeveParts(b, `wsl-${index}`),
      [
        b.port(
          'wall-in', `wsl-${index}-wall-a`,
          { expr: '-(wallThickness + extension * 2) / 2' }, 0, 0,
          runDirection, medium, { expr: 'sleeveDiameter' },
        ),
        b.port(
          'wall-out', `wsl-${index}-wall-b`,
          { expr: '(wallThickness + extension * 2) / 2' }, 0, 0,
          { x: 1, y: 0, z: 0 }, medium, { expr: 'sleeveDiameter' },
        ),
      ],
      [
        number('pipeDiameter', 0.3),
        number('sleeveDiameter', 0.5),
        number('wallThickness', 0.45),
        number('extension', 0.12),
      ],
    );
  }
}

export function placeTee(b, {
  key,
  name,
  x,
  y,
  z,
  yaw = 0,
  medium,
  lineId,
  parameters,
  upstream,
  downstream,
  branch,
}) {
  const nodeId = b.instance(
    `pipe-tee-${medium}`,
    key,
    name,
    x,
    y,
    z,
    {
      role: 'pipe-fitting',
      fittingKind: 'tee',
      lineId,
      stage: upstream.metadata.stage,
      routeObstacle: 'solid',
      clearanceRadius: '0',
    },
    parameters,
    yaw,
  );
  b.connect(
    upstream.key, upstream.port, key, 'run-in',
    upstream.service ?? `${name} 主管入口`, medium, parameters.runDiameter, {
      lineId,
      supportLineId: `${lineId}-${key}-run-in`,
    },
  );
  b.connect(
    key, 'run-out', downstream.key, downstream.port,
    downstream.service ?? `${name} 主管出口`, medium, parameters.runDiameter, {
      lineId,
      supportLineId: `${lineId}-${key}-run-out`,
    },
  );
  b.connect(
    key, 'branch', branch.key, branch.port,
    branch.service ?? `${name} 支管`, medium, parameters.branchDiameter, {
      lineId,
      supportLineId: `${lineId}-${key}-branch`,
    },
  );
  return nodeId;
}

export function placeReducer(b, {
  key,
  name,
  x,
  y,
  z,
  yaw = 0,
  medium,
  lineId,
  parameters,
  upstream,
  downstream,
}) {
  const nodeId = b.instance(
    `pipe-reducer-${medium}`,
    key,
    name,
    x,
    y,
    z,
    {
      role: 'pipe-fitting',
      fittingKind: 'reducer',
      lineId,
      stage: upstream.metadata.stage,
      routeObstacle: 'solid',
      clearanceRadius: '0',
    },
    parameters,
    yaw,
  );
  b.connect(
    upstream.key, upstream.port, key, 'large',
    upstream.service ?? `${name} 大端入口`, medium, parameters.largeDiameter, {
      lineId,
      supportLineId: `${lineId}-${key}-large`,
    },
  );
  b.connect(
    key, 'small', downstream.key, downstream.port,
    downstream.service ?? `${name} 小端出口`, medium, parameters.smallDiameter, {
      lineId,
      supportLineId: `${lineId}-${key}-small`,
    },
  );
  return nodeId;
}

export function placeSleeve(b, {
  key,
  name,
  x,
  y,
  z,
  yaw = 0,
  medium,
  lineId,
  hostNodeId,
  parameters,
  stage,
}) {
  return b.instance(
    `wall-sleeve-${medium}`,
    key,
    name,
    x,
    y,
    z,
    {
      role: 'pipe-penetration',
      fittingKind: 'sleeve',
      lineId,
      hostNodeId,
      stage,
      routeObstacle: 'solid',
      clearanceRadius: '0',
    },
    parameters,
    yaw,
  );
}
