#version 450

layout(set = 0, binding = 0) uniform sampler2D atlas;

layout(location = 0) in vec2 fragUv;
layout(location = 1) in vec4 fragColor;

layout(location = 0) out vec4 outColor;

void main() {
    float coverage = texture(atlas, fragUv).r;
    outColor = vec4(1.0, 1.0, 1.0, coverage) * fragColor;
}
