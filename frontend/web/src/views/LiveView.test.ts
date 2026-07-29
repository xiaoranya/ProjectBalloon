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
});
