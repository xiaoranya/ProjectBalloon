import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import LiveFirstBloodPopup from './LiveFirstBloodPopup.vue';

describe('live first-blood popup', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows and animates the first popup, then auto-hides when transient', async () => {
    vi.useFakeTimers();
    const wrapper = mount(LiveFirstBloodPopup, {
      props: { teamName: 'Trailblazers', problemAlias: 'A', playKey: '7:A', transient: true },
    });
    await flushPromises();
    expect(wrapper.find('.fb-popup').exists()).toBe(true);
    expect(wrapper.text()).toContain('FIRST BLOOD');
    expect(wrapper.text()).toContain('Trailblazers');

    await vi.advanceTimersByTimeAsync(12_000);
    expect(wrapper.find('.fb-popup').exists()).toBe(false);
    wrapper.unmount();
  });

  it('replays the entrance when a new first blood arrives', async () => {
    vi.useFakeTimers();
    const wrapper = mount(LiveFirstBloodPopup, {
      props: { teamName: 'Trailblazers', playKey: '7:A', transient: true },
    });
    await flushPromises();
    expect(wrapper.find('.fb-popup').exists()).toBe(true);
    await vi.advanceTimersByTimeAsync(12_000);
    expect(wrapper.find('.fb-popup').exists()).toBe(false);

    await wrapper.setProps({ teamName: 'Comets', playKey: '9:C' });
    await flushPromises();
    expect(wrapper.find('.fb-popup').exists()).toBe(true);
    expect(wrapper.text()).toContain('Comets');
    wrapper.unmount();
  });

  it('stays on air until cleared when persistent', async () => {
    vi.useFakeTimers();
    const wrapper = mount(LiveFirstBloodPopup, {
      props: { teamName: 'Trailblazers', playKey: '7:A' },
    });
    await flushPromises();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(wrapper.find('.fb-popup').exists()).toBe(true);

    await wrapper.setProps({ playKey: null });
    await flushPromises();
    expect(wrapper.find('.fb-popup').exists()).toBe(false);
    wrapper.unmount();
  });

  it('never shows without a team', () => {
    const wrapper = mount(LiveFirstBloodPopup, {
      props: { teamName: '', playKey: '7:A' },
    });
    expect(wrapper.find('.fb-popup').exists()).toBe(false);
    wrapper.unmount();
  });
});
