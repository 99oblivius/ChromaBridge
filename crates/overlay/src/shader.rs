// GPU-accelerated color processing shader
pub const VERTEX_SHADER: &str = r#"
#version 330 core
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;

out vec2 v_uv;

void main() {
    v_uv = a_uv;
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
"#;

pub const FRAGMENT_SHADER: &str = r#"
#version 330 core
in vec2 v_uv;
out vec4 FragColor;

uniform sampler2D u_texture;
uniform float u_strength;
uniform sampler2D u_noise_texture;
uniform bool u_has_noise;
uniform bool u_has_dual_spectrum;

// Hue mapping uniforms (we'll use a 1D texture for the spectrum LUT)
uniform sampler1D u_spectrum1_lut;
uniform sampler1D u_spectrum2_lut;

// RGB to HSV conversion
vec3 rgb2hsv(vec3 c) {
    vec4 K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    vec4 p = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
    vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));

    float d = q.x - min(q.w, q.y);
    float e = 1.0e-10;
    return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

// HSV to RGB conversion
vec3 hsv2rgb(vec3 c) {
    vec4 K = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
}

void main() {
    vec4 color = texture(u_texture, v_uv);
    vec3 input_hsv = rgb2hsv(color.rgb);

    // Determine spectrum based on noise (if applicable)
    vec3 spectrum_rgb;
    if (u_has_noise && u_has_dual_spectrum) {
        float noise_val = texture(u_noise_texture, v_uv).r;
        spectrum_rgb = texture(noise_val < 0.5 ? u_spectrum2_lut : u_spectrum1_lut, input_hsv.x).rgb;
    } else {
        spectrum_rgb = texture(u_spectrum1_lut, input_hsv.x).rgb;
    }

    vec3 spectrum_hsv = rgb2hsv(spectrum_rgb);

    // Direct calculations without intermediate variables
    float final_hue = input_hsv.x + u_strength * (spectrum_hsv.x - input_hsv.x);
    float final_saturation = input_hsv.y * mix(1.0, spectrum_hsv.y, u_strength);
    float final_value = input_hsv.z * ((1.0 - input_hsv.y) + input_hsv.y * mix(1.0, spectrum_hsv.z, u_strength));

    vec3 corrected_rgb = hsv2rgb(vec3(final_hue, final_saturation, final_value));
    FragColor = vec4(corrected_rgb, color.a);
}
"#;
