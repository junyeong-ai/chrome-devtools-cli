import * as RecordingStore from './lib/recording-store';
import { getConnection, DaemonConnection } from './lib/daemon-connection';

const API_BASE = 'http://127.0.0.1:9223';
const MIN_CAPTURE_INTERVAL = 100;
const RETRY_DELAY = 300;
const MAX_RETRIES = 3;
const TRACE_STATE_KEY = 'traceState';

let wsConnection: DaemonConnection | null = null;
let useWebSocket = true;

interface ScreenRecordingState {
  id: string;
  tabId: number;
  windowId: number;
  isActive: boolean;
  fps: number;
  quality: number;
  dpr: number;
  frameCount: number;
  startTime: number;
}

interface PerformanceTraceState {
  id: string;
  tabId: number;
  isActive: boolean;
  startTime: number;
}

let recording: ScreenRecordingState | null = null;
let recordingInterval: ReturnType<typeof setInterval> | null = null;
let tracing: PerformanceTraceState | null = null;

async function saveRecordingState(): Promise<void> {
  if (recording) {
    await RecordingStore.updateRecording({
      id: recording.id,
      frameCount: recording.frameCount,
      isActive: recording.isActive,
    });
  }
}

async function restoreRecordingState(): Promise<void> {
  if (recording) return;

  const activeRecording = await RecordingStore.getActiveRecording();
  if (activeRecording) {
    recording = {
      id: activeRecording.id,
      tabId: activeRecording.tabId,
      windowId: activeRecording.windowId,
      isActive: true,
      fps: activeRecording.fps,
      quality: activeRecording.quality,
      dpr: activeRecording.dpr,
      frameCount: activeRecording.frameCount,
      startTime: activeRecording.startTime,
    };
    startFrameCapture();
  }

  RecordingStore.cleanOldRecordings().catch(() => {});
}

function startFrameCapture(): void {
  if (!recording?.isActive || recordingInterval) return;

  const captureInterval = Math.max(1000 / recording.fps, MIN_CAPTURE_INTERVAL);

  recordingInterval = setInterval(async () => {
    if (!recording?.isActive) {
      if (recordingInterval) {
        clearInterval(recordingInterval);
        recordingInterval = null;
      }
      return;
    }

    try {
      const dataUrl = await chrome.tabs.captureVisibleTab(recording.windowId, {
        format: 'jpeg',
        quality: recording.quality,
      });

      if (recording?.isActive && dataUrl) {
        const resized = await resizeToLogicalPixels(dataUrl, recording.quality, recording.dpr);
        const offsetMs = Date.now() - recording.startTime;

        await RecordingStore.saveFrame({
          recordingId: recording.id,
          index: recording.frameCount,
          data: resized,
          offsetMs,
          timestamp: Date.now(),
        });

        sendFrame(recording.id, recording.frameCount, offsetMs, resized);
        recording.frameCount++;

        if (recording.frameCount % 10 === 0) {
          saveRecordingState();
        }
      }
    } catch {}
  }, captureInterval);
}

restoreRecordingState();

interface ElementInfo {
  selector: string;
  tagName?: string;
  className?: string;
  id?: string;
  text?: string;
  dimensions?: string;
}

interface ScreenshotOptions {
  tabId: number;
  bounds?: { x: number; y: number; width: number; height: number };
  elementInfo?: ElementInfo;
}

async function getSessionId(): Promise<string | null> {
  try {
    const response = await fetch(`${API_BASE}/api/session`);
    if (response.ok) {
      const data = await response.json();
      return data.ok && data.session_id ? data.session_id : null;
    }
  } catch {}
  return null;
}

async function initWebSocket(): Promise<void> {
  const sessionId = await getSessionId();
  if (!sessionId) {
    setTimeout(initWebSocket, 5000);
    return;
  }

  wsConnection = getConnection();
  wsConnection.on('connected', () => {
    console.log('[Chrome DevTools CLI] WebSocket connected');
  });
  wsConnection.on('reconnecting', (data) => {
    console.log('[Chrome DevTools CLI] WebSocket reconnecting...', data);
  });
  wsConnection.connect(sessionId);
}

async function getActiveTab(): Promise<chrome.tabs.Tab | null> {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  return tab?.id ? tab : null;
}

async function sendToContent(tabId: number, message: object, maxRetries = MAX_RETRIES): Promise<unknown> {
  for (let i = 0; i < maxRetries; i++) {
    try {
      return await chrome.tabs.sendMessage(tabId, message);
    } catch {
      if (i < maxRetries - 1) {
        await new Promise(r => setTimeout(r, RETRY_DELAY));
      }
    }
  }
  return null;
}

async function api(endpoint: string, data: object): Promise<{ ok: boolean; error?: string; recording_id?: string }> {
  const sid = await getSessionId();
  try {
    const response = await fetch(`${API_BASE}${endpoint}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ session_id: sid, ...data }),
    });
    return await response.json();
  } catch {
    return { ok: false };
  }
}

async function startRecordingOnServer(fps: number, quality: number): Promise<string | null> {
  const result = await api('/api/recording/start', { fps, quality });
  return result.ok ? result.recording_id ?? null : null;
}

async function stopRecordingOnServer(recordingId: string, frameCount: number, durationMs: number): Promise<boolean> {
  const result = await api('/api/recording/stop', { recording_id: recordingId, frame_count: frameCount, duration_ms: durationMs });
  return result.ok;
}

async function sendFrame(recordingId: string, index: number, offsetMs: number, data: string): Promise<boolean> {
  const result = await api('/api/recording/frame', { recording_id: recordingId, index, offset_ms: offsetMs, data });
  return result.ok;
}

async function sendSessionEvent(event: object): Promise<boolean> {
  if (useWebSocket && wsConnection?.getState() === 'connected') {
    wsConnection.sendEvent(event);
    return true;
  }
  const result = await api('/api/events', { event });
  return result.ok;
}

async function sendScreenshot(data: string, filename?: string): Promise<boolean> {
  const result = await api('/api/screenshots', { data, filename });
  return result.ok;
}

async function startTraceOnServer(): Promise<string | null> {
  const result = await api('/api/trace/start', {});
  return result.ok ? (result as { trace_id?: string }).trace_id ?? null : null;
}

async function stopTraceOnServer(): Promise<{ trace_id?: string; event_count?: number } | null> {
  const result = await api('/api/trace/stop', {});
  return result.ok ? result as { trace_id?: string; event_count?: number } : null;
}

async function saveTraceState(): Promise<void> {
  if (tracing) {
    await chrome.storage.session.set({ [TRACE_STATE_KEY]: tracing });
  } else {
    await chrome.storage.session.remove(TRACE_STATE_KEY);
  }
}

async function restoreTraceState(): Promise<void> {
  if (tracing) return;
  const result = await chrome.storage.session.get(TRACE_STATE_KEY);
  const savedState = result[TRACE_STATE_KEY];
  if (savedState?.isActive) {
    tracing = savedState;
  }
}

restoreTraceState();

async function checkDaemonConnection(): Promise<boolean> {
  try {
    const response = await fetch(`${API_BASE}/api/health`);
    const data = await response.json();
    return data.ok === true;
  } catch {
    return false;
  }
}

async function executeCommand(cmd: { type: string; [key: string]: unknown }): Promise<unknown> {
  const tab = await getActiveTab();
  if (!tab?.id) return { error: 'No active tab' };

  switch (cmd.type) {
    case 'request_selection':
      await sendToContent(tab.id, {
        type: 'start_selection',
        mode: cmd.mode || 'single',
        filter: cmd.filter,
      });
      return { success: true };

    case 'cancel_selection':
      await sendToContent(tab.id, { type: 'cancel_selection' });
      return { success: true };

    case 'take_snapshot':
      return await sendToContent(tab.id, { type: 'get_snapshot', verbose: cmd.verbose || false });

    case 'take_screenshot':
      return await captureScreenshot({ tabId: tab.id });

    case 'start_recording':
      return await startRecording(
        tab.id,
        tab.windowId!,
        (cmd.fps as number) || 5,
        (cmd.quality as number) || 70
      );

    case 'stop_recording':
      return await stopRecording();

    case 'start_trace':
      return await startTrace(tab.id);

    case 'stop_trace':
      return await stopTrace();

    case 'highlight':
      await sendToContent(tab.id, { type: 'highlight', selector: cmd.selector, color: cmd.color });
      return { success: true };

    case 'clear_highlight':
      await sendToContent(tab.id, { type: 'clear_highlight' });
      return { success: true };

    default:
      return { error: `Unknown command: ${cmd.type}` };
  }
}

async function captureScreenshot(options: ScreenshotOptions): Promise<object> {
  const { tabId, bounds, elementInfo } = options;

  try {
    const tab = await chrome.tabs.get(tabId);
    const dataUrl = await chrome.tabs.captureVisibleTab(tab.windowId, { format: 'png' });
    if (!dataUrl) return { error: 'Capture failed' };

    const finalDataUrl = bounds ? await cropImage(dataUrl, bounds) : dataUrl;
    const filename = `screenshot_${Date.now()}.png`;

    sendScreenshot(finalDataUrl, filename);
    sendSessionEvent({
      screenshot: {
        filename,
        url: tab.url,
        element: elementInfo,
        bounds,
        ts: Date.now(),
      },
    });
    showScreenshotDialog(tabId, finalDataUrl, elementInfo);

    return { success: true, dataUrl: finalDataUrl };
  } catch {
    return { error: 'Capture failed' };
  }
}

async function cropImage(
  dataUrl: string,
  bounds: { x: number; y: number; width: number; height: number }
): Promise<string> {
  const response = await fetch(dataUrl);
  const blob = await response.blob();
  const bitmap = await createImageBitmap(blob, bounds.x, bounds.y, bounds.width, bounds.height);

  const canvas = new OffscreenCanvas(bounds.width, bounds.height);
  const ctx = canvas.getContext('2d');
  if (!ctx) return dataUrl;

  ctx.drawImage(bitmap, 0, 0);
  const resultBlob = await canvas.convertToBlob({ type: 'image/png' });

  return new Promise(resolve => {
    const reader = new FileReader();
    reader.onloadend = () => resolve(reader.result as string);
    reader.readAsDataURL(resultBlob);
  });
}

async function resizeToLogicalPixels(dataUrl: string, quality: number, dpr: number): Promise<string> {
  if (dpr <= 1) return dataUrl;

  const response = await fetch(dataUrl);
  const blob = await response.blob();
  const bitmap = await createImageBitmap(blob);

  const targetWidth = Math.round(bitmap.width / dpr);
  const targetHeight = Math.round(bitmap.height / dpr);

  const canvas = new OffscreenCanvas(targetWidth, targetHeight);
  const ctx = canvas.getContext('2d');
  if (!ctx) return dataUrl;

  ctx.drawImage(bitmap, 0, 0, targetWidth, targetHeight);
  const resultBlob = await canvas.convertToBlob({ type: 'image/jpeg', quality: quality / 100 });

  return new Promise(resolve => {
    const reader = new FileReader();
    reader.onloadend = () => resolve(reader.result as string);
    reader.readAsDataURL(resultBlob);
  });
}

async function getDpr(tabId: number): Promise<number> {
  try {
    const [result] = await chrome.scripting.executeScript({
      target: { tabId },
      func: () => window.devicePixelRatio || 1,
    });
    return result?.result || 2;
  } catch {
    return 2;
  }
}

async function startRecording(
  tabId: number,
  windowId: number,
  fps: number,
  quality: number
): Promise<object> {
  if (recording?.isActive) {
    return { error: 'Already recording' };
  }

  const recordingId = await startRecordingOnServer(fps, quality);
  if (!recordingId) {
    return { error: 'Failed to start recording on server' };
  }

  const dpr = await getDpr(tabId);
  const startTime = Date.now();

  recording = {
    id: recordingId,
    tabId,
    windowId,
    isActive: true,
    fps,
    quality,
    dpr,
    frameCount: 0,
    startTime,
  };

  await RecordingStore.createRecording({
    id: recordingId,
    tabId,
    windowId,
    fps,
    quality,
    dpr,
    startTime,
    frameCount: 0,
    isActive: true,
  });

  await showRecordingIndicator(tabId);
  startFrameCapture();

  return { success: true, recording_id: recordingId };
}

async function stopRecording(): Promise<object> {
  if (!recording) {
    return { error: 'Not recording' };
  }

  const { id, tabId, frameCount, startTime, fps } = recording;
  recording.isActive = false;

  if (recordingInterval) {
    clearInterval(recordingInterval);
    recordingInterval = null;
  }

  await hideRecordingIndicator(tabId);

  const durationMs = Date.now() - startTime;
  await stopRecordingOnServer(id, frameCount, durationMs);

  await RecordingStore.updateRecording({
    id,
    isActive: false,
    endTime: Date.now(),
    frameCount,
  });

  const recordingData = await RecordingStore.getRecordingWithFrames(id);
  const frames = recordingData?.frames ?? [];

  showRecordingPreview(tabId, frames, fps, durationMs);

  const result = { recording_id: id, frameCount, durationMs };
  recording = null;

  return result;
}

async function startTrace(tabId: number): Promise<object> {
  if (tracing?.isActive) {
    return { error: 'Already tracing' };
  }

  const traceId = await startTraceOnServer();
  if (!traceId) {
    return { error: 'Failed to start trace on server' };
  }

  tracing = {
    id: traceId,
    tabId,
    isActive: true,
    startTime: Date.now(),
  };

  await saveTraceState();
  await showTraceIndicator(tabId);

  return { success: true, trace_id: traceId };
}

async function stopTrace(): Promise<object> {
  if (!tracing) {
    return { error: 'Not tracing' };
  }

  const { tabId, startTime } = tracing;
  tracing.isActive = false;

  await hideTraceIndicator(tabId);

  const serverResult = await stopTraceOnServer();
  const durationMs = Date.now() - startTime;

  const result = {
    trace_id: serverResult?.trace_id,
    event_count: serverResult?.event_count,
    durationMs,
  };
  tracing = null;

  await saveTraceState();

  return result;
}

async function showTraceIndicator(tabId: number): Promise<void> {
  try {
    await chrome.scripting.executeScript({
      target: { tabId },
      func: () => {
        document.getElementById('__cdtcli_trace__')?.remove();
        document.getElementById('__cdtcli_trace_style__')?.remove();

        const indicator = document.createElement('div');
        indicator.id = '__cdtcli_trace__';
        indicator.innerHTML = `
          <span style="display:inline-block;width:8px;height:8px;background:#3b82f6;border-radius:50%;margin-right:8px;animation:__cdtcli_trace_pulse__ 1s infinite"></span>
          <span>Tracing</span>
          <span style="margin-left:12px;padding:2px 8px;background:rgba(255,255,255,0.15);border-radius:4px;font-size:12px">Click to stop</span>
        `;
        indicator.style.cssText =
          'position:fixed;top:16px;right:16px;padding:8px 16px;background:#1e40af;color:#fff;font:14px/1.4 system-ui,sans-serif;border-radius:8px;z-index:2147483647;box-shadow:0 4px 16px rgba(0,0,0,0.3);cursor:pointer;display:flex;align-items:center;user-select:none';

        indicator.addEventListener('click', () => {
          chrome.runtime.sendMessage({ type: 'execute_local', command: { type: 'stop_trace' } });
        });

        indicator.addEventListener('mouseenter', () => {
          indicator.style.background = '#1e3a8a';
        });
        indicator.addEventListener('mouseleave', () => {
          indicator.style.background = '#1e40af';
        });

        const style = document.createElement('style');
        style.id = '__cdtcli_trace_style__';
        style.textContent = '@keyframes __cdtcli_trace_pulse__{0%,100%{opacity:1}50%{opacity:0.5}}';
        document.head.appendChild(style);
        document.body.appendChild(indicator);
      },
    });
  } catch {}
}

async function hideTraceIndicator(tabId: number): Promise<void> {
  try {
    await chrome.scripting.executeScript({
      target: { tabId },
      func: () => {
        document.getElementById('__cdtcli_trace__')?.remove();
        document.getElementById('__cdtcli_trace_style__')?.remove();
      },
    });
  } catch {}
}

async function showRecordingIndicator(tabId: number): Promise<void> {
  try {
    await chrome.scripting.executeScript({
      target: { tabId },
      func: () => {
        document.getElementById('__cdtcli_recording__')?.remove();
        document.getElementById('__cdtcli_recording_style__')?.remove();

        const indicator = document.createElement('div');
        indicator.id = '__cdtcli_recording__';
        indicator.innerHTML = `
          <span style="display:inline-block;width:8px;height:8px;background:#ff0000;border-radius:50%;margin-right:8px;animation:__cdtcli_pulse__ 1s infinite"></span>
          <span>Recording</span>
          <span style="margin-left:12px;padding:2px 8px;background:rgba(255,255,255,0.15);border-radius:4px;font-size:12px">Click to stop</span>
        `;
        indicator.style.cssText =
          'position:fixed;top:16px;right:16px;padding:8px 16px;background:#1a1a1a;color:#fff;font:14px/1.4 system-ui,sans-serif;border-radius:8px;z-index:2147483647;box-shadow:0 4px 16px rgba(0,0,0,0.3);cursor:pointer;display:flex;align-items:center;user-select:none';

        indicator.addEventListener('click', () => {
          chrome.runtime.sendMessage({ type: 'execute_local', command: { type: 'stop_recording' } });
        });

        indicator.addEventListener('mouseenter', () => {
          indicator.style.background = '#2a2a2a';
        });
        indicator.addEventListener('mouseleave', () => {
          indicator.style.background = '#1a1a1a';
        });

        const style = document.createElement('style');
        style.id = '__cdtcli_recording_style__';
        style.textContent = '@keyframes __cdtcli_pulse__{0%,100%{opacity:1}50%{opacity:0.5}}';
        document.head.appendChild(style);
        document.body.appendChild(indicator);
      },
    });
  } catch {}
}

async function hideRecordingIndicator(tabId: number): Promise<void> {
  try {
    await chrome.scripting.executeScript({
      target: { tabId },
      func: () => {
        document.getElementById('__cdtcli_recording__')?.remove();
        document.getElementById('__cdtcli_recording_style__')?.remove();
      },
    });
  } catch {}
}

function showScreenshotDialog(tabId: number, dataUrl: string, elementInfo?: ElementInfo): void {
  chrome.scripting.executeScript({
    target: { tabId },
    func: (dataUrl: string, elementInfo: ElementInfo | null) => {
      document.getElementById('__cdtcli_dialog__')?.remove();

      const host = document.createElement('div');
      host.id = '__cdtcli_dialog__';
      host.style.cssText = 'all:initial;position:fixed;inset:0;z-index:2147483647';
      const shadow = host.attachShadow({ mode: 'closed' });

      let infoSection = '';
      if (elementInfo) {
        const tagDisplay = elementInfo.tagName || 'element';
        let labelParts = [tagDisplay];
        if (elementInfo.id) labelParts.push(`#${elementInfo.id}`);
        if (elementInfo.className && typeof elementInfo.className === 'string') {
          const classes = elementInfo.className.trim().split(/\s+/).slice(0, 2);
          if (classes.length > 0 && classes[0]) {
            labelParts.push('.' + classes.join('.'));
          }
        }
        const elementLabel = labelParts.join('');

        infoSection = `
          <div style="padding:12px 16px;background:#f5f5f5;border-bottom:1px solid #e0e0e0">
            <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px">
              <span style="font-family:monospace;font-size:13px;color:#c26400">${elementLabel}</span>
              ${elementInfo.dimensions ? `<span style="font-size:12px;color:#666">${elementInfo.dimensions}</span>` : ''}
            </div>
            <div style="display:flex;align-items:center;gap:8px">
              <input id="__selector_input__" type="text" value="${elementInfo.selector.replace(/"/g, '&quot;')}"
                style="flex:1;padding:6px 8px;border:1px solid #ccc;border-radius:4px;font-family:monospace;font-size:12px;background:#fff" readonly />
              <button id="__copy_selector__" style="padding:6px 12px;background:#fff;border:1px solid #ccc;border-radius:4px;cursor:pointer;font-size:12px">Copy</button>
            </div>
          </div>
        `;
      }

      shadow.innerHTML = `
        <style>
          * { box-sizing: border-box; margin: 0; padding: 0; }
          .overlay { position:fixed;inset:0;background:rgba(0,0,0,0.6);display:flex;align-items:center;justify-content:center;font-family:system-ui,-apple-system,sans-serif }
          .dialog { background:#fff;border-radius:8px;max-width:90vw;max-height:90vh;box-shadow:0 4px 24px rgba(0,0,0,0.2);display:flex;flex-direction:column;overflow:hidden }
          .header { padding:12px 16px;border-bottom:1px solid #e0e0e0;display:flex;justify-content:space-between;align-items:center;background:#fafafa }
          .title { font-size:14px;font-weight:500;color:#333 }
          .close-btn { width:28px;height:28px;border:none;background:transparent;cursor:pointer;font-size:18px;color:#666;display:flex;align-items:center;justify-content:center;border-radius:4px }
          .close-btn:hover { background:#eee }
          .image-container { padding:16px;overflow:auto;flex:1;background:#f0f0f0;display:flex;align-items:center;justify-content:center }
          .image-container img { max-width:100%;max-height:60vh;box-shadow:0 2px 8px rgba(0,0,0,0.15) }
          .footer { padding:12px 16px;border-top:1px solid #e0e0e0;display:flex;gap:8px;justify-content:flex-end;background:#fafafa }
          .btn { padding:8px 16px;border-radius:4px;cursor:pointer;font-size:13px;border:1px solid #ccc;background:#fff }
          .btn:hover { background:#f5f5f5 }
          .btn-primary { background:#1a73e8;color:white;border:none }
          .btn-primary:hover { background:#1557b0 }
        </style>
        <div class="overlay">
          <div class="dialog">
            <div class="header">
              <span class="title">${elementInfo ? 'Element Screenshot' : 'Screenshot'}</span>
              <button class="close-btn" id="__close__">×</button>
            </div>
            ${infoSection}
            <div class="image-container">
              <img src="${dataUrl}" />
            </div>
            <div class="footer">
              <button class="btn" id="__copy__">Copy Image</button>
              <button class="btn btn-primary" id="__download__">Download</button>
            </div>
          </div>
        </div>
      `;

      document.body.appendChild(host);

      const overlay = shadow.querySelector('.overlay') as HTMLElement;
      const close = () => host.remove();

      shadow.getElementById('__close__')?.addEventListener('click', close);
      overlay.addEventListener('click', e => e.target === overlay && close());

      shadow.getElementById('__copy_selector__')?.addEventListener('click', () => {
        const input = shadow.getElementById('__selector_input__') as HTMLInputElement;
        navigator.clipboard.writeText(input.value);
        const btn = shadow.getElementById('__copy_selector__') as HTMLButtonElement;
        btn.textContent = 'Copied!';
        setTimeout(() => { if (btn) btn.textContent = 'Copy'; }, 1500);
      });

      shadow.getElementById('__copy__')?.addEventListener('click', async () => {
        try {
          const res = await fetch(dataUrl);
          const blob = await res.blob();
          await navigator.clipboard.write([new ClipboardItem({ [blob.type]: blob })]);
          const btn = shadow.getElementById('__copy__') as HTMLButtonElement;
          btn.textContent = 'Copied!';
          setTimeout(() => { if (btn) btn.textContent = 'Copy Image'; }, 1500);
        } catch {}
      });

      shadow.getElementById('__download__')?.addEventListener('click', () => {
        const a = document.createElement('a');
        a.href = dataUrl;
        a.download = `screenshot_${Date.now()}.png`;
        a.click();
      });
    },
    args: [dataUrl, elementInfo || null],
  });
}

function showRecordingPreview(tabId: number, frames: string[], fps: number, durationMs: number): void {
  chrome.scripting.executeScript({
    target: { tabId },
    func: (frames: string[], fps: number, durationMs: number) => {
      document.getElementById('__cdtcli_dialog__')?.remove();

      const host = document.createElement('div');
      host.id = '__cdtcli_dialog__';
      host.style.cssText = 'all:initial;position:fixed;inset:0;z-index:2147483647';
      const shadow = host.attachShadow({ mode: 'closed' });

      const durationSec = (durationMs / 1000).toFixed(1);
      const hasFrames = frames.length > 0;

      const playerSection = hasFrames
        ? `<img id="__player__" class="player" />`
        : `<div class="no-frames">No frames captured</div>`;

      const controlsSection = hasFrames
        ? `<div class="controls">
            <button id="__play__" class="play-btn">▶</button>
            <input id="__slider__" type="range" min="0" max="${frames.length - 1}" value="0" class="slider" />
            <span id="__time__" class="time">1 / ${frames.length}</span>
          </div>`
        : '';

      shadow.innerHTML = `
        <style>
          * { box-sizing: border-box; margin: 0; padding: 0; }
          .overlay { position:fixed;inset:0;background:rgba(0,0,0,0.6);display:flex;align-items:center;justify-content:center;font-family:system-ui,-apple-system,sans-serif }
          .dialog { background:#fff;border-radius:8px;max-width:800px;width:90%;box-shadow:0 4px 24px rgba(0,0,0,0.2);overflow:hidden }
          .header { padding:12px 16px;border-bottom:1px solid #e0e0e0;display:flex;justify-content:space-between;align-items:center;background:#fafafa }
          .title { font-size:14px;font-weight:500;color:#333 }
          .close-btn { width:28px;height:28px;border:none;background:transparent;cursor:pointer;font-size:18px;color:#666;display:flex;align-items:center;justify-content:center;border-radius:4px }
          .close-btn:hover { background:#eee }
          .video-container { background:#1a1a1a;aspect-ratio:16/9;display:flex;align-items:center;justify-content:center }
          .player { max-width:100%;max-height:100% }
          .no-frames { color:#999;font-size:14px }
          .controls { display:flex;align-items:center;gap:12px;padding:12px 16px;border-top:1px solid #e0e0e0;background:#fafafa }
          .play-btn { width:36px;height:36px;border:none;background:#1a73e8;color:white;border-radius:50%;cursor:pointer;font-size:14px;display:flex;align-items:center;justify-content:center }
          .play-btn:hover { background:#1557b0 }
          .slider { flex:1 }
          .time { font-size:12px;color:#666;min-width:70px;text-align:right }
          .stats { display:flex;border-top:1px solid #e0e0e0 }
          .stat { flex:1;padding:16px;text-align:center }
          .stat:first-child { border-right:1px solid #e0e0e0 }
          .stat-value { font-size:24px;font-weight:600;color:#333 }
          .stat-label { font-size:12px;color:#666;margin-top:4px }
          .footer { padding:12px 16px;border-top:1px solid #e0e0e0;display:flex;gap:8px;justify-content:flex-end;background:#fafafa }
          .btn { padding:8px 16px;border-radius:4px;cursor:pointer;font-size:13px;border:none;background:#1a73e8;color:white }
          .btn:hover { background:#1557b0 }
          .btn:disabled { background:#ccc;cursor:not-allowed }
        </style>
        <div class="overlay">
          <div class="dialog">
            <div class="header">
              <span class="title">Recording Complete</span>
              <button class="close-btn" id="__close__">×</button>
            </div>
            <div class="video-container">
              ${playerSection}
            </div>
            ${controlsSection}
            <div class="stats">
              <div class="stat">
                <div class="stat-value">${frames.length}</div>
                <div class="stat-label">Frames</div>
              </div>
              <div class="stat">
                <div class="stat-value">${durationSec}s</div>
                <div class="stat-label">Duration</div>
              </div>
            </div>
            <div class="footer">
              <button class="btn" id="__download__" ${!hasFrames ? 'disabled' : ''}>Download</button>
            </div>
          </div>
        </div>
      `;

      document.body.appendChild(host);

      const overlay = shadow.querySelector('.overlay') as HTMLElement;
      const close = () => host.remove();

      shadow.getElementById('__close__')?.addEventListener('click', close);
      overlay.addEventListener('click', e => e.target === overlay && close());

      if (hasFrames) {
        const player = shadow.getElementById('__player__') as HTMLImageElement;
        const slider = shadow.getElementById('__slider__') as HTMLInputElement;
        const timeDisplay = shadow.getElementById('__time__') as HTMLElement;
        const playBtn = shadow.getElementById('__play__') as HTMLButtonElement;

        let playing = false;
        let currentFrame = 0;
        let playInterval: number | null = null;

        const showFrame = (idx: number) => {
          if (frames[idx]) {
            player.src = frames[idx];
            slider.value = String(idx);
            timeDisplay.textContent = `${idx + 1} / ${frames.length}`;
            currentFrame = idx;
          }
        };

        showFrame(0);
        slider.addEventListener('input', () => showFrame(parseInt(slider.value)));

        playBtn.addEventListener('click', () => {
          if (playing) {
            if (playInterval) clearInterval(playInterval);
            playBtn.textContent = '▶';
            playing = false;
          } else {
            playBtn.textContent = '⏸';
            playing = true;
            playInterval = setInterval(() => {
              currentFrame = (currentFrame + 1) % frames.length;
              showFrame(currentFrame);
            }, 1000 / fps) as unknown as number;
          }
        });

        shadow.getElementById('__download__')?.addEventListener('click', () => {
          const a = document.createElement('a');
          a.href = frames[0];
          a.download = `recording_${Date.now()}.jpg`;
          a.click();
        });
      }
    },
    args: [frames, fps, durationMs],
  });
}

chrome.webNavigation.onCompleted.addListener(async details => {
  if (details.frameId !== 0 || details.url.startsWith('chrome://')) return;

  if (recording?.isActive && details.tabId === recording.tabId) {
    sendSessionEvent({
      navigate: {
        url: details.url,
        from: null,
        type: 'page_load',
        ts: Date.now(),
      },
    });
    await showRecordingIndicator(details.tabId);
  }
});

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  switch (message.type) {
    case 'execute_local':
      executeCommand(message.command).then(sendResponse);
      return true;

    case 'get_status':
      Promise.all([checkDaemonConnection(), getSessionId()]).then(([daemonConnected, sid]) => {
        sendResponse({
          recording: recording?.isActive ?? false,
          recording_id: recording?.id ?? null,
          tracing: tracing?.isActive ?? false,
          trace_id: tracing?.id ?? null,
          daemonConnected,
          sessionId: sid
        });
      });
      return true;

    case 'capture_screenshot':
      (async () => {
        try {
          const dataUrl = await chrome.tabs.captureVisibleTab({ format: 'png' });
          sendResponse({ dataUrl });
        } catch {
          sendResponse({ dataUrl: null });
        }
      })();
      return true;

    case 'capture_element_screenshot':
      getActiveTab().then(async tab => {
        if (tab?.id) {
          const result = await captureScreenshot({
            tabId: tab.id,
            bounds: message.bounds,
            elementInfo: message.elementInfo,
          });
          sendResponse(result);
        } else {
          sendResponse({ error: 'No active tab' });
        }
      });
      return true;

    case 'user_action':
      sendSessionEvent(message.action);
      return false;

    default:
      // Unknown message type - do not indicate async response
      return false;
  }
});

initWebSocket();

export {};
