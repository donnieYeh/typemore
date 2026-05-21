import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type RecordingState =
  | "Idle"
  | "Recording"
  | "LocalTranscribing"
  | "LlmRepairing"
  | "Delivering"
  | "Completed"
  | "Error";

export type AsrProfile = "speed" | "standard";
export type PromptStyle = "default" | "concise" | "formal" | "creative";

export interface Settings {
  deepseek_api_key: string;
  microphone_device_id: string | null;
  whisper_model_path: string;
  whisper_speed_model_path: string;
  whisper_sidecar_path: string;
  asr_profile: AsrProfile;
  language: string;
  auto_paste: boolean;
  hotkey_buffer_ms: number;
  prompt_style: PromptStyle;
}

export interface ProgressEvent {
  state: RecordingState;
  message: string;
}

export interface DeliveryResultEvent {
  mode: "Pasted" | "ClipboardOnly";
  text: string;
}

export interface ErrorEvent {
  state: RecordingState;
  error: string;
}

export interface BootstrapResult {
  whisper_sidecar_path: string;
  whisper_model_path: string;
  whisper_speed_model_path: string;
}

export async function getSettings(): Promise<Settings> {
  return invoke("get_settings");
}

export async function saveSettings(settings: Settings): Promise<Settings> {
  return invoke("save_settings", { settings });
}

export async function startRecording(): Promise<void> {
  return invoke("start_recording");
}

export async function stopRecording(): Promise<void> {
  return invoke("stop_recording");
}

export async function retryLastPipeline(): Promise<void> {
  return invoke("retry_last_pipeline");
}

export async function openLogsFolder(): Promise<void> {
  return invoke("open_logs_folder");
}

export async function bootstrapResources(): Promise<BootstrapResult> {
  return invoke("bootstrap_resources");
}

export async function bootstrapSpeedModel(): Promise<string> {
  return invoke("bootstrap_speed_model");
}

export interface AudioDevice {
  id: string;
  name: string;
}

export async function listAudioDevices(): Promise<AudioDevice[]> {
  return invoke("list_audio_devices");
}

export interface ModelInfo {
  size: string;
  path: string;
  downloaded: boolean;
}

export async function listAvailableModels(): Promise<ModelInfo[]> {
  return invoke("list_available_models");
}

export async function downloadModel(model_size: string): Promise<string> {
  return invoke("download_model", { model_size });
}

export async function getLastPrompt(): Promise<string | null> {
  return invoke("get_last_prompt");
}

export async function copyLastPrompt(): Promise<string | null> {
  return invoke("copy_last_prompt");
}

export function onProgress(handler: (event: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<ProgressEvent>("pipeline-progress", (event) => handler(event.payload));
}

export function onError(handler: (event: ErrorEvent) => void): Promise<UnlistenFn> {
  return listen<ErrorEvent>("pipeline-error", (event) => handler(event.payload));
}

export function onDeliveryResult(
  handler: (event: DeliveryResultEvent) => void
): Promise<UnlistenFn> {
  return listen<DeliveryResultEvent>("delivery-result", (event) => handler(event.payload));
}
