use crate::types::{DetectionResult, RiskLevel};
use std::{fs, path::PathBuf};

pub fn write_html_report(file_path: &str, result: &DetectionResult) -> Result<String, String> {
    let source = PathBuf::from(file_path);
    let parent = source
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("video-report");
    let report_path = parent.join(format!("{stem}_video_inspector_report.html"));

    let problems = result
        .problems
        .iter()
        .map(|problem| {
            let level = match problem.level {
                RiskLevel::Red => "红线",
                RiskLevel::Yellow => "黄线",
                RiskLevel::Green => "绿线",
            };
            let start_screenshot = shot_html("开始", problem.start_screenshot.as_ref().or(problem.screenshot.as_ref()), &problem.r#type);
            let end_screenshot = shot_html("结束", problem.end_screenshot.as_ref().or(problem.screenshot.as_ref()), &problem.r#type);
            format!(
                r#"<article class="problem {class}">
  <div class="shots">{start_screenshot}{end_screenshot}</div>
  <div class="problem-body">
    <div class="problem-head"><strong>{level}</strong><span>{kind}</span><code>{start:.3}s - {end:.3}s</code></div>
    <p>{description}</p>
  </div>
</article>"#,
                class = level_class(&problem.level),
                start_screenshot = start_screenshot,
                end_screenshot = end_screenshot,
                kind = html_escape(&problem.r#type),
                start = problem.start_time,
                end = problem.end_time,
                description = html_escape(&problem.description)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let html = format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>VideoInspector Pro Report</title>
  <style>
    body {{ margin: 0; background: #0f172a; color: #f1f5f9; font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }}
    main {{ max-width: 960px; margin: 0 auto; padding: 32px; }}
    header {{ border-bottom: 1px solid #334155; padding-bottom: 20px; margin-bottom: 24px; }}
    h1 {{ margin: 0 0 8px; font-size: 28px; }}
    .muted {{ color: #94a3b8; }}
    .summary {{ display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; margin: 20px 0; }}
    .tile {{ border: 1px solid #334155; background: #1e293b; padding: 16px; border-radius: 8px; }}
    .tile b {{ display: block; font-size: 26px; margin-top: 6px; }}
    .problem {{ display: grid; grid-template-columns: 220px 1fr; gap: 16px; border: 1px solid #334155; background: #111827; border-radius: 8px; padding: 14px; margin: 12px 0; }}
    .problem-head {{ display: flex; gap: 12px; align-items: center; }}
    .problem-head code {{ margin-left: auto; color: #cbd5e1; }}
    .shots {{ display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }}
    .shot-wrap span {{ display: block; margin-bottom: 4px; color: #94a3b8; font-size: 12px; }}
    .shot {{ width: 100%; aspect-ratio: 16 / 9; object-fit: cover; border-radius: 6px; border: 1px solid #334155; background: #020617; }}
    .placeholder {{ display: flex; align-items: center; justify-content: center; color: #64748b; font-size: 13px; }}
    .red {{ border-color: rgba(239, 68, 68, .6); }}
    .yellow {{ border-color: rgba(234, 179, 8, .6); }}
    .green {{ border-color: rgba(34, 197, 94, .6); }}
  </style>
</head>
<body>
  <main>
    <header>
      <h1>VideoInspector Pro 检测报告</h1>
      <p class="muted">{file}</p>
    </header>
    <section class="summary">
      <div class="tile">红线问题<b style="color:#fca5a5">{red}</b></div>
      <div class="tile">黄线问题<b style="color:#fde68a">{yellow}</b></div>
      <div class="tile">绿线提示<b style="color:#86efac">{green}</b></div>
    </section>
    <section class="tile">
      <p>时长：{duration:.3}s | 分辨率：{resolution} | 帧率：{fps:.2} fps | 编码：{codec}</p>
    </section>
    <section>
      {problems}
    </section>
  </main>
</body>
</html>"#,
        file = html_escape(file_path),
        red = result.red_count,
        yellow = result.yellow_count,
        green = result.green_count,
        duration = result.basic_info.duration,
        resolution = html_escape(&result.basic_info.resolution),
        fps = result.basic_info.fps,
        codec = html_escape(&result.basic_info.codec),
        problems = problems
    );

    fs::write(&report_path, html)
        .map_err(|error| format!("failed to write report {}: {error}", report_path.display()))?;

    Ok(report_path.to_string_lossy().to_string())
}

fn level_class(level: &RiskLevel) -> &'static str {
    match level {
        RiskLevel::Red => "red",
        RiskLevel::Yellow => "yellow",
        RiskLevel::Green => "green",
    }
}

fn shot_html(label: &str, src: Option<&String>, kind: &str) -> String {
    let media = src
        .map(|value| {
            format!(
                r#"<img class="shot" src="{src}" alt="{kind} {label}截图" />"#,
                src = html_escape(value),
                kind = html_escape(kind),
                label = html_escape(label)
            )
        })
        .unwrap_or_else(|| r#"<div class="shot placeholder">暂无截图</div>"#.to_string());
    format!(r#"<div class="shot-wrap"><span>{}</span>{}</div>"#, html_escape(label), media)
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
