import { Activity, Clock3, Cpu } from "lucide-react";
import type { VideoFile } from "../types";

interface StatusBarProps {
  files: VideoFile[];
  totalProgress: number;
  activeStage: string;
}

export default function StatusBar({ files, totalProgress, activeStage }: StatusBarProps) {
  const completed = files.filter((file) => file.status === "completed").length;
  const detecting = files.some((file) => file.status === "detecting");

  return (
    <footer className="flex h-12 items-center justify-between border-t border-slate-700 bg-slate-950 px-4 text-xs text-slate-400">
      <div className="flex items-center gap-5">
        <span className="flex items-center gap-2">
          <Activity size={14} className={detecting ? "text-blue-300" : "text-slate-500"} />
          总进度 {totalProgress}%
        </span>
        <span>已完成 {completed}/{files.length}</span>
        <span className="flex items-center gap-2">
          <Clock3 size={14} />
          预计剩余 {detecting ? "计算中" : "--"}
        </span>
        <span className="flex items-center gap-2">
          <Cpu size={14} />
          {activeStage || "空闲"}
        </span>
      </div>
      <div className="h-2 w-[34vw] max-w-[440px] overflow-hidden rounded bg-slate-800">
        <div className="h-full rounded bg-blue-500 transition-all duration-300" style={{ width: `${totalProgress}%` }} />
      </div>
    </footer>
  );
}
