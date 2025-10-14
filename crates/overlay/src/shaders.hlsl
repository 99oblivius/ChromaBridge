struct VS_INPUT {
    float2 pos : POSITION;
    float2 tex : TEXCOORD0;
};

struct PS_INPUT {
    float4 pos : SV_POSITION;
    float2 tex : TEXCOORD0;
};

Texture2D screenTexture : register(t0);
Texture2D spectrum1Texture : register(t1);
Texture2D spectrum2Texture : register(t2);
Texture2D noiseTexture : register(t3);
SamplerState textureSampler : register(s0);
SamplerState spectrumSampler : register(s1);

cbuffer SpectrumParams : register(b0) {
    float strength;
    int useDualSpectrum;
    int useNoiseTexture;
    float padding;
};

static const float EPSILON = 0.0001;
static const float HUE_MAX = 360.0;

PS_INPUT VS_Main(VS_INPUT input) {
    PS_INPUT output;
    output.pos = float4(input.pos, 0.0, 1.0);
    output.tex = input.tex;
    return output;
}

float3 rgb_to_hsv(float3 rgb) {
    float cmax = max(rgb.r, max(rgb.g, rgb.b));
    float cmin = min(rgb.r, min(rgb.g, rgb.b));
    float delta = cmax - cmin;

    float h = 0.0;
    if (delta > EPSILON) {
        if (cmax == rgb.r) {
            h = 60.0 * fmod((rgb.g - rgb.b) / delta, 6.0);
        } else if (cmax == rgb.g) {
            h = 60.0 * ((rgb.b - rgb.r) / delta + 2.0);
        } else {
            h = 60.0 * ((rgb.r - rgb.g) / delta + 4.0);
        }
        if (h < 0.0) h += HUE_MAX;
    }

    float s = (cmax > EPSILON) ? (delta / cmax) : 0.0;
    return float3(h, s, cmax);
}

// Input: hue (0-360°), saturation (0-1), value (0-1)
float3 hsv_to_rgb(float3 hsv) {
    float h = fmod(hsv.x, HUE_MAX) / 60.0;
    float c = hsv.z * hsv.y;
    float x = c * (1.0 - abs(fmod(h, 2.0) - 1.0));
    float m = hsv.z - c;

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

    return rgb + m;
}

// Samples 1D spectrum texture (stored as horizontal 2D texture)
float3 lookup_spectrum_hsv(Texture2D spectrumTex, float hue) {
    float u = fmod(hue, HUE_MAX) / HUE_MAX;
    float3 spectrum_rgb = spectrumTex.Sample(spectrumSampler, float2(u, 0.5)).rgb;
    return rgb_to_hsv(spectrum_rgb);
}

float4 PS_Main(PS_INPUT input) : SV_Target {
    float4 color = screenTexture.Sample(textureSampler, input.tex);

    // Early exit if no correction applied
    if (strength < EPSILON) {
        return color;
    }

    float3 input_hsv = rgb_to_hsv(color.rgb);

    // Select spectrum based on noise texture for dual-spectrum mode
    float3 spectrum_hsv;
    if (useDualSpectrum && useNoiseTexture) {
        float noise_value = noiseTexture.Sample(textureSampler, input.tex).r;
        if (noise_value > 0.5) {
            spectrum_hsv = lookup_spectrum_hsv(spectrum1Texture, input_hsv.x);
        } else {
            spectrum_hsv = lookup_spectrum_hsv(spectrum2Texture, input_hsv.x);
        }
    } else {
        spectrum_hsv = lookup_spectrum_hsv(spectrum1Texture, input_hsv.x);
    }

    // Apply color correction with strength blending
    float final_hue = lerp(input_hsv.x, spectrum_hsv.x, strength);
    float final_saturation = input_hsv.y * lerp(1.0, spectrum_hsv.y, strength);

    // Preserve brightness: white component stays bright, colored component affected by spectrum
    float final_value = input_hsv.z * ((1.0 - input_hsv.y) + input_hsv.y * lerp(1.0, spectrum_hsv.z, strength));

    return float4(hsv_to_rgb(float3(final_hue, final_saturation, final_value)), color.a);
}
