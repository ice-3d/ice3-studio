import { TEMPLATE_TRIANGLE_BUDGETS } from './templates.mjs';

const PROFILE_BY_KIND = Object.freeze({
  'inlet-structure': { shape: 'basin', width: 22, depth: 13, height: 3.4 },
  'bar-screen': { shape: 'screen', width: 16, depth: 8, height: 4.2 },
  'lift-pump': { shape: 'skid', width: 12, depth: 7, height: 4.8 },
  'grit-basin': { shape: 'basin', width: 18, depth: 11, height: 4.5 },
  'primary-structure': { shape: 'clarifier', width: 30, depth: 30, height: 5.4 },
  'distribution-well': { shape: 'round-basin', width: 14, depth: 14, height: 4.2 },
  'biological-reactor': { shape: 'basin', width: 38, depth: 22, height: 6.2 },
  mixer: { shape: 'mixer', width: 7, depth: 7, height: 8.2 },
  'secondary-structure': { shape: 'clarifier', width: 28, depth: 28, height: 5.8 },
  'solids-pump': { shape: 'skid', width: 9, depth: 5, height: 3.2 },
  'rotating-bridge': { shape: 'bridge', width: 30, depth: 3, height: 3.2 },
  'filter-cell': { shape: 'filter', width: 19, depth: 13, height: 4.2 },
  'disinfection-basin': { shape: 'serpentine', width: 22, depth: 12, height: 3.2 },
  'blower-equipment': { shape: 'building', width: 15, depth: 10, height: 7.4 },
  'chemical-tank': { shape: 'tank', width: 8, depth: 8, height: 11 },
  'dosing-pump': { shape: 'skid', width: 7, depth: 4, height: 2.8 },
  thickener: { shape: 'clarifier', width: 25, depth: 25, height: 7 },
  digester: { shape: 'digester', width: 14, depth: 14, height: 20 },
  'dewatering-unit': { shape: 'building', width: 17, depth: 11, height: 8 },
  conveyor: { shape: 'conveyor', width: 20, depth: 4, height: 5.5 },
  analyzer: { shape: 'cabinet', width: 3.5, depth: 2.5, height: 5.5 },
  valve: { shape: 'valve', width: 5, depth: 3, height: 3.4 },
  'process-building': { shape: 'building', width: 25, depth: 16, height: 11 },
});

export function createEquipmentMesh(kind, ordinal) {
  const budget = TEMPLATE_TRIANGLE_BUDGETS[kind];
  if (!budget) throw new Error(`Missing triangle budget for ${kind}`);
  const profile = PROFILE_BY_KIND[kind];
  if (!profile) throw new Error(`Missing geometry profile for ${kind}`);
  const seed = [...kind].reduce(
    (value, character) => value * 31 + character.charCodeAt(0),
    ordinal,
  );
  const positions = [];
  const indices = [];
  appendProfile(profile, positions, indices, seed);
  appendProfileDetails(profile, positions, indices, seed);
  fillBudget(profile, positions, indices, seed, budget);
  return { positions, indices };
}

function appendProfile(profile, positions, indices, seed) {
  const { shape, width, depth, height } = profile;
  if (shape === 'basin') {
    appendRectangularBasin(positions, indices, width, depth, height, seed);
  } else if (shape === 'clarifier' || shape === 'round-basin') {
    appendCircularBasin(positions, indices, width / 2, height, seed);
  } else if (shape === 'building') {
    appendBuilding(positions, indices, width, depth, height, seed);
  } else if (shape === 'digester') {
    appendDigester(positions, indices, width / 2, height, seed);
  } else if (shape === 'tank') {
    appendTank(positions, indices, width / 2, height, seed);
  } else if (shape === 'screen') {
    appendScreen(positions, indices, width, depth, height, seed);
  } else if (shape === 'filter') {
    appendFilter(positions, indices, width, depth, height);
  } else if (shape === 'serpentine') {
    appendSerpentine(positions, indices, width, depth, height);
  } else if (shape === 'conveyor') {
    appendConveyor(positions, indices, width, depth, height);
  } else if (shape === 'mixer') {
    appendMixer(positions, indices, width, depth, height, seed);
  } else if (shape === 'bridge') {
    appendBridge(positions, indices, width, depth, height);
  } else if (shape === 'cabinet') {
    appendCabinet(positions, indices, width, depth, height);
  } else if (shape === 'valve') {
    appendValve(positions, indices, width, depth, height);
  } else {
    appendSkid(positions, indices, width, depth, height);
  }
}

function appendRectangularBasin(positions, indices, width, depth, height, seed) {
  const wall = 0.55;
  appendBox(positions, indices, [0, 0.25, 0], [width, 0.5, depth], 0);
  appendBox(positions, indices, [0, height / 2, depth / 2 - wall / 2], [width, height, wall], 0);
  appendBox(positions, indices, [0, height / 2, -depth / 2 + wall / 2], [width, height, wall], 0);
  appendBox(positions, indices, [width / 2 - wall / 2, height / 2, 0], [wall, height, depth], 0);
  appendBox(positions, indices, [-width / 2 + wall / 2, height / 2, 0], [wall, height, depth], 0);
  const bays = 3 + Math.abs(seed) % 3;
  for (let bay = 1; bay < bays; bay += 1) {
    const x = -width / 2 + bay * width / bays;
    appendBox(positions, indices, [x, height * 0.46, 0], [0.35, height * 0.85, depth - 1.2], 0);
  }
  appendBox(positions, indices, [0, height + 0.15, 0], [width + 1.2, 0.28, 1.15], 0);
  appendBox(positions, indices, [0, height + 0.2, depth * 0.3], [width + 0.6, 0.22, 0.9], 0);
}

function appendCircularBasin(positions, indices, radius, height, seed) {
  const segments = 72 + Math.abs(seed) % 32;
  appendCylinder(positions, indices, 0, 0, 0.45, radius, segments, true);
  appendAnnulusWall(
    positions,
    indices,
    0.45,
    height,
    radius - 0.7,
    radius,
    segments,
  );
  appendCylinder(positions, indices, 0, 0, height + 0.15, radius - 1.3, segments, false);
  appendBox(positions, indices, [0, height + 0.7, 0], [radius * 2 - 1.2, 0.42, 1.3], 0);
  appendCylinder(positions, indices, 0, 0, height + 0.6, 2.1, 32, true);
  for (let arm = 0; arm < 4; arm += 1) {
    appendBox(
      positions,
      indices,
      [0, height + 0.55, 0],
      [radius * 2 - 2.2, 0.18, 0.26],
      arm * Math.PI / 4,
    );
  }
}

function appendBuilding(positions, indices, width, depth, height, seed) {
  appendBox(positions, indices, [0, 0.3, 0], [width + 1.4, 0.6, depth + 1.4], 0);
  appendBox(positions, indices, [0, height / 2, 0], [width, height, depth], 0);
  appendBox(positions, indices, [0, height + 0.45, 0], [width + 1.2, 1.1, depth + 1.2], 0);
  appendBox(positions, indices, [0, height * 0.62, 0], [width + 2.4, 0.4, depth + 2.4], 0);
  const bands = 3 + Math.abs(seed) % 3;
  for (let floor = 1; floor <= bands; floor += 1) {
    const y = floor * height / (bands + 1);
    for (let column = -2; column <= 2; column += 1) {
      const x = column * width / 6;
      appendBox(positions, indices, [x, y, depth / 2 + 0.08], [width / 9, 0.9, 0.18], 0);
      appendBox(positions, indices, [x, y, -depth / 2 - 0.08], [width / 9, 0.9, 0.18], 0);
    }
  }
}

function appendDigester(positions, indices, radius, height, seed) {
  const segments = 72 + Math.abs(seed) % 24;
  appendCylinder(positions, indices, 0, 0, height * 0.86, radius, segments, true);
  appendCone(positions, indices, height * 0.86, height, radius, 0, 0, segments);
  for (let band = 1; band <= 4; band += 1) {
    appendAnnulusWall(
      positions,
      indices,
      height * band * 0.18,
      height * band * 0.18 + 0.22,
      radius,
      radius + 0.16,
      segments,
    );
  }
  appendCylinder(positions, indices, 0, 0, 0.8, radius + 1.4, segments, true);
  appendBox(positions, indices, [radius + 1.2, height * 0.42, 0], [0.8, height * 0.75, 1.4], 0);
}

function appendTank(positions, indices, radius, height, seed) {
  const segments = 56 + Math.abs(seed) % 24;
  appendCylinder(positions, indices, 0, 0, height, radius, segments, true);
  appendCylinder(positions, indices, 0, 0, height + 0.15, radius + 0.3, segments, false);
  for (let band = 1; band <= 5; band += 1) {
    appendAnnulusWall(
      positions,
      indices,
      height * band / 6,
      height * band / 6 + 0.22,
      radius,
      radius + 0.22,
      segments,
    );
  }
}

function appendScreen(positions, indices, width, depth, height, seed) {
  appendRectangularBasin(positions, indices, width, depth, height, seed);
  const bars = 18 + Math.abs(seed) % 12;
  for (let bar = 0; bar < bars; bar += 1) {
    const x = -width * 0.34 + bar * width * 0.68 / (bars - 1);
    appendBox(positions, indices, [x, height * 0.65, 0], [0.1, height * 0.8, depth * 0.72], 0);
  }
}

function appendFilter(positions, indices, width, depth, height) {
  const bays = 4;
  for (let bay = 0; bay < bays; bay += 1) {
    const x = -width / 2 + (bay + 0.5) * width / bays;
    appendBox(positions, indices, [x, height / 2, 0], [width / bays - 0.45, height, depth], 0);
  }
  appendBox(positions, indices, [0, height + 0.35, 0], [width + 1, 0.55, 1.1], 0);
  appendCylinder(positions, indices, 0, depth * 0.38, height * 0.55, 0.42, 24, true);
}

function appendSerpentine(positions, indices, width, depth, height) {
  appendBox(positions, indices, [0, 0.25, 0], [width, 0.5, depth], 0);
  const channels = 4;
  for (let channel = 0; channel <= channels; channel += 1) {
    const z = -depth / 2 + channel * depth / channels;
    appendBox(positions, indices, [0, height / 2, z], [width, height, 0.3], 0);
  }
  for (let divider = 0; divider < 3; divider += 1) {
    const x = divider % 2 === 0 ? width * 0.3 : -width * 0.3;
    appendBox(positions, indices, [x, height / 2, 0], [0.3, height, depth * 0.72], 0);
  }
}

function appendConveyor(positions, indices, width, depth, height) {
  appendBox(positions, indices, [0, 1.2, 0], [width, 0.45, depth], 0);
  appendBox(
    positions,
    indices,
    [0, height * 0.55, 0],
    [width * 0.92, 0.5, depth * 0.68],
    Math.PI * 0.07,
  );
  appendBox(positions, indices, [-width * 0.35, 1.8, 0], [1, 3.5, depth], 0);
  appendBox(positions, indices, [width * 0.35, 1.8, 0], [1, 3.5, depth], 0);
}

function appendMixer(positions, indices, width, depth, height, seed) {
  appendCylinder(positions, indices, 0, 0, 1.2, width * 0.38, 40, true);
  appendCylinder(positions, indices, 0, 0, height, 0.24, 24, true);
  appendBox(positions, indices, [0, height * 0.78, 0], [width * 0.7, 1.1, depth * 0.7], seed % 2);
  appendCylinder(positions, indices, 0, 0, height + 0.7, 0.8, 20, true);
}

function appendBridge(positions, indices, width, depth, height) {
  appendBox(positions, indices, [0, height, 0], [width, 0.45, depth], 0);
  for (let support = -2; support <= 2; support += 1) {
    appendBox(
      positions,
      indices,
      [support * width / 5, height / 2, 0],
      [0.28, height, depth * 0.65],
      0,
    );
  }
}

function appendCabinet(positions, indices, width, depth, height) {
  appendBox(positions, indices, [0, height / 2, 0], [width, height, depth], 0);
  appendBox(positions, indices, [0, height + 0.22, 0], [width + 0.4, 0.45, depth + 0.4], 0);
  appendCylinder(positions, indices, 0, 0, height + 1.4, 0.16, 16, true);
}

function appendValve(positions, indices, width, depth, height) {
  appendBox(positions, indices, [0, height * 0.45, 0], [width, 0.65, depth * 0.45], 0);
  appendCylinder(positions, indices, 0, 0, height, 0.8, 24, true);
  appendCylinder(positions, indices, 0, 0, height + 0.5, 1.5, 24, false);
}

function appendSkid(positions, indices, width, depth, height) {
  appendBox(positions, indices, [0, 0.25, 0], [width, 0.5, depth], 0);
  appendBox(
    positions,
    indices,
    [0, height * 0.48, 0],
    [width * 0.7, height * 0.82, depth * 0.68],
    0,
  );
  appendCylinder(
    positions,
    indices,
    width * 0.28,
    0,
    height * 0.9,
    Math.min(1.3, depth * 0.22),
    28,
    true,
  );
}

function appendProfileDetails(profile, positions, indices, seed) {
  const { shape, width, depth, height } = profile;
  const walkwayY = shape === 'building' ? height + 1.1 : height + 0.4;
  if (shape === 'basin' || shape === 'screen' || shape === 'filter' || shape === 'serpentine') {
    for (let rail = -8; rail <= 8; rail += 2) {
      const x = rail * width / 18;
      appendBox(positions, indices, [x, walkwayY, depth / 2 - 0.15], [0.1, 0.9, 0.1], 0);
      appendBox(positions, indices, [x, walkwayY, -depth / 2 + 0.15], [0.1, 0.9, 0.1], 0);
    }
  }
  if (shape === 'clarifier' || shape === 'round-basin' || shape === 'digester' || shape === 'tank') {
    const radius = width / 2;
    for (let post = 0; post < 16; post += 1) {
      const angle = post / 16 * Math.PI * 2;
      appendBox(
        positions,
        indices,
        [Math.cos(angle) * (radius - 0.15), walkwayY, Math.sin(angle) * (radius - 0.15)],
        [0.14, 0.95, 0.14],
        angle,
      );
    }
  }
  if (shape === 'skid' || shape === 'mixer' || shape === 'cabinet' || shape === 'valve') {
    for (let item = 0; item < 6; item += 1) {
      const angle = item * 2.399963 + seed * 0.001;
      appendBox(
        positions,
        indices,
        [Math.cos(angle) * width * 0.22, height * (0.25 + item * 0.08), Math.sin(angle) * depth * 0.22],
        [0.22, 0.18, 0.34],
        angle,
      );
    }
  }
}

function fillBudget(profile, positions, indices, seed, budget) {
  const remaining = budget - indices.length / 3;
  const panels = Math.max(0, Math.floor(remaining / 12));
  const { shape, width, depth, height } = profile;
  for (let index = 0; index < panels; index += 1) {
    const level = Math.floor(index / 64);
    const slot = index % 64;
    const y = Math.min(
      Math.max(0.5, height * (0.12 + level % 12 * 0.065)),
      height + 1.2,
    );
    let x;
    let z;
    let rotation = 0;
    if (shape === 'clarifier' || shape === 'round-basin' || shape === 'digester' || shape === 'tank') {
      const angle = slot / 64 * Math.PI * 2 + seed * 0.002 + level * 0.01;
      const radius = width / 2 - 0.18;
      x = Math.cos(angle) * radius;
      z = Math.sin(angle) * radius;
      rotation = angle;
    } else if (shape === 'building') {
      const columns = 16;
      const column = slot % columns;
      const side = Math.floor(slot / columns) % 4;
      x = side >= 2 ? (side === 2 ? 1 : -1) * width / 2 : -width / 2 + column * width / (columns - 1);
      z = side >= 2 ? -depth / 2 + column * depth / (columns - 1) : (side === 0 ? depth / 2 : -depth / 2);
      rotation = side >= 2 ? Math.PI / 2 : 0;
    } else {
      const side = level % 4;
      const fraction = slot / 63;
      x = side < 2 ? -width / 2 + fraction * width : (side === 2 ? width / 2 : -width / 2);
      z = side < 2 ? (side === 0 ? depth / 2 : -depth / 2) : -depth / 2 + fraction * depth;
      rotation = side >= 2 ? Math.PI / 2 : 0;
    }
    appendBox(
      positions,
      indices,
      [x, y, z],
      [0.18, Math.min(0.4, Math.max(0.08, height * 0.035)), 0.44],
      rotation,
    );
  }
}

function appendCylinder(
  positions,
  indices,
  centerX,
  centerZ,
  top,
  radius,
  segments,
  caps,
) {
  const base = positions.length;
  for (const y of [0, top]) {
    for (let segment = 0; segment <= segments; segment += 1) {
      const angle = segment / segments * Math.PI * 2;
      positions.push([
        centerX + Math.cos(angle) * radius,
        y,
        centerZ + Math.sin(angle) * radius,
      ]);
    }
  }
  const stride = segments + 1;
  for (let segment = 0; segment < segments; segment += 1) {
    const a = base + segment;
    const b = a + stride;
    indices.push(a, b, a + 1, a + 1, b, b + 1);
  }
  if (caps) {
    const bottomCenter = positions.length;
    positions.push([centerX, 0, centerZ]);
    const topCenter = positions.length;
    positions.push([centerX, top, centerZ]);
    for (let segment = 0; segment < segments; segment += 1) {
      indices.push(bottomCenter, base + segment + 1, base + segment);
      indices.push(topCenter, base + stride + segment, base + stride + segment + 1);
    }
  }
}

function appendAnnulusWall(
  positions,
  indices,
  bottom,
  top,
  innerRadius,
  outerRadius,
  segments,
) {
  const base = positions.length;
  for (const radius of [innerRadius, outerRadius]) {
    for (const y of [bottom, top]) {
      for (let segment = 0; segment <= segments; segment += 1) {
        const angle = segment / segments * Math.PI * 2;
        positions.push([
          Math.cos(angle) * radius,
          y,
          Math.sin(angle) * radius,
        ]);
      }
    }
  }
  const ring = segments + 1;
  const lowerInner = base;
  const upperInner = base + ring;
  const lowerOuter = base + ring * 2;
  const upperOuter = base + ring * 3;
  for (let segment = 0; segment < segments; segment += 1) {
    indices.push(
      lowerInner + segment,
      upperInner + segment,
      lowerInner + segment + 1,
      lowerInner + segment + 1,
      upperInner + segment,
      upperInner + segment + 1,
      lowerOuter + segment + 1,
      upperOuter + segment + 1,
      lowerOuter + segment,
      lowerOuter + segment,
      upperOuter + segment + 1,
      upperOuter + segment,
      upperInner + segment,
      upperInner + segment + 1,
      upperOuter + segment,
      upperInner + segment + 1,
      upperOuter + segment + 1,
      upperOuter + segment,
    );
  }
}

function appendCone(positions, indices, bottom, top, radius, centerX, centerZ, segments) {
  const base = positions.length;
  positions.push([centerX, top, centerZ]);
  for (let segment = 0; segment <= segments; segment += 1) {
    const angle = segment / segments * Math.PI * 2;
    positions.push([
      centerX + Math.cos(angle) * radius,
      bottom,
      centerZ + Math.sin(angle) * radius,
    ]);
  }
  for (let segment = 0; segment < segments; segment += 1) {
    indices.push(base, base + segment + 1, base + segment + 2);
  }
}

function appendBox(positions, indices, center, size, rotation) {
  const halfX = size[0] / 2;
  const halfY = size[1] / 2;
  const halfZ = size[2] / 2;
  const cosine = Math.cos(rotation);
  const sine = Math.sin(rotation);
  const base = positions.length;
  for (const x of [-halfX, halfX]) {
    for (const y of [-halfY, halfY]) {
      for (const z of [-halfZ, halfZ]) {
        positions.push([
          center[0] + x * cosine - z * sine,
          center[1] + y,
          center[2] + x * sine + z * cosine,
        ]);
      }
    }
  }
  indices.push(
    base + 0, base + 1, base + 3, base + 0, base + 3, base + 2,
    base + 4, base + 6, base + 7, base + 4, base + 7, base + 5,
    base + 0, base + 4, base + 5, base + 0, base + 5, base + 1,
    base + 2, base + 3, base + 7, base + 2, base + 7, base + 6,
    base + 0, base + 2, base + 6, base + 0, base + 6, base + 4,
    base + 1, base + 5, base + 7, base + 1, base + 7, base + 3,
  );
}
