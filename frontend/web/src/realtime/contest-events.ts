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

export function subscribeContestEvents(options: ContestRealtimeOptions): ContestRealtimeSubscription {
  const pollIntervalMs = options.pollIntervalMs ?? 10_000;
  const path = options.scope === 'TEAM'
    ? `/api/team/events/contests/${options.contestId}`
    : options.scope === 'PUBLIC'
      ? `/api/public/events/contests/${options.contestId}`
      : `/api/events/contests/${options.contestId}`;
  let source: EventSource | null = null;
  let pollingTimer: number | undefined;
  let stopped = false;

  const stopPolling = () => {
    if (pollingTimer !== undefined) {
      window.clearInterval(pollingTimer);
      pollingTimer = undefined;
    }
  };

  const runPoll = () => {
    if (!document.hidden) void options.poll();
  };

  const startPolling = () => {
    if (pollingTimer !== undefined || stopped) return;
    runPoll();
    pollingTimer = window.setInterval(runPoll, pollIntervalMs);
  };

  const handleMessage = (message: MessageEvent<string>) => {
    try {
      const event = JSON.parse(message.data) as RealtimeEvent;
      if (
        event.version !== 1
        || event.contestId !== options.contestId
        || event.scope !== options.scope
      ) return;
      if (event.type === 'CONNECTED') {
        stopPolling();
        options.onConnectionChange?.(true);
        return;
      }
      if (options.eventTypes.includes(event.type)) options.onEvent(event);
    } catch {
      startPolling();
    }
  };

  const handleVisibility = () => {
    if (!document.hidden && pollingTimer !== undefined) runPoll();
  };
  document.addEventListener('visibilitychange', handleVisibility);

  if (typeof EventSource === 'undefined') {
    startPolling();
  } else {
    source = new EventSource(path);
    source.addEventListener('message', handleMessage as EventListener);
    source.addEventListener('error', () => {
      if (stopped) return;
      options.onConnectionChange?.(false);
      startPolling();
    });
  }

  return {
    stop() {
      stopped = true;
      source?.close();
      source = null;
      stopPolling();
      document.removeEventListener('visibilitychange', handleVisibility);
    },
  };
}
