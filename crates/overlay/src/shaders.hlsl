// Fullscreen quad vertex shader
struct VS_INPUT {
    float2 pos : POSITION;
    float2 tex : TEXCOORD0;
};

struct PS_INPUT {
    float4 pos : SV_POSITION;
    float2 tex : TEXCOORD0;
};

// Vertex Shader - transforms to screen space
PS_INPUT VS_Main(VS_INPUT input) {
    PS_INPUT output;
    output.pos = float4(input.pos, 0.0, 1.0);
    output.tex = input.tex;
    return output;
}

// Pixel Shader - samples texture and applies color correction via spectrum lookup
Texture2D screenTexture : register(t0);       // Captured screen
Texture2D spectrum1Texture : register(t1);    // First spectrum lookup (1D texture stored as 2D)
Texture2D spectrum2Texture : register(t2);    // Second spectrum lookup (optional, for dual-spectrum)
Texture2D noiseTexture : register(t3);        // Noise texture for dual-spectrum interlacing
SamplerState textureSampler : register(s0);   // Linear sampler for screen
SamplerState spectrumSampler : register(s1);  // Sampler for spectrum lookup

cbuffer SpectrumParams : register(b0) {
    float strength;              // Correction strength (0.0 = off, 1.0 = full)
    int useDualSpectrum;         // 1 if using dual spectrum, 0 otherwise
    int useNoiseTexture;         // 1 if noise texture is loaded, 0 otherwise
    float padding;               // Padding for alignment
};

// RGB to HSV conversion
float3 rgb_to_hsv(float3 rgb) {
    float cmax = max(rgb.r, max(rgb.g, rgb.b));
    float cmin = min(rgb.r, min(rgb.g, rgb.b));
    float delta = cmax - cmin;

    // Hue calculation
    float h = 0.0;
    if (delta > 0.0001) {
        if (cmax == rgb.r) {
            h = 60.0 * fmod(((rgb.g - rgb.b) / delta), 6.0);
        } else if (cmax == rgb.g) {
            h = 60.0 * (((rgb.b - rgb.r) / delta) + 2.0);
        } else {
            h = 60.0 * (((rgb.r - rgb.g) / delta) + 4.0);
        }
    }
    if (h < 0.0) h += 360.0;

    // Saturation
    float s = (cmax > 0.0001) ? (delta / cmax) : 0.0;

    // Value
    float v = cmax;

    return float3(h, s, v);
}

// HSV to RGB conversion
// Input: hsv.x = hue in degrees (0-360), hsv.y = saturation (0-1), hsv.z = value (0-1)
// Output: RGB in 0-1 range
float3 hsv_to_rgb(float3 hsv) {
    float h = fmod(hsv.x, 360.0) / 60.0; // Convert to 0-6 range
    float s = hsv.y;
    float v = hsv.z;

    float c = v * s;
    float x = c * (1.0 - abs(fmod(h, 2.0) - 1.0));
    float m = v - c;

    float3 rgb;
    if (h < 1.0) {
        rgb = float3(c, x, 0.0);
    } else if (h < 2.0) {
        rgb = float3(x, c, 0.0);
    } else if (h < 3.0) {
        rgb = float3(0.0, c, x);
    } else if (h < 4.0) {
        rgb = float3(0.0, x, c);
    } else if (h < 5.0) {
        rgb = float3(x, 0.0, c);
    } else {
        rgb = float3(c, 0.0, x);
    }

    return rgb + float3(m, m, m);
}

// Lookup HSV from spectrum texture based on input hue
// Returns full HSV: hue (degrees), saturation (0-1), value (0-1)
float3 lookup_spectrum_hsv(Texture2D spectrumTex, float input_hue) {
    // Normalize hue to 0.0-1.0 range
    float u = fmod(input_hue, 360.0) / 360.0;

    // Sample from 1D spectrum texture (stored as horizontal strip)
    // Use 0.5 for v coordinate to sample from center of texture
    float3 spectrum_rgb = spectrumTex.Sample(spectrumSampler, float2(u, 0.5)).rgb;

    // Convert the spectrum RGB to HSV to extract full HSV
    float3 spectrum_hsv = rgb_to_hsv(spectrum_rgb);

    return spectrum_hsv; // Returns (hue, saturation, value)
}

float4 PS_Main(PS_INPUT input) : SV_Target {
    // Sample the captured screen texture
    float4 color = screenTexture.Sample(textureSampler, input.tex);

    // If strength is 0, return original color (no correction)
    if (strength < 0.001) {
        return color;
    }

    // Convert to HSV to get the original hue, saturation, and value (brightness)
    float3 input_hsv = rgb_to_hsv(color.rgb);
    float input_hue = input_hsv.x;        // Original hue (0-360 degrees)
    float input_saturation = input_hsv.y;  // Original saturation (0-1)
    float input_value = input_hsv.z;       // Original value/brightness (0-1)

    // Determine which spectrum to use and look up the mapped HSV
    float3 spectrum_hsv;

    if (useDualSpectrum && useNoiseTexture) {
        // Sample noise texture to decide which spectrum to use
        float noise_value = noiseTexture.Sample(textureSampler, input.tex).r;

        // Black pixels (< 0.5) use spectrum1, white pixels (>= 0.5) use spectrum2
        if (noise_value > 0.5) {
            spectrum_hsv = lookup_spectrum_hsv(spectrum1Texture, input_hue);
        } else {
            spectrum_hsv = lookup_spectrum_hsv(spectrum2Texture, input_hue);
        }
    } else {
        // Single spectrum mode - always use spectrum1
        spectrum_hsv = lookup_spectrum_hsv(spectrum1Texture, input_hue);
    }

    float spectrum_hue = spectrum_hsv.x;
    float spectrum_saturation = spectrum_hsv.y;
    float spectrum_value = spectrum_hsv.z;

    // Apply hue remapping with strength
    float final_hue = lerp(input_hue, spectrum_hue, strength);

    // Apply saturation transformation
    // Multiply by spectrum saturation to allow desaturation (white/gray)
    float spectrum_sat_factor = lerp(1.0, spectrum_saturation, strength);
    float final_saturation = input_saturation * spectrum_sat_factor;

    // Preserve brightness based on the input's white component
    // Low saturation inputs (pink) have more white → should stay brighter
    // High saturation inputs (pure red) have less white → should get darker when desaturated
    float input_white_component = 1.0 - input_saturation;
    float final_white_component = 1.0 - final_saturation;

    // Blend value: preserve the input's white as brightness, apply spectrum's darkness to the colored part
    float spectrum_val_factor = lerp(1.0, spectrum_value, strength);
    float final_value = input_value * (input_white_component + input_saturation * spectrum_val_factor);

    // Reconstruct RGB from transformed HSV
    float3 corrected_hsv = float3(final_hue, final_saturation, final_value);
    float3 corrected_rgb = hsv_to_rgb(corrected_hsv);

    // Return with original alpha
    return float4(corrected_rgb, color.a);
}
