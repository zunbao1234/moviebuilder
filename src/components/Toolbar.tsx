import {
  Ban,
  FileDown,
  FolderOpen,
  Play,
  Pause,
  RotateCcw,
  Settings2,
  Trash2,
  Upload,
} from "lucide-react";
import type { DetectionMode } from "../types";

interface ToolbarProps {
  isDetecting: boolean;
  hasFiles: boolean;
  mode: DetectionMode;
  onModeChange: (mode: DetectionMode) => void;
  onImportFiles: () => void;
  onImportFolder: () => void;
  onStartDetection: () => void;
  onPauseDetection: () => void;
  onCancelDetection: () => void;
  onClearList: () => void;
  onExportReport: () => void;
}

const modes: Array<{ value: DetectionMode; label: string }> = [
  { value: "fast", label: "极速" },
  { value: "balanced", label: "平衡" },
  { value: "accurate", label: "精准" },
];

export default function Toolbar({
  isDetecting,
  hasFiles,
  mode,
  onModeChange,
  onImportFiles,
  onImportFolder,
  onStartDetection,
  onPauseDetection,
  onCancelDetection,
  onClearList,
  onExportReport,
}: ToolbarProps) {
  return (
    <header className="flex min-h-16 items-center justify-between border-b border-slate-700/80 bg-slate-950/95 px-4">
      <div className="flex items-center gap-3">
        <div className="flex h-10 w-10 items-center justify-center rounded bg-blue-500 text-slate-950">
          <Settings2 size={20} strokeWidth={2.4} />
        </div>
        <div>
          <h1 className="text-base font-semibold tracking-normal text-slate-50">VideoInspector Pro</h1>
          <p className="text-xs text-slate-400">MP4 质量检查 MVP</p>
        </div>
      </div>

      <div className="flex items-center gap-2">
        <div className="mr-2 flex rounded border border-slate-700 bg-slate-900 p-1">
          {modes.map((item) => (
            <button
              key={item.value}
              type="button"
              className={`h-8 min-w-12 rounded px-3 text-xs font-medium transition ${
                mode === item.value
                  ? "bg-blue-500 text-white"
                  : "text-slate-400 hover:bg-slate-800 hover:text-slate-100"
              }`}
              onClick={() => onModeChange(item.value)}
              title={`切换到${item.label}检测模式`}
            >
              {item.label}
            </button>
          ))}
        </div>
        <ToolbarButton icon={<Upload size={16} />} label="导入文件" onClick={onImportFiles} />
        <ToolbarButton icon={<FolderOpen size={16} />} label="导入文件夹" onClick={onImportFolder} />
        <ToolbarButton
          icon={<Play size={16} />}
          label="开始检测"
          onClick={onStartDetection}
          disabled={!hasFiles || isDetecting}
          intent="primary"
        />
        <ToolbarButton
          icon={<Pause size={16} />}
          label="暂停"
          onClick={onPauseDetection}
          disabled={!isDetecting}
        />
        <ToolbarButton
          icon={<Ban size={16} />}
          label="取消"
          onClick={onCancelDetection}
          disabled={!isDetecting}
        />
        <ToolbarButton
          icon={<FileDown size={16} />}
          label="导出报告"
          onClick={onExportReport}
          disabled={!hasFiles || isDetecting}
        />
        <ToolbarButton
          icon={<Trash2 size={16} />}
          label="清空"
          onClick={onClearList}
          disabled={!hasFiles || isDetecting}
        />
      </div>
    </header>
  );
}

function ToolbarButton({
  icon,
  label,
  disabled,
  intent = "default",
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  disabled?: boolean;
  intent?: "default" | "primary";
  onClick: () => void;
}) {
  const classes =
    intent === "primary"
      ? "border-blue-400/70 bg-blue-500 text-white hover:bg-blue-400"
      : "border-slate-700 bg-slate-900 text-slate-200 hover:border-slate-600 hover:bg-slate-800";

  return (
    <button
      type="button"
      className={`inline-flex h-9 items-center gap-2 rounded border px-3 text-sm font-medium transition disabled:cursor-not-allowed disabled:border-slate-800 disabled:bg-slate-900/70 disabled:text-slate-600 ${classes}`}
      onClick={onClick}
      disabled={disabled}
      title={label}
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}
