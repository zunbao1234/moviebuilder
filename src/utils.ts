import type { DetectionResult, Problem, RiskLevel, VideoFile } from "./types";

export function basename(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  return normalized.split("/").pop() || path;
}

export function isMp4Path(path: string): boolean {
  return path.toLowerCase().endsWith(".mp4");
}

export function dedupeFiles(existing: VideoFile[], paths: string[]): VideoFile[] {
  const seen = new Set(existing.map((file) => file.path));
  return paths
    .filter(isMp4Path)
    .filter((path) => {
      if (seen.has(path)) {
        return false;
      }
      seen.add(path);
      return true;
    })
    .map((path) => ({
      path,
      name: basename(path),
      status: "pending",
      progress: 0,
      result: null,
    }));
}

export function formatTime(seconds: number): string {
  const safeSeconds = Math.max(0, seconds);
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  const secs = Math.floor(safeSeconds % 60);
  const millis = Math.round((safeSeconds - Math.floor(safeSeconds)) * 1000);

  const base = [hours, minutes, secs].map((part) => String(part).padStart(2, "0")).join(":");
  return `${base}.${String(millis).padStart(3, "0")}`;
}

export function levelLabel(level: RiskLevel): string {
  if (level === "red") return "红线";
  if (level === "yellow") return "黄线";
  return "绿线";
}

export function levelClasses(level: RiskLevel): string {
  if (level === "red") return "border-red-500/35 bg-red-500/10 text-red-100";
  if (level === "yellow") return "border-yellow-400/35 bg-yellow-400/10 text-yellow-100";
  return "border-green-400/35 bg-green-400/10 text-green-100";
}

export function countProblems(result: DetectionResult | null, level: RiskLevel): number {
  if (!result) return 0;
  return result.problems.filter((problem) => problem.level === level).length;
}

export function problemsByLevel(result: DetectionResult | null, level: RiskLevel): Problem[] {
  if (!result) return [];
  return result.problems.filter((problem) => problem.level === level);
}
