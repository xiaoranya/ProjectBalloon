import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ScoreboardView from './ScoreboardView.vue';

const mocks = vi.hoisted(() => ({ getScoreboard: vi.fn() }));
const route = { params: { contestId: '7' } };
vi.mock('../api/contest', () => ({ contestApi: { getScoreboard: mocks.getScoreboard } }));
vi.mock('vue-router', () => ({ useRoute: () => route }));

const scoreboard = {
  contestId: 7,
  variant: 'PUBLIC',
  frozen: false,
  scoringMode: 'ICPC' as const,
  scoreAggregation: 'BEST' as const,
  generatedAt: '2026-08-01T09:00:00Z',
  problems: [
    { problemId: 1, alias: 'A', displayOrder: 1, firstBloodTeamId: 2, firstBloodAt: null },
    { problemId: 2, alias: 'B', displayOrder: 2, firstBloodTeamId: null, firstBloodAt: null },
  ],
  rows: [{
    rank: 1,
    officialRank: 1,
    teamId: 2,
    teamName: 'Blue Team',
    school: 'University',
    participationType: 'OFFICIAL',
    groupName: null,
    isStar: false,
    solvedCount: 1,
    penaltyMinutes: 42,
    totalScoreMilli: 100_000,
    lastSolvedAt: null,
    problems: [{ problemId: 1, wrongAttempts: 1, solved: true, solvedAt: null, penaltyMinutes: 42, scoreMilli: 100_000, firstBlood: true }],
  }],
};

describe('ScoreboardView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getScoreboard.mockResolvedValue(scoreboard);
  });

  it('renders frozen state, team metrics, and placeholder cells for missing problems', async () => {
    const wrapper = mount(ScoreboardView);
    await flushPromises();

    expect(mocks.getScoreboard).toHaveBeenCalledWith(7);
    expect(wrapper.text()).toContain('Blue Team');
    expect(wrapper.text()).toContain('University');
    expect(wrapper.text()).toContain('A');
    expect(wrapper.text()).toContain('B');
    expect(wrapper.text()).toContain('+1');
    expect(wrapper.findAll('.score-cell')).toHaveLength(2);
    wrapper.unmount();
  });
});
