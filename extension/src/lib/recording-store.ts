import { openDB, type IDBPDatabase, type DBSchema } from 'idb';

interface RecordingFrame {
  recordingId: string;
  index: number;
  data: string;
  offsetMs: number;
  timestamp: number;
}

interface RecordingMeta {
  id: string;
  tabId: number;
  windowId: number;
  fps: number;
  quality: number;
  dpr: number;
  startTime: number;
  endTime?: number;
  frameCount: number;
  isActive: boolean;
}

interface RecordingStoreSchema extends DBSchema {
  recordings: {
    key: string;
    value: RecordingMeta;
    indexes: { 'by-time': number };
  };
  frames: {
    key: [string, number];
    value: RecordingFrame;
    indexes: { 'by-recording': string };
  };
}

const DB_NAME = 'chrome-devtools-recordings';
const DB_VERSION = 1;
const MAX_RECORDING_AGE_MS = 24 * 60 * 60 * 1000;

let dbInstance: IDBPDatabase<RecordingStoreSchema> | null = null;

async function getDB(): Promise<IDBPDatabase<RecordingStoreSchema>> {
  if (dbInstance) return dbInstance;

  dbInstance = await openDB<RecordingStoreSchema>(DB_NAME, DB_VERSION, {
    upgrade(db) {
      if (!db.objectStoreNames.contains('recordings')) {
        const recordingStore = db.createObjectStore('recordings', { keyPath: 'id' });
        recordingStore.createIndex('by-time', 'startTime');
      }

      if (!db.objectStoreNames.contains('frames')) {
        const frameStore = db.createObjectStore('frames', { keyPath: ['recordingId', 'index'] });
        frameStore.createIndex('by-recording', 'recordingId');
      }
    },
  });

  return dbInstance;
}

export async function createRecording(meta: RecordingMeta): Promise<void> {
  const db = await getDB();
  await db.put('recordings', meta);
}

export async function updateRecording(meta: Partial<RecordingMeta> & { id: string }): Promise<void> {
  const db = await getDB();
  const existing = await db.get('recordings', meta.id);
  if (existing) {
    await db.put('recordings', { ...existing, ...meta });
  }
}

export async function getRecording(id: string): Promise<RecordingMeta | undefined> {
  const db = await getDB();
  return db.get('recordings', id);
}

export async function getActiveRecording(): Promise<RecordingMeta | undefined> {
  const db = await getDB();
  const all = await db.getAll('recordings');
  return all.find(r => r.isActive);
}

export async function saveFrame(frame: RecordingFrame): Promise<void> {
  const db = await getDB();
  await db.put('frames', frame);
}

export async function getFrames(recordingId: string): Promise<RecordingFrame[]> {
  const db = await getDB();
  const index = db.transaction('frames').store.index('by-recording');
  const frames = await index.getAll(recordingId);
  return frames.sort((a, b) => a.index - b.index);
}

export async function getFrameCount(recordingId: string): Promise<number> {
  const db = await getDB();
  const index = db.transaction('frames').store.index('by-recording');
  return index.count(recordingId);
}

export async function deleteRecording(id: string): Promise<void> {
  const db = await getDB();
  const tx = db.transaction(['recordings', 'frames'], 'readwrite');

  const frameIndex = tx.objectStore('frames').index('by-recording');
  const frameKeys = await frameIndex.getAllKeys(id);
  for (const key of frameKeys) {
    await tx.objectStore('frames').delete(key);
  }

  await tx.objectStore('recordings').delete(id);
  await tx.done;
}

export async function cleanOldRecordings(): Promise<number> {
  const db = await getDB();
  const cutoff = Date.now() - MAX_RECORDING_AGE_MS;

  const index = db.transaction('recordings').store.index('by-time');
  const oldRecordings = await index.getAll(IDBKeyRange.upperBound(cutoff));

  let deleted = 0;
  for (const recording of oldRecordings) {
    if (!recording.isActive) {
      await deleteRecording(recording.id);
      deleted++;
    }
  }

  return deleted;
}

export async function getRecordingWithFrames(
  id: string
): Promise<{ meta: RecordingMeta; frames: string[] } | null> {
  const meta = await getRecording(id);
  if (!meta) return null;

  const frames = await getFrames(id);
  return {
    meta,
    frames: frames.map(f => f.data),
  };
}
