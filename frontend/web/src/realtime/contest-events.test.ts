import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { subscribeContestEvents } from './contest-events';

const mocks = vi.hoisted(() => ({ apiRequest: vi.fn() }));
vi.mock('../api/client', () => ({ apiRequest: mocks.apiRequest }));

class EventSourceMock extends EventTarget {
  static instances: EventSourceMock[] = [];
  readonly url: string;
  close = vi.fn();

  constructor(url: string | URL) {
    super();
    this.url = String(url);
    EventSourceMock.instances.push(this);
  }

  message(data: unknown) {
    this.dispatchEvent(new MessageEvent('message', { data: JSON.stringify(data) }));
  }

  fail() {
    this.dispatchEvent(new Event('error'));
  }
}

const baseEvent = {
  id: '00000000-0000-4000-8000-000000000001',
  version: 1,
  scope: 'TEAM',
  contestId: 7,
  occurredAt: '2026-07-20T08:00:00Z',
  payload: {},
};

describe('Rust contest SSE client', () => {
  beforeEach(() => {
    EventSourceMock.instances = [];
    vi.stubGlobal('EventSource', EventSourceMock);
    vi.useFakeTimers();
    // Pin the backoff jitter so exact interval assertions are deterministic.
    vi.spyOn(Math, 'random').mockReturnValue(0.5);
    mocks.apiRequest.mockClear();
    mocks.apiRequest.mockResolvedValue(undefined);
  });

  afterEach(() => vi.useRealTimers());

  it('uses the team endpoint and reads domain names from data.type on event: message', () => {
    const onEvent = vi.fn();
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: ['CLARIFICATION_UPDATED'],
      onEvent,
      poll: vi.fn(),
    });

    const source = EventSourceMock.instances[0];
    expect(source.url).toBe('/api/team/events/contests/7');
    source.message({
      ...baseEvent,
      type: 'CLARIFICATION_UPDATED',
      payload: { clarificationId: 9, action: 'REPLIED' },
    });
    expect(onEvent).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'CLARIFICATION_UPDATED' }),
    );
    subscription.stop();
    expect(source.close).toHaveBeenCalled();
  });

  it('stops fallback polling after the Rust CONNECTED data event', () => {
    const poll = vi.fn();
    const connected = vi.fn();
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: ['CLARIFICATION_UPDATED'],
      onEvent: vi.fn(),
      onConnectionChange: connected,
      poll,
      pollIntervalMs: 1_000,
    });
    const source = EventSourceMock.instances[0];
    source.fail();
    expect(poll).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1_000);
    expect(poll).toHaveBeenCalledTimes(2);

    source.message({ ...baseEvent, type: 'CONNECTED' });
    vi.advanceTimersByTime(3_000);
    expect(poll).toHaveBeenCalledTimes(2);
    expect(connected).toHaveBeenLastCalledWith(true);
    subscription.stop();
  });

  it('delivers print update data from the message event without requiring a named SSE event', () => {
    const onEvent = vi.fn();
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: ['PRINT_REQUEST_UPDATED'],
      onEvent,
      poll: vi.fn(),
    });
    EventSourceMock.instances[0].message({
      ...baseEvent,
      type: 'PRINT_REQUEST_UPDATED',
      payload: { printRequestId: 19, action: 'COMPLETED' },
    });
    expect(onEvent).toHaveBeenCalledWith(
      expect.objectContaining({
        payload: { printRequestId: 19, action: 'COMPLETED' },
      }),
    );
    subscription.stop();
  });

  it('uses the staff endpoint and ignores events for another scope or contest', () => {
    const onEvent = vi.fn();
    const subscription = subscribeContestEvents({
      contestId: 8,
      scope: 'STAFF',
      eventTypes: ['CLARIFICATION_UPDATED'],
      onEvent,
      poll: vi.fn(),
    });
    const source = EventSourceMock.instances[0];
    expect(source.url).toBe('/api/events/contests/8');
    source.message({ ...baseEvent, type: 'CLARIFICATION_UPDATED' });
    expect(onEvent).not.toHaveBeenCalled();
    subscription.stop();
  });

  it('uses the unauthenticated public endpoint for Resolver displays', () => {
    const subscription = subscribeContestEvents({
      contestId: 9,
      scope: 'PUBLIC',
      eventTypes: ['RESOLVER_STATE_CHANGED'],
      onEvent: vi.fn(),
      poll: vi.fn(),
    });
    expect(EventSourceMock.instances[0].url).toBe('/api/public/events/contests/9');
    subscription.stop();
  });

  it('starts fallback polling for malformed messages and stops it after a valid connection event', () => {
    const poll = vi.fn();
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: [],
      onEvent: vi.fn(),
      poll,
      pollIntervalMs: 1_000,
    });
    const source = EventSourceMock.instances[0];

    source.dispatchEvent(new MessageEvent('message', { data: '{malformed' }));
    expect(poll).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1_000);
    expect(poll).toHaveBeenCalledTimes(2);

    source.message({ ...baseEvent, type: 'CONNECTED' });
    vi.advanceTimersByTime(2_000);
    expect(poll).toHaveBeenCalledTimes(2);
    subscription.stop();
  });

  it('defers fallback polling while the document is hidden and resumes on visibility change', () => {
    Object.defineProperty(document, 'hidden', { configurable: true, value: true });
    const poll = vi.fn();
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: [],
      onEvent: vi.fn(),
      poll,
      pollIntervalMs: 1_000,
    });
    const source = EventSourceMock.instances[0];
    source.fail();
    expect(poll).not.toHaveBeenCalled();

    Object.defineProperty(document, 'hidden', { configurable: true, value: false });
    document.dispatchEvent(new Event('visibilitychange'));
    expect(poll).toHaveBeenCalledTimes(1);
    subscription.stop();
  });

  it('does not overlap fallback polls while an async refresh is pending', async () => {
    let resolvePoll!: () => void;
    const poll = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolvePoll = resolve;
        }),
    );
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: [],
      onEvent: vi.fn(),
      poll,
      pollIntervalMs: 1_000,
    });
    const source = EventSourceMock.instances[0];
    source.fail();
    expect(poll).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(3_000);
    expect(poll).toHaveBeenCalledTimes(1);

    resolvePoll();
    await Promise.resolve();
    vi.advanceTimersByTime(1_000);
    expect(poll).toHaveBeenCalledTimes(2);
    subscription.stop();
  });

  it('ignores a late EventSource error after the subscription has stopped', () => {
    const onConnectionChange = vi.fn();
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: [],
      onEvent: vi.fn(),
      onConnectionChange,
      poll: vi.fn(),
    });
    const source = EventSourceMock.instances[0];
    subscription.stop();

    source.fail();

    expect(onConnectionChange).not.toHaveBeenCalled();
  });

  it('ignores a late EventSource message after the subscription has stopped', () => {
    const onEvent = vi.fn();
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: ['CLARIFICATION_UPDATED'],
      onEvent,
      poll: vi.fn(),
    });
    const source = EventSourceMock.instances[0];
    subscription.stop();

    source.message({ ...baseEvent, type: 'CLARIFICATION_UPDATED' });

    expect(onEvent).not.toHaveBeenCalled();
  });

  it('backs off fallback polling exponentially across consecutive errors, capped at 60s', () => {
    const poll = vi.fn();
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: [],
      onEvent: vi.fn(),
      poll,
      pollIntervalMs: 10_000,
    });
    const source = EventSourceMock.instances[0];

    source.fail();
    expect(poll).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(10_000);
    expect(poll).toHaveBeenCalledTimes(2);

    source.fail();
    vi.advanceTimersByTime(10_000);
    expect(poll).toHaveBeenCalledTimes(2);
    vi.advanceTimersByTime(10_000);
    expect(poll).toHaveBeenCalledTimes(3);

    source.fail();
    vi.advanceTimersByTime(40_000);
    expect(poll).toHaveBeenCalledTimes(4);

    source.fail();
    source.fail();
    vi.advanceTimersByTime(60_000);
    expect(poll).toHaveBeenCalledTimes(5);
    vi.advanceTimersByTime(60_000);
    expect(poll).toHaveBeenCalledTimes(6);

    source.message({ ...baseEvent, type: 'CONNECTED' });
    source.fail();
    expect(poll).toHaveBeenCalledTimes(7);
    vi.advanceTimersByTime(10_000);
    expect(poll).toHaveBeenCalledTimes(8);
    subscription.stop();
  });

  it('delivers an event id only once', () => {
    const onEvent = vi.fn();
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: ['CLARIFICATION_UPDATED'],
      onEvent,
      poll: vi.fn(),
    });
    const source = EventSourceMock.instances[0];

    source.message({ ...baseEvent, type: 'CLARIFICATION_UPDATED' });
    source.message({ ...baseEvent, type: 'CLARIFICATION_UPDATED' });
    source.message({
      ...baseEvent,
      type: 'CLARIFICATION_UPDATED',
      id: '00000000-0000-4000-8000-000000000002',
    });

    expect(onEvent).toHaveBeenCalledTimes(2);
    subscription.stop();
  });

  it('reconnects manually after an error and carries the last seen event id', () => {
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: ['CLARIFICATION_UPDATED'],
      onEvent: vi.fn(),
      poll: vi.fn(),
      pollIntervalMs: 1_000,
    });
    const source = EventSourceMock.instances[0];
    expect(source.url).toBe('/api/team/events/contests/7');

    source.message({ ...baseEvent, type: 'CLARIFICATION_UPDATED' });
    source.fail();

    // The broken source is closed immediately; the resume connection opens
    // once the backoff elapses and appends lastEventId to the path.
    expect(source.close).toHaveBeenCalled();
    // handleConnectionLost doubled the poll interval to 2s before scheduling.
    vi.advanceTimersByTime(2_000);
    expect(EventSourceMock.instances).toHaveLength(2);
    expect(EventSourceMock.instances[1].url).toBe(
      '/api/team/events/contests/7?lastEventId=00000000-0000-4000-8000-000000000001',
    );
    subscription.stop();
  });

  it('does not use the CONNECTED frame id as the resume anchor', () => {
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: [],
      onEvent: vi.fn(),
      poll: vi.fn(),
      pollIntervalMs: 1_000,
    });
    const source = EventSourceMock.instances[0];
    source.message({ ...baseEvent, id: 'connected-frame-id', type: 'CONNECTED' });
    source.fail();

    vi.advanceTimersByTime(2_000);
    expect(EventSourceMock.instances[1].url).toBe('/api/team/events/contests/7');
    subscription.stop();
  });

  it('still deduplicates replayed events against already delivered ids', () => {
    const onEvent = vi.fn();
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: ['CLARIFICATION_UPDATED'],
      onEvent,
      poll: vi.fn(),
      pollIntervalMs: 1_000,
    });
    const source = EventSourceMock.instances[0];
    source.message({ ...baseEvent, type: 'CLARIFICATION_UPDATED' });
    source.fail();
    vi.advanceTimersByTime(2_000);

    // The server replays event ...0001 on the resumed connection; the
    // batch-1 duplicate guard must swallow it.
    EventSourceMock.instances[1].message({ ...baseEvent, type: 'CLARIFICATION_UPDATED' });
    expect(onEvent).toHaveBeenCalledTimes(1);
    subscription.stop();
  });

  it('probes the session at most once while an auth probe is in flight', () => {
    mocks.apiRequest.mockImplementation(() => new Promise(() => {}));
    const subscription = subscribeContestEvents({
      contestId: 7,
      scope: 'TEAM',
      eventTypes: [],
      onEvent: vi.fn(),
      poll: vi.fn(),
    });
    const source = EventSourceMock.instances[0];

    source.fail();
    source.fail();
    source.fail();

    expect(mocks.apiRequest).toHaveBeenCalledTimes(1);
    expect(mocks.apiRequest).toHaveBeenCalledWith('/api/auth/me');
    subscription.stop();
  });
});
