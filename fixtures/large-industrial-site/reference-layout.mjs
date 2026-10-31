const TRAIN_SUFFIXES = Object.freeze([101, 201, 301, 401, 501, 601]);
const CLARIFIER_SUFFIXES = Object.freeze([
  101, 201, 301, 401, 501, 601, 701, 801, 901, 1001,
]);
const PRIMARY_SUFFIXES = Object.freeze([101, 201, 301, 401]);

const compactCenters = new Map();

for (const [index, suffix] of TRAIN_SUFFIXES.entries()) {
  const z = 190 + index * 20;
  compactCenters.set(`AT-${suffix}`, { x: 330, z });
  compactCenters.set(`AX-${suffix}`, { x: 355, z });
  compactCenters.set(`AE-${suffix}`, { x: 380, z });
}
for (const [index, suffix] of PRIMARY_SUFFIXES.entries()) {
  compactCenters.set(`PC-${suffix}`, {
    x: 280 + (index % 2) * 25,
    z: 190 + Math.floor(index / 2) * 20,
  });
}

for (const [index, suffix] of CLARIFIER_SUFFIXES.entries()) {
  compactCenters.set(`SC-${suffix}`, {
    x: 418 + (index % 2) * 28,
    z: 190 + Math.floor(index / 2) * 20,
  });
}

for (const [index, suffix] of [301, 401, 501, 601].entries()) {
  const z = 230 + index * 20;
  compactCenters.set(`P-IRC-${suffix}`, { x: 480, z });
  compactCenters.set(`BLW-B-${suffix}`, { x: 500, z });
  compactCenters.set(`P-RAS-${suffix}`, { x: 480, z: z + 7 });
}

for (const [tag, x, z] of [
  ['P-RAS-101', 480, 190],
  ['P-RAS-201', 480, 210],
  ['P-WAS-301', 500, 230],
  ['P-WAS-401', 500, 250],
  ['P-WAS-402', 500, 270],
  ['P-LIFT-111', 500, 190],
  ['P-LIFT-112', 500, 210],
  ['P-LIFT-211', 500, 230],
  ['P-LIFT-212', 500, 250],
  ['P-LIFT-311', 500, 270],
  ['P-LIFT-312', 500, 290],
]) {
  compactCenters.set(tag, { x, z });
}

export function compactLayoutTranslation(equipment) {
  const target = compactCenters.get(equipment.tag);
  if (!target || !equipment.bounds) return null;
  const centerX = (equipment.bounds.min[0] + equipment.bounds.max[0]) / 2;
  const centerZ = (equipment.bounds.min[2] + equipment.bounds.max[2]) / 2;
  return {
    x: target.x - centerX,
    y: 0,
    z: target.z - centerZ,
  };
}
