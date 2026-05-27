import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import FileList from "./components/FileList";
import ResultPanel from "./components/ResultPanel";
import StatusBar from "./components/StatusBar";
import Toolbar from "./components/Toolbar";
import { isTauriRuntime } from "./tauriRuntime";
import type {
  DetectionCompletePayload,
  DetectionErrorPayload,
  DetectionMode,
  DetectionSettings,
  DetectionProgressPayload,
  VideoFile,
} from "./types";
import { dedupeFiles, isMp4Path } from "./utils";

export default function App() {
  const [files, setFiles] = useState<VideoFile[]>([]);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [isDetecting, setIsDetecting] = useState(false);
  const [mode, setMode] = useState<DetectionMode>("balanced");
  const [settings, setSettings] = useState<DetectionSettings>({
    blackBorderYellowThreshold: 0.03,
    blackBorderRedThreshold: 0.1,
    blackBorderIrregularThreshold: 0.03,
  });
  const [activeStage, setActiveStage] = useState("空闲");
  const [dragActive, setDragActive] = useState(false);
  const [lastReportPath, setLastReportPath] = useState<string | null>(null);

  const selectedFile = useMemo(
    () => files.find((file) => file.path === selectedPath) ?? files[0] ?? null,
    [files, selectedPath],
  );

  const totalProgress = useMemo(() => {
    if (files.length === 0) return 0;
    const total = files.reduce((sum, file) => sum + file.progress, 0);
    return Math.round(total / files.length);
  }, [files]);

  useEffect(() => {
    const unlisteners = [
      listen<DetectionProgressPayload>("detection-progress", (event) => {
        const { filePath, progress, stage } = event.payload;
        setActiveStage(stage);
        setFiles((prev) =>
          prev.map((file) =>
            file.path === filePath ? { ...file, progress, status: "detecting", error: undefined } : file,
          ),
        );
      }),
      listen<DetectionCompletePayload>("detection-complete", (event) => {
        const { filePath, result } = event.payload;
        setFiles((prev) =>
          prev.map((file) =>
            file.path === filePath ? { ...file, progress: 100, status: "completed", result } : file,
          ),
        );
      }),
      listen<DetectionErrorPayload>("detection-error", (event) => {
        const { filePath, message } = event.payload;
        setFiles((prev) =>
          prev.map((file) => (file.path === filePath ? { ...file, status: "error", error: message } : file)),
        );
      }),
      listen("detection-cancelled", () => {
        setIsDetecting(false);
        setActiveStage("已取消");
        setFiles((prev) =>
          prev.map((file) =>
            file.status === "detecting" ? { ...file, status: "cancelled", progress: 0 } : file,
          ),
        );
      }),
    ];

    return () => {
      unlisteners.forEach((unlisten) => {
        void unlisten.then((dispose) => dispose());
      });
    };
  }, []);

  useEffect(() => {
    if (files.length > 0 && files.every((file) => ["completed", "error", "cancelled"].includes(file.status))) {
      setIsDetecting(false);
      setActiveStage("检测完成");
    }
  }, [files]);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    const webview = getCurrentWebviewWindow();
    const unlisten = webview.onDragDropEvent((event) => {
      if (event.payload.type === "over") {
        setDragActive(true);
      }
      if (event.payload.type === "leave") {
        setDragActive(false);
      }
      if (event.payload.type === "drop") {
        setDragActive(false);
        const paths = event.payload.paths.filter(isMp4Path);
        addFiles(paths);
      }
    });

    return () => {
      void unlisten.then((dispose) => dispose());
    };
  }, [files]);

  function addFiles(paths: string[]) {
    if (paths.length === 0) return;
    setFiles((prev) => {
      const nextFiles = dedupeFiles(prev, paths);
      if (prev.length === 0 && nextFiles.length > 0) {
        setSelectedPath(nextFiles[0].path);
      }
      return [...prev, ...nextFiles];
    });
  }

  async function handleImportFiles() {
    if (!isTauriRuntime()) {
      setActiveStage("文件对话框需要 Tauri 桌面运行时");
      return;
    }

    try {
      const selected = await open({
        multiple: true,
        directory: false,
        filters: [{ name: "MP4 Videos", extensions: ["mp4"] }],
      });

      if (Array.isArray(selected)) {
        addFiles(selected);
        setActiveStage(selected.length > 0 ? "文件已导入" : "未选择文件");
      } else if (typeof selected === "string") {
        addFiles([selected]);
        setActiveStage("文件已导入");
      } else {
        setActiveStage("未选择文件");
      }
    } catch (error) {
      setActiveStage(`导入失败：${String(error)}`);
      console.error(error);
    }
  }

  async function handleImportFolder() {
    if (!isTauriRuntime()) {
      setActiveStage("文件夹导入需要 Tauri 桌面运行时");
      return;
    }

    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });

      if (typeof selected === "string") {
        const folderFiles = await invoke<string[]>("read_folder_mp4", { folderPath: selected });
        addFiles(folderFiles);
        setActiveStage(`已导入 ${folderFiles.length} 个 MP4`);
      } else {
        setActiveStage("未选择文件夹");
      }
    } catch (error) {
      setActiveStage(`导入文件夹失败：${String(error)}`);
      console.error(error);
    }
  }

  async function handleStartDetection() {
    const targetFiles = files.filter((file) => file.status !== "completed");
    if (targetFiles.length === 0) return;

    setIsDetecting(true);
    setLastReportPath(null);
    setActiveStage("准备检测");
    setFiles((prev) =>
      prev.map((file) =>
        file.status === "completed"
          ? file
          : { ...file, status: "pending", progress: 0, result: null, error: undefined },
      ),
    );

    try {
      if (!isTauriRuntime()) {
        console.info("检测任务需要在 Tauri 桌面运行时中使用。");
        setIsDetecting(false);
        setActiveStage("需要 Tauri 运行时");
        return;
      }

      await invoke("start_detection", {
        files: targetFiles.map((file) => file.path),
        mode,
        settings,
      });
    } catch (error) {
      setIsDetecting(false);
      setActiveStage("启动失败");
      console.error(error);
    }
  }

  async function handlePauseDetection() {
    if (!isTauriRuntime()) {
      setIsDetecting(false);
      setActiveStage("需要 Tauri 运行时");
      return;
    }

    await invoke("pause_detection");
    setIsDetecting(false);
    setActiveStage("已暂停");
    setFiles((prev) => prev.map((file) => (file.status === "detecting" ? { ...file, status: "paused" } : file)));
  }

  async function handleCancelDetection() {
    if (!isTauriRuntime()) {
      setIsDetecting(false);
      setActiveStage("需要 Tauri 运行时");
      return;
    }

    await invoke("cancel_detection");
  }

  function handleDeleteFile(path: string) {
    setFiles((prev) => prev.filter((file) => file.path !== path));
    if (selectedPath === path) {
      setSelectedPath(null);
    }
  }

  function handleClearList() {
    if (files.length === 0) return;
    const confirmed = window.confirm("确定清空所有待检测文件吗？");
    if (!confirmed) return;
    setFiles([]);
    setSelectedPath(null);
    setIsDetecting(false);
    setActiveStage("空闲");
    setLastReportPath(null);
  }

  async function handleExportReport() {
    if (!selectedFile) return;
    if (!isTauriRuntime()) {
      console.info("报告导出需要在 Tauri 桌面运行时中使用。");
      return;
    }

    const reportPath = await invoke<string>("generate_html_report", { filePath: selectedFile.path, settings });
    setLastReportPath(reportPath);
  }

  return (
    <div className="relative flex h-screen min-w-[960px] flex-col overflow-hidden bg-slate-950 text-slate-100">
      <Toolbar
        isDetecting={isDetecting}
        hasFiles={files.length > 0}
        mode={mode}
        onModeChange={setMode}
        settings={settings}
        onSettingsChange={setSettings}
        onImportFiles={handleImportFiles}
        onImportFolder={handleImportFolder}
        onStartDetection={handleStartDetection}
        onPauseDetection={handlePauseDetection}
        onCancelDetection={handleCancelDetection}
        onClearList={handleClearList}
        onExportReport={handleExportReport}
      />

      <main className="flex min-h-0 flex-1 flex-col gap-4 bg-[radial-gradient(circle_at_20%_0%,rgba(59,130,246,0.14),transparent_34%),#0f172a] p-4">
        {lastReportPath && (
          <div className="rounded border border-green-400/30 bg-green-400/10 px-4 py-2 text-sm text-green-100">
            报告已生成：{lastReportPath}
          </div>
        )}
        <FileList
          files={files}
          selectedPath={selectedFile?.path ?? null}
          activeStage={activeStage}
          onSelectFile={(file) => setSelectedPath(file.path)}
          onDeleteFile={handleDeleteFile}
        />
        <ResultPanel file={selectedFile} />
      </main>

      <StatusBar files={files} totalProgress={totalProgress} activeStage={activeStage} />

      {dragActive && (
        <div className="pointer-events-none absolute inset-0 z-20 flex items-center justify-center border-4 border-blue-400 bg-blue-950/70">
          <div className="rounded border border-blue-300/60 bg-slate-950 px-8 py-5 text-center shadow-inspector">
            <p className="text-lg font-semibold text-blue-100">释放以导入 MP4 文件</p>
            <p className="mt-1 text-sm text-slate-400">非 MP4 文件会被自动忽略</p>
          </div>
        </div>
      )}
    </div>
  );
}
