#version 460

layout(set = 0, binding = 0) uniform texture2D text_texture;
layout(set = 0, binding = 1) uniform sampler image_sampler;
layout(set = 1, binding = 0) uniform texture2D image_texture;

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 2) flat in uint v_mode;

layout(location = 0) out vec4 out_color;

const uint VMODE_TEXT = 0;
const uint VMODE_IMAGE = 1;
const uint VMODE_GEOMETRY = 2;

void main() {
    switch(v_mode) {
        case VMODE_TEXT:
            float a = texture(sampler2D(text_texture, image_sampler), v_uv).r;
            out_color = v_color * vec4(1.0, 1.0, 1.0, a);
            break;
        case VMODE_IMAGE:
            out_color = texture(sampler2D(image_texture, image_sampler), v_uv);
            break;
        case VMODE_GEOMETRY:
            out_color = v_color;
    }
}
