struct MeshParams {
    vertex_count: u32,
    triangle_count: u32,
    workgroup_count: u32,
    reserved: u32,
}

@group(0) @binding(0) var<uniform> params: MeshParams;
@group(0) @binding(1) var<storage, read> positions: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> indices: array<u32>;
@group(0) @binding(3) var<storage, read> offsets: array<u32>;
@group(0) @binding(4) var<storage, read> incidence: array<u32>;
@group(0) @binding(5) var<storage, read_write> face_normals: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> normals: array<vec4<f32>>;
@group(0) @binding(7) var<storage, read_write> bounds_partials: array<vec4<f32>>;

const NORMAL_EPSILON: f32 = 0.00000011920928955078125;
const WORKGROUP_SIZE: u32 = 64u;
const LARGE_VALUE: f32 = 3.402823466e+38;

@compute @workgroup_size(64)
fn generate_face_normals(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let triangle = invocation.x;
    if (triangle >= params.triangle_count) {
        return;
    }
    let base = triangle * 3u;
    let a = indices[base];
    let b = indices[base + 1u];
    let c = indices[base + 2u];
    let ab = positions[b].xyz - positions[a].xyz;
    let ac = positions[c].xyz - positions[a].xyz;
    face_normals[triangle] = vec4<f32>(cross(ab, ac), 0.0);
}

@compute @workgroup_size(64)
fn accumulate_normals(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let vertex = invocation.x;
    if (vertex >= params.vertex_count) {
        return;
    }
    var accumulated = vec3<f32>(0.0);
    let start = offsets[vertex];
    let end = offsets[vertex + 1u];
    for (var cursor = start; cursor < end; cursor += 1u) {
        accumulated += face_normals[incidence[cursor]].xyz;
    }
    let length = sqrt(dot(accumulated, accumulated));
    let normal = select(vec3<f32>(0.0, 1.0, 0.0), accumulated / length, length > NORMAL_EPSILON);
    normals[vertex] = vec4<f32>(normal, 0.0);
}

var<workgroup> minimum_shared: array<vec3<f32>, 64>;
var<workgroup> maximum_shared: array<vec3<f32>, 64>;

@compute @workgroup_size(64)
fn reduce_bounds(
    @builtin(global_invocation_id) invocation: vec3<u32>,
    @builtin(local_invocation_id) local_invocation: vec3<u32>,
    @builtin(workgroup_id) workgroup: vec3<u32>,
) {
    let vertex = invocation.x;
    let lane = local_invocation.x;
    var minimum = vec3<f32>(LARGE_VALUE);
    var maximum = vec3<f32>(-LARGE_VALUE);
    if (vertex < params.vertex_count) {
        let position = positions[vertex].xyz;
        minimum = position;
        maximum = position;
    }
    minimum_shared[lane] = minimum;
    maximum_shared[lane] = maximum;
    workgroupBarrier();

    var stride = WORKGROUP_SIZE / 2u;
    loop {
        if (stride == 0u) {
            break;
        }
        if (lane < stride) {
            minimum_shared[lane] = min(minimum_shared[lane], minimum_shared[lane + stride]);
            maximum_shared[lane] = max(maximum_shared[lane], maximum_shared[lane + stride]);
        }
        workgroupBarrier();
        stride /= 2u;
    }

    if (lane == 0u) {
        bounds_partials[workgroup.x * 2u] = vec4<f32>(minimum_shared[0], 0.0);
        bounds_partials[workgroup.x * 2u + 1u] = vec4<f32>(maximum_shared[0], 0.0);
    }
}
