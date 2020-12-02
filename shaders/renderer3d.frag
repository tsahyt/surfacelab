#version 450

layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform sampler s_Texture;

layout(set = 0, binding = 1) uniform Occupancy {
    uint has_albedo;
    uint has_roughness;
    uint has_normal;
    uint has_displacement;
    uint has_metallic;
};

layout(set = 0, binding = 2) uniform Camera {
    vec4 center;
    vec4 light_pos;
    vec2 resolution;
    float focal_length;
    float aperture_size;
    float focal_distance;

    float phi;
    float theta;
    float radius;

    float displacement_amount;
    float tex_scale;
    float texel_size;

    float environment_strength;
    float environment_blur;

    uint light_type;
    float light_strength;
    float fog_strength;

    uint draw_shadow;
    uint draw_ao;
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
layout(set = 0, binding = 8) uniform textureCube irradiance_map;
layout(set = 0, binding = 9) uniform textureCube environment_map;
layout(set = 0, binding = 10) uniform texture2D brdf_lut;

const float PI = 3.141592654;

const float INFINITY = 1.0 / 0.0;
const int MAX_STEPS = 300;
const int MAX_STEPS_AO = 32;
const int MAX_STEPS_SHD = 64;
const float MAX_DIST = 24.0;
const float SURF_DIST = .0002;
const float TEX_MIDLEVEL = .5;

const float MAX_REFLECTION_LOD = 5.0;

// DEBUG FLAGS
// #define DBG_ITERCNT 100
// #define DBG_TEXGRID 0.01
// #define DBG_AO

// - performance tuning (see ITERCNT)
// - stepping size has to adjust with displacement amount/slope
// - check validity of ambient occlusion approximation
//
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

float heightfield(vec2 p, float lod) {
    if(has_displacement != 0) {
        float h = textureLod(sampler2D(t_Displ, s_Texture), p / tex_scale, lod).r;
        return h - TEX_MIDLEVEL;
    } else {
        return 0.;
    }
}

vec3 albedo(vec2 p, float lod) {
    if(has_albedo != 0) {
        return textureLod(sampler2D(t_Albedo, s_Texture), p / tex_scale, lod).rgb;
    } else {
        return vec3(0.75);
    }
}

float roughness(vec2 p, float lod) {
    if(has_roughness != 0) {
        float r = textureLod(sampler2D(t_Roughness, s_Texture), p / tex_scale, lod).x;
        return r;
    } else {
        return 0.5;
    }
}

float metallic(vec2 p, float lod) {
    if(has_metallic != 0) {
        float r = textureLod(sampler2D(t_Metallic, s_Texture), p / tex_scale, lod).x;
        return r;
    } else {
        return 0.;
    }
}

float sdf(vec3 p, float lod) {
    float height = heightfield(p.xz, lod);
    float planeDist = p.y  - (height * displacement_amount);

    return planeDist;
}

float outer_bound(vec3 ro, vec3 rd, float d) {
    return - (ro.y - d) / rd.y;
}

// Compute the normal from the SDF numerically
vec3 sdf_normal(vec3 p, float s) {
    float d = sdf(p, 3.);
    vec2 e = vec2(s, 0);
    return normalize(d -
                     vec3(sdf(p - e.xyy, 3.),
                          sdf(p - e.yxy, 3.),
                          sdf(p - e.yyx, 3.)));
}

// Approximate normal numerically from heightfield
vec3 heightfield_normal(vec2 p, float s) {
    vec2 e = vec2(s, 0);
    float height_p = displacement_amount * heightfield(p, 0.);
    float height_x = displacement_amount * heightfield(p + e.xy, 0.);
    float height_z = displacement_amount * heightfield(p + e.yx, 0.);

    vec3 dx = vec3(e.x, height_x - height_p, e.y);
    vec3 dy = vec3(e.y, height_z - height_p, e.x);
    return normalize(cross(dy, dx));
}

//  Get normals from normal map
vec3 normal(vec3 p, float s, float lod) {
    if(has_normal != 0) {
        vec3 n = textureLod(sampler2D(t_Normal, s_Texture), p.xz / tex_scale, lod).xzy;
        return normalize(n * 2. - 1);
    } else {
        return heightfield_normal(p.xz, s);
    }
}

// --- Ray Marching

float rayMarch(vec3 ro, vec3 rd) {
    float t = outer_bound(ro, rd, displacement_amount);

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
    float max_dist = outer_bound(ro, rd, displacement_amount);
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

float ambientOcclusionCone(vec3 p, vec3 n, vec3 cd, float lod) {
    float cone_arc_width = PI / 16;
    float occlusion = 0.0;
    float t = 128 * SURF_DIST;
    float max_dist = outer_bound(p, cd, displacement_amount);

    for(int i = 0; i < MAX_STEPS_AO; i++) {
        float d = sdf(p + cd * t, lod);
        float w = abs(t * cone_arc_width);

        float local_occlusion = clamp(0, 1, ((w / 2) - d) / w);
        occlusion = max(occlusion, local_occlusion);

        if (t > max_dist) break;
        t += max_dist / MAX_STEPS_AO;
    }

    return 1.0 - occlusion;
}

float ambientOcclusion(vec3 p, vec3 n, float lod) {
    // Halton sequence sample to cosine weighted hemisphere
    float phi = constants.sample_offset.x * 2.0 * PI;
    float cosTheta = sqrt(1.0 - constants.sample_offset.y);
    float sinTheta = sqrt(1.0 - cosTheta * cosTheta);

    // tangent space sample
    vec3 h = vec3(cos(phi) * sinTheta, sin(phi) * sinTheta, cosTheta);

    // orient at n, convert to world coordinates
    vec3 up = abs(n.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
    vec3 tangent = normalize(cross(up, n));
    vec3 bitangent = cross(n, tangent);

    float ao = ambientOcclusionCone(p, n, tangent * h.x + bitangent * h.y + n * h.z, lod);
    ao += ambientOcclusionCone(p, n, tangent * -h.x + bitangent * h.y + n * h.z, lod);
    ao += ambientOcclusionCone(p, n, tangent * h.x + bitangent * -h.y + n * h.z, lod);
    ao += ambientOcclusionCone(p, n, tangent * -h.x + bitangent * -h.y + n * h.z, lod);

    return ao / 4.0;
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

vec3 environment(vec3 n, vec3 rd, vec3 f0, vec3 albedo, float roughness, float metallic, float ao) {
    // Diffuse
    vec3 kS = fresnelSchlickRoughness(max(dot(n, -rd), 0.0), f0, roughness);
    vec3 kD = 1.0 - kS;
    kD *= 1.0 - metallic;
    vec3 irradiance = texture(samplerCube(irradiance_map, s_Texture), n).rgb;
    vec3 diffuse = irradiance * albedo;

    // Specular
    vec3 r = reflect(rd, n);
    vec3 refl_color = textureLod(samplerCube(environment_map, s_Texture), r, roughness * MAX_REFLECTION_LOD).rgb;
    vec3 f = fresnelSchlickRoughness(max(dot(n, -rd), 0.0), f0, roughness);
    vec2 env_brdf = texture(sampler2D(brdf_lut, s_Texture), vec2(max(dot(n, -rd), 0.0), roughness)).rg;
    vec3 specular = refl_color * (f * env_brdf.x + env_brdf.y);

    return (kD * diffuse + specular) * ao * environment_strength;
}

vec2 concentric_sample_disk(vec2 uv) {
    float r = sqrt(uv.x);
    float theta = 2.0 * PI * uv.y;
    return vec2(r * cos(theta), r * sin(theta));
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
        vec2 lens_uv = concentric_sample_disk(constants.sample_offset) * lens_radius;
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

vec3 render(vec3 ro, vec3 rd) {
    vec3 col = vec3(0.);

    float d = rayMarch(ro, rd);

    // Early termination for non-surface pixels
    vec3 world = textureLod(samplerCube(environment_map, s_Texture), rd, environment_blur).rgb * environment_strength;
    if (d == INFINITY) { return world; }

    vec3 p = ro + rd * d;
    vec3 n = normal(p, max(texel_size, world_space_sample_size(d)), lod_by_distance(d));

    // Texture fetching
    vec3 albedo = albedo(p.xz, lod_by_distance(d));
    float metallic = metallic(p.xz, lod_by_distance(d));
    float roughness = roughness(p.xz, lod_by_distance(d));

    // Lights
    vec3 f0 = vec3(0.04);
    f0 = mix(f0, albedo, metallic);

    col += light(p, n, rd, f0, d, albedo, metallic, roughness, vec3(1.), light_pos.xyz, 1.);

    // Ambient Light
    float ao;
    if (draw_ao == 1) {
        ao = ambientOcclusion(p, n, lod_by_distance(d));
    } else {
        ao = 1.;
    }

    col += environment(n, rd, f0, albedo, roughness, metallic, ao);

    #ifdef DBG_TEXGRID
    if (fract(p.x / tex_scale) < DBG_TEXGRID / tex_scale || fract(p.z / tex_scale) < DBG_TEXGRID / tex_scale) { col += vec3(0.3, 0.8, 0.); }
    #endif

    // View Falloff
    col += vec3(0.5,0.5,0.4) * smoothstep(2,20,d) * fog_strength;
    col = mix(world, col, smoothstep(10., 9., length(p.xz)));

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

    vec3 col = render(ro, rd);

    outColor = vec4(col, 1.0);
}
