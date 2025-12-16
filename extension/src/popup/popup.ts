const API_BASE = 'http://127.0.0.1:9223';

const selectBtn = document.getElementById('select') as HTMLButtonElement;
const recordBtn = document.getElementById('record') as HTMLButtonElement;
const traceBtn = document.getElementById('trace') as HTMLButtonElement;
const screenshotBtn = document.getElementById('screenshot') as HTMLButtonElement;
const statusEl = document.getElementById('status') as HTMLElement;
const statusText = document.getElementById('status-text') as HTMLElement;

let isRecording = false;
let isTracing = false;

function updateRecordButton(recording: boolean): void {
  isRecording = recording;
  if (recording) {
    recordBtn.innerHTML = '<span class="icon">⏹️</span> Stop Recording';
    recordBtn.classList.add('recording');
  } else {
    recordBtn.innerHTML = '<span class="icon">⏺️</span> Start Recording';
    recordBtn.classList.remove('recording');
  }
}

function updateTraceButton(tracing: boolean): void {
  isTracing = tracing;
  if (tracing) {
    traceBtn.innerHTML = '<span class="icon">⏹️</span> Stop Trace';
    traceBtn.classList.add('tracing');
  } else {
    traceBtn.innerHTML = '<span class="icon">📊</span> Start Trace';
    traceBtn.classList.remove('tracing');
  }
}

function updateConnectionStatus(connected: boolean, sessionId?: string): void {
  if (connected) {
    statusEl.classList.remove('disconnected');
    statusEl.classList.add('connected');
    statusText.textContent = sessionId ? 'Connected' : 'Connected';
  } else {
    statusEl.classList.remove('connected');
    statusEl.classList.add('disconnected');
    statusText.textContent = 'Standalone';
  }
}

async function checkConnection(): Promise<{ connected: boolean; sessionId?: string }> {
  try {
    const response = await fetch(`${API_BASE}/api/session`);
    if (response.ok) {
      const data = await response.json();
      if (data.ok && data.session_id) {
        return { connected: true, sessionId: data.session_id };
      }
    }
  } catch {
    // Connection failed
  }
  return { connected: false };
}

selectBtn.addEventListener('click', async () => {
  await chrome.runtime.sendMessage({
    type: 'execute_local',
    command: { type: 'request_selection', mode: 'single' },
  });
  window.close();
});

recordBtn.addEventListener('click', async () => {
  if (!isRecording) {
    updateRecordButton(true);
    await chrome.runtime.sendMessage({
      type: 'execute_local',
      command: { type: 'start_recording', fps: 5, quality: 70, trackActions: true },
    });
    window.close();
  } else {
    await chrome.runtime.sendMessage({
      type: 'execute_local',
      command: { type: 'stop_recording' },
    });
    updateRecordButton(false);
    window.close();
  }
});

traceBtn.addEventListener('click', async () => {
  if (!isTracing) {
    updateTraceButton(true);
    await chrome.runtime.sendMessage({
      type: 'execute_local',
      command: { type: 'start_trace' },
    });
    window.close();
  } else {
    await chrome.runtime.sendMessage({
      type: 'execute_local',
      command: { type: 'stop_trace' },
    });
    updateTraceButton(false);
    window.close();
  }
});

screenshotBtn.addEventListener('click', async () => {
  await chrome.runtime.sendMessage({
    type: 'execute_local',
    command: { type: 'take_screenshot' },
  });
  window.close();
});

// Initialize: check connection directly via HTTP API
(async () => {
  // Check daemon connection directly
  const { connected, sessionId } = await checkConnection();
  updateConnectionStatus(connected, sessionId);

  // Get recording/tracing state from service worker
  const swStatus = await chrome.runtime.sendMessage({ type: 'get_status' });
  if (swStatus?.recording) {
    updateRecordButton(true);
  }
  if (swStatus?.tracing) {
    updateTraceButton(true);
  }
})();

export {};
