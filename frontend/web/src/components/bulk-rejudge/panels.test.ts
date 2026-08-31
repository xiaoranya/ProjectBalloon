import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import RejudgeConfirmPanel from './RejudgeConfirmPanel.vue';
import RejudgeFilterPanel from './RejudgeFilterPanel.vue';
import RejudgeTaskDetailDialog from './RejudgeTaskDetailDialog.vue';
import RejudgeTasksTable from './RejudgeTasksTable.vue';
import type { BatchRejudgePreview, BatchRejudgeTask } from '../../api/bulk-rejudge';
import type { ContestProblem, ContestTeamResponse } from '../../api/types';

interface RejudgeFilterState {
  problemId: number | null;
  teamId: number | null;
  language: string | null;
  verdict: 'ACCEPTED' | null;
  submittedRange: [Date, Date] | null;
}

const problem: ContestProblem = {
  contestId: 42,
  problemId: 3,
  alias: 'A',
  displayOrder: 1,
  color: '#ef4444',
  slug: 'balloon',
  title: 'Balloon',
  timeLimitMs: 1000,
  memoryLimitMb: 256,
  outputLimitKb: 1024,
  languages: ['cpp'],
  statement: null,
};
const team: ContestTeamResponse = {
  id: 1,
  contestId: 42,
  teamId: 8,
  teamName: 'Team Eight',
  participationType: 'OFFICIAL',
  groupName: null,
  createdAt: '2026-07-20T08:00:00Z',
};

const runningTask: BatchRejudgeTask = {
  id: 5,
  contestId: 42,
  status: 'RUNNING',
  totalItems: 2,
  processedItems: 1,
  succeededItems: 1,
  failedItems: 0,
  cancelRequested: false,
  createdByUserId: 7,
  startedAt: '2026-07-20T08:00:00Z',
  completedAt: null,
  createdAt: '2026-07-20T08:00:00Z',
  updatedAt: '2026-07-20T08:00:01Z',
  items: [],
  itemsTruncated: false,
};
const pausedTask: BatchRejudgeTask = {
  ...runningTask,
  id: 6,
  status: 'PAUSED',
  failedItems: 1,
};
const completedTask: BatchRejudgeTask = {
  ...runningTask,
  id: 7,
  status: 'COMPLETED',
  processedItems: 2,
  completedAt: '2026-07-20T08:10:00Z',
};

function button(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((candidate) => candidate.text().includes(text))!;
}

describe('bulk-rejudge panels', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('RejudgeFilterPanel previews and warns about stale previews', async () => {
    const onUpdateFilter = vi.fn();
    const wrapper = mount(RejudgeFilterPanel, {
      props: {
        contestProblems: [problem],
        contestTeams: [team],
        verdictOptions: [{ value: 'ACCEPTED' as const, label: '通过' }],
        previewing: false,
        previewResult: null,
        previewStale: false,
        filter: {
          problemId: null,
          teamId: null,
          language: null,
          verdict: null,
          submittedRange: null,
        } as RejudgeFilterState,
        'onUpdate:filter': onUpdateFilter,
      },
    });
    expect(wrapper.text()).toContain('1. 筛选与预览');
    expect(wrapper.find('input[placeholder="起始时间"]').exists()).toBe(true);
    await button(wrapper, '预览影响范围').trigger('click');
    expect(wrapper.emitted('preview')).toHaveLength(1);
    expect(wrapper.text()).not.toContain('筛选条件已变化');
    wrapper.unmount();

    const stale = mount(RejudgeFilterPanel, {
      props: {
        contestProblems: [problem],
        contestTeams: [team],
        verdictOptions: [],
        previewing: false,
        previewResult: { matchedSubmissions: 2 } as BatchRejudgePreview,
        previewStale: true,
        filter: {
          problemId: null,
          teamId: null,
          language: null,
          verdict: null,
          submittedRange: null,
        } as RejudgeFilterState,
        'onUpdate:filter': onUpdateFilter,
      },
    });
    expect(stale.text()).toContain('筛选条件已变化，当前预览已失效，请重新预览。');
    stale.unmount();
  });

  it('RejudgeConfirmPanel gates creation behind the preview and confirmation text', async () => {
    const updateKey = vi.fn();
    const updateText = vi.fn();
    const base = {
      creating: false,
      canCreate: false,
      confirmationRequirement: '确认',
      idempotencyKey: '00000000-0000-4000-8000-000000000001',
      confirmationText: '',
      'onUpdate:idempotencyKey': updateKey,
      'onUpdate:confirmationText': updateText,
    };
    const empty = mount(RejudgeConfirmPanel, {
      props: { ...base, previewResult: null },
    });
    expect(empty.text()).toContain('先预览筛选结果');
    empty.unmount();

    const ready = mount(RejudgeConfirmPanel, {
      props: { ...base, previewResult: { matchedSubmissions: 2 } as BatchRejudgePreview },
    });
    expect(ready.text()).toContain('匹配提交');
    expect(button(ready, '创建批量重判任务').attributes('disabled')).toBeDefined();
    const regenerate = ready
      .findAll('button')
      .find((row) => row.attributes('aria-label')?.includes('幂等键'))!;
    await regenerate.trigger('click');
    expect(ready.emitted('regenerate-key')).toHaveLength(1);
    const inputs = ready.findAll('input.el-input__inner');
    await inputs[0].setValue('key-2');
    expect(updateKey).toHaveBeenCalledWith('key-2');
    await inputs[1].setValue('确认');
    expect(updateText).toHaveBeenCalledWith('确认');
    ready.unmount();

    const enabled = mount(RejudgeConfirmPanel, {
      props: {
        ...base,
        canCreate: true,
        previewResult: { matchedSubmissions: 2 } as BatchRejudgePreview,
      },
    });
    expect(button(enabled, '创建批量重判任务').attributes('disabled')).toBeUndefined();
    await button(enabled, '创建批量重判任务').trigger('click');
    expect(enabled.emitted('create')).toHaveLength(1);
    enabled.unmount();

    const zero = mount(RejudgeConfirmPanel, {
      props: { ...base, previewResult: { matchedSubmissions: 0 } as BatchRejudgePreview },
    });
    expect(zero.text()).toContain('当前筛选没有可重判的已完成提交。');
    zero.unmount();

    const tooMany = mount(RejudgeConfirmPanel, {
      props: { ...base, previewResult: { matchedSubmissions: 10_001 } as BatchRejudgePreview },
    });
    expect(tooMany.text()).toContain('匹配数量超过单任务上限，请缩小筛选范围。');
    tooMany.unmount();
  });

  it('RejudgeTasksTable shows progress and action buttons per status', async () => {
    const wrapper = mount(RejudgeTasksTable, {
      props: {
        tasks: [runningTask, pausedTask, completedTask],
        tasksLoading: false,
        polling: true,
        mutatingTaskId: null,
      },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('自动刷新中');
    expect(wrapper.text()).toContain('执行中');
    expect(wrapper.text()).toContain('已暂停');
    expect(wrapper.text()).toContain('已完成');
    expect(wrapper.text()).toContain('1 / 2 · 成功 1 · 失败 0');
    const rows = wrapper.findAll('.el-table__row');
    expect(rows).toHaveLength(3);
    const pauseButtons = wrapper.findAll('button').filter((row) => row.text().includes('暂停'));
    expect(pauseButtons).toHaveLength(1);
    await pauseButtons[0].trigger('click');
    expect(wrapper.emitted('pause')![0][0]).toEqual(runningTask);
    await wrapper
      .findAll('button')
      .find((row) => row.text().includes('恢复'))!
      .trigger('click');
    expect(wrapper.emitted('resume')![0][0]).toEqual(pausedTask);
    await wrapper
      .findAll('button')
      .find((row) => row.text().includes('查看明细'))!
      .trigger('click');
    expect(wrapper.emitted('select')![0][0]).toBe(5);
    wrapper.unmount();

    const empty = mount(RejudgeTasksTable, {
      props: { tasks: [], tasksLoading: false, polling: false, mutatingTaskId: null },
    });
    await flushPromises();
    expect(empty.text()).toContain('暂无活动任务');
    expect(empty.text()).toContain('尚未创建批量重判任务');
    empty.unmount();
  });

  it('RejudgeTaskDetailDialog renders item rows and the truncation warning', async () => {
    const detail: BatchRejudgeTask = {
      ...runningTask,
      status: 'COMPLETED',
      processedItems: 2,
      itemsTruncated: true,
      items: [
        {
          id: 100,
          submissionId: 900,
          status: 'SUCCEEDED',
          oldJudgementId: '11111111-1111-4111-8111-111111111111',
          newJudgementId: '22222222-2222-4222-8222-222222222222',
          errorMessage: null,
          attempts: 1,
          processedAt: '2026-07-20T08:01:00Z',
        },
        {
          id: 101,
          submissionId: 901,
          status: 'FAILED',
          oldJudgementId: null,
          newJudgementId: null,
          errorMessage: 'judge worker lost',
          attempts: 3,
          processedAt: '2026-07-20T08:02:00Z',
        },
      ],
    };
    const wrapper = mount(RejudgeTaskDetailDialog, {
      props: { detailLoading: false, selectedTask: detail, detailVisible: true },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('批量重判任务明细');
    expect(wrapper.text()).toContain('2 / 2');
    expect(wrapper.text()).toContain('任务共有 2 条，明细仅返回按 ID 排序的前 1,000 条。');
    expect(wrapper.text()).toContain('900');
    expect(wrapper.text()).toContain('成功');
    expect(wrapper.text()).toContain('失败');
    expect(wrapper.text()).toContain('judge worker lost');
    expect(wrapper.text()).toContain('11111111-1111-4111-8111-111111111111');
    expect(wrapper.text()).toContain('—');
    wrapper.unmount();

    const loading = mount(RejudgeTaskDetailDialog, {
      props: { detailLoading: true, selectedTask: null, detailVisible: true },
    });
    await flushPromises();
    expect(loading.find('.el-skeleton').exists()).toBe(true);
    loading.unmount();
  });
});
