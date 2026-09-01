import { mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import LiveTickerBar from './LiveTickerBar.vue';

const announcements = [
  { id: 1, title: '欢迎', body: '欢迎来到决赛' },
  { id: 2, title: '注意', body: '封榜时间即将到来' },
];

describe('live ticker bar', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('rotates announcements on the configured interval', async () => {
    const wrapper = mount(LiveTickerBar, {
      props: { announcements, intervalSeconds: 5 },
    });
    expect(wrapper.text()).toContain('欢迎');
    expect(wrapper.text()).not.toContain('封榜时间');

    await vi.advanceTimersByTimeAsync(5_000);
    expect(wrapper.text()).toContain('封榜时间');

    await vi.advanceTimersByTimeAsync(5_000);
    expect(wrapper.text()).toContain('欢迎');
    wrapper.unmount();
  });

  it('falls back to the brand line without announcements when a fallback is given', () => {
    const wrapper = mount(LiveTickerBar, {
      props: { announcements: [], fallback: 'ProjectBalloon LIVE' },
    });
    expect(wrapper.text()).toContain('ProjectBalloon LIVE');
    wrapper.unmount();
  });

  it('renders nothing when there is nothing to show', () => {
    const wrapper = mount(LiveTickerBar, { props: { announcements: [] } });
    expect(wrapper.find('.ticker').exists()).toBe(false);
    wrapper.unmount();
  });

  it('restarts rotation when the announcement set changes', async () => {
    const wrapper = mount(LiveTickerBar, { props: { announcements: [announcements[0]] } });
    await wrapper.setProps({ announcements });
    await vi.advanceTimersByTimeAsync(10_000);
    expect(wrapper.text()).toContain('封榜时间');
    wrapper.unmount();
  });
});
