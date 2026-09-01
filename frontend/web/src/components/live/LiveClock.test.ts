import { mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import LiveClock from './LiveClock.vue';

describe('live clock', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // Anchor performance.now to a fixed ramp so the server-anchored clock is
    // deterministic under fake timers.
    let base = 1_000_000;
    vi.spyOn(performance, 'now').mockImplementation(() => {
      base += 1000;
      return base;
    });
  });
  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it('renders a local wall clock from the server time anchor', async () => {
    // 2026-07-29T08:09:10Z rendered in the test-runner's local zone.
    const wrapper = mount(LiveClock, { props: { serverTime: '2026-07-29T08:09:10Z' } });
    const first = wrapper.find('.live-clock').text();
    expect(first).toMatch(/^\d{2}:\d{2}:\d{2}$/);

    await vi.advanceTimersByTimeAsync(3000);
    const later = wrapper.find('.live-clock').text();
    expect(later).toMatch(/^\d{2}:\d{2}:\d{2}$/);
    expect(later).not.toBe(first);
    wrapper.unmount();
  });

  it('re-anchors when a new server time arrives', async () => {
    const wrapper = mount(LiveClock, { props: { serverTime: '2026-07-29T08:09:10Z' } });
    await vi.advanceTimersByTimeAsync(10_000);
    await wrapper.setProps({ serverTime: '2026-07-29T09:00:00Z' });
    const before = wrapper.find('.live-clock').text();
    const base = before;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(wrapper.find('.live-clock').text()).not.toBe(base);
    wrapper.unmount();
  });
});
