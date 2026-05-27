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
    settings: Option<DetectionSettings>,
) -> Result<String, String> {
    let result = build_detection_result(
        &file_path,
        &DetectionMode::Balanced,
        &settings.unwrap_or_default(),
    )?;
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
        (56, "检查时间戳连续性"),
        (74, "分析音频能量曲线"),
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
            "FFmpeg cropdetect 实测：{position}；裁剪建议 crop={}:{}:{}:{}；最大黑边占比 {:.1}%，{risk_text}。",
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
}
