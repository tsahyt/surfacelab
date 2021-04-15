#version 450

layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 outColor;

layout(constant_id = 0) const uint OBJECT_TYPE = 1;

const uint OBJECT_TYPE_PLANE = 0;
const uint OBJECT_TYPE_FINITEPLANE = 1;
const uint OBJECT_TYPE_CUBE = 2;
const uint OBJECT_TYPE_SPHERE = 3;
const uint OBJECT_TYPE_CYLINDER = 4;

layout(constant_id = 1) const uint RENDER_TYPE = 1;

const uint RENDER_TYPE_PBR = 0;
const uint RENDER_TYPE_MATCAP = 1;

layout(set = 0, binding = 0) uniform sampler s_Texture;

layout(set = 0, binding = 1) uniform Occupancy {
    uint has_albedo;
    uint has_roughness;
    uint has_normal;
    uint has_displacement;
    uint has_metallic;
    uint has_ao;
    uint has_view;
};

layout(set = 0, binding = 2) uniform Camera {
    vec4 center;
    vec4 light_pos;
    vec2 resolution;
    float focal_length;
    float aperture_size;
    int aperture_blades;
    float aperture_rotation;
    float focal_distance;

    float phi;
    float theta;
    float radius;

    float displacement_amount;
    float tex_scale;

    float environment_strength;
    float environment_blur;
    float environment_rotation;
    float ao_strength;

    uint light_type;
    float light_strength;
    float fog_strength;

    uint draw_shadow;
};

layout(push_constant) uniform constants_t {
    vec2 sample_offset;
} constants;

const uint LIGHT_TYPE_POINT = 0;
const uint LIGHT_TYPE_SUN = 1;

layout(set = 0, binding = 3) uniform texture2D t_Displ;
layout(set = 0, binding = 4) uniform texture2D t_Albedo;
layout(set = 0, binding = 5) uniform texture2D t_Normal;
layout(set = 0, binding = 6) uniform texture2D t_Roughness;
layout(set = 0, binding = 7) uniform texture2D t_Metallic;
layout(set = 0, binding = 8) uniform texture2D t_AO;
layout(set = 0, binding = 9) uniform texture2D t_View;
layout(set = 0, binding = 10) uniform textureCube irradiance_map;
layout(set = 0, binding = 11) uniform textureCube environment_map;
layout(set = 0, binding = 12) uniform texture2D brdf_lut;
layout(set = 0, binding = 13) uniform texture2D matcap;

const float PI = 3.141592654;

const float INFINITY = 1.0 / 0.0;
const int MAX_STEPS = 300;
const int MAX_STEPS_AO = 32;
const int MAX_STEPS_SHD = 64;
const float MAX_DIST = 24.0;
const float SURF_DIST = .0002;
const float TEX_MIDLEVEL = .5;

const float MAX_REFLECTION_LOD = 5.0;
const float LUT_SIZE = 64.;

// TODO: Include texture scale in mip mapping considerations for performance gain

#define LOD_BIAS .5
#define SHADOW_LOD_OFFSET 2.

float hash13(vec3 p3)
{
    p3 = fract(p3 * .1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

float lod_by_distance(float d) {
    return log(d * LOD_BIAS);
}

vec2 plane_mapping(vec3 p) {
    return (vec2(- 1., 1.) * p.xz / 4.) + .5;
}

vec2 sphere_mapping(vec3 p) {
    p = normalize(p);
    float u = 0.5 + atan(p.x, p.z) / (2 * PI);
    float v = 0.5 - asin(p.y) / PI;
    return vec2(-4 * u, 2 * v);
}

vec2 cylinder_mapping(vec3 p) {
    float u = - atan(p.x, p.z) / (2 * PI);
    return vec2(3 * u, -p.y / 4.) + 0.5;
}

vec3 world(vec3 d, float lod) {
    float c = cos(environment_rotation);
    float s = sin(environment_rotation);
    mat3 rot = mat3(
        vec3(c, 0., s),
        vec3(0., 1., 0.),
        vec3(- s, 0., c)
    );
    return textureLod(samplerCube(environment_map, s_Texture), rot * d, lod).rgb * environment_strength;
}

vec3 irradiance(vec3 d) {
    float c = cos(environment_rotation);
    float s = sin(environment_rotation);
    mat3 rot = mat3(
        vec3(c, 0., s),
        vec3(0., 1., 0.),
        vec3(- s, 0., c)
    );
    return texture(samplerCube(irradiance_map, s_Texture), rot * d).rgb;
}

// Read the heightfield at a given texture coordinate
float heightfield(vec2 p, float lod) {
    if(has_displacement != 0) {
        float h = textureLod(sampler2D(t_Displ, s_Texture), p / tex_scale, lod).r;
        return h - TEX_MIDLEVEL;
    } else {
        return 0.;
    }
}

// Read the albedo at a given texture coordinate
vec3 albedo(vec2 p, float lod) {
    if(has_albedo != 0) {
        return textureLod(sampler2D(t_Albedo, s_Texture), p / tex_scale, lod).rgb;
    } else {
        return vec3(0.5);
    }
}

vec3 triplanar_albedo(vec3 p, vec3 n, float lod) {
    n = pow(abs(n), vec3(4.0));
    n = n / (n.x + n.y + n.z);

    vec3 col_front = albedo(-p.xy + 0.5, lod);
    vec3 col_side = albedo(-p.zy + 0.5, lod);
    vec3 col_top = albedo(-p.xz + 0.5, lod);

    col_front *= n.b;
    col_side *= n.r;
    col_top *= n.g;

    return col_front + col_side + col_top;
}

// Read the roughness at a given texture coordinate
float roughness(vec2 p, float lod) {
    if(has_roughness != 0) {
        float r = textureLod(sampler2D(t_Roughness, s_Texture), p / tex_scale, lod).x;
        return r;
    } else {
        return 0.5;
    }
}

float triplanar_roughness(vec3 p, vec3 n, float lod) {
    n = pow(abs(n), vec3(4.0));
    n = n / (n.x + n.y + n.z);

    float col_front = roughness(-p.xy + 0.5, lod);
    float col_side = roughness(-p.zy + 0.5, lod);
    float col_top = roughness(-p.xz + 0.5, lod);

    col_front *= n.b;
    col_side *= n.r;
    col_top *= n.g;

    return col_front + col_side + col_top;
}

// Read the metallic map at a given texture coordinate
float metallic(vec2 p, float lod) {
    if(has_metallic != 0) {
        float r = textureLod(sampler2D(t_Metallic, s_Texture), p / tex_scale, lod).x;
        return r;
    } else {
        return 0.;
    }
}

float triplanar_metallic(vec3 p, vec3 n, float lod) {
    n = pow(abs(n), vec3(4.0));
    n = n / (n.x + n.y + n.z);

    float col_front = metallic(-p.xy + 0.5, lod);
    float col_side = metallic(-p.zy + 0.5, lod);
    float col_top = metallic(-p.xz + 0.5, lod);

    col_front *= n.b;
    col_side *= n.r;
    col_top *= n.g;

    return col_front + col_side + col_top;
}

float baked_ao(vec2 p, float lod) {
    if(has_ao != 0) {
        float ao = textureLod(sampler2D(t_AO, s_Texture), p / tex_scale, lod).x;
        return ao;
    } else {
        return 1.;
    }
}

float triplanar_baked_ao(vec3 p, vec3 n, float lod) {
    n = pow(abs(n), vec3(4.0));
    n = n / (n.x + n.y + n.z);

    float ao_front = baked_ao(-p.xy + 0.5, lod);
    float ao_side = baked_ao(-p.zy + 0.5, lod);
    float ao_top = baked_ao(-p.xz + 0.5, lod);

    ao_front *= n.b;
    ao_side *= n.r;
    ao_top *= n.g;

    return ao_front + ao_side + ao_top;
}

vec3 normal_map(vec2 p, float lod) {
    if(has_normal != 0) {
        vec3 n = textureLod(sampler2D(t_Normal, s_Texture), p / tex_scale, lod).rgb;
        return normalize(n * 2. - 1.);
    } else {
        return vec3(0., 0., 1.);
    }
}

vec3 triplanar_normal_map(vec3 p, vec3 n, float lod) {
    n = pow(abs(n), vec3(4.0));
    n = n / (n.x + n.y + n.z);

    vec3 nrm_front = normal_map(-p.xy + 0.5, lod);
    vec3 nrm_side = normal_map(-p.zy + 0.5, lod);
    vec3 nrm_top = normal_map(-p.xz + 0.5, lod);

    nrm_front *= n.b;
    nrm_side *= n.r;
    nrm_top *= n.g;

    return nrm_front + nrm_side + nrm_top;
}

float sdBox(vec3 p, vec3 b)
{
    vec3 q = abs(p) - b;
    return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0);
}

float sdCappedCylinder(vec3 p, float h, float dia)
{
    vec2 d = abs(vec2(length(p.xz),p.y)) - vec2(dia, h);
    return min(max(d.x, d.y), 0.0) + length(max(d, 0.0));
}

// Special normals function for cube. Used to get proper triplanar projection on
// undistorted cube.
vec3 cubeNormal(vec3 p, float s)
{
    return sign(p) * normalize(max(abs(p) - vec3(s), 0.0));
}

float sdf(vec3 p, float lod) {
    float height = 0.;
    switch (OBJECT_TYPE) {
        case OBJECT_TYPE_PLANE:
        case OBJECT_TYPE_FINITEPLANE:
            height = heightfield(plane_mapping(p), lod) * displacement_amount;
            float planeDist = p.y;
            return planeDist - height;
        case OBJECT_TYPE_CUBE:
            float boxDist = sdBox(p, vec3(0.9)) - 0.1;
            vec2 e = vec2(0.01, 0);
            vec3 n = cubeNormal(p, 0.9);

            n = pow(abs(n), vec3(4.0));
            n = n / (n.x + n.y + n.z);

            p /= 2.;

            float height_front = heightfield(-p.xy + 0.5, lod) * displacement_amount * n.b;
            float height_side = heightfield(-p.zy + 0.5, lod) * displacement_amount * n.r;
            float height_top = heightfield(-p.xz + 0.5, lod) * displacement_amount * n.g;

            height = height_front + height_side + height_top;

            return boxDist - height;
        case OBJECT_TYPE_SPHERE:
            height = heightfield(sphere_mapping(p), lod) * displacement_amount;
            float sphereDist = length(p) - 1.;
            return sphereDist - height;
        case OBJECT_TYPE_CYLINDER:
            height = heightfield(cylinder_mapping(p), lod) * displacement_amount;
            height = mix(height, 0., smoothstep(2 * PI / 3 - 0.1, 2 * PI / 3, abs(p.y)));
            float cylinderDist = sdCappedCylinder(p, 2 * PI / 3 - 0.1, 1.9) - 0.1;
            return cylinderDist - height;
    }

    return 0.;
}

vec2 intsSphere(vec3 ro, vec3 rd, float ra)
{
    float b = dot(ro, rd);
    float c = dot(ro, ro) - ra * ra;
    float h = b*b - c;
    if(h < 0.0) return vec2(- 1.0); // no intersection
    h = sqrt(h);
    return vec2(-b-h, -b+h);
}

vec2 intsBox(vec3 ro, vec3 rd, vec3 boxSize)
{
    vec3 m = 1.0 / rd; // can precompute if traversing a set of aligned boxes
    vec3 n = m * ro;   // can precompute if traversing a set of aligned boxes
    vec3 k = abs(m) * boxSize;
    vec3 t1 = -n - k;
    vec3 t2 = -n + k;
    float tN = max(max(t1.x, t1.y), t1.z);
    float tF = min(min(t2.x, t2.y), t2.z);
    if(tN > tF || tF < 0.0) return vec2(- 1.0); // no intersection
    return vec2(tN, tF);
}

vec2 outer_bound(vec3 ro, vec3 rd, float d) {
    switch (OBJECT_TYPE) {
        case OBJECT_TYPE_PLANE:
        case OBJECT_TYPE_FINITEPLANE:
            return vec2(- (ro.y - d) / rd.y);
        case OBJECT_TYPE_CUBE:
            return intsBox(ro, rd, vec3(1. + d));
        case OBJECT_TYPE_SPHERE:
            return intsSphere(ro, rd, 2. + d);
        case OBJECT_TYPE_CYLINDER:
            return intsBox(ro, rd, vec3(2., 2. * PI / 3., 2.) + vec3(d));
    }

    return vec2(0.);
}

// Get normals from SDF
vec3 normal(vec3 p, vec3 tangent_normal, float s, float lod) {
    float d = sdf(p, lod);
    vec2 e = vec2(s, 0);
    vec3 world_normal = normalize(d -
                            vec3(sdf(p - e.xyy, lod),
                                sdf(p - e.yxy, lod),
                                sdf(p - e.yyx, lod)));

    vec3 tangent = abs(world_normal.y) > 0.99999 ?
        vec3(1., 0., 0.) :
        normalize(cross(vec3(0., 1., 0.), world_normal));
    vec3 bitangent = normalize(cross(world_normal, tangent));
    mat3 tbn = mat3(tangent, bitangent, world_normal);

    return normalize(tbn * tangent_normal);
}

// --- Ray Marching

float rayMarch(vec3 ro, vec3 rd) {
    float t = outer_bound(ro, rd, displacement_amount).x;

    if (ro.y < displacement_amount) {
        t = 0;
    }
    if (t < 0 || length(ro + t * rd) > MAX_DIST) { return INFINITY; }

    float bias = max(1, 4 * displacement_amount);

    for(int i = 0; i < MAX_STEPS; i++) {
        vec3 p = ro + t * rd;
        float d = sdf(p, lod_by_distance(t));
        if (length(p) > MAX_DIST || abs(d) < (SURF_DIST * t)) { break; }
        if (d < 0.) {
            // when inside the surface make sure to step back out again
            t -= SURF_DIST;
        } else {
            t += d / bias;
        }
    }

    return t;
}

float rayShadowSoft(vec3 ro, vec3 rd, float w) {
    float s = 1.0;
    float t = 128 * SURF_DIST;
    float max_dist = outer_bound(ro, rd, displacement_amount).y;
    float step_size = max_dist / MAX_STEPS_SHD;

    t += hash13(rd + vec3(constants.sample_offset, 0.)) * step_size;

    for(int i = 0; i < MAX_STEPS_SHD; i++) {
        vec3 p = ro + rd * t;
        float d = sdf(p, lod_by_distance(t) + SHADOW_LOD_OFFSET);
        s = min(s, 0.5 + 0.5 * d / (w * t));
        if (s < 0 || t > max_dist) break;
        t += (step_size + (d / 2.)) / 2.;
    }

    s = max(s, 0.0);

    return smoothstep(0.5, 0.6, s);
}

// --- Shading

vec3 fresnelSchlick(float cosTheta, vec3 F0)
{
    return F0 + (1.0 - F0) * pow(1.0 - cosTheta, 5.0);
}

vec3 fresnelSchlickRoughness(float cosTheta, vec3 F0, float roughness)
{
    return F0 + (max(vec3(1.0 - roughness), F0) - F0) * pow(1.0 - cosTheta, 5.0);
}

float distributionGGX(vec3 N, vec3 H, float roughness)
{
    float a      = roughness*roughness;
    float a2     = a*a;
    float NdotH  = max(dot(N, H), 0.0);
    float NdotH2 = NdotH*NdotH;

    float num   = a2;
    float denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return num / denom;
}

float geometrySchlickGGX(float NdotV, float roughness)
{
    float r = (roughness + 1.0);
    float k = (r*r) / 8.0;

    float num   = NdotV;
    float denom = NdotV * (1.0 - k) + k;

    return num / denom;
}

float geometrySmith(vec3 N, vec3 V, vec3 L, float roughness)
{
    float NdotV = max(dot(N, V), 0.0);
    float NdotL = max(dot(N, L), 0.0);
    float ggx2  = geometrySchlickGGX(NdotV, roughness);
    float ggx1  = geometrySchlickGGX(NdotL, roughness);

    return ggx1 * ggx2;
}

float point_light(vec3 p, vec3 lightPos, out vec3 direction) {
    direction = normalize(lightPos - p);
    return length(lightPos - p);
}

float sun_light(vec3 p, vec3 lightPos, out vec3 direction) {
    direction = normalize(lightPos);
    return 2.0;
}

vec3 light(vec3 p, vec3 n, vec3 rd, vec3 f0, float d, vec3 albedo, float metallic, float roughness, vec3 lightColor, vec3 lightPos, float w) {
    rd *= -1;

    // Radiance
    vec3 l;
    float dist;
    if (light_type == LIGHT_TYPE_POINT) {
        dist = point_light(p, lightPos, l);
    } else if (light_type == LIGHT_TYPE_SUN) {
        dist = sun_light(p, lightPos, l);
    } else {
        dist = 1.0;
        l = vec3(0., 1., 0.);
    }
    vec3 h = normalize(rd + l);
    float attenuation = light_strength / (dist * dist);
    vec3 radiance = lightColor * attenuation;

    // Cook-Torrance BRDF
    float ndf = distributionGGX(n, h, roughness);
    float g = geometrySmith(n, rd, l, roughness);
    vec3 f = fresnelSchlick(max(dot(h, rd), 0.), f0);

    // Specular/Diffuse coefficients
    vec3 kS = f;
    vec3 kD = vec3(1.0) - kS;
    kD *= 1.0 - metallic;

    vec3 numerator = ndf * g * f;
    float denominator = 4.0 * max(dot(n, rd), 0.) * max(dot(n, l), 0.);
    vec3 specular = numerator / max(denominator, 0.001);

    float ndotl = max(dot(n,l), 0.);

    // Shadow
    float shadow;
    if (draw_shadow == 1) {
        shadow = rayShadowSoft(p, l, w);
    } else {
        shadow = 1.;
    }

    return (kD * albedo / PI + specular) * radiance * ndotl * shadow;
}

// Scale and bias coordinates, for correct filtered lookup
vec2 lut_coords_ltc(float cosTheta, float roughness)
{
    vec2 coords = vec2(roughness, sqrt(1.0 - cosTheta));
    return coords * (LUT_SIZE - 1.0) / LUT_SIZE + 0.5 / LUT_SIZE;
}

vec3 environment(vec3 n, vec3 rd, vec3 f0, vec3 albedo, float roughness, float metallic, float ao) {
    // Diffuse
    vec3 kS = fresnelSchlickRoughness(max(dot(n, -rd), 0.0), f0, roughness);
    vec3 kD = 1.0 - kS;
    kD *= 1.0 - metallic;
    vec3 irradiance = irradiance(n);
    vec3 diffuse = irradiance * albedo;

    // Specular
    vec3 r = reflect(rd, n);
    vec3 refl_color = world(r, roughness * MAX_REFLECTION_LOD);
    vec3 f = fresnelSchlickRoughness(max(dot(n, -rd), 0.0), f0, roughness);
    vec2 env_brdf = texture(sampler2D(brdf_lut, s_Texture), lut_coords_ltc(max(dot(n, -rd), 0.0), roughness)).rg;
    vec3 specular = refl_color * (f * env_brdf.x + env_brdf.y);

    return (kD * diffuse + specular) * ao * environment_strength;
}

vec2 concentric_sample_disk(vec2 uv) {
    float r = sqrt(uv.x);
    float theta = 2.0 * PI * uv.y;
    return vec2(r * cos(theta), r * sin(theta));
}

vec2 regular_polygon_sample(float corners, float rotation, vec2 uv) {
    float u = uv.x;
    float v = uv.y;
    float corner = floor(u * corners);
    u = u * corners - corner;

    // uniform sampled triangle weights
    u = sqrt(u);
    v = v * u;
    u = 1.0 - u;

    // point in triangle
    float angle = PI / corners;
    vec2 p = vec2((u + v) * cos(angle), (u - v) * sin(angle));

    rotation += corner * 2.0f * angle;

    float cr = cos(rotation);
    float sr = sin(rotation);

    return vec2(cr * p.x - sr * p.y, sr * p.x + cr * p.y);
}

vec2 aperture_sample(vec2 uv) {
    if(aperture_blades == 0) {
        return concentric_sample_disk(uv);
    } else {
        return regular_polygon_sample(aperture_blades, aperture_rotation, uv);
    }
}

vec3 camera(vec3 p, vec3 look_at, vec2 uv, float focal_length, float focal_dist, float lens_radius, out vec3 ro) {
    // Basis of camera space in world space coordinates
    vec3 forward = normalize(look_at - p);
    vec3 right = normalize(cross(vec3(0,1,0), forward));
    vec3 up = cross(forward, right);

    // Ray direction in camera space
    vec3 cro = vec3(0.);
    vec3 crd = vec3(uv.x, uv.y, focal_length);

    if (lens_radius > 0.) {
        vec2 lens_uv = aperture_sample(constants.sample_offset) * lens_radius;
        float ft = focal_dist / crd.z;
        vec3 pf = crd * ft;

        cro = vec3(lens_uv, 0.);
        crd = normalize(pf - cro);
    }

    // Transform to world space
    vec3 rd = right * crd.x + up * crd.y + forward * crd.z;
    ro = right * cro.x + up * cro.y + forward * cro.z;
    ro += p;

    return normalize(rd);
}

float world_space_sample_size(float d) {
    float z = 1.0 / min(resolution.x, resolution.y);
    return z * d;
}

vec3 render_matcap(vec3 ro, vec3 rd, vec3 look_at) {
    float d = rayMarch(ro, rd);

    // Early termination for non-surface pixels
    vec3 world = world(rd, environment_blur);
    if (d == INFINITY) { return world; }

    vec3 p = ro + rd * d;
    vec3 n = normal(p, vec3(0., 0., 1.), world_space_sample_size(d), lod_by_distance(d));

    // Construct view matrix
    vec3 forward = normalize(look_at - ro);
    vec3 right = normalize(cross(vec3(0,1,0), forward));
    vec3 up = cross(forward, right);

    mat3 view = mat3(
        vec3(right.x, forward.x, up.x),
        vec3(right.y, forward.y, up.y),
        vec3(right.z, forward.z, up.z)
    );

    // Matcap Render
    vec3 view_normal = vec3(view * n);
    vec2 muv = view_normal.xz * 0.5 + vec2(0.5, 0.5);
    vec3 col = textureLod(sampler2D(matcap, s_Texture), vec2(muv.x, muv.y), 0.).rgb;

    // Shadowing
    float shadow;
    if (draw_shadow == 1) {
        vec3 l = vec3(0., 1., 0.);
        if (light_type == LIGHT_TYPE_POINT) {
            point_light(p, light_pos.xyz, l);
        } else if (light_type == LIGHT_TYPE_SUN) {
            sun_light(p, light_pos.xyz, l);
        }
        shadow = rayShadowSoft(p, l, 0.025);
    } else {
        shadow = 1.;
    }

    col *= vec3(smoothstep(6., 3., length(p.xz)));

    return shadow * col;
}

vec3 render(vec3 ro, vec3 rd) {
    vec3 col = vec3(0.);

    float d = rayMarch(ro, rd);

    // Early termination for non-surface pixels
    vec3 world = world(rd, environment_blur);
    if (d == INFINITY) { return world; }

    vec3 p = ro + rd * d;

    // Texture fetching
    vec3 albedo_;
    vec3 normal_;
    float metallic_;
    float roughness_;
    float baked_ao_;

    switch (OBJECT_TYPE) {
        case OBJECT_TYPE_PLANE:
        case OBJECT_TYPE_FINITEPLANE:
            albedo_ = albedo(plane_mapping(p), lod_by_distance(d));
            metallic_ = metallic(plane_mapping(p), lod_by_distance(d));
            roughness_ = roughness(plane_mapping(p), lod_by_distance(d));
            normal_ = normal_map(plane_mapping(p), lod_by_distance(d));
            baked_ao_ = baked_ao(plane_mapping(p), lod_by_distance(d));
            break;
        case OBJECT_TYPE_CUBE:
            vec3 nprime = cubeNormal(p, 0.9);
            albedo_ = triplanar_albedo(p / 2., nprime, lod_by_distance(d));
            metallic_ = triplanar_metallic(p / 2., nprime, lod_by_distance(d));
            roughness_ = triplanar_roughness(p / 2., nprime, lod_by_distance(d));
            normal_ = triplanar_normal_map(p / 2., nprime, lod_by_distance(d));
            baked_ao_ = triplanar_baked_ao(p / 2., nprime, lod_by_distance(d));
            break;
        case OBJECT_TYPE_SPHERE:
            albedo_ = albedo(sphere_mapping(p), lod_by_distance(d));
            metallic_ = metallic(sphere_mapping(p), lod_by_distance(d));
            roughness_ = roughness(sphere_mapping(p), lod_by_distance(d));
            normal_ = normal_map(sphere_mapping(p), lod_by_distance(d));
            baked_ao_ = baked_ao(sphere_mapping(p), lod_by_distance(d));
            break;
        case OBJECT_TYPE_CYLINDER:
            albedo_ = albedo(cylinder_mapping(p), lod_by_distance(d));
            metallic_ = metallic(cylinder_mapping(p), lod_by_distance(d));
            roughness_ = roughness(cylinder_mapping(p), lod_by_distance(d));
            normal_ = normal_map(cylinder_mapping(p), lod_by_distance(d));
            baked_ao_ = baked_ao(cylinder_mapping(p), lod_by_distance(d));
            break;
    }

    vec3 n = normal(p, normal_, world_space_sample_size(d), lod_by_distance(d));

    // Lights
    vec3 f0 = vec3(0.04);
    f0 = mix(f0, albedo_, metallic_);

    if (light_strength > 0.) {
        col += light(p, n, rd, f0, d, albedo_, metallic_, roughness_, vec3(1.), light_pos.xyz, 1.);
    }

    // Ambient Light
    float ao = clamp(pow(baked_ao_, ao_strength * displacement_amount * 10.), 0., 1.);
    col += environment(n, rd, f0, albedo_, roughness_, metallic_, ao);

    // View Falloff
    col += vec3(0.5,0.5,0.4) * smoothstep(2,20,d) * fog_strength;
    switch(OBJECT_TYPE) {
        case OBJECT_TYPE_FINITEPLANE:
            vec2 d = abs(p.xz);
            col = mix(col, world, step(2., max(d.x, d.y)));
            break;
        case OBJECT_TYPE_PLANE:
            col = mix(world, col, smoothstep(10., 9., distance(center.xyz, p)));
            break;
        default:
            col = mix(world, col, smoothstep(10., 9., length(p)));
    }

    return col;
}

void main() {
    vec2 uv = (v_TexCoord - 0.5) * vec2(resolution.x / resolution.y, 1);

    // Spherical Coordinate Input (phi, theta)
    vec3 camera_pos = center.xyz + (radius * vec3(
                                        sin(phi) * cos(theta),
                                        cos(phi),
                                        sin(phi) * sin(theta)));

    vec2 subpixel_offset = (constants.sample_offset - vec2(1.0)) * (1.0 / resolution);

    vec3 ro;
    vec3 rd = camera(camera_pos, center.xyz, uv + subpixel_offset, focal_length, focal_distance, aperture_size, ro);

    vec3 col = vec3(0.);

    switch (RENDER_TYPE) {
        case RENDER_TYPE_PBR:
            col = render(ro, rd);
            break;
        case RENDER_TYPE_MATCAP:
            col = render_matcap(ro, rd, center.xyz);
            break;
    }

    outColor = vec4(col, 1.0);
}
