import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SubmissionsTab from './SubmissionsTab.vue';
import TeamsTab from './TeamsTab.vue';
import { adminContestApi, type JudgeQueueStatus } from '../../api/admin-contests';
import { ApiError } from '../../api/client';
import type {
  ContestProblem,
  ContestTeamResponse,
  PageResponse,
  ProblemResponse,
  SubmissionSummary,
  SimilarityPairResponse,
  SimilarityBackfillResponse,
  TeamResponse,
} from '../../api/types';

const elementMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  confirm: vi.fn(),
}));
// Only message/box need stubbing. The real ElTable renders fine under jsdom
// (src/test/setup.ts polyfills ResizeObserver), and mocking 'element-plus/es'
// on top of 'element-plus' breaks the ElMessage override entirely — the
// components then resolve a real ElMessage and the mocked fns never fire.
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return {
    ...actual,
    ElMessage: { success: elementMocks.success, error: elementMocks.error },
    ElMessageBox: { confirm: elementMocks.confirm },
  };
});

vi.mock('../CodeEditor.vue', () => ({ default: { name: 'CodeEditor', template: '<div />' } }));

vi.mock('../../api/admin-contests', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/admin-contests')>();
  return {
    ...actual,
    adminContestApi: {
      ...actual.adminContestApi,
      listSubmissions: vi.fn(),
      getJudgeQueueStatus: vi.fn(),
      getSubmission: vi.fn(),
      rejudgeSubmission: vi.fn(),
      exportScoreboard: vi.fn(),
      exportSubmissions: vi.fn(),
      exportSubmissionSources: vi.fn(),
      listSubmissionSimilarityPairs: vi.fn(),
      backfillSubmissionSimilarity: vi.fn(),
      assignTeam: vi.fn(),
      unassignTeam: vi.fn(),
      listContestTeams: vi.fn(),
    },
  };
});

const mockedApi = vi.mocked(adminContestApi, true);

const problem: ProblemResponse = {
  id: 10,
  slug: 'two-sum',
  title: 'Two Sum',
  timeLimitMs: 1000,
  memoryLimitMb: 256,
  outputLimitKb: 65536,
  languages: ['cpp'],
  testdataVersion: 1,
  testdataSha256: null,
  defaultLangCode: 'cpp',
  createdBy: null,
  version: 1,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
  judgeMode: 'STANDARD',
  interactorObjectKey: null,
  interactorSha256: null,
};

const contestProblem: ContestProblem = {
  contestId: 1,
  problemId: 10,
  alias: 'A',
  displayOrder: 1,
  color: null,
  slug: 'two-sum',
  title: 'Two Sum',
  timeLimitMs: 1000,
  memoryLimitMb: 256,
  outputLimitKb: 65536,
  languages: ['cpp'],
  statement: null,
};

const submission: SubmissionSummary = {
  id: 501,
  contestId: 1,
  problemId: 10,
  problemAlias: 'A',
  teamId: 7,
  teamName: 'Team Rocket',
  language: 'cpp',
  sourceSizeBytes: 128,
  status: 'COMPLETED',
  submittedAt: '2026-01-01T10:00:00Z',
  judgedAt: '2026-01-01T10:00:05Z',
  activeJudgementId: 'judge-1',
  verdict: 'ACCEPTED',
  totalTimeMs: 12,
  peakMemoryKb: 2048,
  scoreMilli: 100000,
};

const submissionsPage: PageResponse<SubmissionSummary> = {
  content: [submission],
  page: 0,
  size: 30,
  totalElements: 1,
  totalPages: 1,
};

const queueStatus: JudgeQueueStatus = {
  contestId: 1,
  drained: true,
  pendingSubmissions: 0,
  judgingSubmissions: 0,
  outboxPending: 0,
  outboxFailed: 0,
  checkedAt: '2026-01-01T10:00:10Z',
};

const similarityPair: SimilarityPairResponse = {
  problemId: 10,
  language: 'cpp',
  submissionId: 501,
  teamId: 7,
  otherSubmissionId: 502,
  otherTeamId: 8,
  hammingDistance: 4,
  similarityPercent: 94,
};

const backfillResult: SimilarityBackfillResponse = { scanned: 5, updated: 4, failed: 1 };

const team: TeamResponse = {
  id: 7,
  name: 'Team Rocket',
  school: null,
  seatNo: null,
  groupName: null,
  star: false,
  version: 1,
  account: null,
  deletedAt: null,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
};

const otherTeam: TeamResponse = { ...team, id: 8, name: 'Team Rocket Jr.' };

const contestTeam: ContestTeamResponse = {
  id: 1,
  contestId: 1,
  teamId: 7,
  teamName: 'Team Rocket',
  participationType: 'OFFICIAL',
  groupName: null,
  createdAt: '2026-01-01T00:00:00Z',
};

beforeEach(() => {
  vi.clearAllMocks();
});

function findButton(wrapper: ReturnType<typeof mount>, text: string) {
  const button = wrapper.findAll('button').find((b) => b.text().includes(text));
  if (!button) throw new Error(`Button with text "${text}" not found`);
  return button;
}

describe('SubmissionsTab', () => {
  function mountTab() {
    return mount(SubmissionsTab, {
      props: {
        contestId: 1,
        contestName: 'Final Round',
        problems: [problem],
        contestProblems: [contestProblem],
      },
    });
  }

  it('loads submissions and judge queue status on mount', async () => {
    mockedApi.listSubmissions.mockResolvedValue(submissionsPage);
    mockedApi.getJudgeQueueStatus.mockResolvedValue(queueStatus);

    const wrapper = mountTab();
    await flushPromises();

    expect(mockedApi.listSubmissions).toHaveBeenCalledWith(1);
    expect(mockedApi.getJudgeQueueStatus).toHaveBeenCalledWith(1);
    expect(wrapper.text()).toContain('已排空');
    expect(wrapper.text()).toContain('Two Sum');
  });

  it('shows an error message when loading submissions fails', async () => {
    mockedApi.listSubmissions.mockRejectedValue(new ApiError(500, 'INTERNAL_ERROR', 'boom'));
    mockedApi.getJudgeQueueStatus.mockResolvedValue(queueStatus);

    mountTab();
    await flushPromises();

    expect(elementMocks.error).toHaveBeenCalled();
  });

  it('rejudges a submission after confirmation and refreshes the list', async () => {
    elementMocks.confirm.mockResolvedValue(undefined);
    mockedApi.listSubmissions.mockResolvedValue(submissionsPage);
    mockedApi.getJudgeQueueStatus.mockResolvedValue(queueStatus);
    mockedApi.rejudgeSubmission.mockResolvedValue({
      submissionId: 501,
      previousJudgementId: 'judge-1',
      judgementId: 'judge-2',
      status: 'QUEUED',
      queuedAt: '2026-01-01T10:00:11Z',
    });

    const wrapper = mountTab();
    await flushPromises();
    await findButton(wrapper, '重判').trigger('click');
    await flushPromises();

    expect(elementMocks.confirm).toHaveBeenCalled();
    expect(mockedApi.rejudgeSubmission).toHaveBeenCalledWith(1, 501, 'judge-1');
    expect(elementMocks.success).toHaveBeenCalled();
    expect(mockedApi.listSubmissions).toHaveBeenCalledTimes(2);
  });

  it('does not call the rejudge API when the confirmation is dismissed', async () => {
    elementMocks.confirm.mockRejectedValue('cancel');
    mockedApi.listSubmissions.mockResolvedValue(submissionsPage);
    mockedApi.getJudgeQueueStatus.mockResolvedValue(queueStatus);

    const wrapper = mountTab();
    await flushPromises();
    await findButton(wrapper, '重判').trigger('click');
    await flushPromises();

    expect(mockedApi.rejudgeSubmission).not.toHaveBeenCalled();
  });

  it('refreshes the list and warns when the judgement version is stale', async () => {
    elementMocks.confirm.mockResolvedValue(undefined);
    mockedApi.listSubmissions.mockResolvedValue(submissionsPage);
    mockedApi.getJudgeQueueStatus.mockResolvedValue(queueStatus);
    mockedApi.rejudgeSubmission.mockRejectedValue(
      new ApiError(409, 'JUDGEMENT_VERSION_STALE', 'JUDGEMENT_VERSION_STALE'),
    );

    const wrapper = mountTab();
    await flushPromises();
    await findButton(wrapper, '重判').trigger('click');
    await flushPromises();

    expect(elementMocks.error).toHaveBeenCalled();
    expect(mockedApi.listSubmissions).toHaveBeenCalledTimes(2);
  });

  it('downloads the scoreboard CSV export', async () => {
    const createObjectURL = vi.fn(() => 'blob:mock');
    const revokeObjectURL = vi.fn();
    Object.defineProperty(URL, 'createObjectURL', { value: createObjectURL, configurable: true });
    Object.defineProperty(URL, 'revokeObjectURL', { value: revokeObjectURL, configurable: true });

    const blob = new Blob(['id,team\n501,7']);
    mockedApi.listSubmissions.mockResolvedValue(submissionsPage);
    mockedApi.getJudgeQueueStatus.mockResolvedValue(queueStatus);
    mockedApi.exportScoreboard.mockResolvedValue(blob);

    const wrapper = mountTab();
    await flushPromises();
    await findButton(wrapper, '榜单 CSV').trigger('click');
    await flushPromises();

    expect(mockedApi.exportScoreboard).toHaveBeenCalledWith(1);
    expect(createObjectURL).toHaveBeenCalledWith(blob);
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:mock');
    expect(elementMocks.success).toHaveBeenCalled();
  });

  it('scans similarity pairs with the selected problem and threshold', async () => {
    mockedApi.listSubmissions.mockResolvedValue(submissionsPage);
    mockedApi.getJudgeQueueStatus.mockResolvedValue(queueStatus);
    mockedApi.listSubmissionSimilarityPairs.mockResolvedValue([similarityPair]);

    const wrapper = mountTab();
    await flushPromises();
    await findButton(wrapper, '扫描候选').trigger('click');
    await flushPromises();

    expect(mockedApi.listSubmissionSimilarityPairs).toHaveBeenCalledWith(1, {
      problemId: undefined,
      minSimilarityPercent: 85,
    });
    expect(wrapper.text()).toContain('94%');
  });

  it('reports backfill progress and reloads the candidate pairs', async () => {
    mockedApi.listSubmissions.mockResolvedValue(submissionsPage);
    mockedApi.getJudgeQueueStatus.mockResolvedValue(queueStatus);
    mockedApi.backfillSubmissionSimilarity.mockResolvedValue(backfillResult);
    mockedApi.listSubmissionSimilarityPairs.mockResolvedValue([]);

    const wrapper = mountTab();
    await flushPromises();
    await findButton(wrapper, '历史回填').trigger('click');
    await flushPromises();

    expect(mockedApi.backfillSubmissionSimilarity).toHaveBeenCalledWith(1);
    expect(elementMocks.success).toHaveBeenCalledWith('已扫描 5，更新 4，失败 1');
    expect(mockedApi.listSubmissionSimilarityPairs).toHaveBeenCalled();
  });
});

describe('TeamsTab', () => {
  function mountTab() {
    return mount(TeamsTab, {
      props: {
        contestId: 1,
        teams: [team, otherTeam],
        contestTeams: [contestTeam],
      },
    });
  }

  it('only offers teams that are not yet assigned', async () => {
    const wrapper = mountTab();
    await flushPromises();
    // The available teams drive the option list of the first ElSelect (the
    // team picker); script-setup internals are not exposed on the vm proxy.
    const optionValues = wrapper
      .findComponent({ name: 'ElSelect' })
      .findAllComponents({ name: 'ElOption' })
      .map((option) => option.props('value'));
    expect(optionValues).toEqual([8]);
  });

  it('assigns a team and refreshes the contest team list', async () => {
    mockedApi.assignTeam.mockResolvedValue(contestTeam);
    mockedApi.listContestTeams.mockResolvedValue([contestTeam]);

    const wrapper = mountTab();
    // Drive the v-model on the first ElSelect (the team picker) instead of
    // mutating script-setup internals, which the vm proxy does not expose.
    const teamSelect = wrapper.findComponent({ name: 'ElSelect' });
    teamSelect.vm.$emit('update:modelValue', 8);
    await wrapper.vm.$nextTick();
    await findButton(wrapper, '分配队伍').trigger('click');
    await flushPromises();

    expect(mockedApi.assignTeam).toHaveBeenCalledWith(1, {
      teamId: 8,
      participationType: 'OFFICIAL',
      groupName: null,
    });
    expect(mockedApi.listContestTeams).toHaveBeenCalledWith(1);
    expect(wrapper.emitted('update:contest-teams')).toEqual([[ [contestTeam] ]]);
    expect(elementMocks.success).toHaveBeenCalled();
  });

  it('shows an error message when assigning fails', async () => {
    mockedApi.assignTeam.mockRejectedValue(new ApiError(409, 'CONTEST_TEAM_ALREADY_ASSIGNED', 'x'));

    const wrapper = mountTab();
    const teamSelect = wrapper.findComponent({ name: 'ElSelect' });
    teamSelect.vm.$emit('update:modelValue', 8);
    await wrapper.vm.$nextTick();
    await findButton(wrapper, '分配队伍').trigger('click');
    await flushPromises();

    expect(elementMocks.error).toHaveBeenCalled();
  });

  it('unassigns a team after confirmation', async () => {
    elementMocks.confirm.mockResolvedValue(undefined);
    mockedApi.unassignTeam.mockResolvedValue(undefined);
    mockedApi.listContestTeams.mockResolvedValue([]);

    const wrapper = mountTab();
    await flushPromises();
    await findButton(wrapper, '移除').trigger('click');
    await flushPromises();

    expect(elementMocks.confirm).toHaveBeenCalled();
    expect(mockedApi.unassignTeam).toHaveBeenCalledWith(1, 7);
    expect(wrapper.emitted('update:contest-teams')).toEqual([[[]]]);
  });

  it('stays silent when the unassign confirmation is dismissed', async () => {
    elementMocks.confirm.mockRejectedValue('cancel');

    const wrapper = mountTab();
    await flushPromises();
    await findButton(wrapper, '移除').trigger('click');
    await flushPromises();

    expect(mockedApi.unassignTeam).not.toHaveBeenCalled();
    expect(elementMocks.error).not.toHaveBeenCalled();
  });
});
