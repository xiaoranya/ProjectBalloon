import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import LiveView from './LiveView.vue';
import { presentationApi, type PublishedPresentation } from '../api/presentation';

vi.mock('vue-router', () => ({ useRoute: () => ({ query: { contestId: '7' } }) }));
vi.mock('../api/presentation', () => ({
  presentationApi: { published: vi.fn(), metrics: vi.fn() },
}));

const published: PublishedPresentation = {
  contestId: 7,
  contestName: 'Finals',
  contestStatus: 'RUNNING',
  startAt: null,
  freezeAt: null,
  endAt: null,
  serverTime: '2026-07-29T00:00:00Z',
  config: {
    contestId: 7,
    mode: 'LIVE',
    enabled: true,
    title: 'Finals',
    subtitle: 'On air',
    accentColor: '#ef4444',
    rowLimit: 12,
    showAnnouncements: true,
    announcementIntervalSeconds: 10,
    template: 'CINEMATIC',
    updatedAt: null,
  },
  scoreboard: {
    contestId: 7,
    variant: 'PUBLIC',
    frozen: false,
    scoringMode: 'ICPC',
    scoreAggregation: 'BEST',
    generatedAt: '2026-07-29T00:00:00Z',
    problems: [],
    rows: [],
  },
  announcements: [],
};

describe('live presentation templates', () => {
  beforeEach(() => {
    location.hash = 'token=broadcast-token';
    vi.mocked(presentationApi.published).mockResolvedValue(published);
  });

  it('applies the selected visual template to the OBS page', async () => {
    const wrapper = mount(LiveView);
    await flushPromises();

    expect(presentationApi.published).toHaveBeenCalledWith(7, 'LIVE', 'broadcast-token');
    expect(wrapper.classes()).toContain('template-cinematic');
    expect(wrapper.attributes('style')).toContain('--accent: #ef4444');
    wrapper.unmount();
  });

  it('ignores a stale poll response that resolves after a newer one', async () => {
    vi.useFakeTimers();
    const stale = { ...published, config: { ...published.config, title: 'STALE' } };
    const fresh = { ...published, config: { ...published.config, title: 'FRESH' } };
    let releaseStale!: (value: PublishedPresentation) => void;
    const stalePromise = new Promise<PublishedPresentation>((resolve) => {
      releaseStale = resolve;
    });
    vi.mocked(presentationApi.published)
      .mockReturnValueOnce(stalePromise)
      .mockResolvedValueOnce(fresh);

    const wrapper = mount(LiveView);
    try {
      await flushPromises();
      await vi.advanceTimersByTimeAsync(10_000);
      await flushPromises();
      expect(wrapper.text()).toContain('FRESH');

      releaseStale(stale);
      await flushPromises();
      expect(wrapper.text()).toContain('FRESH');
      expect(wrapper.text()).not.toContain('STALE');
    } finally {
      wrapper.unmount();
      vi.useRealTimers();
    }
  });
});
