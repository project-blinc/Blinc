//! Recording inputs live beside the page metadata, so fixtures share one viewport.
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub scale: u32,
}
#[derive(Clone, Default, Deserialize)]
pub struct Interaction {
    pub target: String,
    pub kind: String,
    #[serde(default)]
    pub target_index: usize,
    pub secondary: Option<String>,
    #[serde(default)]
    pub secondary_index: usize,
    pub text: Option<String>,
    pub frames: Option<usize>,
}
#[derive(Clone, Deserialize)]
pub struct Fixture {
    pub viewport: Viewport,
    pub capabilities: Vec<String>,
    pub recording: Option<Interaction>,
}
impl Fixture {
    pub fn load(slug: &str) -> Self {
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../../../web/components/catalog.json")).unwrap();
        for c in catalog["components"].as_array().unwrap() {
            if c["slug"] == slug {
                return serde_json::from_value(c.clone()).unwrap();
            }
            if let Some(example) = c["examples"]
                .as_array()
                .and_then(|v| v.iter().find(|v| v["slug"] == slug))
            {
                let mut fixture = c.clone();
                if let Some(caps) = example.get("capabilities") {
                    fixture["capabilities"] = caps.clone();
                }
                return serde_json::from_value(fixture).unwrap();
            }
        }
        panic!("missing recording metadata for {slug}")
    }
    pub fn animated(&self) -> bool {
        self.capabilities
            .iter()
            .any(|v| matches!(v.as_str(), "pointer" | "keyboard" | "animation"))
    }
}
