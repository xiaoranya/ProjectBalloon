import { apiRequest } from '../api/client';

export type RealtimeScope = 'PUBLIC' | 'STAFF' | 'TEAM';

export interface RealtimeEvent {
  id: string;
  version: number;
  type: string;
  scope: RealtimeScope;
  contestId: number;
  occurredAt: string;
  payload: Record<string, unknown>;
}

export interface ContestRealtimeOptions {
  contestId: number;
  scope: RealtimeScope;
  eventTypes: readonly string[];
  onEvent: (event: RealtimeEvent) => void;
  onConnectionChange?: (connected: boolean) => void;
  poll: () => void | Promise<void>;
  pollIntervalMs?: number;
}

export interface ContestRealtimeSubscription {
  stop(): void;
}

const DEFAULT_POLL_INTERVAL_MS = 10_000;
const MAX_POLL_INTERVAL_MS = 60_000;
const MAX_SEEN_EVENT_IDS = 500;
const MIN_RECONNECT_DELAY_MS = 1_000;

export function subscribeContestEvents(
  options: ContestRealtimeOptions,
): ContestRealtimeSubscription {
  const basePollIntervalMs = options.pollIntervalMs ?? DEFAULT_POLL_INTERVAL_MS;
  const path =
    options.scope === 'TEAM'
      ? `/api/team/events/contests/${options.contestId}`
      : options.scope === 'PUBLIC'
        ? `/api/public/events/contests/${options.contestId}`
        : `/api/events/contests/${options.contestId}`;
  let source: EventSource | null = null;
  let pollingTimer: number | undefined;
  let reconnectTimer: number | undefined;
  let polling = false;
  let pollInFlight = false;
  let pollIntervalMs = basePollIntervalMs;
  let authProbeInFlight = false;
  let stopped = false;
  // Last domain event id seen on the stream; the server replays outbox events
  // after it on the next connection (?lastEventId=; the browser's native
  // Last-Event-ID header only covers automatic reconnects).
  let lastEventId: string | null = null;
  const seenEventIds = new Set<string>();
  const seenEventIdOrder: string[] = [];

  const stopPolling = () => {
    if (pollingTimer !== undefined) {
      window.clearTimeout(pollingTimer);
      pollingTimer = undefined;
    }
    polling = false;
  };

  const runPoll = () => {
    if (stopped || !polling || document.hidden || pollInFlight) return;
    pollInFlight = true;
    try {
      const result = options.poll();
      if (result && typeof result.then === 'function') {
        void result.then(
          () => {
            pollInFlight = false;
          },
          () => {
            pollInFlight = false;
          },
        );
      } else {
        pollInFlight = false;
      }
    } catch {
      pollInFlight = false;
    }
  };

  const scheduleNextPoll = (delayMs: number) => {
    pollingTimer = window.setTimeout(() => {
      pollingTimer = undefined;
      runPoll();
      if (polling) scheduleNextPoll(delayMs);
    }, delayMs);
  };

  const startPolling = () => {
    if (polling || stopped) return;
    polling = true;
    runPoll();
    scheduleNextPoll(pollIntervalMs);
  };

  // SSE failure: keep fallback polling alive with exponential backoff so a
  // long broker/API outage does not hammer the server, and probe the session
  // once so a 401 reaches the shared unauthorized handler (which owns the
  // redirect to login).
  const handleConnectionLost = () => {
    if (stopped) return;
    options.onConnectionChange?.(false);
    if (polling) {
      if (pollingTimer !== undefined) window.clearTimeout(pollingTimer);
      scheduleNextPoll(pollIntervalMs);
    } else {
      startPolling();
    }
    pollIntervalMs = Math.min(pollIntervalMs * 2, MAX_POLL_INTERVAL_MS);
  };

  const probeSession = () => {
    if (authProbeInFlight || stopped) return;
    authProbeInFlight = true;
    void apiRequest('/api/auth/me')
      .catch(() => {})
      .finally(() => {
        authProbeInFlight = false;
      });
  };

  const isDuplicate = (id: string) => {
    if (seenEventIds.has(id)) return true;
    seenEventIds.add(id);
    seenEventIdOrder.push(id);
    if (seenEventIdOrder.length > MAX_SEEN_EVENT_IDS) {
      const oldest = seenEventIdOrder.shift();
      if (oldest !== undefined) seenEventIds.delete(oldest);
    }
    return false;
  };

  const handleMessage = (message: MessageEvent<string>) => {
    if (stopped) return;
    try {
      const event = JSON.parse(message.data) as RealtimeEvent;
      if (
        event.version !== 1 ||
        event.contestId !== options.contestId ||
        event.scope !== options.scope
      )
        return;
      pollIntervalMs = basePollIntervalMs;
      if (event.type === 'CONNECTED') {
        stopPolling();
        options.onConnectionChange?.(true);
        return;
      }
      // Track the resume position before any filtering: even events outside
      // eventTypes advance the stream, and the id must follow it.
      if (event.id) lastEventId = event.id;
      if (options.eventTypes.includes(event.type)) {
        if (event.id && isDuplicate(event.id)) return;
        options.onEvent(event);
      }
    } catch {
      handleConnectionLost();
    }
  };

  const handleVisibility = () => {
    if (!document.hidden && pollingTimer !== undefined) runPoll();
  };
  document.addEventListener('visibilitychange', handleVisibility);

  const connect = () => {
    if (stopped) return;
    const url =
      lastEventId !== null ? `${path}?lastEventId=${encodeURIComponent(lastEventId)}` : path;
    source = new EventSource(url);
    source.addEventListener('message', handleMessage as EventListener);
    source.addEventListener('error', () => {
      handleConnectionLost();
      probeSession();
      // Take over reconnection pacing from the browser: close the broken
      // source and reopen it (with the resume id) once the current backoff
      // elapses, so a long outage neither hammers the server nor leaves a
      // half-dead stream attached.
      if (stopped || reconnectTimer !== undefined) return;
      source?.close();
      source = null;
      reconnectTimer = window.setTimeout(
        () => {
          reconnectTimer = undefined;
          connect();
        },
        Math.max(MIN_RECONNECT_DELAY_MS, pollIntervalMs),
      );
    });
  };

  if (typeof EventSource === 'undefined') {
    startPolling();
  } else {
    connect();
  }

  return {
    stop() {
      stopped = true;
      source?.close();
      source = null;
      stopPolling();
      if (reconnectTimer !== undefined) {
        window.clearTimeout(reconnectTimer);
        reconnectTimer = undefined;
      }
      seenEventIds.clear();
      seenEventIdOrder.length = 0;
      document.removeEventListener('visibilitychange', handleVisibility);
    },
  };
}
