// SDF raymarching shader

const PI: f32 = 3.14159265359;
const MAX_STEPS: u32 = 128u;
const MAX_DIST: f32 = 100.0;
const EPSILON: f32 = 0.001;

struct CameraUniform {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    view_projection: mat4x4<f32>,
    inverse_view: mat4x4<f32>,
    position: vec4<f32>,
    direction: vec4<f32>,
    near_far: vec4<f32>,
}

struct SdfUniform {
    camera_pos: vec4<f32>,
    camera_dir: vec4<f32>,
    camera_up: vec4<f32>,
    camera_right: vec4<f32>,
    resolution: vec2<f32>,
    time: f32,
    fov: f32,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> sdf: SdfUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Fullscreen triangle
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Generate fullscreen triangle
    let x = f32(i32(vertex_index) - 1);
    let y = f32(i32(vertex_index & 1u) * 2 - 1);

    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + 0.5;

    return out;
}

// ==================== SDF Primitives ====================

fn sdf_sphere(p: vec3<f32>, r: f32) -> f32 {
    return length(p) - r;
}

fn sdf_box(p: vec3<f32>, b: vec3<f32>) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn sdf_torus(p: vec3<f32>, t: vec2<f32>) -> f32 {
    let q = vec2<f32>(length(p.xz) - t.x, p.y);
    return length(q) - t.y;
}

fn sdf_cylinder(p: vec3<f32>, h: f32, r: f32) -> f32 {
    let d = abs(vec2<f32>(length(p.xz), p.y)) - vec2<f32>(r, h);
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0)));
}

fn sdf_plane(p: vec3<f32>, n: vec3<f32>, h: f32) -> f32 {
    return dot(p, n) + h;
}

fn sdf_capsule(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, r: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h) - r;
}

fn sdf_cone(p: vec3<f32>, c: vec2<f32>, h: f32) -> f32 {
    let q = h * vec2<f32>(c.x / c.y, -1.0);
    let w = vec2<f32>(length(p.xz), p.y);
    let a = w - q * clamp(dot(w, q) / dot(q, q), 0.0, 1.0);
    let b = w - q * vec2<f32>(clamp(w.x / q.x, 0.0, 1.0), 1.0);
    let k = sign(q.y);
    let d = min(dot(a, a), dot(b, b));
    let s = max(k * (w.x * q.y - w.y * q.x), k * (w.y - q.y));
    return sqrt(d) * sign(s);
}

// ==================== SDF Operations ====================

fn op_union(d1: f32, d2: f32) -> f32 {
    return min(d1, d2);
}

fn op_subtract(d1: f32, d2: f32) -> f32 {
    return max(-d1, d2);
}

fn op_intersect(d1: f32, d2: f32) -> f32 {
    return max(d1, d2);
}

fn op_smooth_union(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) - k * h * (1.0 - h);
}

fn op_smooth_subtract(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (d2 + d1) / k, 0.0, 1.0);
    return mix(d2, -d1, h) + k * h * (1.0 - h);
}

fn op_smooth_intersect(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) + k * h * (1.0 - h);
}

// ==================== Transformations ====================

fn op_translate(p: vec3<f32>, t: vec3<f32>) -> vec3<f32> {
    return p - t;
}

fn op_rotate_x(p: vec3<f32>, a: f32) -> vec3<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(p.x, c * p.y - s * p.z, s * p.y + c * p.z);
}

fn op_rotate_y(p: vec3<f32>, a: f32) -> vec3<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(c * p.x + s * p.z, p.y, -s * p.x + c * p.z);
}

fn op_rotate_z(p: vec3<f32>, a: f32) -> vec3<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(c * p.x - s * p.y, s * p.x + c * p.y, p.z);
}

fn op_repeat(p: vec3<f32>, c: vec3<f32>) -> vec3<f32> {
    return p - c * round(p / c);
}

fn op_repeat_limited(p: vec3<f32>, c: f32, l: vec3<f32>) -> vec3<f32> {
    return p - c * clamp(round(p / c), -l, l);
}

// ==================== Scene Definition ====================

// Example scene - can be customized or generated
fn map_scene(p: vec3<f32>) -> f32 {
    // Ground plane
    let ground = sdf_plane(p, vec3<f32>(0.0, 1.0, 0.0), 0.0);

    // Animated sphere
    let sphere_pos = vec3<f32>(sin(sdf.time) * 2.0, 1.0 + sin(sdf.time * 2.0) * 0.5, 0.0);
    let sphere = sdf_sphere(p - sphere_pos, 1.0);

    // Box
    let box_p = op_rotate_y(p - vec3<f32>(-2.0, 1.0, 0.0), sdf.time * 0.5);
    let box_obj = sdf_box(box_p, vec3<f32>(0.8));

    // Torus
    let torus_p = op_rotate_x(p - vec3<f32>(2.0, 1.0, 0.0), sdf.time);
    let torus = sdf_torus(torus_p, vec2<f32>(0.8, 0.3));

    // Combine with smooth union
    var d = ground;
    d = op_smooth_union(d, sphere, 0.3);
    d = op_smooth_union(d, box_obj, 0.3);
    d = op_smooth_union(d, torus, 0.3);

    return d;
}

// ==================== Raymarching ====================

fn raymarch(ro: vec3<f32>, rd: vec3<f32>) -> f32 {
    var t = 0.0;

    for (var i = 0u; i < MAX_STEPS; i++) {
        let p = ro + rd * t;
        let d = map_scene(p);

        if (d < EPSILON) {
            return t;
        }
        if (t > MAX_DIST) {
            break;
        }

        t += d;
    }

    return -1.0;
}

// Calculate normal via gradient
fn calc_normal(p: vec3<f32>) -> vec3<f32> {
    let e = vec2<f32>(EPSILON, 0.0);
    return normalize(vec3<f32>(
        map_scene(p + e.xyy) - map_scene(p - e.xyy),
        map_scene(p + e.yxy) - map_scene(p - e.yxy),
        map_scene(p + e.yyx) - map_scene(p - e.yyx)
    ));
}

// Soft shadows
fn soft_shadow(ro: vec3<f32>, rd: vec3<f32>, mint: f32, maxt: f32, k: f32) -> f32 {
    var res = 1.0;
    var t = mint;

    for (var i = 0u; i < 32u; i++) {
        let h = map_scene(ro + rd * t);
        res = min(res, k * h / t);
        t += clamp(h, 0.02, 0.1);
        if (h < 0.001 || t > maxt) {
            break;
        }
    }

    return clamp(res, 0.0, 1.0);
}

// Ambient occlusion
fn calc_ao(p: vec3<f32>, n: vec3<f32>) -> f32 {
    var occ = 0.0;
    var sca = 1.0;

    for (var i = 0u; i < 5u; i++) {
        let h = 0.01 + 0.12 * f32(i) / 4.0;
        let d = map_scene(p + h * n);
        occ += (h - d) * sca;
        sca *= 0.95;
    }

    return clamp(1.0 - 3.0 * occ, 0.0, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Calculate ray direction
    let aspect = sdf.resolution.x / sdf.resolution.y;
    let uv = (in.uv * 2.0 - 1.0) * vec2<f32>(aspect, 1.0);

    let ro = sdf.camera_pos.xyz;
    let rd = normalize(
        sdf.camera_dir.xyz +
        sdf.camera_right.xyz * uv.x * tan(sdf.fov * 0.5) +
        sdf.camera_up.xyz * uv.y * tan(sdf.fov * 0.5)
    );

    // Raymarch
    let t = raymarch(ro, rd);

    if (t < 0.0) {
        // Sky gradient
        let sky = mix(vec3<f32>(0.5, 0.7, 1.0), vec3<f32>(0.1, 0.2, 0.4), rd.y * 0.5 + 0.5);
        return vec4<f32>(sky, 1.0);
    }

    let p = ro + rd * t;
    let n = calc_normal(p);

    // Lighting
    let light_dir = normalize(vec3<f32>(1.0, 1.0, 1.0));
    let light_color = vec3<f32>(1.0, 0.95, 0.9);

    // Diffuse
    let diff = max(dot(n, light_dir), 0.0);

    // Specular
    let h = normalize(light_dir - rd);
    let spec = pow(max(dot(n, h), 0.0), 32.0);

    // Shadow
    let shadow = soft_shadow(p + n * 0.02, light_dir, 0.02, 10.0, 16.0);

    // Ambient occlusion
    let ao = calc_ao(p, n);

    // Material color (based on position for now)
    let mat_color = vec3<f32>(0.8, 0.7, 0.6);

    // Combine lighting
    var color = vec3<f32>(0.0);
    color += mat_color * 0.2 * ao; // Ambient
    color += mat_color * diff * shadow * light_color; // Diffuse
    color += vec3<f32>(1.0) * spec * shadow * 0.5; // Specular

    // Fog
    let fog = exp(-t * 0.02);
    color = mix(vec3<f32>(0.5, 0.6, 0.7), color, fog);

    // Gamma correction
    color = pow(color, vec3<f32>(1.0 / 2.2));

    return vec4<f32>(color, 1.0);
}
