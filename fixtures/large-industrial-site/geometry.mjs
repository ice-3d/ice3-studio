import { TEMPLATE_TRIANGLE_BUDGETS } from './templates.mjs';

export function createEquipmentMesh(kind, ordinal) {
  const budget = TEMPLATE_TRIANGLE_BUDGETS[kind];
  const positions = [];
  const indices = [];
  const seed = [...kind].reduce(
    (value, character) => value * 31 + character.charCodeAt(0),
    ordinal,
  );
  addTaperedShell(positions, indices, seed);
  addServiceDeck(positions, indices, seed);
  addInstrumentation(positions, indices, seed);

  const remainingTriangles = Math.max(0, budget - indices.length / 3);
  const panelCount = Math.floor(remainingTriangles / 12);
  addStructuralPanels(positions, indices, seed, panelCount);
  return { positions, indices };
}

function addTaperedShell(positions, indices, seed) {
  const segments = 64 + (Math.abs(seed) % 4) * 16;
  const rings = 16;
  for (let ring = 0; ring <= rings; ring += 1) {
    const y = ring * 0.4;
    const radius = 3.2 + Math.sin(ring / rings * Math.PI) * 0.4;
    for (let segment = 0; segment <= segments; segment += 1) {
      const angle = segment / segments * Math.PI * 2;
      positions.push([
        Math.cos(angle) * radius,
        y,
        Math.sin(angle) * radius,
      ]);
    }
  }
  const stride = segments + 1;
  for (let ring = 0; ring < rings; ring += 1) {
    for (let segment = 0; segment < segments; segment += 1) {
      const a = ring * stride + segment;
      const b = a + stride;
      indices.push(a, b, a + 1, a + 1, b, b + 1);
    }
  }
}

function addServiceDeck(positions, indices, seed) {
  const count = 96 + Math.abs(seed) % 64;
  const base = positions.length;
  positions.push([0, 6.1, 0]);
  for (let index = 0; index <= count; index += 1) {
    const angle = index / count * Math.PI * 2;
    positions.push([
      Math.cos(angle) * 4.2,
      6.1,
      Math.sin(angle) * 4.2,
    ]);
  }
  for (let index = 0; index < count; index += 1) {
    indices.push(base, base + index + 1, base + index + 2);
  }
}

function addInstrumentation(positions, indices, seed) {
  for (let item = 0; item < 24 + Math.abs(seed) % 17; item += 1) {
    const angle = item * 2.399963;
    const base = positions.length;
    const x = Math.cos(angle) * 3.6;
    const z = Math.sin(angle) * 3.6;
    positions.push(
      [x, 4.0, z],
      [x + 0.12, 4.4, z],
      [x, 4.8, z],
      [x - 0.12, 4.4, z],
    );
    indices.push(base, base + 1, base + 2, base, base + 2, base + 3);
  }
}

function addStructuralPanels(positions, indices, seed, count) {
  const slots = 32;
  for (let index = 0; index < count; index += 1) {
    const level = Math.floor(index / slots);
    const slot = index % slots;
    const angle = slot / slots * Math.PI * 2 + (seed % 7) * 0.01;
    const radius = 3.75 + level % 3 * 0.04;
    appendBox(
      positions,
      indices,
      [
        Math.cos(angle) * radius,
        0.45 + level * 0.32,
        Math.sin(angle) * radius,
      ],
      [0.18, 0.08, 0.42],
      angle,
    );
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
