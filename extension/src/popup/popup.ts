const selectBtn = document.getElementById('select') as HTMLButtonElement;
const recordBtn = document.getElementById('record') as HTMLButtonElement;
const screenshotBtn = document.getElementById('screenshot') as HTMLButtonElement;
const statusEl = document.getElementById('status') as HTMLElement;
const statusText = document.getElementById('status-text') as HTMLElement;

let isRecording = false;

function updateRecordButton(recording: boolean): void {
  isRecording = recording;
  if (recording) {
    recordBtn.innerHTML = '<span class="icon">⏹</span> Stop Recording';
    recordBtn.classList.add('recording');
  } else {
    recordBtn.innerHTML = '<span class="icon">⏺</span> Start Recording';
    recordBtn.classList.remove('recording');
  }
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

screenshotBtn.addEventListener('click', async () => {
  await chrome.runtime.sendMessage({
    type: 'execute_local',
    command: { type: 'take_screenshot' },
  });
  window.close();
});

chrome.runtime.sendMessage({ type: 'get_status' }).then(response => {
  if (response?.recording) {
    updateRecordButton(true);
  }

  if (response?.daemonConnected) {
    statusEl.classList.remove('disconnected');
    statusEl.classList.add('connected');
    statusText.textContent = 'Connected';
  }
});
