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
