#version 450

layout(set = 0, binding = 0) uniform sampler2D atlas;

layout(push_constant) uniform PushConstants {
    mat4 proj;
    vec4 clip_rect; // left, top, right, bottom
} pc;

layout(location = 0) in vec2 fragUv;
layout(location = 1) in vec4 fragColor;

layout(location = 0) out vec4 outColor;

void main() {
    if (gl_FragCoord.x < pc.clip_rect.x || gl_FragCoord.x > pc.clip_rect.z ||
        gl_FragCoord.y < pc.clip_rect.y || gl_FragCoord.y > pc.clip_rect.w) {
        discard;
    }
    float coverage = texture(atlas, fragUv).r;
    outColor = vec4(1.0, 1.0, 1.0, coverage) * fragColor;
}
