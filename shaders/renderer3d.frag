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
    float phi;
    float theta;
    float radius;
    float displacement_amount;
    float tex_scale;
    uint light_type;
    float light_strength;
    float fog_strength;
    uint draw_shadow;
    uint draw_ao;
};

const uint LIGHT_TYPE_POINT = 0;
const uint LIGHT_TYPE_SUN = 1;

layout(set = 0, binding = 3) uniform texture2D t_Displ;
layout(set = 0, binding = 4) uniform texture2D t_Albedo;
layout(set = 0, binding = 5) uniform texture2D t_Normal;
layout(set = 0, binding = 6) uniform texture2D t_Roughness;
layout(set = 0, binding = 7) uniform texture2D t_Metallic;
layout(set = 0, binding = 8) uniform textureCube irradiance_map;

const float PI = 3.141592654;

const int MAX_STEPS = 300;
const int MAX_STEPS_AO = 6;
const float MAX_DIST = 24.0;
const float SURF_DIST = .0002;
const float TEX_MIDLEVEL = .5;

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

    return planeDist / 2.;
}

// Compute the normal from the SDF numerically
vec3 sdf_normal(vec3 p, float lod) {
    float d = sdf(p, lod);
    vec2 e = vec2(0.01, 0);
    return normalize(d -
                     vec3(sdf(p - e.xyy, lod),
                          sdf(p - e.yxy, lod),
                          sdf(p - e.yyx, lod)));
}

//  Get normals from normal map
vec3 normal(vec3 p, float lod) {
    if(has_normal != 0) {
        vec3 n = textureLod(sampler2D(t_Normal, s_Texture), p.xz / tex_scale, lod).xzy;
        return normalize(n * 2. - 1);
    } else {
        return sdf_normal(p, lod);
    }
}

// Approximate normal numerically from heightfield
vec3 heightfield_normal(vec2 p) {
    vec2 e = vec2(0.01, 0);
    float height_p = heightfield(p, 0.);
    float height_x = heightfield(p + e.xy, 0.);
    float height_z = heightfield(p + e.yx, 0.);

    vec3 dx = normalize(vec3(e.x, height_x - height_p, e.y));
    vec3 dy = normalize(vec3(e.y, height_z - height_p, e.x));
    return cross(dy, dx);
}

// --- Ray Marching

float rayMarch(vec3 ro, vec3 rd, out float itrc) {
    float dO = 0.;

    for(int i = 0; i < MAX_STEPS; i++) {
        vec3 p = ro + rd * dO;
        float dS = sdf(p, lod_by_distance(dO));
        if (dO > MAX_DIST || abs(dS) < (SURF_DIST * dO)) { break; }
        if (dS < 0.) {
            // when inside the surface make sure to step back out again
            dO -= SURF_DIST;
        } else {
            dO += dS / 1.;
        }
        itrc += 1;
    }

    return dO;
}

float rayShadowSoft(vec3 ro, vec3 rd, float w, out float itrc) {
    float s = 1.0;
    float dO = 256 * SURF_DIST;

    for(int i = 0; i < MAX_STEPS / 4; i++) {
        // get distance and correct for cases where we are already inside because of faulty starting points
        float dS = max(sdf(ro + rd * dO, lod_by_distance(dO) + SHADOW_LOD_OFFSET), SURF_DIST);
        s = min(s, 0.5 + 0.5 * dS / (w * dO));
        if (s < 0 || dO > MAX_DIST) break;
        dO += 2 * tex_scale * dS;
        itrc += 1;
    }

    s = max(s, 0.0);

    return smoothstep(0.5, 0.6, s);
}

// TODO: better AO
float ambientOcclusion(vec3 p, vec3 n) {
    float dO = SURF_DIST;
    float ao = 1.;
    float increment = 1. / MAX_STEPS_AO;

    for(int i = 0; i < MAX_STEPS_AO; i++) {
        float d = max(sdf(p + n * dO, 5.), SURF_DIST);
        ao = min(d / dO, ao);
        dO += increment;
    }

    return ao;
}

// --- Shading

vec3 fresnelSchlick(float cosTheta, vec3 F0)
{
    return F0 + (1.0 - F0) * pow(1.0 - cosTheta, 5.0);
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

vec3 light(vec3 p, vec3 n, vec3 rd, float d, vec3 lightColor, vec3 lightPos, float w, out float sitr) {
    rd *= -1;

    vec3 albedo = albedo(p.xz, lod_by_distance(d));
    float metallic = metallic(p.xz, lod_by_distance(d));
    float roughness = roughness(p.xz, lod_by_distance(d));

    vec3 F0 = vec3(0.04);
    F0 = mix(F0, albedo, metallic);

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
    vec3 f = fresnelSchlick(max(dot(h, rd), 0.), F0);

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
        shadow = rayShadowSoft(p, l, w, sitr);
    } else {
        shadow = 1.;
    }

    return (kD * albedo / PI + specular) * radiance * ndotl * shadow;
}

vec3 camera(vec3 ro, vec3 lookAt, vec2 uv, float zoom) {
    vec3 forward = normalize(lookAt - ro);
    vec3 right = normalize(cross(vec3(0,1,0), forward));
    vec3 up = cross(forward, right);

    vec3 c = ro + forward * zoom;
    vec3 i = c + uv.x * right + uv.y * up;

    return normalize(i - ro);
}

void main() {
    vec2 uv = (v_TexCoord - 0.5) * vec2(resolution.x / resolution.y, 1);

    // Spherical Coordinate Input (phi, theta)
    vec3 ro = center.xyz + (radius * vec3(
                   sin(phi) * cos(theta),
                   cos(phi),
                   sin(phi) * sin(theta)));

    // Camera
    float itrc = 0.;
    float sitrc = 0.;
    vec3 col = vec3(0.);

    vec3 rd = camera(ro, center.xyz, uv, 1.);
    float d = rayMarch(ro, rd, itrc);
    vec3 p = ro + rd * d;
    vec3 n = normal(p, lod_by_distance(d));

    col += light(p, n, rd, d, vec3(1.), light_pos.xyz, 1., sitrc);

    // Ambient Light
    float ao;
    if (draw_ao == 1) {
        ao = ambientOcclusion(p, n);
    } else {
        ao = 1.;
    }

    col += vec3(0.06) * ao * albedo(p.xz, lod_by_distance(d));

    // Light Transform
    col /= (col + vec3(1.));
    col = pow(col, vec3(1. / 1.2));

    #ifdef DBG_TEXGRID
    if (fract(p.x / tex_scale) < DBG_TEXGRID / tex_scale || fract(p.z / tex_scale) < DBG_TEXGRID / tex_scale) { col += vec3(0.3, 0.8, 0.); }
    #endif

    // View Falloff
    col = mix(col, vec3(0.), step(MAX_DIST, d));
    col += vec3(0.5,0.5,0.4) * smoothstep(2,20,d) * fog_strength;
    col *= vec3(smoothstep(10., 2., length(p.xz)));

    // debugging views
    #ifdef DBG_ITERCNT
    col.r += step(DBG_ITERCNT, itrc);
    col.g += step(DBG_ITERCNT, sitrc);
    #endif

    #ifdef DBG_AO
    col.r += 1 - ao;
    #endif

    outColor = vec4(col, 1.0);
}
