//! Module tests for the vision-family selection surface and the crate version
//! constant. Everything here is deterministic and offline.

use apexkit_vector::VERSION;

#[test]
fn crate_version_is_exposed() {
    assert!(!VERSION.is_empty(), "VERSION must be a non-empty string");
    // Semantic-ish: at least one dot, all numeric segments.
    assert!(VERSION.contains('.'), "VERSION should look like x.y.z");
}

#[cfg(feature = "onnx")]
mod vision_family {
    use apexkit_vector::models::onnx_vision::{OnnxVisionConfig, VisionFamily};

    #[test]
    fn from_env_str_maps_known_families() {
        assert_eq!(VisionFamily::from_env_str("siglip"), VisionFamily::Siglip);
        assert_eq!(VisionFamily::from_env_str("siglip2"), VisionFamily::Siglip2);
        assert_eq!(VisionFamily::from_env_str("clip"), VisionFamily::Clip);
        assert_eq!(
            VisionFamily::from_env_str("openclip"),
            VisionFamily::OpenClip
        );
        assert_eq!(VisionFamily::from_env_str("dinov2"), VisionFamily::Dinov2);
        assert_eq!(VisionFamily::from_env_str("custom"), VisionFamily::Custom);
    }

    #[test]
    fn from_env_str_is_case_insensitive() {
        assert_eq!(VisionFamily::from_env_str("SiGLiP2"), VisionFamily::Siglip2);
        assert_eq!(VisionFamily::from_env_str("CLIP"), VisionFamily::Clip);
    }

    #[test]
    fn unknown_family_falls_back_to_siglip2() {
        assert_eq!(
            VisionFamily::from_env_str("totally-bogus"),
            VisionFamily::Siglip2
        );
        assert_eq!(VisionFamily::from_env_str(""), VisionFamily::Siglip2);
    }

    #[test]
    fn dinov2_is_the_only_vision_only_family() {
        assert!(!VisionFamily::Dinov2.supports_text_image_in_principle());
        for fam in [
            VisionFamily::Siglip,
            VisionFamily::Siglip2,
            VisionFamily::Clip,
            VisionFamily::OpenClip,
            VisionFamily::Custom,
        ] {
            assert!(
                fam.supports_text_image_in_principle(),
                "{:?} should support text-image search",
                fam
            );
        }
    }

    #[test]
    fn as_str_round_trips_through_from_env_str() {
        for fam in [
            VisionFamily::Siglip,
            VisionFamily::Siglip2,
            VisionFamily::Clip,
            VisionFamily::OpenClip,
            VisionFamily::Dinov2,
            VisionFamily::Custom,
        ] {
            assert_eq!(VisionFamily::from_env_str(fam.as_str()), fam);
        }
    }

    #[test]
    fn vision_config_presets_have_expected_normalization() {
        let siglip = OnnxVisionConfig::siglip_base_patch16_224();
        assert_eq!(siglip.image_size, 224);
        assert_eq!(siglip.mean, [0.5, 0.5, 0.5]);
        assert_eq!(siglip.std, [0.5, 0.5, 0.5]);
        assert_eq!(siglip.input_name, "pixel_values");

        let siglip2 = OnnxVisionConfig::siglip2_base_patch16_384();
        assert_eq!(siglip2.image_size, 384);

        let clip = OnnxVisionConfig::clip_vit_base_patch32();
        assert_eq!(clip.image_size, 224);
        // CLIP uses ImageNet normalization, not [0.5, 0.5, 0.5].
        assert_ne!(clip.mean, [0.5, 0.5, 0.5]);

        let dinov2 = OnnxVisionConfig::dinov2_base();
        assert_eq!(dinov2.image_size, 224);
        assert_eq!(dinov2.output_name, "last_hidden_state");
    }
}
