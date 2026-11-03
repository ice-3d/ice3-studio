struct Params {
    dimensions: vec4<f32>,
    vertex_count: u32,
    index_count: u32,
    item_count: u32,
    point_count: u32,
}

struct GpuVertex {
    position: vec4<f32>,
    normal: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> vertices: array<GpuVertex>;
@group(0) @binding(2) var<storage, read_write> indices: array<u32>;
@group(0) @binding(3) var<storage, read> points: array<vec4<f32>>;

const PI: f32 = 3.141592653589793;
const TAU: f32 = 6.283185307179586;
const NORMAL_EPSILON: f32 = 0.00000011920928955078125;

fn safe_normalize(value: vec3<f32>) -> vec3<f32> {
    let length = sqrt(dot(value, value));
    if (length <= NORMAL_EPSILON) {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    return value / length;
}

@compute @workgroup_size(64)
fn generate_cylinder(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if (index >= params.vertex_count && index >= params.index_count) {
        return;
    }

    let segments = params.item_count;
    let radius = params.dimensions.x;
    let half_height = params.dimensions.y * 0.5;
    let side_vertices = (segments + 1u) * 2u;
    let cap_vertices = segments + 2u;

    if (index < params.vertex_count) {
        if (index < side_vertices) {
            let ring = index / 2u;
            let upper = index % 2u;
            let angle = TAU * f32(ring) / f32(segments);
            let normal = vec3<f32>(cos(angle), 0.0, sin(angle));
            let y = select(-half_height, half_height, upper == 1u);
            vertices[index] = GpuVertex(
                vec4<f32>(normal * radius + vec3<f32>(0.0, y, 0.0), 1.0),
                vec4<f32>(normal, 0.0),
            );
        } else {
            let cap_index = index - side_vertices;
            let on_top = cap_index >= cap_vertices;
            let local = select(cap_index, cap_index - cap_vertices, on_top);
            let y = select(-half_height, half_height, on_top);
            let normal = vec3<f32>(0.0, select(-1.0, 1.0, on_top), 0.0);
            if (local == 0u) {
                vertices[index] = GpuVertex(
                    vec4<f32>(0.0, y, 0.0, 1.0),
                    vec4<f32>(normal, 0.0),
                );
            } else {
                let segment = local - 1u;
                let angle = TAU * f32(segment) / f32(segments);
                vertices[index] = GpuVertex(
                    vec4<f32>(cos(angle) * radius, y, sin(angle) * radius, 1.0),
                    vec4<f32>(normal, 0.0),
                );
            }
        }
    }

    if (index < params.index_count) {
        let side_indices = segments * 6u;
        let cap_indices = segments * 3u;
        if (index < side_indices) {
            let ring = index / 6u;
            let corner = index % 6u;
            var pattern = array<u32, 6>(0u, 3u, 2u, 0u, 1u, 3u);
            indices[index] = ring * 2u + pattern[corner];
        } else if (index < side_indices + cap_indices) {
            let local = index - side_indices;
            let segment = local / 3u;
            let corner = local % 3u;
            var pattern = array<u32, 3>(0u, 1u, 2u);
            let center = side_vertices;
            let rim = center + 1u;
            indices[index] = select(center, rim + segment + pattern[corner] - 1u, corner > 0u);
        } else {
            let local = index - side_indices - cap_indices;
            let segment = local / 3u;
            let corner = local % 3u;
            let center = side_vertices + cap_vertices;
            let rim = center + 1u;
            var pattern = array<u32, 3>(0u, 2u, 1u);
            indices[index] = select(center, rim + segment + pattern[corner] - 1u, corner > 0u);
        }
    }
}

@compute @workgroup_size(64)
fn generate_sphere(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if (index >= params.vertex_count && index >= params.index_count) {
        return;
    }
    let segments = params.item_count;
    let rings = params.point_count;
    let stride = segments + 1u;
    let radius = params.dimensions.x;

    if (index < params.vertex_count) {
        let ring = index / stride;
        let segment = index % stride;
        let polar = PI * f32(ring) / f32(rings);
        let azimuth = TAU * f32(segment) / f32(segments);
        let normal = vec3<f32>(
            sin(polar) * cos(azimuth),
            cos(polar),
            sin(polar) * sin(azimuth),
        );
        vertices[index] = GpuVertex(
            vec4<f32>(normal * radius, 1.0),
            vec4<f32>(normal, 0.0),
        );
    }

    if (index < params.index_count) {
        let cap_indices = segments * 3u;
        let bottom_start = params.index_count - cap_indices;
        var ring = 0u;
        var segment = 0u;
        var corner = 0u;
        var branch = 0u;
        if (index < cap_indices) {
            segment = index / 3u;
            corner = index % 3u;
        } else if (index >= bottom_start) {
            ring = rings - 1u;
            let local = index - bottom_start;
            segment = local / 3u;
            corner = local % 3u;
        } else {
            let local = index - cap_indices;
            let indices_per_ring = segments * 6u;
            ring = 1u + local / indices_per_ring;
            let local_in_ring = local % indices_per_ring;
            branch = (local_in_ring % 6u) / 3u;
            segment = local_in_ring / 6u;
            corner = local_in_ring % 3u;
        }
        let a = ring * stride + segment;
        let b = a + stride;
        if (ring == 0u) {
            indices[index] = select(select(a, b + 1u, corner == 1u), b, corner == 2u);
        } else if (ring == rings - 1u) {
            indices[index] = select(select(a, a + 1u, corner == 1u), b, corner == 2u);
        } else if (branch == 0u) {
            indices[index] = select(select(a, a + 1u, corner == 1u), b, corner == 2u);
        } else {
            indices[index] = select(select(a + 1u, b + 1u, corner == 1u), b, corner == 2u);
        }
    }
}

fn rotate_around_axis(value: vec3<f32>, axis: vec3<f32>, angle: f32) -> vec3<f32> {
    let cosine = cos(angle);
    let sine = sin(angle);
    return value * cosine
        + cross(axis, value) * sine
        + axis * dot(axis, value) * (1.0 - cosine);
}

fn transported_frame(point_index: u32) -> array<vec3<f32>, 2> {
    var previous_tangent = safe_normalize(points[1u].xyz - points[0u].xyz);
    let reference = select(
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(1.0, 0.0, 0.0),
        abs(previous_tangent.y) > 0.9,
    );
    var side = safe_normalize(cross(previous_tangent, reference));
    for (var index = 1u; index <= point_index; index += 1u) {
        let tangent = safe_normalize(points[index].xyz - points[index - 1u].xyz);
        let axis = cross(previous_tangent, tangent);
        let axis_length = length(axis);
        if (axis_length > NORMAL_EPSILON) {
            let angle = acos(clamp(dot(previous_tangent, tangent), -1.0, 1.0));
            side = rotate_around_axis(side, axis / axis_length, angle);
        }
        side = safe_normalize(side - tangent * dot(side, tangent));
        previous_tangent = tangent;
    }
    return array<vec3<f32>, 2>(
        side,
        safe_normalize(cross(side, previous_tangent)),
    );
}

@compute @workgroup_size(64)
fn generate_sweep(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if (index >= params.vertex_count && index >= params.index_count) {
        return;
    }
    let segments = params.item_count;
    let point_count = params.point_count;
    let radius = params.dimensions.x;

    if (index < params.vertex_count) {
        let point_index = index / segments;
        let segment = index % segments;
        let frame = transported_frame(point_index);
        let side = frame[0];
        let up = frame[1];
        let angle = TAU * f32(segment) / f32(segments);
        let normal = safe_normalize(side * cos(angle) + up * sin(angle));
        vertices[index] = GpuVertex(
            vec4<f32>(points[point_index].xyz + normal * radius, 1.0),
            vec4<f32>(normal, 0.0),
        );
    }

    if (index < params.index_count) {
        let side_indices = (point_count - 1u) * segments * 6u;
        let cap_indices = (segments - 2u) * 3u;
        if (index < side_indices) {
            let ring_index = index / (segments * 6u);
            let segment_face = (index % (segments * 6u)) / 6u;
            let corner = index % 6u;
            let next = (segment_face + 1u) % segments;
            let a = ring_index * segments + segment_face;
            let b = ring_index * segments + next;
            let c = (ring_index + 1u) * segments + segment_face;
            let d = (ring_index + 1u) * segments + next;
            let face_offsets = array<u32, 6>(a, c, b, b, c, d);
            indices[index] = face_offsets[corner];
        } else if (index < side_indices + cap_indices) {
            let local = index - side_indices;
            let segment = local / 3u + 1u;
            let corner = local % 3u;
            var pattern = array<u32, 3>(0u, 0u, 1u);
            indices[index] = select(0u, segment + pattern[corner], corner > 0u);
        } else {
            let local = index - side_indices - cap_indices;
            let segment = local / 3u + 1u;
            let corner = local % 3u;
            let base = (point_count - 1u) * segments;
            var pattern = array<u32, 3>(0u, 2u, 1u);
            indices[index] = select(base, base + segment + pattern[corner] - 1u, corner > 0u);
        }
    }
}
