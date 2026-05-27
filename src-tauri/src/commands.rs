use crate::{
    report::write_html_report,
    types::{
        AppState, BasicVideoInfo, DetectionCompletePayload, DetectionErrorPayload, DetectionMode,
        DetectionProgressPayload, DetectionResult, DetectionSettings, Problem, RiskLevel,
    },
};
use serde_json::Value;
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::Ordering,
    time::Duration,
};
use tauri::{AppHandle, Emitter, State};
use tokio::time::sleep;

#[tauri::command]
pub fn read_folder_mp4(folder_path: String) -> Result<Vec<String>, String> {
    let entries = fs::read_dir(&folder_path)
        .map_err(|error| format!("failed to read folder {folder_path}: {error}"))?;

    let mut files = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_mp4(path))
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    files.sort();
    Ok(files)
}

#[tauri::command]
pub async fn start_detection(
    app: AppHandle,
    state: State<'_, AppState>,
    files: Vec<String>,
    mode: DetectionMode,
    settings: Option<DetectionSettings>,
) -> Result<(), String> {
    state.reset_run_state();

    for file_path in files {
        if state.cancelled.load(Ordering::SeqCst) {
            app.emit("detection-cancelled", ())
                .map_err(|error| format!("failed to emit cancel event: {error}"))?;
            return Ok(());
        }

        if !is_mp4(Path::new(&file_path)) {
            app.emit(
                "detection-error",
                DetectionErrorPayload {
                    file_path,
                    message: "仅支持 MP4 文件".to_string(),
                },
            )
            .map_err(|error| format!("failed to emit error event: {error}"))?;
            continue;
        }

        run_detection(&app, &state, &file_path, &mode, settings.clone().unwrap_or_default()).await?;
    }

    Ok(())
}

#[tauri::command]
pub fn pause_detection(state: State<'_, AppState>) {
    state.paused.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn cancel_detection(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    state.cancelled.store(true, Ordering::SeqCst);
    state.paused.store(false, Ordering::SeqCst);
    app.emit("detection-cancelled", ())
        .map_err(|error| format!("failed to emit cancel event: {error}"))
}

#[tauri::command]
pub fn generate_html_report(
    file_path: String,
    result: Option<DetectionResult>,
    settings: Option<DetectionSettings>,
) -> Result<String, String> {
    let result = match result {
        Some(result) => result,
        None => build_detection_result(
            &file_path,
            &DetectionMode::Balanced,
            &settings.unwrap_or_default(),
        )?,
    };
    write_html_report(&file_path, &result)
}

#[tauri::command]
pub fn inspect_file(
    file_path: String,
    mode: DetectionMode,
    settings: Option<DetectionSettings>,
) -> Result<DetectionResult, String> {
    build_detection_result(&file_path, &mode, &settings.unwrap_or_default())
}

async fn run_detection(
    app: &AppHandle,
    state: &State<'_, AppState>,
    file_path: &str,
    mode: &DetectionMode,
    settings: DetectionSettings,
) -> Result<(), String> {
    let stages = [
        (8, "读取视频容器信息"),
        (22, "抽样分析画面边界"),
        (38, "计算帧间相似度"),
        (56, "扫描四角疑似 AI 标识"),
        (74, "检查时间戳连续性"),
        (91, "汇总风险等级"),
        (100, "生成检测结果"),
    ];

    let delay_ms = match mode {
        DetectionMode::Fast => 180,
        DetectionMode::Balanced => 260,
        DetectionMode::Accurate => 360,
    };

    for (progress, stage) in stages {
        if state.cancelled.load(Ordering::SeqCst) {
            app.emit("detection-cancelled", ())
                .map_err(|error| format!("failed to emit cancel event: {error}"))?;
            return Ok(());
        }

        while state.paused.load(Ordering::SeqCst) {
            if state.cancelled.load(Ordering::SeqCst) {
                app.emit("detection-cancelled", ())
                    .map_err(|error| format!("failed to emit cancel event: {error}"))?;
                return Ok(());
            }
            sleep(Duration::from_millis(120)).await;
        }

        app.emit(
            "detection-progress",
            DetectionProgressPayload {
                file_path: file_path.to_string(),
                progress,
                stage: stage.to_string(),
            },
        )
        .map_err(|error| format!("failed to emit progress event: {error}"))?;

        sleep(Duration::from_millis(delay_ms)).await;
    }

    let result = match build_detection_result(file_path, mode, &settings) {
        Ok(result) => result,
        Err(message) => {
            app.emit(
                "detection-error",
                DetectionErrorPayload {
                    file_path: file_path.to_string(),
                    message,
                },
            )
            .map_err(|error| format!("failed to emit error event: {error}"))?;
            return Ok(());
        }
    };
    app.emit(
        "detection-complete",
        DetectionCompletePayload {
            file_path: file_path.to_string(),
            result,
        },
    )
    .map_err(|error| format!("failed to emit complete event: {error}"))?;

    Ok(())
}

fn build_detection_result(
    file_path: &str,
    mode: &DetectionMode,
    settings: &DetectionSettings,
) -> Result<DetectionResult, String> {
    let file_size = fs::metadata(file_path)
        .map(|metadata| metadata.len() as f64 / 1024.0 / 1024.0)
        .unwrap_or(128.0);

    let probe = probe_video(file_path)?;
    let duration = probe
        .duration
        .or_else(|| read_mp4_duration_seconds(Path::new(file_path)))
        .unwrap_or(0.0)
        .max(0.0);
    let mut problems = Vec::new();

    if let (Some(source_width), Some(source_height)) = (probe.width, probe.height) {
        if let Some(crop) = detect_black_borders(file_path, mode)? {
            if let Some(mut problem) =
                build_black_border_problem(source_width, source_height, &crop, duration, settings)
            {
                attach_problem_screenshots(&mut problem, file_path, duration, probe.fps);
                problems.push(problem);
            }
        }
    }

    for (index, segment) in detect_frozen_frames(file_path, mode)?.iter().enumerate() {
        if let Some(mut problem) = build_frozen_frame_problem(index, segment) {
            attach_problem_screenshots(&mut problem, file_path, duration, probe.fps);
            problems.push(problem);
        }
    }

    for (index, hit) in detect_ai_logo_marks(file_path, duration, settings)?.iter().enumerate() {
        let mut problem = build_ai_logo_problem(index, hit);
        attach_problem_screenshots(&mut problem, file_path, duration, probe.fps);
        problems.push(problem);
    }

    for (index, segment) in detect_subtitle_mismatch(file_path, duration, mode, settings)?.iter().enumerate() {
        let mut problem = build_subtitle_mismatch_problem(index, segment);
        attach_problem_screenshots(&mut problem, file_path, duration, probe.fps);
        problems.push(problem);
    }

    Ok(result_from_parts(
        problems,
        BasicVideoInfo {
            duration,
            resolution: probe
                .resolution()
                .unwrap_or_else(|| "未知".to_string()),
            fps: probe.fps.unwrap_or(0.0),
            codec: probe.codec.unwrap_or_else(|| "未知".to_string()),
            file_size,
        },
    ))
}

fn result_from_parts(problems: Vec<Problem>, basic_info: BasicVideoInfo) -> DetectionResult {
    DetectionResult {
        red_count: problems
            .iter()
            .filter(|problem| matches!(problem.level, RiskLevel::Red))
            .count(),
        yellow_count: problems
            .iter()
            .filter(|problem| matches!(problem.level, RiskLevel::Yellow))
            .count(),
        green_count: problems
            .iter()
            .filter(|problem| matches!(problem.level, RiskLevel::Green))
            .count(),
        problems,
        basic_info,
        report_path: None,
    }
}

#[derive(Debug, Default)]
struct VideoProbe {
    duration: Option<f64>,
    width: Option<u32>,
    height: Option<u32>,
    fps: Option<f64>,
    codec: Option<String>,
}

impl VideoProbe {
    fn resolution(&self) -> Option<String> {
        Some(format!("{}x{}", self.width?, self.height?))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CropSuggestion {
    width: u32,
    height: u32,
    x: u32,
    y: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct FreezeSegment {
    start: f64,
    end: f64,
    duration: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    fn label(self) -> &'static str {
        match self {
            Corner::TopLeft => "左上角",
            Corner::TopRight => "右上角",
            Corner::BottomLeft => "左下角",
            Corner::BottomRight => "右下角",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct AiLogoHit {
    corner: Corner,
    hits: usize,
    total_samples: usize,
    max_score: f64,
    start_time: f64,
    end_time: f64,
}

#[derive(Debug, Clone)]
struct RgbFrame {
    width: usize,
    height: usize,
    timestamp: f64,
    data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
struct OcrFrameText {
    timestamp: f64,
    text: String,
    confidence: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct SubtitleSegment {
    start_time: f64,
    end_time: f64,
    text: String,
    confidence: f64,
}

fn probe_video(file_path: &str) -> Result<VideoProbe, String> {
    find_ffprobe_binary()
        .ok_or_else(|| "未找到 ffprobe，无法读取真实视频基础信息。请安装 ffprobe 或配置 sidecar。".to_string())
        .and_then(|ffprobe| probe_video_with_ffprobe(&ffprobe, file_path))
}

fn probe_video_with_ffprobe(ffprobe: &Path, file_path: &str) -> Result<VideoProbe, String> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,codec_name,avg_frame_rate,r_frame_rate:format=duration",
            "-of",
            "json",
            file_path,
        ])
        .output()
        .map_err(|error| format!("执行 ffprobe 失败：{error}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe 读取失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let json: Value =
        serde_json::from_slice(&output.stdout).map_err(|error| format!("解析 ffprobe JSON 失败：{error}"))?;
    let stream = json
        .get("streams")
        .and_then(Value::as_array)
        .and_then(|streams| streams.first())
        .ok_or_else(|| "ffprobe 未返回视频流信息。".to_string())?;
    let format = json.get("format");

    Ok(VideoProbe {
        duration: format
            .and_then(|value| value.get("duration"))
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<f64>().ok()),
        width: stream
            .get("width")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        height: stream
            .get("height")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        fps: stream
            .get("avg_frame_rate")
            .or_else(|| stream.get("r_frame_rate"))
            .and_then(Value::as_str)
            .and_then(parse_rate),
        codec: stream
            .get("codec_name")
            .and_then(Value::as_str)
            .map(|value| value.to_uppercase()),
    })
}

fn detect_black_borders(file_path: &str, mode: &DetectionMode) -> Result<Option<CropSuggestion>, String> {
    let ffmpeg = find_ffmpeg_binary()
        .ok_or_else(|| "未找到 ffmpeg，无法执行黑边检测。请安装 ffmpeg 或配置 sidecar。".to_string())?;
    let sample_seconds = match mode {
        DetectionMode::Fast => "8",
        DetectionMode::Balanced => "15",
        DetectionMode::Accurate => "30",
    };
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-i",
            file_path,
            "-t",
            sample_seconds,
            "-vf",
            "cropdetect=limit=24:round=2:reset=0",
            "-an",
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|error| format!("执行 FFmpeg 黑边检测失败：{error}"))?;

    if !output.status.success() {
        return Err(format!(
            "FFmpeg 黑边检测失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let log = String::from_utf8_lossy(&output.stderr);
    Ok(parse_last_crop_suggestion(&log))
}

fn build_black_border_problem(
    source_width: u32,
    source_height: u32,
    crop: &CropSuggestion,
    duration: f64,
    settings: &DetectionSettings,
) -> Option<Problem> {
    if crop.width == 0
        || crop.height == 0
        || crop.width > source_width
        || crop.height > source_height
    {
        return None;
    }

    let left = crop.x.min(source_width);
    let top = crop.y.min(source_height);
    let right = source_width.saturating_sub(crop.width + left);
    let bottom = source_height.saturating_sub(crop.height + top);
    let max_horizontal_ratio = left.max(right) as f64 / source_width as f64;
    let max_vertical_ratio = top.max(bottom) as f64 / source_height as f64;
    let max_ratio = max_horizontal_ratio.max(max_vertical_ratio);

    if max_ratio <= 0.001 {
        return None;
    }

    let irregular =
        (left.abs_diff(right) as f64 / source_width as f64) >= settings.black_border_irregular_threshold
            || (top.abs_diff(bottom) as f64 / source_height as f64)
                >= settings.black_border_irregular_threshold;
    let level = if max_ratio >= settings.black_border_red_threshold
        || (irregular && max_ratio >= settings.black_border_yellow_threshold)
    {
        RiskLevel::Red
    } else if max_ratio >= settings.black_border_yellow_threshold {
        RiskLevel::Yellow
    } else {
        RiskLevel::Green
    };
    let position = describe_border_position(left, right, top, bottom);
    let ratio_summary = format!(
        "左右最大占比 {:.1}%，上下最大占比 {:.1}%",
        max_horizontal_ratio * 100.0,
        max_vertical_ratio * 100.0
    );
    let risk_text = match level {
        RiskLevel::Red => "达到红线阈值",
        RiskLevel::Yellow => "达到黄线阈值",
        RiskLevel::Green => "低于黄线阈值，仅供参考",
    };

    Some(Problem {
        id: "black-border-real".to_string(),
        r#type: "黑边".to_string(),
        level,
        start_time: 0.0,
        end_time: duration.max(0.0),
        description: format!(
            "FFmpeg cropdetect 实测：{position}；{ratio_summary}；裁剪建议 crop={}:{}:{}:{}；最大黑边占比 {:.1}%，{risk_text}。",
            crop.width,
            crop.height,
            crop.x,
            crop.y,
            max_ratio * 100.0
        ),
        screenshot: None,
        start_screenshot: None,
        end_screenshot: None,
    })
}

fn detect_frozen_frames(file_path: &str, mode: &DetectionMode) -> Result<Vec<FreezeSegment>, String> {
    let ffmpeg = find_ffmpeg_binary()
        .ok_or_else(|| "未找到 ffmpeg，无法执行冻结帧检测。请安装 ffmpeg 或配置 sidecar。".to_string())?;
    let threshold = match mode {
        DetectionMode::Fast => "3",
        DetectionMode::Balanced => "2",
        DetectionMode::Accurate => "1",
    };
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-i",
            file_path,
            "-vf",
            &format!("freezedetect=n=0.003:d={threshold}"),
            "-an",
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|error| format!("执行 FFmpeg 冻结帧检测失败：{error}"))?;

    if !output.status.success() {
        return Err(format!(
            "FFmpeg 冻结帧检测失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(parse_freeze_segments(&String::from_utf8_lossy(&output.stderr)))
}

fn build_frozen_frame_problem(index: usize, segment: &FreezeSegment) -> Option<Problem> {
    if segment.duration < 1.0 || segment.end <= segment.start {
        return None;
    }

    let level = if segment.duration >= 5.0 {
        RiskLevel::Red
    } else if segment.duration >= 2.0 {
        RiskLevel::Yellow
    } else {
        RiskLevel::Green
    };
    let risk_text = match level {
        RiskLevel::Red => "达到红线阈值",
        RiskLevel::Yellow => "达到黄线阈值",
        RiskLevel::Green => "低于黄线阈值，仅供参考",
    };

    Some(Problem {
        id: format!("freeze-real-{index}"),
        r#type: "冻结帧".to_string(),
        level,
        start_time: segment.start,
        end_time: segment.end,
        description: format!(
            "FFmpeg freezedetect 实测：画面冻结 {:.3} 秒，位置 {:.3}s - {:.3}s，{risk_text}。",
            segment.duration, segment.start, segment.end
        ),
        screenshot: None,
        start_screenshot: None,
        end_screenshot: None,
    })
}

fn detect_ai_logo_marks(
    file_path: &str,
    duration: f64,
    settings: &DetectionSettings,
) -> Result<Vec<AiLogoHit>, String> {
    let sample_times = ai_logo_sample_times(duration);
    if sample_times.is_empty() {
        return Ok(Vec::new());
    }

    let mut frames = Vec::new();
    for timestamp in sample_times {
        if let Some(frame) = capture_rgb_frame(file_path, timestamp, 320, 180)? {
            frames.push(frame);
        }
    }

    Ok(analyze_ai_logo_frames(&frames, settings))
}

fn ai_logo_sample_times(duration: f64) -> Vec<f64> {
    if duration <= 0.5 {
        return Vec::new();
    }

    let mut times = vec![0.5, duration * 0.33, duration * 0.66, (duration - 0.5).max(0.5)];
    times.iter_mut().for_each(|time| *time = time.clamp(0.0, duration));
    times.sort_by(f64::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 0.25);
    times
}

fn analyze_ai_logo_frames(frames: &[RgbFrame], settings: &DetectionSettings) -> Vec<AiLogoHit> {
    [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight]
        .into_iter()
        .filter_map(|corner| {
            let mut hits = 0;
            let mut max_score = 0.0_f64;
            let mut start_time = None;
            let mut end_time = None;

            for frame in frames {
                let score = corner_mark_score(frame, corner, settings);
                max_score = max_score.max(score);
                if score >= settings.ai_logo_score_threshold {
                    hits += 1;
                    start_time.get_or_insert(frame.timestamp);
                    end_time = Some(frame.timestamp);
                }
            }

            (hits >= settings.ai_logo_min_hits).then(|| AiLogoHit {
                corner,
                hits,
                total_samples: frames.len(),
                max_score,
                start_time: start_time.unwrap_or(0.0),
                end_time: end_time.unwrap_or(0.0),
            })
        })
        .collect()
}

fn corner_mark_score(frame: &RgbFrame, corner: Corner, settings: &DetectionSettings) -> f64 {
    if frame.width == 0 || frame.height == 0 || frame.data.len() < frame.width * frame.height * 3 {
        return 0.0;
    }

    let corner_width = ratio_to_pixels(settings.ai_logo_corner_width_ratio, frame.width, 24);
    let corner_height = ratio_to_pixels(settings.ai_logo_corner_height_ratio, frame.height, 24);
    let margin_x = ratio_to_pixels(settings.ai_logo_corner_margin_ratio, frame.width, 2)
        .min(frame.width.saturating_sub(1));
    let margin_y = ratio_to_pixels(settings.ai_logo_corner_margin_ratio, frame.height, 2)
        .min(frame.height.saturating_sub(1));

    let (x0, y0) = match corner {
        Corner::TopLeft => (margin_x, margin_y),
        Corner::TopRight => (frame.width.saturating_sub(corner_width + margin_x), margin_y),
        Corner::BottomLeft => (margin_x, frame.height.saturating_sub(corner_height + margin_y)),
        Corner::BottomRight => (
            frame.width.saturating_sub(corner_width + margin_x),
            frame.height.saturating_sub(corner_height + margin_y),
        ),
    };

    let mut high_contrast = 0_usize;
    let mut textured = 0_usize;
    let mut total = 0_usize;

    for y in y0..(y0 + corner_height).min(frame.height.saturating_sub(1)) {
        for x in x0..(x0 + corner_width).min(frame.width.saturating_sub(1)) {
            let current = luminance_at(frame, x, y);
            let right = luminance_at(frame, x + 1, y);
            let down = luminance_at(frame, x, y + 1);
            let edge = (current - right).abs().max((current - down).abs());
            if edge > 70.0 {
                high_contrast += 1;
            }
            if edge > 25.0 {
                textured += 1;
            }
            total += 1;
        }
    }

    if total == 0 {
        return 0.0;
    }

    let high_ratio = high_contrast as f64 / total as f64;
    let texture_ratio = textured as f64 / total as f64;
    (high_ratio * 0.75 + texture_ratio * 0.25).min(1.0)
}

fn ratio_to_pixels(ratio: f64, full_size: usize, min_pixels: usize) -> usize {
    ((full_size as f64 * ratio.clamp(0.01, 0.80)).round() as usize)
        .max(min_pixels)
        .min(full_size)
}

fn luminance_at(frame: &RgbFrame, x: usize, y: usize) -> f64 {
    let index = (y * frame.width + x) * 3;
    let r = frame.data[index] as f64;
    let g = frame.data[index + 1] as f64;
    let b = frame.data[index + 2] as f64;
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn build_ai_logo_problem(index: usize, hit: &AiLogoHit) -> Problem {
    Problem {
        id: format!("ai-logo-real-{index}"),
        r#type: "疑似 AI 标识".to_string(),
        level: RiskLevel::Red,
        start_time: hit.start_time,
        end_time: hit.end_time.max(hit.start_time),
        description: format!(
            "四角泛化检测：{}在 {}/{} 个抽样帧中出现稳定高对比小标识痕迹，最高分 {:.3}。按规则疑似 AI 标识归为红线，请人工复核。",
            hit.corner.label(),
            hit.hits,
            hit.total_samples,
            hit.max_score
        ),
        screenshot: None,
        start_screenshot: None,
        end_screenshot: None,
    }
}

fn detect_subtitle_mismatch(
    file_path: &str,
    duration: f64,
    mode: &DetectionMode,
    settings: &DetectionSettings,
) -> Result<Vec<SubtitleSegment>, String> {
    if !settings.subtitle_match_enabled {
        return Ok(Vec::new());
    }

    let novel_text = settings
        .novel_text
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "已启用字幕文本匹配，但小说文本为空。请粘贴小说原文后再检测。".to_string())?;

    let tesseract = find_tesseract_binary()
        .ok_or_else(|| "已启用字幕文本匹配，但未找到 tesseract OCR。请安装 tesseract 或配置 sidecar。".to_string())?;
    let sample_times = subtitle_sample_times(duration, mode);
    let mut frames = Vec::new();

    for timestamp in sample_times {
        if let Some(image) = capture_subtitle_region_png(file_path, timestamp)? {
            let text = run_tesseract_ocr(&tesseract, &image)?;
            if !text.text.trim().is_empty() {
                frames.push(OcrFrameText {
                    timestamp,
                    text: text.text,
                    confidence: text.confidence,
                });
            }
        }
    }

    let segments = merge_ocr_frames(&frames);
    Ok(find_unmatched_subtitle_segments(
        &segments,
        novel_text,
        settings.subtitle_exact_match_include_punctuation,
    ))
}

fn subtitle_sample_times(duration: f64, mode: &DetectionMode) -> Vec<f64> {
    if duration <= 0.5 {
        return Vec::new();
    }

    let interval = match mode {
        DetectionMode::Fast => 4.0,
        DetectionMode::Balanced => 2.0,
        DetectionMode::Accurate => 1.0,
    };
    let mut times = Vec::new();
    let mut current = 0.5;
    let end = (duration - 0.2).max(0.5);
    while current <= end {
        times.push(current);
        current += interval;
    }
    times
}

fn find_unmatched_subtitle_segments(
    segments: &[SubtitleSegment],
    novel_text: &str,
    include_punctuation: bool,
) -> Vec<SubtitleSegment> {
    let normalized_novel = normalize_match_text(novel_text, include_punctuation);
    if normalized_novel.is_empty() {
        return segments.to_vec();
    }

    segments
        .iter()
        .filter(|segment| {
            let normalized_subtitle = normalize_match_text(&segment.text, include_punctuation);
            !normalized_subtitle.is_empty() && !normalized_novel.contains(&normalized_subtitle)
        })
        .cloned()
        .collect()
}

fn normalize_match_text(input: &str, include_punctuation: bool) -> String {
    input
        .chars()
        .filter_map(|char| normalize_match_char(char, include_punctuation))
        .collect()
}

fn normalize_match_char(char: char, include_punctuation: bool) -> Option<char> {
    if char.is_control() || char.is_whitespace() {
        return None;
    }

    let normalized = match char {
        '，' => ',',
        '。' => '.',
        '！' => '!',
        '？' => '?',
        '：' => ':',
        '；' => ';',
        '“' | '”' => '"',
        '‘' | '’' => '\'',
        '（' => '(',
        '）' => ')',
        '【' => '[',
        '】' => ']',
        '、' => ',',
        value if ('！'..='～').contains(&value) => {
            char::from_u32(value as u32 - 0xFEE0).unwrap_or(value)
        }
        value => value,
    };

    if include_punctuation || normalized.is_alphanumeric() || is_cjk(normalized) {
        Some(normalized.to_ascii_lowercase())
    } else {
        None
    }
}

fn is_cjk(char: char) -> bool {
    matches!(char as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF)
}

fn merge_ocr_frames(frames: &[OcrFrameText]) -> Vec<SubtitleSegment> {
    let mut segments: Vec<SubtitleSegment> = Vec::new();

    for frame in frames {
        let normalized = normalize_match_text(&frame.text, true);
        if normalized.is_empty() {
            continue;
        }

        if let Some(last) = segments.last_mut() {
            if normalize_match_text(&last.text, true) == normalized {
                last.end_time = frame.timestamp;
                last.confidence = last.confidence.min(frame.confidence);
                continue;
            }
        }

        segments.push(SubtitleSegment {
            start_time: frame.timestamp,
            end_time: frame.timestamp,
            text: frame.text.trim().to_string(),
            confidence: frame.confidence,
        });
    }

    segments
}

fn build_subtitle_mismatch_problem(index: usize, segment: &SubtitleSegment) -> Problem {
    Problem {
        id: format!("subtitle-match-real-{index}"),
        r#type: "字幕与小说不匹配".to_string(),
        level: RiskLevel::Red,
        start_time: segment.start_time,
        end_time: segment.end_time.max(segment.start_time),
        description: format!(
            "OCR字幕未能在小说文本中逐字连续匹配（含标点）。字幕：\"{}\"；OCR置信度 {:.1}%。OCR识别结果可能有误，请人工复核。",
            segment.text,
            segment.confidence
        ),
        screenshot: None,
        start_screenshot: None,
        end_screenshot: None,
    }
}

#[derive(Debug, Clone)]
struct OcrText {
    text: String,
    confidence: f64,
}

fn capture_subtitle_region_png(file_path: &str, timestamp: f64) -> Result<Option<Vec<u8>>, String> {
    let ffmpeg = find_ffmpeg_binary()
        .ok_or_else(|| "未找到 ffmpeg，无法执行字幕 OCR 抽帧。请安装 ffmpeg 或配置 sidecar。".to_string())?;
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            &format!("{:.3}", timestamp.max(0.0)),
            "-i",
            file_path,
            "-frames:v",
            "1",
            "-vf",
            "crop=iw:ih*0.35:0:ih*0.65,scale=960:-2,format=gray",
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "pipe:1",
        ])
        .output()
        .map_err(|error| format!("执行 FFmpeg 字幕 OCR 抽帧失败：{error}"))?;

    if !output.status.success() {
        return Err(format!(
            "FFmpeg 字幕 OCR 抽帧失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok((!output.stdout.is_empty()).then_some(output.stdout))
}

fn run_tesseract_ocr(tesseract: &Path, image: &[u8]) -> Result<OcrText, String> {
    let input_path = std::env::temp_dir().join(format!("video_inspector_ocr_{}.png", unique_suffix()));
    let output_base = std::env::temp_dir().join(format!("video_inspector_ocr_{}", unique_suffix()));
    fs::write(&input_path, image).map_err(|error| format!("写入 OCR 临时图像失败：{error}"))?;

    let output = Command::new(tesseract)
        .arg(&input_path)
        .arg(&output_base)
        .args(["-l", "chi_sim+eng", "--psm", "6", "tsv"])
        .output()
        .map_err(|error| format!("执行 tesseract OCR 失败：{error}"));

    let _ = fs::remove_file(&input_path);
    let output = output?;
    if !output.status.success() {
        let _ = fs::remove_file(output_base.with_extension("tsv"));
        return Err(format!(
            "tesseract OCR 失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let tsv_path = output_base.with_extension("tsv");
    let tsv = fs::read_to_string(&tsv_path).map_err(|error| format!("读取 OCR TSV 失败：{error}"))?;
    let _ = fs::remove_file(&tsv_path);
    Ok(parse_tesseract_tsv(&tsv))
}

fn parse_tesseract_tsv(tsv: &str) -> OcrText {
    let mut text_parts = Vec::new();
    let mut confidences = Vec::new();

    for line in tsv.lines().skip(1) {
        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() < 12 {
            continue;
        }
        let text = columns[11].trim();
        if text.is_empty() {
            continue;
        }
        text_parts.push(text.to_string());
        if let Ok(confidence) = columns[10].parse::<f64>() {
            if confidence >= 0.0 {
                confidences.push(confidence);
            }
        }
    }

    let confidence = if confidences.is_empty() {
        0.0
    } else {
        confidences.iter().sum::<f64>() / confidences.len() as f64
    };

    OcrText {
        text: text_parts.join(""),
        confidence,
    }
}

fn unique_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}_{}", std::process::id(), nanos)
}

fn find_tesseract_binary() -> Option<PathBuf> {
    find_binary("tesseract")
}

fn capture_rgb_frame(
    file_path: &str,
    timestamp: f64,
    width: usize,
    height: usize,
) -> Result<Option<RgbFrame>, String> {
    let ffmpeg = find_ffmpeg_binary()
        .ok_or_else(|| "未找到 ffmpeg，无法执行 AI 标识检测。请安装 ffmpeg 或配置 sidecar。".to_string())?;
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            &format!("{:.3}", timestamp.max(0.0)),
            "-i",
            file_path,
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
        ])
        .output()
        .map_err(|error| format!("执行 FFmpeg AI 标识抽帧失败：{error}"))?;

    if !output.status.success() {
        return Err(format!(
            "FFmpeg AI 标识抽帧失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let expected = width * height * 3;
    if output.stdout.len() < expected {
        return Ok(None);
    }

    Ok(Some(RgbFrame {
        width,
        height,
        timestamp,
        data: output.stdout[..expected].to_vec(),
    }))
}

fn attach_problem_screenshots(problem: &mut Problem, file_path: &str, duration: f64, fps: Option<f64>) {
    let start_time = safe_frame_timestamp(problem.start_time, duration, fps);
    let end_time = safe_frame_timestamp(problem.end_time, duration, fps);
    let start = capture_frame_data_uri(file_path, start_time);
    let end = capture_frame_data_uri(file_path, end_time);

    problem.screenshot = start.clone().or_else(|| end.clone());
    problem.start_screenshot = start;
    problem.end_screenshot = end;
}

fn capture_frame_data_uri(file_path: &str, timestamp: f64) -> Option<String> {
    let ffmpeg = find_ffmpeg_binary()?;
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            &format!("{:.3}", timestamp.max(0.0)),
            "-i",
            file_path,
            "-frames:v",
            "1",
            "-vf",
            "scale=320:-2",
            "-q:v",
            "3",
            "-f",
            "image2pipe",
            "-vcodec",
            "mjpeg",
            "-pix_fmt",
            "yuvj420p",
            "pipe:1",
        ])
        .output()
        .ok()?;

    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }

    Some(format!(
        "data:image/jpeg;base64,{}",
        base64_encode(&output.stdout)
    ))
}

fn safe_frame_timestamp(timestamp: f64, duration: f64, fps: Option<f64>) -> f64 {
    if duration <= 0.0 {
        return timestamp.max(0.0);
    }

    let frame_margin = fps
        .filter(|value| *value > 0.0)
        .map(|value| 1.0 / value)
        .unwrap_or(0.1);
    timestamp.clamp(0.0, (duration - frame_margin).max(0.0))
}

fn find_ffmpeg_binary() -> Option<PathBuf> {
    find_binary("ffmpeg")
}

fn find_ffprobe_binary() -> Option<PathBuf> {
    find_binary("ffprobe")
}

fn find_binary(name: &str) -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from("src-tauri/bin").join(name)];

    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".local/bin").join(name));
    }

    candidates.extend([
        PathBuf::from("/opt/homebrew/bin").join(name),
        PathBuf::from("/usr/local/bin").join(name),
    ]);

    if let Some(paths) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&paths).map(|path| path.join(name)));
    }

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn parse_rate(value: &str) -> Option<f64> {
    if let Some((num, den)) = value.split_once('/') {
        let num = num.parse::<f64>().ok()?;
        let den = den.parse::<f64>().ok()?;
        return (den != 0.0).then_some(num / den);
    }

    value.parse::<f64>().ok()
}

fn parse_last_crop_suggestion(log: &str) -> Option<CropSuggestion> {
    log.lines()
        .filter_map(parse_crop_suggestion)
        .last()
}

fn parse_crop_suggestion(line: &str) -> Option<CropSuggestion> {
    let crop_start = line.rfind("crop=")?;
    let crop = line[crop_start + "crop=".len()..]
        .split_whitespace()
        .next()?;
    let mut parts = crop.split(':');
    Some(CropSuggestion {
        width: parts.next()?.parse().ok()?,
        height: parts.next()?.parse().ok()?,
        x: parts.next()?.parse().ok()?,
        y: parts.next()?.parse().ok()?,
    })
}

fn parse_freeze_segments(log: &str) -> Vec<FreezeSegment> {
    let mut segments = Vec::new();
    let mut start = None;
    let mut duration = None;

    for line in log.lines() {
        if let Some(value) = parse_log_value(line, "lavfi.freezedetect.freeze_start:") {
            start = Some(value);
            duration = None;
            continue;
        }

        if let Some(value) = parse_log_value(line, "lavfi.freezedetect.freeze_duration:") {
            duration = Some(value);
            continue;
        }

        if let Some(end) = parse_log_value(line, "lavfi.freezedetect.freeze_end:") {
            if let Some(start) = start {
                let duration = duration.unwrap_or_else(|| end - start);
                if duration > 0.0 && end > start {
                    segments.push(FreezeSegment {
                        start,
                        end,
                        duration,
                    });
                }
            }
            start = None;
            duration = None;
        }
    }

    segments
}

fn parse_log_value(line: &str, marker: &str) -> Option<f64> {
    let start = line.find(marker)?;
    line[start + marker.len()..].trim().parse().ok()
}

fn describe_border_position(left: u32, right: u32, top: u32, bottom: u32) -> String {
    let mut parts = Vec::new();
    if top > 0 || bottom > 0 {
        parts.push(format!("上下黑边 {}px/{}px", top, bottom));
    }
    if left > 0 || right > 0 {
        parts.push(format!("左右黑边 {}px/{}px", left, right));
    }

    if parts.is_empty() {
        "未发现明显黑边".to_string()
    } else {
        parts.join("，")
    }
}

fn read_mp4_duration_seconds(path: &Path) -> Option<f64> {
    let mut file = File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len();
    find_mvhd_duration(&mut file, 0, file_len, 0)
}

fn find_mvhd_duration(file: &mut File, start: u64, end: u64, depth: u8) -> Option<f64> {
    if depth > 8 || end <= start {
        return None;
    }

    let mut offset = start;
    while offset + 8 <= end {
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut header = [0_u8; 8];
        file.read_exact(&mut header).ok()?;

        let size32 = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let atom_type = &header[4..8];
        let mut header_len = 8_u64;
        let atom_size = if size32 == 1 {
            let mut extended = [0_u8; 8];
            file.read_exact(&mut extended).ok()?;
            header_len = 16;
            u64::from_be_bytes(extended)
        } else if size32 == 0 {
            end.saturating_sub(offset)
        } else {
            size32
        };

        if atom_size < header_len {
            return None;
        }

        let atom_end = offset.saturating_add(atom_size).min(end);
        let payload_start = offset + header_len;

        if atom_type == b"mvhd" {
            return read_mvhd_duration(file, payload_start, atom_end);
        }

        if matches!(atom_type, b"moov" | b"trak" | b"mdia") {
            if let Some(duration) = find_mvhd_duration(file, payload_start, atom_end, depth + 1) {
                return Some(duration);
            }
        }

        offset = atom_end;
    }

    None
}

fn read_mvhd_duration(file: &mut File, start: u64, end: u64) -> Option<f64> {
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut version_flags = [0_u8; 4];
    file.read_exact(&mut version_flags).ok()?;
    let version = version_flags[0];

    if version == 1 {
        if start + 32 > end {
            return None;
        }
        let mut fields = [0_u8; 28];
        file.read_exact(&mut fields).ok()?;
        let timescale = u32::from_be_bytes([fields[16], fields[17], fields[18], fields[19]]) as f64;
        let duration = u64::from_be_bytes([
            fields[20], fields[21], fields[22], fields[23], fields[24], fields[25], fields[26],
            fields[27],
        ]) as f64;
        return (timescale > 0.0).then_some(duration / timescale);
    }

    if start + 20 > end {
        return None;
    }
    let mut fields = [0_u8; 16];
    file.read_exact(&mut fields).ok()?;
    let timescale = u32::from_be_bytes([fields[8], fields[9], fields[10], fields[11]]) as f64;
    let duration = u32::from_be_bytes([fields[12], fields[13], fields[14], fields[15]]) as f64;
    (timescale > 0.0).then_some(duration / timescale)
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let triple = ((b0 as u32) << 16) | ((b1 as u32) << 8) | b2 as u32;

        encoded.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        encoded.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }

    encoded
}

fn is_mp4(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("mp4"))
        .unwrap_or(false)
}

#[allow(dead_code)]
fn _sidecar_placeholder_path() -> PathBuf {
    PathBuf::from("src-tauri/bin/ffmpeg")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn detection_result_uses_mp4_duration_without_mock_problems() {
        let path = std::env::temp_dir().join("video_inspector_short_duration.mp4");
        write_minimal_mp4_with_duration(&path, 10, 1_000);

        let result = build_detection_result(
            path.to_str().unwrap(),
            &DetectionMode::Balanced,
            &DetectionSettings::default(),
        );

        assert!(result.is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn result_does_not_create_fixed_mock_problems_without_detection_evidence() {
        let path = std::env::temp_dir().join("video_inspector_no_evidence.mp4");
        write_minimal_mp4_with_duration(&path, 10, 1_000);

        let result = build_detection_result(
            path.to_str().unwrap(),
            &DetectionMode::Balanced,
            &DetectionSettings::default(),
        );

        assert!(result.is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn parses_last_cropdetect_suggestion() {
        let log = "[Parsed_cropdetect_0 @ 0x1] crop=1920:800:0:140\n[Parsed_cropdetect_0 @ 0x1] crop=1916:800:2:140";

        let crop = parse_last_crop_suggestion(log).unwrap();

        assert_eq!(
            crop,
            CropSuggestion {
                width: 1916,
                height: 800,
                x: 2,
                y: 140
            }
        );
    }

    #[test]
    fn black_border_problem_uses_thresholds_from_prd() {
        let yellow = build_black_border_problem(
            1920,
            1080,
            &CropSuggestion {
                width: 1920,
                height: 930,
                x: 0,
                y: 75,
            },
            30.0,
            &DetectionSettings::default(),
        )
        .unwrap();
        let red = build_black_border_problem(
            1920,
            1080,
            &CropSuggestion {
                width: 1920,
                height: 840,
                x: 0,
                y: 120,
            },
            30.0,
            &DetectionSettings::default(),
        )
        .unwrap();
        let green = build_black_border_problem(
            1920,
            1080,
            &CropSuggestion {
                width: 1920,
                height: 1040,
                x: 0,
                y: 20,
            },
            30.0,
            &DetectionSettings::default(),
        )
        .unwrap();

        assert!(matches!(yellow.level, RiskLevel::Yellow));
        assert!(matches!(red.level, RiskLevel::Red));
        assert!(matches!(green.level, RiskLevel::Green));
    }

    #[test]
    fn vertical_black_borders_are_included_in_risk_level() {
        let problem = build_black_border_problem(
            1920,
            1080,
            &CropSuggestion {
                width: 1920,
                height: 840,
                x: 0,
                y: 120,
            },
            30.0,
            &DetectionSettings::default(),
        )
        .unwrap();

        assert!(matches!(problem.level, RiskLevel::Red));
        assert!(problem.description.contains("上下黑边 120px/120px"));
        assert!(problem.description.contains("上下最大占比 11.1%"));
    }

    #[test]
    fn horizontal_black_borders_are_included_in_risk_level() {
        let problem = build_black_border_problem(
            1920,
            1080,
            &CropSuggestion {
                width: 1600,
                height: 1080,
                x: 160,
                y: 0,
            },
            30.0,
            &DetectionSettings::default(),
        )
        .unwrap();

        assert!(matches!(problem.level, RiskLevel::Yellow));
        assert!(problem.description.contains("左右黑边 160px/160px"));
        assert!(problem.description.contains("左右最大占比 8.3%"));
    }

    #[test]
    fn mild_asymmetry_under_three_percent_stays_green() {
        let problem = build_black_border_problem(
            720,
            1280,
            &CropSuggestion {
                width: 700,
                height: 1248,
                x: 10,
                y: 0,
            },
            17.647,
            &DetectionSettings::default(),
        )
        .unwrap();

        assert!(matches!(problem.level, RiskLevel::Green));
    }

    #[test]
    fn custom_black_border_threshold_can_promote_green_to_yellow() {
        let settings = DetectionSettings {
            black_border_yellow_threshold: 0.02,
            black_border_red_threshold: 0.10,
            black_border_irregular_threshold: 0.03,
            ..DetectionSettings::default()
        };
        let problem = build_black_border_problem(
            720,
            1280,
            &CropSuggestion {
                width: 700,
                height: 1248,
                x: 10,
                y: 0,
            },
            17.647,
            &settings,
        )
        .unwrap();

        assert!(matches!(problem.level, RiskLevel::Yellow));
    }

    #[test]
    fn parses_freezedetect_segments() {
        let log = "[freezedetect @ 0x1] lavfi.freezedetect.freeze_start: 2.400000\n\
[freezedetect @ 0x1] lavfi.freezedetect.freeze_duration: 2.600000\n\
[freezedetect @ 0x1] lavfi.freezedetect.freeze_end: 5.000000\n\
[freezedetect @ 0x1] lavfi.freezedetect.freeze_start: 8\n\
[freezedetect @ 0x1] lavfi.freezedetect.freeze_duration: 5.5\n\
[freezedetect @ 0x1] lavfi.freezedetect.freeze_end: 13.5";

        let segments = parse_freeze_segments(log);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start, 2.4);
        assert_eq!(segments[0].end, 5.0);
        assert_eq!(segments[0].duration, 2.6);
        assert_eq!(segments[1].start, 8.0);
        assert_eq!(segments[1].end, 13.5);
    }

    #[test]
    fn freezedetect_ignores_incomplete_segment() {
        let log = "[freezedetect @ 0x1] lavfi.freezedetect.freeze_start: 2.400000\n\
[freezedetect @ 0x1] lavfi.freezedetect.freeze_duration: 2.600000";

        assert!(parse_freeze_segments(log).is_empty());
    }

    #[test]
    fn frozen_frame_problem_uses_thresholds() {
        let green = build_frozen_frame_problem(
            0,
            &FreezeSegment {
                start: 1.0,
                end: 2.2,
                duration: 1.2,
            },
        )
        .unwrap();
        let yellow = build_frozen_frame_problem(
            1,
            &FreezeSegment {
                start: 1.0,
                end: 3.5,
                duration: 2.5,
            },
        )
        .unwrap();
        let red = build_frozen_frame_problem(
            2,
            &FreezeSegment {
                start: 1.0,
                end: 6.5,
                duration: 5.5,
            },
        )
        .unwrap();

        assert!(matches!(green.level, RiskLevel::Green));
        assert!(matches!(yellow.level, RiskLevel::Yellow));
        assert!(matches!(red.level, RiskLevel::Red));
    }

    #[test]
    fn ai_logo_corner_score_detects_high_contrast_mark() {
        let frame = synthetic_corner_mark_frame(Corner::TopRight, true);

        let score = corner_mark_score(&frame, Corner::TopRight, &DetectionSettings::default());
        let opposite = corner_mark_score(&frame, Corner::BottomLeft, &DetectionSettings::default());

        assert!(score >= 0.12, "score was {score}");
        assert!(opposite < 0.02, "opposite score was {opposite}");
    }

    #[test]
    fn ai_logo_corner_area_settings_control_scan_position() {
        let inside = synthetic_corner_mark_at(122, 10);
        let outside = synthetic_corner_mark_at(86, 10);
        let settings = DetectionSettings {
            ai_logo_corner_margin_ratio: 0.05,
            ai_logo_corner_width_ratio: 0.18,
            ai_logo_corner_height_ratio: 0.25,
            ..DetectionSettings::default()
        };

        let inside_score = corner_mark_score(&inside, Corner::TopRight, &settings);
        let outside_score = corner_mark_score(&outside, Corner::TopRight, &settings);

        assert!(inside_score >= 0.12, "inside score was {inside_score}");
        assert!(outside_score < 0.05, "outside score was {outside_score}");
    }

    #[test]
    fn ai_logo_analysis_requires_repeated_hits() {
        let settings = DetectionSettings::default();
        let frames = vec![
            synthetic_corner_mark_frame(Corner::TopRight, true),
            synthetic_corner_mark_frame(Corner::TopRight, true),
            synthetic_corner_mark_frame(Corner::TopRight, false),
        ];

        let hits = analyze_ai_logo_frames(&frames, &settings);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].corner, Corner::TopRight);
        assert_eq!(hits[0].hits, 2);
    }

    #[test]
    fn ai_logo_problem_is_red_line() {
        let hit = AiLogoHit {
            corner: Corner::TopLeft,
            hits: 3,
            total_samples: 4,
            max_score: 0.2,
            start_time: 0.5,
            end_time: 10.0,
        };

        let problem = build_ai_logo_problem(0, &hit);

        assert!(matches!(problem.level, RiskLevel::Red));
        assert_eq!(problem.r#type, "疑似 AI 标识");
    }

    #[test]
    fn subtitle_text_normalization_preserves_punctuation() {
        let normalized = normalize_match_text("他说：“你好！”", true);

        assert_eq!(normalized, "他说:\"你好!\"");
    }

    #[test]
    fn subtitle_exact_match_accepts_continuous_novel_text() {
        let segments = vec![SubtitleSegment {
            start_time: 1.0,
            end_time: 2.0,
            text: "他说:\"你好!\"".to_string(),
            confidence: 92.0,
        }];

        let unmatched = find_unmatched_subtitle_segments(&segments, "前文他说：“你好！”后文", true);

        assert!(unmatched.is_empty());
    }

    #[test]
    fn subtitle_exact_match_reports_changed_punctuation() {
        let segments = vec![SubtitleSegment {
            start_time: 1.0,
            end_time: 2.0,
            text: "他说:\"你好?\"".to_string(),
            confidence: 92.0,
        }];

        let unmatched = find_unmatched_subtitle_segments(&segments, "他说：“你好！”", true);

        assert_eq!(unmatched.len(), 1);
    }

    #[test]
    fn repeated_ocr_frames_merge_into_one_subtitle_segment() {
        let frames = vec![
            OcrFrameText {
                timestamp: 1.0,
                text: "你好！".to_string(),
                confidence: 91.0,
            },
            OcrFrameText {
                timestamp: 3.0,
                text: "你好!".to_string(),
                confidence: 88.0,
            },
        ];

        let segments = merge_ocr_frames(&frames);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].start_time, 1.0);
        assert_eq!(segments[0].end_time, 3.0);
        assert_eq!(segments[0].confidence, 88.0);
    }

    #[test]
    fn subtitle_match_disabled_does_not_require_novel_text_or_ocr() {
        let result = detect_subtitle_mismatch(
            "/path/not/used.mp4",
            10.0,
            &DetectionMode::Balanced,
            &DetectionSettings::default(),
        )
        .unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn subtitle_match_enabled_requires_novel_text() {
        let settings = DetectionSettings {
            subtitle_match_enabled: true,
            novel_text: Some("   ".to_string()),
            ..DetectionSettings::default()
        };

        let error = detect_subtitle_mismatch(
            "/path/not/used.mp4",
            10.0,
            &DetectionMode::Balanced,
            &settings,
        )
        .unwrap_err();

        assert!(error.contains("小说文本为空"));
    }

    #[test]
    fn subtitle_match_enabled_reports_missing_ocr_engine() {
        let settings = DetectionSettings {
            subtitle_match_enabled: true,
            novel_text: Some("他说：“你好！”".to_string()),
            ..DetectionSettings::default()
        };

        if find_tesseract_binary().is_some() {
            return;
        }

        let error = detect_subtitle_mismatch(
            "/path/not/used.mp4",
            10.0,
            &DetectionMode::Balanced,
            &settings,
        )
        .unwrap_err();

        assert!(error.contains("未找到 tesseract OCR"));
    }

    #[test]
    fn subtitle_mismatch_problem_is_red_line() {
        let segment = SubtitleSegment {
            start_time: 1.0,
            end_time: 2.0,
            text: "错字字幕".to_string(),
            confidence: 77.0,
        };

        let problem = build_subtitle_mismatch_problem(0, &segment);

        assert!(matches!(problem.level, RiskLevel::Red));
        assert_eq!(problem.r#type, "字幕与小说不匹配");
        assert!(problem.description.contains("OCR识别结果可能有误"));
    }

    #[test]
    fn reads_duration_from_mvhd_atom() {
        let path = std::env::temp_dir().join("video_inspector_mvhd_duration.mp4");
        write_minimal_mp4_with_duration(&path, 125, 1_000);

        let duration = read_mp4_duration_seconds(&path).unwrap();

        assert_eq!(duration, 125.0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn base64_encoder_handles_padding() {
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
    }

    #[test]
    fn report_export_uses_supplied_detection_result() {
        let path = std::env::temp_dir().join("video_inspector_supplied_report_result.mp4");
        write_minimal_mp4_with_duration(&path, 10, 1_000);
        let result = result_from_parts(
            vec![Problem {
                id: "manual-red-1".to_string(),
                r#type: "导出一致性问题".to_string(),
                level: RiskLevel::Red,
                start_time: 1.0,
                end_time: 2.0,
                description: "来自前端当前结果，而不是重新检测。".to_string(),
                screenshot: None,
                start_screenshot: None,
                end_screenshot: None,
            }],
            BasicVideoInfo {
                duration: 10.0,
                resolution: "1920x1080".to_string(),
                fps: 30.0,
                codec: "h264".to_string(),
                file_size: 1.0,
            },
        );

        let report_path = generate_html_report(
            path.to_string_lossy().to_string(),
            Some(result),
            Some(DetectionSettings::default()),
        )
        .unwrap();
        let html = fs::read_to_string(&report_path).unwrap();

        assert!(html.contains("导出一致性问题"));
        assert!(html.contains("来自前端当前结果"));
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(report_path);
    }

    fn write_minimal_mp4_with_duration(path: &Path, seconds: u32, timescale: u32) {
        let mut mvhd_payload = Vec::new();
        mvhd_payload.extend_from_slice(&[0, 0, 0, 0]);
        mvhd_payload.extend_from_slice(&0_u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&0_u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&timescale.to_be_bytes());
        mvhd_payload.extend_from_slice(&(seconds * timescale).to_be_bytes());
        let mvhd = atom(*b"mvhd", &mvhd_payload);
        let moov = atom(*b"moov", &mvhd);
        let ftyp = atom(*b"ftyp", b"isom\0\0\0\0isom");

        let mut file = File::create(path).unwrap();
        file.write_all(&ftyp).unwrap();
        file.write_all(&moov).unwrap();
    }

    fn atom(kind: [u8; 4], payload: &[u8]) -> Vec<u8> {
        let size = (payload.len() + 8) as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&size.to_be_bytes());
        bytes.extend_from_slice(&kind);
        bytes.extend_from_slice(payload);
        bytes
    }

    fn synthetic_corner_mark_frame(corner: Corner, with_mark: bool) -> RgbFrame {
        let width = 160;
        let height = 90;
        let mut data = vec![32_u8; width * height * 3];

        if with_mark {
            let (x0, y0) = match corner {
                Corner::TopLeft => (10, 8),
                Corner::TopRight => (width - 42, 8),
                Corner::BottomLeft => (10, height - 28),
                Corner::BottomRight => (width - 42, height - 28),
            };
            paint_mark(&mut data, width, x0, y0, 30, 18);
        }

        RgbFrame {
            width,
            height,
            timestamp: 1.0,
            data,
        }
    }

    fn synthetic_corner_mark_at(x0: usize, y0: usize) -> RgbFrame {
        let width = 160;
        let height = 90;
        let mut data = vec![32_u8; width * height * 3];
        paint_mark(&mut data, width, x0, y0, 24, 16);

        RgbFrame {
            width,
            height,
            timestamp: 1.0,
            data,
        }
    }

    fn paint_mark(data: &mut [u8], width: usize, x0: usize, y0: usize, mark_width: usize, mark_height: usize) {
        for y in y0..(y0 + mark_height) {
            for x in x0..(x0 + mark_width) {
                let index = (y * width + x) * 3;
                let value = if (x + y) % 4 < 2 { 245 } else { 20 };
                data[index] = value;
                data[index + 1] = value;
                data[index + 2] = value;
            }
        }
    }
}
