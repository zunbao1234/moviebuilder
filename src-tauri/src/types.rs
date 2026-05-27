use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Default)]
pub struct AppState {
    pub cancelled: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
}

impl AppState {
    pub fn reset_run_state(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BasicVideoInfo {
    pub duration: f64,
    pub resolution: String,
    pub fps: f64,
    pub codec: String,
    pub file_size: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionSettings {
    pub black_border_yellow_threshold: f64,
    pub black_border_red_threshold: f64,
    pub black_border_irregular_threshold: f64,
    pub ai_logo_score_threshold: f64,
    pub ai_logo_min_hits: usize,
    pub ai_logo_corner_margin_ratio: f64,
    pub ai_logo_corner_width_ratio: f64,
    pub ai_logo_corner_height_ratio: f64,
    pub subtitle_match_enabled: bool,
    pub novel_text: Option<String>,
    pub subtitle_exact_match_include_punctuation: bool,
}

impl Default for DetectionSettings {
    fn default() -> Self {
        Self {
            black_border_yellow_threshold: 0.03,
            black_border_red_threshold: 0.10,
            black_border_irregular_threshold: 0.03,
            ai_logo_score_threshold: 0.12,
            ai_logo_min_hits: 2,
            ai_logo_corner_margin_ratio: 0.025,
            ai_logo_corner_width_ratio: 0.25,
            ai_logo_corner_height_ratio: 0.25,
            subtitle_match_enabled: false,
            novel_text: None,
            subtitle_exact_match_include_punctuation: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub id: String,
    pub r#type: String,
    pub level: RiskLevel,
    pub start_time: f64,
    pub end_time: f64,
    pub description: String,
    pub screenshot: Option<String>,
    pub start_screenshot: Option<String>,
    pub end_screenshot: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionResult {
    pub red_count: usize,
    pub yellow_count: usize,
    pub green_count: usize,
    pub problems: Vec<Problem>,
    pub basic_info: BasicVideoInfo,
    pub report_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Red,
    Yellow,
    Green,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetectionMode {
    Fast,
    Balanced,
    Accurate,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionProgressPayload {
    pub file_path: String,
    pub progress: u8,
    pub stage: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionCompletePayload {
    pub file_path: String,
    pub result: DetectionResult,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionErrorPayload {
    pub file_path: String,
    pub message: String,
}
