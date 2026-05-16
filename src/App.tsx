import { useEffect, useMemo, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  type DeliveryResultEvent,
  type ErrorEvent,
  type ProgressEvent,
  type RecordingState,
  type Settings,
  bootstrapResources,
  getSettings,
  onDeliveryResult,
  onError,
  onProgress,
  openLogsFolder,
  retryLastPipeline,
  saveSettings,
  startRecording,
  stopRecording
} from "./lib/tauri";

const defaultSettings: Settings = {
  deepseek_api_key: "",
  microphone_device_id: null,
  whisper_model_path: "",
  whisper_sidecar_path: "",
  language: "zh",
  auto_paste: true,
  hotkey_buffer_ms: 1500
};

const currentLabel = getCurrentWindow().label;
type OrnamentPhase = "recording" | "closing" | "closed";

function App() {
  const isOverlay = currentLabel === "overlay";

  useEffect(() => {
    document.body.dataset.window = currentLabel;
    return () => {
      delete document.body.dataset.window;
    };
  }, []);

  return isOverlay ? <OverlayApp /> : <MainApp />;
}

function MainApp() {
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [bootstrapping, setBootstrapping] = useState(false);
  const [state, setState] = useState<RecordingState>("Idle");
  const [message, setMessage] = useState("按 Alt 开始录音，再按一次结束录音。");
  const [lastText, setLastText] = useState("");

  usePipelineState(setState, setMessage, setLastText);

  useEffect(() => {
    async function boot() {
      try {
        const loaded = await getSettings();
        setSettings(loaded);
      } finally {
        setLoading(false);
      }
    }

    void boot();
  }, []);

  async function handleSave() {
    setSaving(true);
    try {
      const next = await saveSettings(settings);
      setSettings(next);
      setMessage("设置已保存。");
    } finally {
      setSaving(false);
    }
  }

  async function handleBootstrap() {
    setBootstrapping(true);
    try {
      const result = await bootstrapResources();
      setSettings((current) => ({
        ...current,
        whisper_model_path: result.whisper_model_path,
        whisper_sidecar_path: result.whisper_sidecar_path
      }));
      setMessage("Whisper 资源已下载并写入本地配置。");
    } finally {
      setBootstrapping(false);
    }
  }

  return (
    <main className="shell">
      <section className="hero">
        <div>
          <p className="eyebrow">Typemore</p>
          <h1>本地转录，云端修复，直接落到光标里。</h1>
          <p className="lede">{message}</p>
        </div>
        <div className={`status status-${state.toLowerCase()}`}>
          <span className="dot" />
          <strong>{state}</strong>
        </div>
      </section>

      <section className="panel actions">
        <button onClick={() => void startRecording()} disabled={state === "Recording"}>
          开始录音
        </button>
        <button onClick={() => void stopRecording()} disabled={state !== "Recording"}>
          停止录音
        </button>
        <button onClick={() => void retryLastPipeline()}>重试最近一次</button>
        <button className="ghost" onClick={() => void openLogsFolder()}>
          打开数据目录
        </button>
      </section>

      <section className="grid">
        <div className="panel">
          <h2>设置</h2>
          {loading ? (
            <p>加载中...</p>
          ) : (
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void handleSave();
              }}
            >
              <label>
                DeepSeek API Key
                <input
                  type="password"
                  value={settings.deepseek_api_key}
                  onChange={(event) =>
                    setSettings((current) => ({
                      ...current,
                      deepseek_api_key: event.target.value
                    }))
                  }
                />
              </label>
              <label>
                Whisper 模型路径
                <input
                  value={settings.whisper_model_path}
                  onChange={(event) =>
                    setSettings((current) => ({
                      ...current,
                      whisper_model_path: event.target.value
                    }))
                  }
                />
              </label>
              <label>
                Whisper CLI 路径
                <input
                  value={settings.whisper_sidecar_path}
                  onChange={(event) =>
                    setSettings((current) => ({
                      ...current,
                      whisper_sidecar_path: event.target.value
                    }))
                  }
                />
              </label>
              <button type="button" className="ghost" onClick={() => void handleBootstrap()}>
                {bootstrapping ? "下载中..." : "自动下载 Whisper 资源"}
              </button>
              <label>
                语言
                <select
                  value={settings.language}
                  onChange={(event) =>
                    setSettings((current) => ({
                      ...current,
                      language: event.target.value
                    }))
                  }
                >
                  <option value="zh">中文优先</option>
                  <option value="auto">自动检测</option>
                </select>
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={settings.auto_paste}
                  onChange={(event) =>
                    setSettings((current) => ({
                      ...current,
                      auto_paste: event.target.checked
                    }))
                  }
                />
                <span>优先自动粘贴到当前光标</span>
              </label>
              <label>
                右 Alt 缓冲时间（毫秒）
                <input
                  type="number"
                  min={50}
                  max={10000}
                  step={50}
                  value={settings.hotkey_buffer_ms}
                  onChange={(event) =>
                    setSettings((current) => ({
                      ...current,
                      hotkey_buffer_ms: Number(event.target.value || 1500)
                    }))
                  }
                />
              </label>
              <button type="submit" disabled={saving}>
                {saving ? "保存中..." : "保存设置"}
              </button>
            </form>
          )}
        </div>

        <div className="panel">
          <h2>最近结果</h2>
          <div className="result-box">{lastText || "还没有结果。录一次音就会显示在这里。"}</div>
          <p className="hint">
            如果当前没有可编辑光标，结果会进入剪贴板，并通过系统通知提醒你可以粘贴。
          </p>
        </div>
      </section>
    </main>
  );
}

function OverlayApp() {
  const [state, setState] = useState<RecordingState>("Idle");
  const [message, setMessage] = useState("按 Alt 开始录音");
  const [lastText, setLastText] = useState("");
  const [ornamentPhase, setOrnamentPhase] = useState<OrnamentPhase>("closed");
  usePipelineState(setState, setMessage, setLastText);

  useEffect(() => {
    if (state === "Recording") {
      setOrnamentPhase("recording");
      return;
    }

    setOrnamentPhase((current) => (current === "recording" ? "closing" : current));
  }, [state]);

  useEffect(() => {
    if (ornamentPhase !== "closing") {
      return;
    }

    const timeout = window.setTimeout(() => {
      setOrnamentPhase("closed");
    }, 420);

    return () => {
      window.clearTimeout(timeout);
    };
  }, [ornamentPhase]);

  const overlayMode = useMemo(() => {
    if (state === "Recording") {
      return {
        title: "正在录音",
        detail: "按 Alt 结束录音",
        animated: true
      };
    }

    if (state === "LocalTranscribing" || state === "LlmRepairing" || state === "Delivering") {
      return {
        title: "正在处理",
        detail: message,
        animated: false
      };
    }

    return {
      title: "Typemore",
      detail: lastText ? "本次结果已完成" : "等待下一次录音",
      animated: false
    };
  }, [lastText, message, state]);

  return (
    <main className="overlay-shell">
      <section className={`overlay-card overlay-card-${ornamentPhase}`}>
        <div
          className={`orb-stack orb-stack-${ornamentPhase} ${
            overlayMode.animated ? "is-animated" : ""
          }`}
        >
          <span className="orb orb-a" />
          <span className="orb orb-b" />
          <span className="orb orb-c" />
        </div>
        <div className="overlay-copy">
          <p className="overlay-title">{overlayMode.title}</p>
          <p className="overlay-detail">{overlayMode.detail}</p>
        </div>
      </section>
    </main>
  );
}

function usePipelineState(
  setState: (state: RecordingState) => void,
  setMessage: (message: string) => void,
  setLastText: (text: string) => void
) {
  useEffect(() => {
    let dispose: Array<() => void> = [];

    async function subscribe() {
      dispose = await Promise.all([
        onProgress((event: ProgressEvent) => {
          setState(event.state);
          setMessage(event.message);
        }),
        onError((event: ErrorEvent) => {
          setState(event.state);
          setMessage(event.error);
        }),
        onDeliveryResult((event: DeliveryResultEvent) => {
          setLastText(event.text);
        })
      ]);
    }

    void subscribe();
    return () => {
      for (const unlisten of dispose) {
        unlisten();
      }
    };
  }, [setLastText, setMessage, setState]);
}

export default App;
