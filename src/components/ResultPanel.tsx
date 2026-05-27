import { AlertTriangle, CheckCircle2, Clock3, ImageIcon, Info, ShieldAlert } from "lucide-react";
import type { DetectionResult, RiskLevel, VideoFile } from "../types";
import { formatTime, levelClasses, levelLabel, problemsByLevel } from "../utils";

interface ResultPanelProps {
  file: VideoFile | null;
}

const levelMeta: Record<RiskLevel, { icon: React.ReactNode; title: string; empty: string }> = {
  red: {
    icon: <ShieldAlert size={18} />,
    title: "红线问题",
    empty: "没有红线问题",
  },
  yellow: {
    icon: <AlertTriangle size={18} />,
    title: "黄线问题",
    empty: "没有黄线问题",
  },
  green: {
    icon: <CheckCircle2 size={18} />,
    title: "绿线提示",
    empty: "没有绿线提示",
  },
};

export default function ResultPanel({ file }: ResultPanelProps) {
  const result = file?.result ?? null;

  return (
    <section className="flex min-h-[420px] flex-[1.25] flex-col overflow-hidden rounded border border-slate-700 bg-slate-900 shadow-inspector">
      <div className="flex h-12 items-center justify-between border-b border-slate-700 px-4">
        <div>
          <h2 className="text-sm font-semibold text-slate-100">检测结果详情</h2>
          <p className="text-xs text-slate-400">{file ? file.name : "选择一个文件查看详情"}</p>
        </div>
        {result && <Summary result={result} />}
      </div>

      {!file ? (
        <EmptyState title="未选择文件" description="点击上方文件列表中的任意一行查看检测结果。" />
      ) : file.status === "pending" ? (
        <EmptyState title="等待检测" description="点击开始检测后，这里会显示红黄绿风险分组。" />
      ) : file.status === "detecting" || file.status === "paused" ? (
        <EmptyState title={file.status === "paused" ? "检测已暂停" : "检测进行中"} description="后端正在读取真实视频信息并执行画面检测。" />
      ) : file.status === "error" ? (
        <EmptyState title="检测失败" description={file.error || "后端返回了错误事件。"} tone="error" />
      ) : result ? (
        <div className="grid flex-1 grid-cols-[280px_1fr] overflow-hidden">
          <InfoPanel result={result} />
          <div className="overflow-auto p-4">
            <div className="grid gap-4 xl:grid-cols-3">
              {(["red", "yellow", "green"] as RiskLevel[]).map((level) => (
                <ProblemGroup key={level} level={level} result={result} />
              ))}
            </div>
          </div>
        </div>
      ) : (
        <EmptyState title="暂无结果" description="该文件还没有可展示的检测结果。" />
      )}
    </section>
  );
}

function Summary({ result }: { result: DetectionResult }) {
  return (
    <div className="flex items-center gap-3 text-xs">
      <span className="tabular-nums text-red-300">红 {result.redCount}</span>
      <span className="tabular-nums text-yellow-300">黄 {result.yellowCount}</span>
      <span className="tabular-nums text-green-300">绿 {result.greenCount}</span>
    </div>
  );
}

function InfoPanel({ result }: { result: DetectionResult }) {
  return (
    <aside className="border-r border-slate-800 bg-slate-950/45 p-4">
      <div className="mb-4 flex items-center gap-2 text-sm font-semibold text-slate-100">
        <Info size={17} />
        基础信息
      </div>
      <dl className="grid gap-3 text-sm">
        <InfoRow label="时长" value={formatTime(result.basicInfo.duration)} />
        <InfoRow label="分辨率" value={result.basicInfo.resolution} />
        <InfoRow label="帧率" value={`${result.basicInfo.fps} fps`} />
        <InfoRow label="编码" value={result.basicInfo.codec} />
        <InfoRow label="大小" value={`${result.basicInfo.fileSize.toFixed(1)} MB`} />
      </dl>
      <div className="mt-5 rounded border border-slate-800 bg-slate-900 p-3">
        <div className="mb-2 flex items-center gap-2 text-xs font-medium text-slate-300">
          <Clock3 size={14} />
          检测说明
        </div>
        <p className="text-xs leading-5 text-slate-500">
          当前版本读取真实视频基础信息，并使用 FFmpeg cropdetect/freezedetect 进行黑边和冻结帧检测；跳帧、音频等项目仍在后续接入。
        </p>
      </div>
    </aside>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3 border-b border-slate-800 pb-2 last:border-0">
      <dt className="text-slate-500">{label}</dt>
      <dd className="truncate text-right font-medium text-slate-200">{value}</dd>
    </div>
  );
}

function ProblemGroup({ level, result }: { level: RiskLevel; result: DetectionResult }) {
  const problems = problemsByLevel(result, level);
  const meta = levelMeta[level];
  const hasAnyProblem = result.problems.length > 0;

  return (
    <div className={`rounded border ${levelClasses(level)}`}>
      <div className="flex h-11 items-center justify-between border-b border-current/15 px-3">
        <div className="flex items-center gap-2 text-sm font-semibold">
          {meta.icon}
          {meta.title}
        </div>
        <span className="text-xs tabular-nums">{problems.length} 个</span>
      </div>
      <div className="grid gap-3 p-3">
        {problems.length === 0 ? (
          <p className="text-sm opacity-70">{hasAnyProblem ? meta.empty : "真实检测未发现该级别问题"}</p>
        ) : (
          problems.map((problem, index) => (
            <article key={problem.id} className="grid gap-3 rounded border border-current/15 bg-slate-950/30 p-3">
              <div className="grid grid-cols-2 gap-2">
                <Shot label="开始" src={problem.startScreenshot ?? problem.screenshot} alt={`${problem.type}开始截图`} />
                <Shot label="结束" src={problem.endScreenshot ?? problem.screenshot} alt={`${problem.type}结束截图`} />
              </div>
              <div className="min-w-0">
                <div className="mb-1 flex items-center gap-2">
                  <span className="text-xs opacity-70">#{index + 1}</span>
                  <h3 className="truncate text-sm font-semibold text-slate-100">{problem.type}</h3>
                  <span className="rounded bg-slate-950/60 px-2 py-0.5 text-[11px]">{levelLabel(problem.level)}</span>
                </div>
                <p className="text-sm leading-5 text-slate-300">{problem.description}</p>
                <p className="mt-2 text-xs tabular-nums text-slate-500">
                  {formatTime(problem.startTime)} - {formatTime(problem.endTime)}
                </p>
              </div>
            </article>
          ))
        )}
      </div>
    </div>
  );
}

function Shot({ label, src, alt }: { label: string; src?: string; alt: string }) {
  return (
    <div className="min-w-0">
      <div className="mb-1 text-[11px] font-medium text-slate-500">{label}</div>
      <div className="flex aspect-video w-full items-center justify-center rounded border border-current/15 bg-slate-950/50">
        {src ? (
          <img src={src} alt={alt} className="h-full w-full rounded object-cover" />
        ) : (
          <ImageIcon size={20} className="opacity-55" />
        )}
      </div>
    </div>
  );
}

function EmptyState({ title, description, tone = "default" }: { title: string; description: string; tone?: "default" | "error" }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
      <div
        className={`flex h-14 w-14 items-center justify-center rounded border ${
          tone === "error" ? "border-red-400/35 bg-red-500/10 text-red-200" : "border-slate-700 bg-slate-950 text-slate-500"
        }`}
      >
        {tone === "error" ? <AlertTriangle size={24} /> : <Info size={24} />}
      </div>
      <div>
        <p className="text-sm font-medium text-slate-200">{title}</p>
        <p className="mt-1 text-xs text-slate-500">{description}</p>
      </div>
    </div>
  );
}
