import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminContestDetailView from './AdminContestDetailView.vue';

const api = vi.hoisted(() => ({
  getContest: vi.fn(),
  listTeams: vi.fn(),
  listContestTeams: vi.fn(),
  listAllProblems: vi.fn(),
  listContestProblems: vi.fn(),
  updateProblemAssignment: vi.fn(),
  listSubmissions: vi.fn(),
  getJudgeQueueStatus: vi.fn(),
  getScoringPolicy: vi.fn(),
  updateScoringPolicy: vi.fn(),
  getProblemSubtasks: vi.fn(),
  replaceProblemSubtasks: vi.fn(),
  cloneContest: vi.fn(),
  isSuperAdmin: { value: false },
  push: vi.fn(),
}));
vi.mock('../api/admin-contests', () => ({ adminContestApi: api }));
vi.mock('../auth/session', () => ({ useSession: () => ({ isSuperAdmin: api.isSuperAdmin }) }));
vi.mock('vue-router', () => ({
  useRoute: () => ({ params: { contestId: '42' } }),
  useRouter: () => ({ push: api.push }),
}));

describe('AdminContestDetailView', () => {
  beforeEach(() => {
    api.isSuperAdmin.value = false;
    api.getContest.mockResolvedValue({
      id: 42,
      name: 'Rust Regional',
      status: 'RUNNING',
      visibility: 'PRIVATE',
      startAt: '2026-07-20T08:00:00Z',
      freezeAt: '2026-07-20T12:00:00Z',
      endAt: '2026-07-20T13:00:00Z',
      version: 3,
      createdAt: '2026-07-20T00:00:00Z',
      updatedAt: '2026-07-20T00:00:00Z',
      deletedAt: null,
    });
    api.listTeams.mockResolvedValue({ content: [] });
    api.listContestTeams.mockResolvedValue([]);
    api.listAllProblems.mockResolvedValue([]);
    api.listContestProblems.mockResolvedValue([]);
    api.listSubmissions.mockResolvedValue({
      content: [{
        id: 9,
        contestId: 42,
        problemId: 3,
        problemAlias: 'A',
        teamId: 8,
        teamName: 'Team Eight',
        language: 'cpp',
        sourceSizeBytes: 128,
        status: 'ACCEPTED',
        submittedAt: '2026-07-20T09:00:00Z',
        judgedAt: '2026-07-20T09:00:01Z',
        activeJudgementId: '11111111-1111-1111-1111-111111111111',
        verdict: 'ACCEPTED',
        totalTimeMs: 10,
        peakMemoryKb: 1024,
      }],
      page: 0,
      size: 30,
      totalElements: 1,
      totalPages: 1,
    });
    api.getJudgeQueueStatus.mockResolvedValue({
      contestId: 42, drained: true, pendingSubmissions: 0, judgingSubmissions: 0,
      outboxPending: 0, outboxFailed: 0, checkedAt: '2026-07-20T09:00:00Z',
    });
    api.getScoringPolicy.mockResolvedValue({ contestId: 42, scoringMode: 'ICPC', scoreAggregation: 'BEST', feedbackPolicy: 'FULL' });
  });

  it('loads contest administration resources from the Rust-aligned API', async () => {
    const wrapper = mount(AdminContestDetailView);
    await flushPromises();

    expect(api.getContest).toHaveBeenCalledWith(42);
    expect(api.listContestTeams).toHaveBeenCalledWith(42);
    expect(api.listAllProblems).toHaveBeenCalledWith(42);
    expect(api.listContestProblems).toHaveBeenCalledWith(42);
    expect(api.listSubmissions).toHaveBeenCalledWith(42);
    expect(api.getJudgeQueueStatus).toHaveBeenCalledWith(42);
    expect(wrapper.text()).toContain('Rust Regional');
    expect(wrapper.text()).toContain('延长比赛');
  });

  it('links contest managers to the contest-scoped bulk rejudge workbench', async () => {
    const wrapper = mount(AdminContestDetailView);
    await flushPromises();
    const entry = wrapper.findAll('button').find((button) => button.text().includes('批量重判工作台'))!;

    await entry.trigger('click');

    expect(api.push).toHaveBeenCalledWith('/admin/contests/42/rejudge-tasks');
  });

  it('links contest managers to announcement management', async () => {
    const wrapper = mount(AdminContestDetailView);
    await flushPromises();
    const entry = wrapper.findAll('button').find((button) => button.text().includes('公告管理'))!;

    await entry.trigger('click');

    expect(api.push).toHaveBeenCalledWith('/admin/contests/42/announcements');
  });

  it('links an assigned problem to the scoped content editor', async () => {
    api.listAllProblems.mockResolvedValue([{ id: 7, slug: 'sum', title: 'Sum' }]);
    api.listContestProblems.mockResolvedValue([{
      contestId: 42, problemId: 7, alias: 'A', displayOrder: 1, color: '#ff0000',
      slug: 'sum', title: 'Sum', timeLimitMs: 1000, memoryLimitMb: 256,
      outputLimitKb: 65536, languages: ['cpp'], statement: null,
    }]);
    const wrapper = mount(AdminContestDetailView);
    await flushPromises();
    const entry = wrapper.findAll('button').find((button) => button.text().includes('题目内容'))!;

    await entry.trigger('click');

    expect(api.push).toHaveBeenCalledWith('/admin/problems/7?contestId=42');
  });

  it('shows contest cloning only to super administrators', async () => {
    const regularWrapper = mount(AdminContestDetailView);
    await flushPromises();
    expect(regularWrapper.text()).not.toContain('克隆比赛');

    api.isSuperAdmin.value = true;
    const superAdminWrapper = mount(AdminContestDetailView);
    await flushPromises();
    expect(superAdminWrapper.text()).toContain('克隆比赛');
  });
});
