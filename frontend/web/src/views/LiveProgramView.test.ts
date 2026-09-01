import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import LiveProgramView from './LiveProgramView.vue';
import type { AwardPresentation } from '../api/awards';
import type { ResolverPublicRun } from '../api/resolver';
import {
  presentationApi,
  type PresentationMetrics,
  type PublishedLiveProgram,
  type PublishedPresentation,
} from '../api/presentation';
import { subscribeContestEvents } from '../realtime/contest-events';

vi.mock('vue-router', () => ({ useRoute: () => ({ query: { contestId: '7' } }) }));
vi.mock('../api/presentation', () => ({
  presentationApi: {
    published: vi.fn(),
    metrics: vi.fn(),
    publishedProgram: vi.fn(),
  },
}));
vi.mock('../api/awards', () => ({ awardsApi: { presentation: vi.fn() } }));
vi.mock('../api/resolver', () => ({ resolverApi: { publicState: vi.fn() } }));
vi.mock('../realtime/contest-events', () => ({
  subscribeContestEvents: vi.fn(() => ({ stop: vi.fn() })),
}));

const published: PublishedPresentation = {
  contestId: 7,
  contestName: 'Finals',
  contestStatus: 'RUNNING',
  startAt: null,
  freezeAt: '2026-07-29T02:00:00Z',
  endAt: null,
  serverTime: '2026-07-29T01:00:00Z',
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

const metrics: PresentationMetrics = {
  balloons: {
    total: 2,
    firstBlood: 1,
    pending: 0,
    preparing: 0,
    delivering: 1,
    delivered: 1,
    cancelled: 0,
    colors: [{ name: '红', total: 2 }],
  },
  submissions: { total: 30, accepted: 9, pending: 2, languages: [], trend: [] },
};

function programFixture(overrides: Partial<PublishedLiveProgram> = {}): PublishedLiveProgram {
  return {
    contestId: 7,
    currentScene: 'SCOREBOARD',
    resolverRunId: null,
    transitionMilliseconds: 800,
    showClock: true,
    tickerEnabled: true,
    titleCardText: null,
    serverTime: '2026-07-29T01:00:00Z',
    version: 4,
    ...overrides,
  };
}

function latestOptions() {
  return vi.mocked(subscribeContestEvents).mock.calls.at(-1)![0];
}

describe('live program view', () => {
  beforeEach(() => {
    location.hash = 'token=broadcast-token';
    vi.mocked(presentationApi.published).mockResolvedValue(published);
    vi.mocked(presentationApi.metrics).mockResolvedValue(metrics);
    vi.mocked(presentationApi.publishedProgram).mockResolvedValue(programFixture());
  });

  it('renders the on-air scene with clock and ticker', async () => {
    const wrapper = mount(LiveProgramView);
    await flushPromises();

    expect(presentationApi.publishedProgram).toHaveBeenCalledWith(7, 'broadcast-token');
    expect(wrapper.text()).toContain('播出中');
    expect(wrapper.text()).toContain('实时榜单');
    expect(wrapper.text()).toContain('Trailblazers');
    expect(wrapper.text()).toContain('欢迎'); // ticker announcement
    expect(wrapper.find('.live-program-clock').exists()).toBe(true);
    expect(wrapper.find('.live-program-ticker').exists()).toBe(true);
    expect(wrapper.attributes('style')).toContain('--accent: #ef4444');
    wrapper.unmount();
  });

  it('switches scenes when the director pushes LIVE_PROGRAM_UPDATED', async () => {
    const wrapper = mount(LiveProgramView);
    await flushPromises();
    expect(wrapper.text()).toContain('实时榜单');

    vi.mocked(presentationApi.publishedProgram).mockResolvedValue(
      programFixture({ currentScene: 'BALLOONS' }),
    );
    latestOptions().onEvent({
      id: 'e2',
      version: 1,
      type: 'LIVE_PROGRAM_UPDATED',
      scope: 'PUBLIC',
      contestId: 7,
      occurredAt: '2026-07-29T01:01:00Z',
      payload: {},
    });
    await flushPromises();
    expect(wrapper.text()).toContain('气球状态');
    wrapper.unmount();
  });

  it('hides the clock and ticker per the program flags', async () => {
    vi.mocked(presentationApi.publishedProgram).mockResolvedValue(
      programFixture({ showClock: false, tickerEnabled: false }),
    );
    const wrapper = mount(LiveProgramView);
    await flushPromises();
    expect(wrapper.find('.live-program-clock').exists()).toBe(false);
    expect(wrapper.find('.live-program-ticker').exists()).toBe(false);
    wrapper.unmount();
  });

  it('shows the transient first-blood popup on any scene', async () => {
    vi.mocked(presentationApi.publishedProgram).mockResolvedValue(
      programFixture({ currentScene: 'STATISTICS' }),
    );
    const wrapper = mount(LiveProgramView);
    await flushPromises();
    expect(wrapper.text()).toContain('比赛统计');
    expect(wrapper.find('.fb-popup').exists()).toBe(true);
    expect(wrapper.text()).toContain('Trailblazers');
    wrapper.unmount();
  });

  it('renders the resolver scene from the resolved run', async () => {
    const run: ResolverPublicRun = {
      id: 5,
      contestId: 7,
      status: 'RUNNING',
      currentStep: 2,
      totalSteps: 6,
      updatedAt: '2026-07-29T01:00:00Z',
      state: {
        stepIndex: 2,
        totalSteps: 6,
        lastReveal: {
          teamId: 11,
          problemId: 3,
          before: {
            problemId: 3,
            wrongAttempts: 0,
            solved: false,
            solvedAt: null,
            penaltyMinutes: 0,
            scoreMilli: 0,
            firstBlood: false,
          },
          after: {
            problemId: 3,
            wrongAttempts: 0,
            solved: true,
            solvedAt: '2026-07-29T00:30:00Z',
            penaltyMinutes: 12,
            scoreMilli: 0,
            firstBlood: true,
          },
        },
        board: published.scoreboard,
      },
    };
    vi.mocked(presentationApi.publishedProgram).mockResolvedValue(
      programFixture({ currentScene: 'RESOLVER', resolverRunId: 5 }),
    );
    const { resolverApi } = await import('../api/resolver');
    vi.mocked(resolverApi.publicState).mockResolvedValue(run);
    const wrapper = mount(LiveProgramView);
    await flushPromises();
    expect(resolverApi.publicState).toHaveBeenCalledWith(5);
    expect(wrapper.find('.resolver-display-stage').exists()).toBe(true);
    expect(wrapper.text()).toContain('Trailblazers');
    wrapper.unmount();
  });

  it('renders the awards scene', async () => {
    const presentation: AwardPresentation = {
      contestId: 7,
      contestName: 'Finals',
      contestStatus: 'ENDED',
      status: 'PRESENTING',
      currentCategoryId: 1,
      autoRotate: false,
      intervalSeconds: 15,
      stateUpdatedAt: '2026-07-29T01:00:00Z',
      serverTime: '2026-07-29T01:00:00Z',
      categories: [
        {
          id: 1,
          code: 'CHAMPION',
          name: '冠军',
          displayOrder: 1,
          groupName: null,
          firstBlood: false,
          recipients: [
            {
              id: 2,
              problemId: null,
              problemAlias: null,
              teamId: 11,
              teamName: 'Trailblazers',
              school: 'Star',
              seatNo: null,
              groupName: null,
              participationType: 'OFFICIAL',
              star: false,
              rank: 1,
              solved: 9,
              penaltyMinutes: 120,
            },
          ],
        },
      ],
    };
    vi.mocked(presentationApi.publishedProgram).mockResolvedValue(
      programFixture({ currentScene: 'AWARDS' }),
    );
    const { awardsApi } = await import('../api/awards');
    vi.mocked(awardsApi.presentation).mockResolvedValue(presentation);
    const wrapper = mount(LiveProgramView);
    await flushPromises();
    expect(wrapper.find('.award-display-stage').exists()).toBe(true);
    expect(wrapper.text()).toContain('冠军');
    wrapper.unmount();
  });

  it('renders the title card with the configured copy', async () => {
    vi.mocked(presentationApi.publishedProgram).mockResolvedValue(
      programFixture({ currentScene: 'TITLE_CARD', titleCardText: '欢迎来到总决赛' }),
    );
    const wrapper = mount(LiveProgramView);
    await flushPromises();
    expect(wrapper.find('.live-program-title-card').exists()).toBe(true);
    expect(wrapper.text()).toContain('欢迎来到总决赛');
    wrapper.unmount();
  });

  it('fails loudly without a contest id or token', async () => {
    location.hash = '';
    const wrapper = mount(LiveProgramView);
    await flushPromises();
    expect(wrapper.text()).toContain('缺少 contestId 或广播 Token');
    wrapper.unmount();
  });
});
