const API_BASE = 'http://127.0.0.1:9223';
const MIN_CAPTURE_INTERVAL = 1000;
const RETRY_DELAY = 300;
const MAX_RETRIES = 3;

let sessionId: string | null = null;
let recording: RecordingState | null = null;
let recordingInterval: ReturnType<typeof setInterval> | null = null;
let lastCaptureTime = 0;

interface RecordingState {
  sessionId: string;
  tabId: number;
  windowId: number;
  isActive: boolean;
  fps: number;
  quality: number;
  dpr: number;
  frameCount: number;
  startTime: number;
  frames: string[];
}

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

async function loadSessionId(): Promise<string | null> {
  try {
    const url = chrome.runtime.getURL('session.json');
    const response = await fetch(url);
    if (response.ok) {
      const data = await response.json();
      return data.session_id || null;
    }
  } catch {}
  return null;
}

async function getSessionId(): Promise<string | null> {
  if (!sessionId) {
    sessionId = await loadSessionId();
  }
  return sessionId;
}

async function getActiveTab(): Promise<chrome.tabs.Tab | null> {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  return tab?.id ? tab : null;
}

async function sendToContent(tabId: number, message: object): Promise<unknown> {
  try {
    return await chrome.tabs.sendMessage(tabId, message);
  } catch {
    return null;
  }
}

async function sendToContentWithRetry(tabId: number, message: object): Promise<boolean> {
  for (let i = 0; i < MAX_RETRIES; i++) {
    try {
      await chrome.tabs.sendMessage(tabId, message);
      return true;
    } catch {
      if (i < MAX_RETRIES - 1) {
        await new Promise(r => setTimeout(r, RETRY_DELAY));
      }
    }
  }
  return false;
}

async function api(endpoint: string, data: object): Promise<{ ok: boolean; error?: string }> {
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

async function sendEvent(event: object): Promise<boolean> {
  const result = await api('/api/events', { event });
  return result.ok;
}

async function sendFrame(index: number, data: string): Promise<boolean> {
  const result = await api('/api/frames', { index, data });
  return result.ok;
}

async function sendScreenshot(data: string): Promise<boolean> {
  const result = await api('/api/screenshots', { data });
  return result.ok;
}

async function checkDaemonConnection(): Promise<boolean> {
  try {
    const response = await fetch(`${API_BASE}/api/health`);
    const data = await response.json();
    return data.ok === true;
  } catch {
    return false;
  }
}

async function waitForCaptureSlot(): Promise<void> {
  const now = Date.now();
  const elapsed = now - lastCaptureTime;
  if (elapsed < MIN_CAPTURE_INTERVAL) {
    await new Promise(r => setTimeout(r, MIN_CAPTURE_INTERVAL - elapsed));
  }
  lastCaptureTime = Date.now();
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
        (cmd.quality as number) || 70,
        (cmd.trackActions as boolean) ?? true
      );

    case 'stop_recording':
      return await stopRecording();

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

  await waitForCaptureSlot();

  try {
    const tab = await chrome.tabs.get(tabId);
    const dataUrl = await chrome.tabs.captureVisibleTab(tab.windowId, { format: 'png' });
    if (!dataUrl) return { error: 'Capture failed' };

    const finalDataUrl = bounds ? await cropImage(dataUrl, bounds) : dataUrl;
    sendScreenshot(finalDataUrl);
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
  quality: number,
  trackActions: boolean
): Promise<object> {
  if (recording?.isActive) {
    return { error: 'Already recording' };
  }

  const dpr = await getDpr(tabId);

  recording = {
    sessionId: crypto.randomUUID(),
    tabId,
    windowId,
    isActive: true,
    fps,
    quality,
    dpr,
    frameCount: 0,
    startTime: Date.now(),
    frames: [],
  };

  await sendEvent({ recording: { type: 'start', fps } });

  if (trackActions) {
    await sendToContent(tabId, { type: 'start_recording' });
  }

  await showRecordingIndicator(tabId);

  const captureInterval = Math.max(1000 / fps, MIN_CAPTURE_INTERVAL);

  recordingInterval = setInterval(async () => {
    if (!recording?.isActive) {
      if (recordingInterval) {
        clearInterval(recordingInterval);
        recordingInterval = null;
      }
      return;
    }

    const now = Date.now();
    if (now - lastCaptureTime < MIN_CAPTURE_INTERVAL) return;
    lastCaptureTime = now;

    try {
      const dataUrl = await chrome.tabs.captureVisibleTab(recording.windowId, {
        format: 'jpeg',
        quality: recording.quality,
      });

      if (recording?.isActive && dataUrl) {
        const resized = await resizeToLogicalPixels(dataUrl, recording.quality, recording.dpr);
        recording.frames.push(resized);
        recording.frameCount++;
        sendFrame(recording.frameCount - 1, resized);
      }
    } catch {}
  }, captureInterval);

  return { success: true, sessionId: recording.sessionId };
}

async function stopRecording(): Promise<object> {
  if (!recording) {
    return { error: 'Not recording' };
  }

  const { tabId, sessionId: recSessionId, frameCount, startTime, frames, fps } = recording;
  recording.isActive = false;

  if (recordingInterval) {
    clearInterval(recordingInterval);
    recordingInterval = null;
  }

  await hideRecordingIndicator(tabId);
  await sendToContent(tabId, { type: 'stop_recording' });

  const durationMs = Date.now() - startTime;
  await sendEvent({ recording: { type: 'stop', frames: frameCount, ms: durationMs } });
  showRecordingPreview(tabId, frames, fps, durationMs);

  recording = null;

  return { sessionId: recSessionId, frameCount, durationMs };
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

      const overlay = document.createElement('div');
      overlay.id = '__cdtcli_dialog__';
      overlay.style.cssText =
        'position:fixed;inset:0;background:rgba(0,0,0,0.6);z-index:2147483647;display:flex;align-items:center;justify-content:center';

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

      overlay.innerHTML = `
        <div style="background:#fff;border-radius:8px;max-width:90vw;max-height:90vh;box-shadow:0 4px 24px rgba(0,0,0,0.2);font-family:system-ui,sans-serif;display:flex;flex-direction:column;overflow:hidden">
          <div style="padding:12px 16px;border-bottom:1px solid #e0e0e0;display:flex;justify-content:space-between;align-items:center;background:#fafafa">
            <span style="font-size:14px;font-weight:500;color:#333">${elementInfo ? 'Element Screenshot' : 'Screenshot'}</span>
            <button id="__close__" style="width:28px;height:28px;border:none;background:transparent;cursor:pointer;font-size:18px;color:#666;display:flex;align-items:center;justify-content:center;border-radius:4px" onmouseover="this.style.background='#eee'" onmouseout="this.style.background='transparent'">×</button>
          </div>
          ${infoSection}
          <div style="padding:16px;overflow:auto;flex:1;background:#f0f0f0;display:flex;align-items:center;justify-content:center">
            <img src="${dataUrl}" style="max-width:100%;max-height:60vh;box-shadow:0 2px 8px rgba(0,0,0,0.15)" />
          </div>
          <div style="padding:12px 16px;border-top:1px solid #e0e0e0;display:flex;gap:8px;justify-content:flex-end;background:#fafafa">
            <button id="__copy__" style="padding:8px 16px;background:#fff;border:1px solid #ccc;border-radius:4px;cursor:pointer;font-size:13px">Copy Image</button>
            <button id="__download__" style="padding:8px 16px;background:#1a73e8;color:white;border:none;border-radius:4px;cursor:pointer;font-size:13px">Download</button>
          </div>
        </div>
      `;

      document.body.appendChild(overlay);
      overlay.addEventListener('click', e => e.stopPropagation(), true);

      const close = () => overlay.remove();
      overlay.querySelector('#__close__')?.addEventListener('click', close);
      overlay.addEventListener('click', e => e.target === overlay && close());

      overlay.querySelector('#__copy_selector__')?.addEventListener('click', () => {
        const input = overlay.querySelector('#__selector_input__') as HTMLInputElement;
        navigator.clipboard.writeText(input.value);
        (overlay.querySelector('#__copy_selector__') as HTMLButtonElement).textContent = 'Copied!';
        setTimeout(() => {
          const btn = overlay.querySelector('#__copy_selector__') as HTMLButtonElement;
          if (btn) btn.textContent = 'Copy';
        }, 1500);
      });

      overlay.querySelector('#__copy__')?.addEventListener('click', async () => {
        try {
          const res = await fetch(dataUrl);
          const blob = await res.blob();
          await navigator.clipboard.write([new ClipboardItem({ [blob.type]: blob })]);
          (overlay.querySelector('#__copy__') as HTMLButtonElement).textContent = 'Copied!';
          setTimeout(() => {
            const btn = overlay.querySelector('#__copy__') as HTMLButtonElement;
            if (btn) btn.textContent = 'Copy Image';
          }, 1500);
        } catch {}
      });

      overlay.querySelector('#__download__')?.addEventListener('click', () => {
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

      const overlay = document.createElement('div');
      overlay.id = '__cdtcli_dialog__';
      overlay.style.cssText =
        'position:fixed;inset:0;background:rgba(0,0,0,0.6);z-index:2147483647;display:flex;align-items:center;justify-content:center';

      const durationSec = (durationMs / 1000).toFixed(1);
      const hasFrames = frames.length > 0;

      const playerSection = hasFrames
        ? `<img id="__player__" style="max-width:100%;max-height:100%" />`
        : `<div style="color:#999;font-size:14px">No frames captured</div>`;

      const controlsSection = hasFrames
        ? `<div style="display:flex;align-items:center;gap:12px;padding:12px 16px;border-top:1px solid #e0e0e0;background:#fafafa">
            <button id="__play__" style="width:36px;height:36px;border:none;background:#1a73e8;color:white;border-radius:50%;cursor:pointer;font-size:14px;display:flex;align-items:center;justify-content:center">▶</button>
            <input id="__slider__" type="range" min="0" max="${frames.length - 1}" value="0" style="flex:1" />
            <span id="__time__" style="font-size:12px;color:#666;min-width:70px;text-align:right">1 / ${frames.length}</span>
          </div>`
        : '';

      overlay.innerHTML = `
        <div style="background:#fff;border-radius:8px;max-width:800px;width:90%;box-shadow:0 4px 24px rgba(0,0,0,0.2);font-family:system-ui,sans-serif;overflow:hidden">
          <div style="padding:12px 16px;border-bottom:1px solid #e0e0e0;display:flex;justify-content:space-between;align-items:center;background:#fafafa">
            <span style="font-size:14px;font-weight:500;color:#333">Recording Complete</span>
            <button id="__close__" style="width:28px;height:28px;border:none;background:transparent;cursor:pointer;font-size:18px;color:#666;display:flex;align-items:center;justify-content:center;border-radius:4px">×</button>
          </div>
          <div style="background:#1a1a1a;aspect-ratio:16/9;display:flex;align-items:center;justify-content:center">
            ${playerSection}
          </div>
          ${controlsSection}
          <div style="display:flex;border-top:1px solid #e0e0e0">
            <div style="flex:1;padding:16px;text-align:center;border-right:1px solid #e0e0e0">
              <div style="font-size:24px;font-weight:600;color:#333">${frames.length}</div>
              <div style="font-size:12px;color:#666;margin-top:4px">Frames</div>
            </div>
            <div style="flex:1;padding:16px;text-align:center">
              <div style="font-size:24px;font-weight:600;color:#333">${durationSec}s</div>
              <div style="font-size:12px;color:#666;margin-top:4px">Duration</div>
            </div>
          </div>
          <div style="padding:12px 16px;border-top:1px solid #e0e0e0;display:flex;gap:8px;justify-content:flex-end;background:#fafafa">
            <button id="__download__" style="padding:8px 16px;background:${hasFrames ? '#1a73e8' : '#ccc'};color:white;border:none;border-radius:4px;cursor:${hasFrames ? 'pointer' : 'not-allowed'};font-size:13px" ${!hasFrames ? 'disabled' : ''}>Download</button>
          </div>
        </div>
      `;

      document.body.appendChild(overlay);
      overlay.addEventListener('click', e => e.stopPropagation(), true);

      const close = () => overlay.remove();
      overlay.querySelector('#__close__')?.addEventListener('click', close);
      overlay.addEventListener('click', e => e.target === overlay && close());

      if (hasFrames) {
        const player = overlay.querySelector('#__player__') as HTMLImageElement;
        const slider = overlay.querySelector('#__slider__') as HTMLInputElement;
        const timeDisplay = overlay.querySelector('#__time__') as HTMLElement;
        const playBtn = overlay.querySelector('#__play__') as HTMLButtonElement;

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

        overlay.querySelector('#__download__')?.addEventListener('click', () => {
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
    await new Promise(r => setTimeout(r, 500));
    await sendToContentWithRetry(details.tabId, { type: 'start_recording' });
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
          daemonConnected,
          sessionId: sid
        });
      });
      return true;

    case 'capture_screenshot':
      (async () => {
        await waitForCaptureSlot();
        try {
          const dataUrl = await chrome.tabs.captureVisibleTab(undefined, { format: 'png' });
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
      sendEvent(message.action);
      return false;
  }

  return true;
});
