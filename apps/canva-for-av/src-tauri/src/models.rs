use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Presentation {
    pub id: String,
    pub title: String,
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub slides: Vec<Slide>,
    pub fonts: Vec<FontRef>,
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slide {
    pub index: usize,
    pub elements: Vec<SlideElement>,
    pub thumbnail_url: Option<String>,
    pub thumbnail_local: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SlideElement {
    Text {
        content: String,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        font_family: Option<String>,
        font_size: Option<f64>,
        color: Option<String>,
        bold: bool,
        italic: bool,
        rotation: f64,
    },
    Image {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        asset_url: String,
        local_path: Option<String>,
        rotation: f64,
    },
    Shape {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        svg_path: Option<String>,
        fill_color: Option<String>,
        stroke_color: Option<String>,
        rotation: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontRef {
    pub family: String,
    pub woff2_url: Option<String>,
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentEntry {
    pub id: String,
    pub title: String,
    pub url: String,
    pub imported_at: String,
    pub slide_count: usize,
    pub thumbnail_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProgress {
    pub stage: ImportStage,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportStage {
    Fetching,
    Parsing,
    Downloading,
    Complete,
    Failed,
}
