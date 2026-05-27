export type FileStatus = "pending" | "detecting" | "paused" | "completed" | "error" | "cancelled";

export type RiskLevel = "red" | "yellow" | "green";

export type DetectionMode = "fast" | "balanced" | "accurate";

export interface VideoFile {
  path: string;
  name: string;
  status: FileStatus;
  progress: number;
  result: DetectionResult | null;
  error?: string;
}

export interface DetectionResult {
  redCount: number;
  yellowCount: number;
  greenCount: number;
  problems: Problem[];
  basicInfo: BasicVideoInfo;
  reportPath?: string;
}

export interface BasicVideoInfo {
  duration: number;
  resolution: string;
  fps: number;
  codec: string;
  fileSize: number;
}

export interface Problem {
  id: string;
  type: string;
  level: RiskLevel;
  startTime: number;
  endTime: number;
  description: string;
  screenshot?: string;
  startScreenshot?: string;
  endScreenshot?: string;
}

export interface DetectionProgressPayload {
  filePath: string;
  progress: number;
  stage: string;
}

export interface DetectionCompletePayload {
  filePath: string;
  result: DetectionResult;
}

export interface DetectionErrorPayload {
  filePath: string;
  message: string;
}
