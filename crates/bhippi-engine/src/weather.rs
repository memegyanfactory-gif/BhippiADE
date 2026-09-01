//! UltraSky-style weather presets (ENG-105).
//!
//! These used to live in `ui/src/engine/EngineSceneDocument.ts`, where the webview both
//! owned the numbers and decided what changing the weather does to the scene — two things
//! INV-073 forbids. The presets are data; the *effect* of picking one is lowered into ops
//! by [`crate::action::EngineAction::SetWeather`], so a weather change is an ordinary
//! undoable transaction like every other edit.

use serde::{Deserialize, Serialize};
use specta::Type;

/// One weather preset. `sky` is a packed `0xRRGGBB` the viewport paints the dome with;
/// `fog` is a 0..1 density the renderer scales its own fog range by.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct WeatherPreset {
    pub id: String,
    pub label: String,
    pub ambient: [f32; 3],
    /// Intensity written onto every `directional` light in the scene.
    pub sun: f32,
    pub fog: f32,
    pub sky: u32,
    /// `none` · `rain` · `snow` — the overlay particle system the viewport runs.
    pub precip: String,
}

/// The eight ids `schema.rs` accepts on `WeatherVolume.preset` and the manifest's
/// `settings.weather`. Anything outside this list is rejected before it reaches disk.
pub const WEATHER_IDS: [&str; 8] = [
    "clear", "overcast", "rain", "snow", "fog", "storm", "sunset", "night",
];

fn preset_of(
    id: &str,
    label: &str,
    ambient: [f32; 3],
    sun: f32,
    fog: f32,
    sky: u32,
    precip: &str,
) -> WeatherPreset {
    WeatherPreset {
        id: id.to_owned(),
        label: label.to_owned(),
        ambient,
        sun,
        fog,
        sky,
        precip: precip.to_owned(),
    }
}

/// Every preset, in display order (stable — the picker renders straight from this).
#[must_use]
pub fn presets() -> Vec<WeatherPreset> {
    vec![
        preset_of(
            "clear",
            "Clear",
            [0.32, 0.36, 0.42],
            2.4,
            0.0,
            0x87a8d4,
            "none",
        ),
        preset_of(
            "overcast",
            "Overcast",
            [0.22, 0.24, 0.26],
            0.8,
            0.12,
            0x6b7380,
            "none",
        ),
        preset_of(
            "rain",
            "Rain",
            [0.16, 0.18, 0.22],
            0.55,
            0.18,
            0x3d4754,
            "rain",
        ),
        preset_of(
            "snow",
            "Snow",
            [0.34, 0.36, 0.4],
            0.7,
            0.22,
            0xb8c4d4,
            "snow",
        ),
        preset_of("fog", "Fog", [0.2, 0.22, 0.22], 0.4, 0.55, 0x8a9096, "none"),
        preset_of(
            "storm",
            "Storm",
            [0.1, 0.12, 0.16],
            0.3,
            0.28,
            0x1c2430,
            "rain",
        ),
        preset_of(
            "sunset",
            "Sunset",
            [0.36, 0.22, 0.14],
            1.6,
            0.08,
            0xc45a2a,
            "none",
        ),
        preset_of(
            "night",
            "Night",
            [0.04, 0.05, 0.08],
            0.12,
            0.06,
            0x07090e,
            "none",
        ),
    ]
}

/// Look one preset up by id.
#[must_use]
pub fn preset(id: &str) -> Option<WeatherPreset> {
    presets().into_iter().find(|preset| preset.id == id)
}

#[cfg(test)]
mod tests {
    use super::{preset, presets, WEATHER_IDS};

    #[test]
    fn every_declared_id_has_a_preset_and_nothing_else_does() {
        for id in WEATHER_IDS {
            assert!(preset(id).is_some(), "missing preset {id}");
        }
        assert_eq!(presets().len(), WEATHER_IDS.len());
        assert!(preset("hurricane").is_none());
    }

    #[test]
    fn presets_are_deterministic_and_ordered() {
        let first: Vec<String> = presets().into_iter().map(|preset| preset.id).collect();
        let second: Vec<String> = presets().into_iter().map(|preset| preset.id).collect();
        assert_eq!(first, second);
        assert_eq!(first.first().map(String::as_str), Some("clear"));
    }

    #[test]
    fn schema_enum_and_weather_ids_agree() {
        // The registry's WeatherVolume.preset enum is the other half of this contract.
        let schema = crate::schema::component("WeatherVolume").expect("registered");
        let field = schema
            .fields
            .iter()
            .find(|field| field.name == "preset")
            .expect("preset field");
        let crate::schema::FieldKind::Enum(values) = field.kind else {
            panic!("WeatherVolume.preset must stay an enum");
        };
        let mut from_schema = values.to_vec();
        let mut from_here = WEATHER_IDS.to_vec();
        from_schema.sort_unstable();
        from_here.sort_unstable();
        assert_eq!(from_schema, from_here);
    }
}
