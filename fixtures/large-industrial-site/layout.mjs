export const SITE_BOUNDS = Object.freeze({
  min: [0, 0, 0],
  max: [1000, 0, 550],
});

export const AREA_BOUNDS = Object.freeze({
  intake: [30, 30, 190, 490],
  primary: [210, 40, 130, 500],
  'biological-a': [350, 80, 110, 210],
  'biological-b': [350, 290, 110, 230],
  secondary: [480, 50, 160, 480],
  advanced: [650, 50, 150, 480],
  solids: [170, 360, 290, 170],
  utilities: [40, 300, 920, 100],
});

export function areaPosition(area, ordinal, total) {
  const [x, z, width, depth] = AREA_BOUNDS[area];
  const columns = Math.max(1, Math.ceil(Math.sqrt(total * width / depth)));
  const rows = Math.max(1, Math.ceil(total / columns));
  const column = ordinal % columns;
  const row = Math.floor(ordinal / columns);
  return {
    x: x + (columns === 1 ? width / 2 : column * width / (columns - 1)),
    y: 0,
    z: z + (rows === 1 ? depth / 2 : row * depth / (rows - 1)),
  };
}

export const FACILITY_SLOTS = Object.freeze(
  Array.from({ length: 12 }, (_, index) => [
    60 + index * 80,
    520,
  ]),
);

export function facilityPosition(ordinal) {
  const [x, z] = FACILITY_SLOTS[ordinal];
  return { x, y: 0, z };
}
