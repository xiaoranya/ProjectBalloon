import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import AdminBulkRejudgeView from './AdminBulkRejudgeView.vue';
import { ApiError } from '../api/client';

const mocks = vi.hoisted(() => ({
  getContest: vi.fn(), listContestProblems: vi.fn(), listContestTeams: vi.fn(),
  preview: vi.fn(), create: vi.fn(), list: vi.fn(), get: vi.fn(), pause: vi.fn(), resume: vi.fn(),
  push: vi.fn(), confirm: vi.fn(), success: vi.fn(), error: vi.fn(),
}));
vi.mock('../api/admin-contests', () => ({ adminContestApi: {
  getContest: mocks.getContest,
  listContestProblems: mocks.listContestProblems,
  listContestTeams: mocks.listContestTeams,
} }));
vi.mock('../api/bulk-rejudge', () => ({ bulkRejudgeApi: {
  preview: mocks.preview,
  create: mocks.create,
  list: mocks.list,
  get: mocks.get,
  pause: mocks.pause,
  resume: mocks.resume,
} }));
vi.mock('vue-router', () => ({
  useRoute: () => ({ params: { contestId: '42' } }),
  useRouter: () => ({ push: mocks.push }),
}));
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return {
    ...actual,
    ElMessage: { success: mocks.success, error: mocks.error },
    ElMessageBox: { confirm: mocks.confirm },
  };
});

const baseTask = {
  id: 5, contestId: 42, status: 'PAUSED', totalItems: 2, processedItems: 1,
  succeededItems: 1, failedItems: 0, cancelRequested: true, createdByUserId: 7,
  startedAt: '2026-07-20T08:00:00Z', completedAt: null,
  createdAt: '2026-07-20T08:00:00Z', updatedAt: '2026-07-20T08:00:01Z',
  items: [], itemsTruncated: false,
};

function button(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((candidate) => candidate.text().includes(text))!;
}

async function previewTwo(wrapper: ReturnType<typeof mount>) {
  mocks.preview.mockResolvedValueOnce({ matchedSubmissions: 2 });
  await button(wrapper, '预览影响范围').trigger('click');
  await flushPromises();
}

describe('AdminBulkRejudgeView', () => {
  beforeEach(() => {
    mocks.getContest.mockResolvedValue({ id: 42, name: 'Rust Regional' });
    mocks.listContestProblems.mockResolvedValue([{ problemId: 3, alias: 'A', title: 'Balloon', displayOrder: 1 }]);
    mocks.listContestTeams.mockResolvedValue([{ teamId: 8, teamName: 'Team Eight' }]);
    mocks.list.mockResolvedValue([]);
    mocks.confirm.mockResolvedValue(undefined);
    mocks.resume.mockResolvedValue({ ...baseTask, status: 'RUNNING', cancelRequested: false });
    vi.stubGlobal('crypto', { randomUUID: () => '00000000-0000-4000-8000-000000000001' });
  });

  afterEach(() => vi.clearAllMocks());

  it('loads only contest-scoped context and previews a camelCase filter without contestId', async () => {
    const wrapper = mount(AdminBulkRejudgeView);
    await flushPromises();
    await previewTwo(wrapper);

    expect(mocks.getContest).toHaveBeenCalledWith(42);
    expect(mocks.listContestProblems).toHaveBeenCalledWith(42);
    expect(mocks.listContestTeams).toHaveBeenCalledWith(42);
    expect(mocks.preview).toHaveBeenCalledWith(42, {
      problemId: null, teamId: null, language: null, verdict: null, submittedFrom: null, submittedTo: null,
    });
    expect(wrapper.text()).toContain('2');
    expect(wrapper.text()).toContain('REJUDGE 2');
  });

  it('invalidates a preview after a filter changes and blocks creation', async () => {
    const wrapper = mount(AdminBulkRejudgeView);
    await flushPromises();
    await previewTwo(wrapper);

    const selects = wrapper.findAllComponents({ name: 'ElSelect' });
    await selects[2].setValue('cpp');
    await flushPromises();

    expect(wrapper.text()).toContain('当前预览已失效');
    expect(button(wrapper, '创建批量重判任务').attributes('disabled')).toBeDefined();
  });

  it('creates with exact confirmation and keeps the generated key for safe retry', async () => {
    mocks.create.mockResolvedValueOnce(baseTask);
    const wrapper = mount(AdminBulkRejudgeView);
    await flushPromises();
    await previewTwo(wrapper);

    const confirmation = wrapper.find('input[placeholder="REJUDGE 2"]');
    await confirmation.setValue('REJUDGE 2');
    await button(wrapper, '创建批量重判任务').trigger('click');
    await flushPromises();

    expect(mocks.create).toHaveBeenCalledWith(42, expect.objectContaining({
      expectedCount: 2,
      confirmationText: 'REJUDGE 2',
      idempotencyKey: 'batch-rejudge-42-00000000-0000-4000-8000-000000000001',
    }));
    expect(wrapper.findAll('input').some((input) => input.element.value === 'batch-rejudge-42-00000000-0000-4000-8000-000000000001')).toBe(true);
  });

  it('clears a stale preview when Rust returns BATCH_REJUDGE_COUNT_CHANGED', async () => {
    mocks.create.mockRejectedValueOnce(new ApiError(409, 'BATCH_REJUDGE_COUNT_CHANGED', 'changed'));
    const wrapper = mount(AdminBulkRejudgeView);
    await flushPromises();
    await previewTwo(wrapper);
    await wrapper.find('input[placeholder="REJUDGE 2"]').setValue('REJUDGE 2');
    await button(wrapper, '创建批量重判任务').trigger('click');
    await flushPromises();

    expect(mocks.error).toHaveBeenCalledWith('符合条件的提交集合已变化，请重新预览并确认');
    expect(wrapper.text()).toContain('先预览筛选结果');
  });

  it('shows an idempotency conflict without silently changing the retry key', async () => {
    mocks.create.mockRejectedValueOnce(new ApiError(409, 'IDEMPOTENCY_KEY_REUSED', 'reused'));
    const wrapper = mount(AdminBulkRejudgeView);
    await flushPromises();
    await previewTwo(wrapper);
    await wrapper.find('input[placeholder="REJUDGE 2"]').setValue('REJUDGE 2');
    await button(wrapper, '创建批量重判任务').trigger('click');
    await flushPromises();

    expect(mocks.error).toHaveBeenCalledWith('该幂等键已用于其他批量重判，请生成新键后重试');
    expect(wrapper.findAll('input').some((input) => input.element.value === 'batch-rejudge-42-00000000-0000-4000-8000-000000000001')).toBe(true);
  });

  it('resumes a paused task without passing an expectedVersion', async () => {
    mocks.list.mockResolvedValueOnce([baseTask]);
    const wrapper = mount(AdminBulkRejudgeView);
    await flushPromises();

    await button(wrapper, '恢复').trigger('click');
    await flushPromises();

    expect(mocks.resume).toHaveBeenCalledWith(42, 5);
    expect(mocks.resume.mock.calls[0]).toHaveLength(2);
  });

  it('renders attempts and an explicit truncation warning in task detail', async () => {
    mocks.list.mockResolvedValueOnce([baseTask]);
    mocks.get.mockResolvedValueOnce({
      ...baseTask,
      totalItems: 1001,
      itemsTruncated: true,
      items: [{
        id: 11, submissionId: 9, status: 'FAILED', oldJudgementId: 'old', newJudgementId: null,
        errorMessage: 'worker failed', attempts: 3, processedAt: '2026-07-20T08:00:10Z',
      }],
    });
    const wrapper = mount(AdminBulkRejudgeView);
    await flushPromises();
    await button(wrapper, '查看明细').trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('前 1,000 条');
    expect(wrapper.text()).toContain('尝试次数');
    expect(wrapper.text()).toContain('worker failed');
    expect(wrapper.text()).toContain('3');
  });
});
