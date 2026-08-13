//! CaptureAsset — port of Sources/KiriCore/CaptureAsset.swift.
//! The JSON representation stays byte-compatible with the Swift app so an
//! existing user library keeps working after migration.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureKind {
    Image,
    Video,
    Gif,
}

impl<'de> Deserialize<'de> for CaptureKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            // The Swift decoder maps the legacy value to image.
            "image" | "longImage" => Ok(CaptureKind::Image),
            "video" => Ok(CaptureKind::Video),
            "gif" => Ok(CaptureKind::Gif),
            other => Err(serde::de::Error::custom(format!(
                "unknown capture kind: {other}"
            ))),
        }
    }
}

impl CaptureKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CaptureKind::Image => "image",
            CaptureKind::Video => "video",
            CaptureKind::Gif => "gif",
        }
    }
}

fn serialize_uuid<S: Serializer>(id: &uuid::Uuid, serializer: S) -> Result<S::Ok, S::Error> {
    // Swift's UUID Codable encodes the uppercase UUID string.
    serializer.serialize_str(&id.to_string().to_uppercase())
}

fn deserialize_uuid<'de, D: Deserializer<'de>>(deserializer: D) -> Result<uuid::Uuid, D::Error> {
    let value = String::deserialize(deserializer)?;
    uuid::Uuid::parse_str(&value).map_err(serde::de::Error::custom)
}

fn serialize_ms<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    // Swift Date.millisecondsSince1970 encodes a numeric value.
    serializer.serialize_f64(*value)
}

fn deserialize_ms<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    f64::deserialize(deserializer)
}

/// Field order matches Swift's `.sortedKeys` (alphabetical) so the written
/// JSON stays close to byte-identical with the Swift app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureAsset {
    #[serde(serialize_with = "serialize_ms", deserialize_with = "deserialize_ms")]
    pub created_at: f64,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        serialize_with = "serialize_opt_ms",
        deserialize_with = "deserialize_opt_ms"
    )]
    pub duration: Option<f64>,
    pub filename: String,
    #[serde(serialize_with = "serialize_uuid", deserialize_with = "deserialize_uuid")]
    pub id: uuid::Uuid,
    pub is_favorite: bool,
    pub kind: CaptureKind,
    pub pixel_height: i64,
    pub pixel_width: i64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source_application: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        serialize_with = "serialize_opt_ms",
        deserialize_with = "deserialize_opt_ms"
    )]
    pub trashed_at: Option<f64>,
}

fn serialize_opt_ms<S: Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        Some(v) => serializer.serialize_f64(*v),
        None => serializer.serialize_none(),
    }
}

fn deserialize_opt_ms<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    Option::<f64>::deserialize(deserializer)
}

impl CaptureAsset {
    /// Mirrors `CaptureAsset.searchableText`.
    pub fn searchable_text(&self) -> String {
        let mut parts: Vec<String> = Vec::with_capacity(3);
        parts.push(self.filename.clone());
        if let Some(app) = &self.source_application {
            parts.push(app.clone());
        }
        parts.push(self.kind.as_str().to_string());
        parts.join(" ").to_lowercase()
    }

    /// Sort key matching `AssetLibrary.allAssets` (createdAt descending).
    pub fn created_at_ms(&self) -> i64 {
        self.created_at.floor() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searchable_text_matches_swift() {
        let asset = CaptureAsset {
            id: uuid::Uuid::new_v4(),
            kind: CaptureKind::Image,
            created_at: 1_700_000_000_000.0,
            filename: "20240101-120000-abc.png".into(),
            pixel_width: 100,
            pixel_height: 200,
            duration: None,
            source_application: Some("Safari".into()),
            is_favorite: false,
            trashed_at: None,
        };
        assert_eq!(
            asset.searchable_text(),
            "20240101-120000-abc.png safari image"
        );
    }

    #[test]
    fn json_round_trip_keeps_swift_shape() {
        let asset = CaptureAsset {
            id: uuid::Uuid::new_v4(),
            kind: CaptureKind::Video,
            created_at: 1_700_000_000_123.0,
            filename: "20240101-120000-abc.mp4".into(),
            pixel_width: 1920,
            pixel_height: 1080,
            duration: Some(12.5),
            source_application: None,
            is_favorite: true,
            trashed_at: None,
        };
        let json = serde_json::to_string(&asset).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["createdAt"], serde_json::json!(1_700_000_000_123.0));
        assert!(parsed.get("sourceApplication").is_none(), "{json}");
        assert!(parsed["id"].as_str().unwrap().chars().all(|c| c.is_ascii_uppercase() || c == '-' || c.is_ascii_digit()));
        let decoded: CaptureAsset = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, asset);
    }

    #[test]
    fn decodes_legacy_long_image_kind() {
        let asset = CaptureAsset {
            id: uuid::Uuid::new_v4(),
            kind: CaptureKind::Image,
            created_at: 1_700_000_000_000.0,
            filename: "legacy.png".into(),
            pixel_width: 10,
            pixel_height: 10,
            duration: None,
            source_application: None,
            is_favorite: false,
            trashed_at: None,
        };
        let json = serde_json::to_string(&asset)
            .unwrap()
            .replace("\"image\"", "\"longImage\"");
        let decoded: CaptureAsset = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.kind, CaptureKind::Image);
    }
}
