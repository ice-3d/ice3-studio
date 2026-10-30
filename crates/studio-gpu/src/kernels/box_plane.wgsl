struct PrimitiveParams {
    dimensions: vec4<f32>,
    vertex_count: u32,
    index_count: u32,
    reserved_0: u32,
    reserved_1: u32,
}

struct GpuVertex {
    position: vec4<f32>,
    normal: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: PrimitiveParams;
@group(0) @binding(1) var<storage, read_write> vertices: array<GpuVertex>;
@group(0) @binding(2) var<storage, read_write> indices: array<u32>;

fn box_position(face: u32, corner: u32, half_extent: vec3<f32>) -> vec3<f32> {
    let x = half_extent.x;
    let y = half_extent.y;
    let z = half_extent.z;

    var positions = array<vec3<f32>, 4>(
        vec3<f32>(x, -y, z),
        vec3<f32>(x, -y, -z),
        vec3<f32>(x, y, -z),
        vec3<f32>(x, y, z),
    );
    if (face == 1u) {
        positions = array<vec3<f32>, 4>(
            vec3<f32>(-x, -y, -z),
            vec3<f32>(-x, -y, z),
            vec3<f32>(-x, y, z),
            vec3<f32>(-x, y, -z),
        );
    } else if (face == 2u) {
        positions = array<vec3<f32>, 4>(
            vec3<f32>(-x, y, z),
            vec3<f32>(x, y, z),
            vec3<f32>(x, y, -z),
            vec3<f32>(-x, y, -z),
        );
    } else if (face == 3u) {
        positions = array<vec3<f32>, 4>(
            vec3<f32>(-x, -y, -z),
            vec3<f32>(x, -y, -z),
            vec3<f32>(x, -y, z),
            vec3<f32>(-x, -y, z),
        );
    } else if (face == 4u) {
        positions = array<vec3<f32>, 4>(
            vec3<f32>(-x, -y, z),
            vec3<f32>(x, -y, z),
            vec3<f32>(x, y, z),
            vec3<f32>(-x, y, z),
        );
    } else if (face == 5u) {
        positions = array<vec3<f32>, 4>(
            vec3<f32>(x, -y, -z),
            vec3<f32>(-x, -y, -z),
            vec3<f32>(-x, y, -z),
            vec3<f32>(x, y, -z),
        );
    }
    return positions[corner];
}

fn box_normal(face: u32) -> vec3<f32> {
    if (face == 0u) {
        return vec3<f32>(1.0, 0.0, 0.0);
    }
    if (face == 1u) {
        return vec3<f32>(-1.0, 0.0, 0.0);
    }
    if (face == 2u) {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    if (face == 3u) {
        return vec3<f32>(0.0, -1.0, 0.0);
    }
    if (face == 4u) {
        return vec3<f32>(0.0, 0.0, 1.0);
    }
    return vec3<f32>(0.0, 0.0, -1.0);
}

@compute @workgroup_size(64)
fn generate_box(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if (index < params.vertex_count) {
        let face = index / 4u;
        let corner = index % 4u;
        let half_extent = params.dimensions.xyz * 0.5;
        vertices[index] = GpuVertex(
            vec4<f32>(box_position(face, corner, half_extent), 1.0),
            vec4<f32>(box_normal(face), 0.0),
        );
    }
    if (index < params.index_count) {
        let face = index / 6u;
        let corner_index = index % 6u;
        var pattern = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
        indices[index] = face * 4u + pattern[corner_index];
    }
}

@compute @workgroup_size(64)
fn generate_plane(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if (index < params.vertex_count) {
        let half_width = params.dimensions.x * 0.5;
        let half_depth = params.dimensions.z * 0.5;
        var positions = array<vec4<f32>, 4>(
            vec4<f32>(-half_width, 0.0, half_depth, 1.0),
            vec4<f32>(half_width, 0.0, half_depth, 1.0),
            vec4<f32>(half_width, 0.0, -half_depth, 1.0),
            vec4<f32>(-half_width, 0.0, -half_depth, 1.0),
        );
        vertices[index] = GpuVertex(
            positions[index],
            vec4<f32>(0.0, 1.0, 0.0, 0.0),
        );
    }
    if (index < params.index_count) {
        var pattern = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
        indices[index] = pattern[index];
    }
}
