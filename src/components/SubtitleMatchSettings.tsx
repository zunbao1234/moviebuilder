import { AlignLeft, ToggleLeft, ToggleRight } from "lucide-react";
import type { DetectionSettings } from "../types";

interface SubtitleMatchSettingsProps {
  settings: DetectionSettings;
  onSettingsChange: (settings: DetectionSettings) => void;
  disabled?: boolean;
}

export default function SubtitleMatchSettings({
  settings,
  onSettingsChange,
  disabled,
}: SubtitleMatchSettingsProps) {
  return (
    <section className="rounded border border-slate-700 bg-slate-900 shadow-inspector">
      <div className="flex items-center justify-between border-b border-slate-800 px-4 py-3">
        <div className="flex items-center gap-2">
          <AlignLeft size={17} className="text-blue-300" />
          <div>
            <h2 className="text-sm font-semibold text-slate-100">小说文本匹配</h2>
            <p className="text-xs text-slate-500">OCR字幕必须在小说文本中逐字匹配，含标点</p>
          </div>
        </div>
        <button
          type="button"
          className={`inline-flex h-8 items-center gap-2 rounded border px-3 text-xs font-medium transition ${
            settings.subtitleMatchEnabled
              ? "border-blue-400/60 bg-blue-500/15 text-blue-100"
              : "border-slate-700 bg-slate-950 text-slate-400"
          }`}
          onClick={() =>
            onSettingsChange({
              ...settings,
              subtitleMatchEnabled: !settings.subtitleMatchEnabled,
            })
          }
          disabled={disabled}
          title="启用或禁用字幕文本匹配"
        >
          {settings.subtitleMatchEnabled ? <ToggleRight size={17} /> : <ToggleLeft size={17} />}
          {settings.subtitleMatchEnabled ? "已启用" : "未启用"}
        </button>
      </div>
      <div className="p-3">
        <textarea
          value={settings.novelText ?? ""}
          onChange={(event) =>
            onSettingsChange({
              ...settings,
              novelText: event.target.value,
            })
          }
          disabled={disabled || !settings.subtitleMatchEnabled}
          placeholder="在这里粘贴小说原文。启用后，OCR 识别到的字幕片段必须能在这段文本中逐字连续匹配。"
          className="h-24 w-full resize-none rounded border border-slate-700 bg-slate-950 p-3 text-sm leading-5 text-slate-100 outline-none placeholder:text-slate-600 focus:border-blue-400 disabled:cursor-not-allowed disabled:opacity-55"
        />
      </div>
    </section>
  );
}
