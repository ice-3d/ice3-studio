struct Planes {
    values: array<vec4<f32>, 6>,
};

@group(0) @binding(0) var<storage, read> instance_matrices: array<mat4x4<f32>>;
@group(0) @binding(1) var<storage, read> instance_bounds: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> planes: Planes;
@group(0) @binding(3) var<storage, read_write> visible_indices: array<u32>;
@group(0) @binding(4) var<storage, read_write> visible_count: atomic<u32>;

@compute @workgroup_size(64)
fn cull_instances(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    let count = arrayLength(&instance_bounds);
    if (index >= count) {
        return;
    }
    let centre = instance_bounds[index].xyz;
    let radius = instance_bounds[index].w;
    for (var plane_index = 0u; plane_index < 6u; plane_index += 1u) {
        let plane = planes.values[plane_index];
        if (dot(plane.xyz, centre) + plane.w < -radius) {
            return;
        }
    }
    let slot = atomicAdd(&visible_count, 1u);
    visible_indices[slot] = index;
}
