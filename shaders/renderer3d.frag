#version 450

layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform texture2D t_Displ;
layout(set = 0, binding = 1) uniform sampler s_Sampler;

const int MAX_STEPS = 1000;
const float MAX_DIST = 24.0;
const float SURF_DIST = .0002;

const float TEX_SCALE = 8.;
const float TEX_DISPL = 1.;
const float TEX_MIDLEVEL = .5;

#define LOD_BIAS .5

float lod_by_distance(float d) {
    return log(d * LOD_BIAS);
}

float heightfield(vec2 p, float lod) {
    float h = textureLod(sampler2D(t_Displ, s_Sampler), p / TEX_SCALE, lod).r;
    return h - TEX_MIDLEVEL;
}

float sdf(vec3 p, float lod) {
    float height = heightfield(p.xz, lod);
    float planeDist = p.y - (height * TEX_DISPL);

    return planeDist / 12.;
}

vec3 normal(vec3 p, float lod) {
    float d = sdf(p, lod);
    vec2 e = vec2(0.01, 0);
    return normalize(d -
                     vec3(sdf(p - e.xyy, lod),
                          sdf(p - e.yxy, lod),
                          sdf(p - e.yyx, lod)));
}

float rayMarch(vec3 ro, vec3 rd) {
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
    }

    return dO;
}

float rayShadowSoft(vec3 ro, vec3 rd, float w) {
    float s = 1.0;
    float dO = 0.;

    for(int i = 0; i < MAX_STEPS; i++) {
        float dS = sdf(ro + rd * dO, 2);
        s = min(s, 0.5 + 0.5 * dS / (w * dO));
        if (s < 0. || dO > MAX_DIST) break;
        dO += dS;
    }

    s = max(s, 0.0);

    return smoothstep(0.2, 1., s);
}

float light(vec3 p, vec3 lightPos, float intensity) {
    vec3 l = normalize(lightPos - p);
    vec3 n = normal(p,1);

    float dif = clamp(dot(n,l), 0., 1.);
    float s = rayShadowSoft(p + n * 2 * SURF_DIST, l, 0.05);

    float ldist = length(p - l);
    float falloff = 1 / ldist * ldist;

    return dif * s * falloff * intensity;
}

vec3 camera(vec3 ro, vec3 lookAt, vec2 uv, float zoom) {
    vec3 forward = normalize(lookAt - ro);
    vec3 right = normalize(cross(vec3(0,1,0), forward));
    vec3 up = cross(forward, right);

    vec3 c = ro + forward * zoom;
    vec3 i = c + uv.x * right + uv.y * up;

    return normalize(i - ro);
}

mat2 rot(float t) {
    float s = sin(t);
    float c = cos(t);
    return mat2(c, -s, s, c);
}

void main() {
    vec2 uv = (v_TexCoord - 0.5);

    // Spherical Coordinate Input (phi, theta)
    vec2 sph = vec2(1,1);
    float rad = 6;
    vec3 ro = rad * vec3(sin(sph.y) * cos(sph.x), cos(sph.y), sin(sph.y) * sin(sph.x));

    // Camera
    vec3 lookAt = vec3(0, 0, 0);
    vec3 rd = camera(ro, lookAt, uv, 1.);

    float d = rayMarch(ro, rd);
    vec3 p = ro + rd * d;

    vec3 col = vec3(0.2, 0.3, 0.7) * light(p, vec3(0, 3, - 2), 0.6);
    col += vec3(0.5, 0.4, 0.2) * light(p, vec3(3, 4, 2), 1.);

    outColor = vec4(col, 1.0);
}
