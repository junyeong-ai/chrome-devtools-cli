type WsMessage =
  | { type: 'event'; session_id: string; event: object }
  | { type: 'recording'; session_id: string; action: string; recording_id: string }
  | { type: 'trace'; session_id: string; action: string; trace_id: string }
  | { type: 'ping' }
  | { type: 'pong' };

interface ConnectionOptions {
  url: string;
  reconnectDelay?: number;
  maxReconnectDelay?: number;
  pingInterval?: number;
  maxQueueSize?: number;
}

type ConnectionState = 'connecting' | 'connected' | 'disconnected' | 'reconnecting';

export class DaemonConnection {
  private ws: WebSocket | null = null;
  private url: string;
  private reconnectDelay: number;
  private maxReconnectDelay: number;
  private pingInterval: number;
  private maxQueueSize: number;
  private currentDelay: number;
  private messageQueue: WsMessage[] = [];
  private pingTimer: ReturnType<typeof setInterval> | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private state: ConnectionState = 'disconnected';
  private sessionId: string | null = null;
  private listeners: Map<string, Set<(data: unknown) => void>> = new Map();

  constructor(options: ConnectionOptions) {
    this.url = options.url;
    this.reconnectDelay = options.reconnectDelay ?? 1000;
    this.maxReconnectDelay = options.maxReconnectDelay ?? 30000;
    this.pingInterval = options.pingInterval ?? 30000;
    this.maxQueueSize = options.maxQueueSize ?? 1000;
    this.currentDelay = this.reconnectDelay;
  }

  connect(sessionId: string): void {
    if (this.state === 'connected' || this.state === 'connecting') {
      if (this.sessionId === sessionId) {
        return;
      }
      this.cleanup();
    }

    this.sessionId = sessionId;
    this.state = 'connecting';
    this.attemptConnection();
  }

  disconnect(): void {
    this.state = 'disconnected';
    this.cleanup();
  }

  send(message: WsMessage): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
    } else if (this.messageQueue.length < this.maxQueueSize) {
      this.messageQueue.push(message);
    }
  }

  sendEvent(event: object): void {
    if (!this.sessionId) return;
    this.send({ type: 'event', session_id: this.sessionId, event });
  }

  on(event: string, callback: (data: unknown) => void): () => void {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, new Set());
    }
    this.listeners.get(event)!.add(callback);
    return () => this.listeners.get(event)?.delete(callback);
  }

  getState(): ConnectionState {
    return this.state;
  }

  private attemptConnection(): void {
    if (this.state === 'disconnected') return;

    try {
      this.ws = new WebSocket(this.url);

      this.ws.onopen = () => {
        this.state = 'connected';
        this.currentDelay = this.reconnectDelay;
        this.flushQueue();
        this.startPing();
        this.emit('connected', null);
      };

      this.ws.onmessage = (event) => {
        try {
          const message = JSON.parse(event.data) as WsMessage;
          this.handleMessage(message);
        } catch {}
      };

      this.ws.onclose = () => {
        this.stopPing();
        if (this.state !== 'disconnected') {
          this.scheduleReconnect();
        }
      };

      this.ws.onerror = () => {
        this.stopPing();
        if (this.state !== 'disconnected') {
          this.scheduleReconnect();
        }
      };
    } catch {
      this.scheduleReconnect();
    }
  }

  private handleMessage(message: WsMessage): void {
    switch (message.type) {
      case 'ping':
        this.send({ type: 'pong' });
        break;
      case 'pong':
        break;
      case 'event':
        this.emit('event', message.event);
        break;
      case 'recording':
        this.emit('recording', { action: message.action, recording_id: message.recording_id });
        break;
      case 'trace':
        this.emit('trace', { action: message.action, trace_id: message.trace_id });
        break;
    }
  }

  private emit(event: string, data: unknown): void {
    this.listeners.get(event)?.forEach((callback) => {
      try {
        callback(data);
      } catch {}
    });
  }

  private flushQueue(): void {
    while (this.messageQueue.length > 0 && this.ws?.readyState === WebSocket.OPEN) {
      const message = this.messageQueue.shift()!;
      this.ws.send(JSON.stringify(message));
    }
  }

  private startPing(): void {
    this.stopPing();
    this.pingTimer = setInterval(() => {
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.send({ type: 'ping' });
      }
    }, this.pingInterval);
  }

  private stopPing(): void {
    if (this.pingTimer) {
      clearInterval(this.pingTimer);
      this.pingTimer = null;
    }
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer) return;

    this.state = 'reconnecting';
    this.emit('reconnecting', { delay: this.currentDelay });

    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.attemptConnection();
    }, this.currentDelay);

    this.currentDelay = Math.min(this.currentDelay * 2, this.maxReconnectDelay);
  }

  private cleanup(): void {
    this.stopPing();

    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }

    if (this.ws) {
      this.ws.onopen = null;
      this.ws.onmessage = null;
      this.ws.onclose = null;
      this.ws.onerror = null;
      if (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING) {
        this.ws.close();
      }
      this.ws = null;
    }

    this.currentDelay = this.reconnectDelay;
  }
}

let connection: DaemonConnection | null = null;

export function getConnection(): DaemonConnection {
  if (!connection) {
    connection = new DaemonConnection({
      url: 'ws://127.0.0.1:9223/ws',
      reconnectDelay: 1000,
      maxReconnectDelay: 30000,
      pingInterval: 30000,
      maxQueueSize: 1000,
    });
  }
  return connection;
}
