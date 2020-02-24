#version 450

layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 outColor;

void main() {
    vec3 uv = vec3(v_TexCoord, 0.);
    outColor = vec4(uv, 1.0);
}
