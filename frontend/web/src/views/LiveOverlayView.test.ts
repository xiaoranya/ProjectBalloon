import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import LiveOverlayView from './LiveOverlayView.vue';
import { presentationApi, type PublishedPresentation } from '../api/presentation';
import { subscribeContestEvents } from '../realtime/contest-events';

vi.mock('../api/presentation', () => ({
  presentationApi: { published: vi.fn() },
}));
vi.mock('../realtime/contest-events', () => ({
  subscribeContestEvents: vi.fn(() => ({ stop: vi.fn() })),
}));

const published: PublishedPresentation = {
  contestId: 7,
  contestName: 'Finals',
  contestStatus: 'RUNNING',
  startAt: null,
  freezeAt: null,
  endAt: null,
  serverTime: '2026-07-29T01:00:00Z',
  config: {
    contestId: 7,
    mode: 'LIVE',
    enabled: true,
    title: 'Finals',
    subtitle: null,
    accentColor: '#ef4444',
    rowLimit: 12,
    showAnnouncements: true,
    announcementIntervalSeconds: 10,
    template: 'DEFAULT',
    updatedAt: null,
  },
  scoreboard: {
    contestId: 7,
    variant: 'PUBLIC',
    frozen: false,
    scoringMode: 'ICPC',
    scoreAggregation: 'BEST',
    generatedAt: '2026-07-29T00:00:00Z',
    problems: [
      {
        problemId: 3,
        alias: 'A',
        displayOrder: 1,
        firstBloodTeamId: 11,
        firstBloodAt: '2026-07-29T00:30:00Z',
      },
    ],
    rows: [
      {
        rank: 1,
        officialRank: 1,
        teamId: 11,
        teamName: 'Trailblazers',
        school: 'Star',
        participationType: 'OFFICIAL',
        groupName: null,
        isStar: false,
        solvedCount: 1,
        penaltyMinutes: 12,
        totalScoreMilli: 0,
        lastSolvedAt: '2026-07-29T00:30:00Z',
        problems: [
          {
            problemId: 3,
            wrongAttempts: 0,
            solved: true,
            solvedAt: '2026-07-29T00:30:00Z',
            penaltyMinutes: 12,
            scoreMilli: 0,
            firstBlood: true,
          },
        ],
      },
    ],
  },
  announcements: [{ id: 1, title: '欢迎', body: '决赛开始', pinned: true, publishedAt: null }],
};

let route: { query: Record<string, string> } = { query: { contestId: '7' } };
vi.mock('vue-router', () => ({ useRoute: () => route }));

function setRoute(query: Record<string, string>) {
  route = { query };
}

describe('live overlay view', () => {
  beforeEach(() => {
    location.hash = 'token=broadcast-token';
    setRoute({ contestId: '7' });
    vi.mocked(presentationApi.published).mockResolvedValue(published);
    document.documentElement.classList.remove('live-overlay-page');
  });

  it('renders every part by default on a transparent page', async () => {
    const wrapper = mount(LiveOverlayView);
    await flushPromises();
    expect(presentationApi.published).toHaveBeenCalledWith(7, 'LIVE', 'broadcast-token');
    expect(wrapper.find('.live-overlay-clock').exists()).toBe(true);
    expect(wrapper.find('.fb-popup').exists()).toBe(true);
    expect(wrapper.text()).toContain('欢迎');
    expect(document.documentElement.classList.contains('live-overlay-page')).toBe(true);
    wrapper.unmount();
    expect(document.documentElement.classList.contains('live-overlay-page')).toBe(false);
  });

  it('honors the ?parts= selector', async () => {
    setRoute({ contestId: '7', parts: 'clock' });
    const wrapper = mount(LiveOverlayView);
    await flushPromises();
    expect(wrapper.find('.live-overlay-clock').exists()).toBe(true);
    expect(wrapper.find('.fb-popup').exists()).toBe(false);
    expect(wrapper.text()).not.toContain('欢迎');
    wrapper.unmount();
  });

  it('subscribes to public announcement events', async () => {
    const wrapper = mount(LiveOverlayView);
    await flushPromises();
    const options = vi.mocked(subscribeContestEvents).mock.calls.at(-1)![0];
    expect(options.contestId).toBe(7);
    expect(options.scope).toBe('PUBLIC');
    expect(options.eventTypes).toContain('ANNOUNCEMENT_UPDATED');
    wrapper.unmount();
  });

  it('fails loudly without a contest id or token', async () => {
    location.hash = '';
    const wrapper = mount(LiveOverlayView);
    await flushPromises();
    expect(wrapper.text()).toContain('缺少 contestId 或广播 Token');
    wrapper.unmount();
  });
});
