import { FileVideo, MoreHorizontal, Trash2 } from "lucide-react";
import type { VideoFile } from "../types";
import { countProblems } from "../utils";

interface FileListProps {
  files: VideoFile[];
  selectedPath: string | null;
  activeStage: string;
  onSelectFile: (file: VideoFile) => void;
  onDeleteFile: (path: string) => void;
}

const statusLabels: Record<VideoFile["status"], string> = {
  pending: "等待",
  detecting: "检测中",
  paused: "暂停",
  completed: "完成",
  error: "错误",
  cancelled: "已取消",
};

export default function FileList({
  files,
  selectedPath,
  activeStage,
  onSelectFile,
  onDeleteFile,
}: FileListProps) {
  return (
    <section className="flex min-h-[300px] flex-1 flex-col overflow-hidden rounded border border-slate-700 bg-slate-900 shadow-inspector">
      <div className="flex h-12 items-center justify-between border-b border-slate-700 px-4">
        <div>
          <h2 className="text-sm font-semibold text-slate-100">文件列表</h2>
          <p className="text-xs text-slate-400">支持拖拽 MP4 到窗口任意位置</p>
        </div>
        <div className="text-xs text-slate-400">{files.length} 个文件</div>
      </div>

      {files.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
          <div className="flex h-16 w-16 items-center justify-center rounded border border-dashed border-slate-600 bg-slate-950">
            <FileVideo size={28} className="text-slate-500" />
          </div>
          <div>
            <p className="text-sm font-medium text-slate-200">拖入 MP4 文件或点击导入开始</p>
            <p className="mt-1 text-xs text-slate-500">导入 MP4 后可读取真实基础信息并执行画面检测</p>
          </div>
        </div>
      ) : (
        <div className="overflow-auto">
          <table className="w-full min-w-[900px] table-fixed border-collapse text-sm">
            <thead className="sticky top-0 bg-slate-950 text-xs uppercase text-slate-500">
              <tr>
                <th className="w-[38%] px-4 py-3 text-left font-semibold">文件名</th>
                <th className="w-[12%] px-3 py-3 text-left font-semibold">状态</th>
                <th className="w-[22%] px-3 py-3 text-left font-semibold">进度</th>
                <th className="w-[8%] px-3 py-3 text-center font-semibold text-red-300">红线</th>
                <th className="w-[8%] px-3 py-3 text-center font-semibold text-yellow-300">黄线</th>
                <th className="w-[8%] px-3 py-3 text-center font-semibold text-green-300">绿线</th>
                <th className="w-[4%] px-3 py-3 text-right font-semibold"></th>
              </tr>
            </thead>
            <tbody>
              {files.map((file) => (
                <tr
                  key={file.path}
                  className={`cursor-pointer border-t border-slate-800 transition hover:bg-slate-800/70 ${
                    selectedPath === file.path ? "bg-blue-500/10 outline outline-1 outline-blue-500/35" : ""
                  }`}
                  onClick={() => onSelectFile(file)}
                >
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-3">
                      <FileVideo size={18} className="shrink-0 text-blue-300" />
                      <div className="min-w-0">
                        <p className="truncate font-medium text-slate-100">{file.name}</p>
                        <p className="truncate text-xs text-slate-500">{file.path}</p>
                      </div>
                    </div>
                  </td>
                  <td className="px-3 py-3">
                    <StatusBadge status={file.status} />
                  </td>
                  <td className="px-3 py-3">
                    <div className="flex items-center gap-3">
                      <div className="h-2 w-full overflow-hidden rounded bg-slate-800">
                        <div
                          className="h-full rounded bg-blue-500 transition-all duration-300"
                          style={{ width: `${file.progress}%` }}
                        />
                      </div>
                      <span className="w-10 text-right text-xs tabular-nums text-slate-300">{file.progress}%</span>
                    </div>
                    {file.status === "detecting" && (
                      <p className="mt-1 truncate text-xs text-slate-500">{activeStage}</p>
                    )}
                  </td>
                  <td className="px-3 py-3 text-center tabular-nums text-red-300">
                    {file.result ? countProblems(file.result, "red") : "-"}
                  </td>
                  <td className="px-3 py-3 text-center tabular-nums text-yellow-300">
                    {file.result ? countProblems(file.result, "yellow") : "-"}
                  </td>
                  <td className="px-3 py-3 text-center tabular-nums text-green-300">
                    {file.result ? countProblems(file.result, "green") : "-"}
                  </td>
                  <td className="px-3 py-3 text-right">
                    <button
                      type="button"
                      className="inline-flex h-8 w-8 items-center justify-center rounded text-slate-500 transition hover:bg-slate-700 hover:text-red-300"
                      onClick={(event) => {
                        event.stopPropagation();
                        onDeleteFile(file.path);
                      }}
                      title="删除文件"
                    >
                      {file.status === "detecting" ? <MoreHorizontal size={16} /> : <Trash2 size={16} />}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function StatusBadge({ status }: { status: VideoFile["status"] }) {
  const classes: Record<VideoFile["status"], string> = {
    pending: "border-slate-600 bg-slate-800 text-slate-300",
    detecting: "border-blue-400/40 bg-blue-500/15 text-blue-200",
    paused: "border-yellow-400/40 bg-yellow-400/15 text-yellow-100",
    completed: "border-green-400/40 bg-green-400/15 text-green-100",
    error: "border-red-400/40 bg-red-500/15 text-red-100",
    cancelled: "border-slate-500/40 bg-slate-700/70 text-slate-300",
  };

  return (
    <span className={`inline-flex h-7 min-w-16 items-center justify-center rounded border px-2 text-xs ${classes[status]}`}>
      {statusLabels[status]}
    </span>
  );
}
