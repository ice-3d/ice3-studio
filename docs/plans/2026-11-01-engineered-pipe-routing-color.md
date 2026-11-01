# Engineered Pipe Routing and Functional Color Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-017
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [x]`) syntax for tracking.

**Goal:** 让污水处理厂管线在近景连续、平滑、可辨，并按亮银普通水线、绿色出水、蓝色气体、
红色污泥和黄色药剂分组着色。

**Architecture:** 在 Studio Core 中把直角折线替换为采样后的圆滑中心线，并把扫掠截面
改为平行传输坐标系，保持整根管道连续且不扭结。资产层继续通过公开 Operation 指定介质
和材质，不把污水处理业务语义写入 Core。

**Tech Stack:** Rust 1.93、Cargo、Node.js 24.22.1、JavaScript ESM、Node Test、
Studio JSON-RPC、React Viewer、Playwright、Google Chrome

**Spec:** `docs/specs/2026-11-01-wastewater-process-piping-correction-design.md`

## Global Constraints

- Studio Core 不增加污水处理业务术语或案例专用类型。
- Document 和 operation history 是唯一真源；GLB、route mesh 和材质结果都是派生值。
- 不增加 Studio Core、Viewer 或 Toolchain 的运行时依赖。
- 所有生产行为先写失败测试，再写最小实现。
- Git author 和 committer 必须为 `damoqiongqiu <damoqiongqiu@126.com>`。
- 不推送远端；当前任务只产生本地提交。
- 工程必须保持 300,000 至 500,000 有效三角形、最多 150,000 唯一三角形、
  15,000 展开节点、500 实例和 48 材质。
- 管线颜色按功能分组，亮银、绿色、蓝色、红色和黄色必须一眼可辨。
- 验收截图写入现有 `output/`，该目录保持 git-ignored。

> **Implementation note:** 最终验收配色覆盖了 Task 1 初稿：普通水线由黄色改为亮银色，
> 化学药剂由紫色改为黄色；下方 Task 1 保留原始 TDD 记录。

## Review Focus

- 任意两个相邻路径点重合、弯曲采样为零或方向反转时，必须失败，不能生成断裂网格。
- 弯头处相邻截面的对应顶点不能翻转，也不能出现明显的麻花状扭曲。
- 直管、埋地管、管廊管和不同管径必须共享同一连续路径语义。
- 相同介质在所有连接、法兰和主管上使用同一功能色，不能落回默认工艺色。
- 管线几何增加后必须保持在现有性能与三角形预算内。

---

## File Structure

### Modified files

- `crates/studio-core/src/large_scene/route.rs`：生成带圆滑弯头的连续路径点。
- `crates/studio-core/src/geometry/sweep.rs`：采用平行传输截面坐标系。
- `crates/studio-core/tests/connections.rs`：弯头路径与退化输入测试。
- `crates/studio-core/tests/geometry_evaluation.rs`：扫掠连续性测试。
- `scripts/wastewater-plant/site-plan.mjs`：功能色材质定义。
- `scripts/tests/wastewater-plant-model.test.mjs`：颜色分组与资产颜色测试。
- `projects/wastewater-plant/**`：经 Studio API 重建的工程。
- `projects/README.md`：管线颜色和阶段二状态。
- `docs/README.md`：阶段二计划完成后的状态。

### Output files

- `output/wwtp-pipe-colors-overview.png`
- `output/wwtp-pipe-colors-close.png`

---

### Task 1: Apply Functional Pipe Colors

**Files:**
- Modify: `scripts/wastewater-plant/site-plan.mjs`
- Test: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Consumes: `PIPE_MATERIAL` from `scripts/wastewater-plant/model-kit.mjs`.
- Produces: material names and base colors grouped by `yellow`, `green`, `blue`,
  `red`, and `chemical`.

- [x] **Step 1: Write failing functional-color tests**

Add:

```js
test('pipe materials use functional yellow, green, blue and red groups', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const materials = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ name, base_color }) => [name, base_color.slice(0, 3)]),
  );
  const groups = {
    yellow: [
      'Pipe / raw water',
      'Pipe / process water',
    ],
    green: ['Pipe / clean water'],
    blue: ['Pipe / air', 'Pipe / biogas'],
    red: [
      'Pipe / sludge',
      'Pipe / primary sludge',
      'Pipe / return sludge',
    ],
  };
  for (const name of groups.yellow) {
    const [r, g, b] = materials.get(name);
    assert.ok(r > b * 1.8 && g > b * 1.5, name);
  }
  for (const name of groups.green) {
    const [r, g, b] = materials.get(name);
    assert.ok(g > r * 1.25 && g > b * 1.05, name);
  }
  for (const name of groups.blue) {
    const [r, g, b] = materials.get(name);
    assert.ok(b > r * 1.6 && b > g * 1.15, name);
  }
  for (const name of groups.red) {
    const [r, g, b] = materials.get(name);
    assert.ok(r > g * 1.7 && r > b * 1.5, name);
  }
  const chemical = materials.get('Pipe / chemical');
  assert.ok(chemical[0] > chemical[1] * 1.3);
  assert.ok(chemical[2] > chemical[1] * 1.2);
});
```

- [x] **Step 2: Run the focused test and verify RED**

Run:

```bash
node --test --test-name-pattern="functional yellow" \
  scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL because the current muted materials do not satisfy the functional
color contract.

- [x] **Step 3: Replace pipe material colors**

Use these material colors in `site-plan.mjs`:

```js
b.material('pipe-raw', 'Pipe / raw water', srgb('#d9b62d'));
b.material('pipe-process', 'Pipe / process water', srgb('#c89f22'));
b.material('pipe-clean', 'Pipe / clean water', srgb('#3f9f62'));
b.material('pipe-sludge', 'Pipe / sludge', srgb('#c43b39'));
b.material('pipe-primary-sludge', 'Pipe / primary sludge', srgb('#b93232'));
b.material('pipe-return-sludge', 'Pipe / return sludge', srgb('#db4945'));
b.material('pipe-air', 'Pipe / air', srgb('#3479c8'));
b.material('pipe-biogas', 'Pipe / biogas', srgb('#2766ad'));
b.material('pipe-chemical', 'Pipe / chemical', srgb('#8b5fc0'));
```

Retain the existing `PIPE_MATERIAL` medium mapping so every connection resolves to
one of these names.

- [x] **Step 4: Run color and complete asset tests**

Run:

```bash
node --test --test-name-pattern="functional yellow" \
  scripts/tests/wastewater-plant-model.test.mjs
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add scripts/wastewater-plant/site-plan.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "feat(wwtp): apply functional pipe colors"
```

---

### Task 2: Generate Continuous Rounded Route Curves

**Files:**
- Modify: `crates/studio-core/src/large_scene/route.rs`
- Test: `crates/studio-core/tests/connections.rs`

**Interfaces:**
- Consumes:
  `route_between_ports(&ResolvedPort, &ResolvedPort, &RoutePolicy) -> Result<Vec<[f32; 3]>, ConnectionError>`.
- Produces the same signature, but adjacent non-endpoint direction changes are
  sampled over `8` intermediate steps.
- Produces private:
  `smooth_route(points: &[[f32; 3]], bend_radius: f32) -> Result<Vec<[f32; 3]>, ConnectionError>`.

- [x] **Step 1: Write failing bend-continuity tests**

Add to `crates/studio-core/tests/connections.rs`:

```rust
#[test]
fn orthogonal_route_uses_continuous_rounded_bends() {
    let points = route_between_ports(
        &port([0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], "water"),
        &port([10.0, 2.0, 4.0], [1.0, 0.0, 0.0], "water"),
        &RoutePolicy::Orthogonal,
    )
    .unwrap();

    for pair in points.windows(2) {
        let dx = pair[1][0] - pair[0][0];
        let dy = pair[1][1] - pair[0][1];
        let dz = pair[1][2] - pair[0][2];
        assert!((dx * dx + dy * dy + dz * dz).sqrt() > 1.0e-4);
    }

    let mut maximum_turn = 0.0_f32;
    for pair in points.windows(3) {
        let a = direction(pair[0], pair[1]);
        let b = direction(pair[1], pair[2]);
        let cosine = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        maximum_turn = maximum_turn.max(cosine.clamp(-1.0, 1.0).acos());
    }
    assert!(maximum_turn < 25.0_f32.to_radians());
}

fn direction(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let value = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let length = (value[0] * value[0] + value[1] * value[1]
        + value[2] * value[2]).sqrt();
    [value[0] / length, value[1] / length, value[2] / length]
}
```

- [x] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test -p studio-core --test connections \
  orthogonal_route_uses_continuous_rounded_bends -- --exact
```

Expected: FAIL because existing route joints turn by approximately 90 degrees in one
step.

- [x] **Step 3: Implement rounded route sampling**

Change `build_route` to accept `diameter` and return `Result`.

```rust
const BEND_SAMPLES: usize = 8;
const BEND_RADIUS_DIAMETERS: f32 = 2.0;

fn build_route(
    start: Vec3,
    start_direction: Vec3,
    end: Vec3,
    end_direction: Vec3,
    elevation: Option<f32>,
    diameter: f32,
) -> Result<Vec<[f32; 3]>, ConnectionError> {
    let Some(elevation) = elevation else {
        return Ok(vec![[start.x, start.y, start.z], [end.x, end.y, end.z]]);
    };
    let start_stub = add_scaled(start, start_direction, 0.9);
    let end_stub = add_scaled(end, end_direction, 0.9);
    let coarse = vec![
        [start.x, start.y, start.z],
        [start_stub.x, start_stub.y, start_stub.z],
        [start_stub.x, elevation, start_stub.z],
        [end_stub.x, elevation, start_stub.z],
        [end_stub.x, elevation, end_stub.z],
        [end_stub.x, end_stub.y, end_stub.z],
        [end.x, end.y, end.z],
    ];
    smooth_route(&coarse, diameter * BEND_RADIUS_DIAMETERS)
}
```

Implement `smooth_route` as quadratic Bezier fillets:

```rust
fn smooth_route(
    points: &[[f32; 3]],
    bend_radius: f32,
) -> Result<Vec<[f32; 3]>, ConnectionError> {
    if points.len() < 2 || !bend_radius.is_finite() || bend_radius <= 0.0 {
        return Err(ConnectionError::Degenerate);
    }
    let mut output = vec![points[0]];
    for index in 1..points.len() - 1 {
        let previous = points[index - 1];
        let corner = points[index];
        let next = points[index + 1];
        let incoming = unit(subtract(corner, previous));
        let outgoing = unit(subtract(next, corner));
        let trim = bend_radius
            .min(distance3(previous, corner) * 0.4)
            .min(distance3(corner, next) * 0.4);
        if trim <= f32::EPSILON {
            return Err(ConnectionError::Degenerate);
        }
        let before = add3(corner, scale(incoming, -trim));
        let after = add3(corner, scale(outgoing, trim));
        push_unique(&mut output, before);
        for step in 1..=BEND_SAMPLES {
            let t = step as f32 / BEND_SAMPLES as f32;
            let one = 1.0 - t;
            let sample = [
                one * one * before[0] + 2.0 * one * t * corner[0] + t * t * after[0],
                one * one * before[1] + 2.0 * one * t * corner[1] + t * t * after[1],
                one * one * before[2] + 2.0 * one * t * corner[2] + t * t * after[2],
            ];
            push_unique(&mut output, sample);
        }
    }
    push_unique(&mut output, points[points.len() - 1]);
    Ok(output)
}
```

Add private vector helpers `unit`, `subtract`, `scale`, `add3`, and `distance3`.
Collinear adjacent segments should skip the fillet and append the next endpoint.

- [x] **Step 4: Run route and geometry tests**

Run:

```bash
cargo test -p studio-core --test connections
cargo test -p studio-core --test geometry_evaluation
```

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene/route.rs \
  crates/studio-core/tests/connections.rs
git commit -m "feat(core): round orthogonal pipe bends"
```

---

### Task 3: Keep Pipe Cross-Sections Parallel Along Bends

**Files:**
- Modify: `crates/studio-core/src/geometry/sweep.rs`
- Test: `crates/studio-core/tests/geometry_evaluation.rs`

**Interfaces:**
- Consumes: `evaluate_sweep(points, radius, radial_segments)`.
- Produces: the same signature with parallel-transported ring frames.

- [x] **Step 1: Write a failing parallel-frame test**

Add:

```rust
#[test]
fn swept_pipe_frames_do_not_twist_at_bends() {
    let points = [
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [2.8, 0.4, 0.0],
        [3.0, 1.0, 0.0],
        [3.0, 3.0, 0.0],
    ];
    let mesh = evaluate_node_content(&NodeContent::Sweep {
        points: points.into(),
        radius: 0.2,
        radial_segments: 16,
    })
    .unwrap();
    for ring in 0..points.len() - 1 {
        for segment in 0..16 {
            let a = mesh.vertices[ring * 16 + segment].normal;
            let b = mesh.vertices[(ring + 1) * 16 + segment].normal;
            let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            assert!(dot > 0.65, "frame flipped at ring {ring}, segment {segment}");
        }
    }
}
```

- [x] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test -p studio-core --test geometry_evaluation \
  swept_pipe_frames_do_not_twist_at_bends -- --exact
```

Expected: FAIL for at least one corresponding normal pair because each ring currently
recomputes a global reference frame.

- [x] **Step 3: Implement parallel transport**

Initialize the first ring frame from the first tangent. For each next point:

```rust
let tangent = unit(subtract(points[index], points[index - 1]));
let axis = cross(previous_tangent, tangent);
let axis_length = length(axis);
let angle = dot(previous_tangent, tangent).clamp(-1.0, 1.0).acos();
if axis_length > f32::EPSILON {
    side = rotate_around_axis(side, scale(axis, 1.0 / axis_length), angle);
    up = rotate_around_axis(up, scale(axis, 1.0 / axis_length), angle);
}
previous_tangent = tangent;
```

`rotate_around_axis(v, axis, angle)` uses Rodrigues:

```rust
v * cos + cross(axis, v) * sin + axis * dot(axis, v) * (1.0 - cos)
```

Generate ring normals from the transported `side` and `up`, not from a fresh global
reference. Keep endpoints capped and preserve triangle indexing.

- [x] **Step 4: Run geometry and full Core tests**

Run:

```bash
cargo test -p studio-core --test geometry_evaluation
cargo test -p studio-core
```

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add crates/studio-core/src/geometry/sweep.rs \
  crates/studio-core/tests/geometry_evaluation.rs
git commit -m "fix(core): keep pipe sweeps from twisting"
```

---

### Task 4: Rebuild and Verify Colored Rounded Pipes

**Files:**
- Generated: `projects/wastewater-plant/**`
- Modify: `projects/README.md`
- Modify: `docs/README.md`
- Output: `output/wwtp-pipe-colors-overview.png`
- Output: `output/wwtp-pipe-colors-close.png`

**Interfaces:**
- Consumes: complete Core routing, sweep and asset color changes.
- Produces: a Studio API-generated project and Chrome visual evidence.

- [x] **Step 1: Rebuild the project through Studio**

Run:

```bash
node scripts/build-wastewater-model.mjs --workspace ./projects
```

Expected: Revision 1, all process graph checks pass, and the triangle budget remains
inside 300,000–500,000.

- [x] **Step 2: Restart the Viewer against the new project**

Start the server with the standard local command, then verify:

```bash
curl -s -X POST -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"system.health","params":{}}' \
  http://127.0.0.1:8150/rpc
```

Expected: `status: "ok"`.

- [x] **Step 3: Capture overview and close pipe views**

Use:

```text
overview: view=20,34,540
close:    target=270,2,0 view=30,28,145
```

Expected:

- Yellow and green water lines are visibly distinct.
- Blue gas/air lines are visible against grass and roads.
- Red sludge lines are visible and not confused with road markings.
- Pipe bends curve continuously without collapsed rings or open joints.
- No black viewport or console errors.

- [x] **Step 4: Update docs and status**

Add the final functional color mapping and routing behavior to
`projects/README.md`. Mark PLAN-017 Implemented after all gates pass. Keep SPEC-013
Approved until path collision and support-spacing validation complete the remaining Stage 2 work.

- [x] **Step 5: Run full verification**

Run:

```bash
npm run verify
```

Expected: all Studio gates pass.

- [x] **Step 6: Commit**

```bash
git add projects/wastewater-plant projects/README.md docs/README.md \
  docs/plans/2026-11-01-engineered-pipe-routing-color.md
git commit -m "feat(wwtp): render rounded functional pipe network"
```

---

## Stage 2B Handoff

本计划交付连续管线几何和功能配色。完成后再写独立的碰撞与支吊架计划，覆盖：

- 管沟和池体碰撞；
- 地上管线支撑间距；
- 穿墙套管与井盖位置；
- 弯头、三通和变径附件。
