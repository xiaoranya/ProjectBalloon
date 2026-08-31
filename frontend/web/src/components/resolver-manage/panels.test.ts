import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ResolverControlCard from './ResolverControlCard.vue';
import ResolverMetricsRow from './ResolverMetricsRow.vue';
import ResolverRunCard from './ResolverRunCard.vue';
import ResolverWorkspaceRow from './ResolverWorkspaceRow.vue';
import type { ResolverEvent, ResolverRun } from '../../api/resolver';

const baseRun: ResolverRun = {
  id: 9,
  contestId: 7,
  official: true,
  status: 'RUNNING',
  currentStep: 0,
  totalSteps: 1,
  sourcePublicSnapshotId: 11,
  sourceFinalSnapshotId: 12,
  planSha256: 'a'.repeat(64),
  createdByUserId: 3,
  startedAt: '2026-07-20T08:00:00Z',
  completedAt: null,
  autoPlayEnabled: false,
  autoPlayIntervalMilliseconds: 3000,
  nextAutoAt: null,
  createdAt: '2026-07-20T08:00:00Z',
  updatedAt: '2026-07-20T08:00:00Z',
  version: 2,
  state: {
    stepIndex: 0,
    totalSteps: 1,
    lastReveal: null,
    board: {
      contestId: 7,
      variant: 'PUBLIC',
      frozen: true,
      scoringMode: 'ICPC',
      scoreAggregation: 'BEST',
      generatedAt: '2026-07-20T08:00:00Z',
      problems: [
        { problemId: 1, alias: 'A', displayOrder: 1, firstBloodTeamId: null, firstBloodAt: null },
        { problemId: 2, alias: 'B', displayOrder: 2, firstBloodTeamId: null, firstBloodAt: null },
      ],
      rows: [
        {
          rank: 1,
          officialRank: 1,
          teamId: 2,
          teamName: 'Resolver Team',
          school: 'School',
          participationType: 'OFFICIAL',
          groupName: null,
          isStar: false,
          solvedCount: 1,
          penaltyMinutes: 12,
          totalScoreMilli: 0,
          lastSolvedAt: null,
          problems: [
            {
              problemId: 1,
              wrongAttempts: 1,
              solved: true,
              solvedAt: '2026-07-20T08:12:00Z',
              penaltyMinutes: 12,
              scoreMilli: 0,
              firstBlood: false,
            },
            {
              problemId: 2,
              wrongAttempts: 2,
              solved: false,
              solvedAt: null,
              penaltyMinutes: 0,
              scoreMilli: 0,
              firstBlood: false,
            },
          ],
        },
      ],
    },
  },
};

const event: ResolverEvent = {
  id: 500,
  eventType: 'NEXT',
  payload: {},
  sequence: 3,
  actorUserId: 3,
  createdAt: '2026-07-20T08:01:00Z',
};

function button(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((candidate) => candidate.text().includes(text))!;
}

describe('resolver-manage panels', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('ResolverMetricsRow reflects run state and sync mode', () => {
    const wrapper = mount(ResolverMetricsRow, { props: { run: baseRun, realtimeConnected: true } });
    expect(wrapper.text()).toContain('运行状态');
    expect(wrapper.text()).toContain('运行中');
    expect(wrapper.text()).toContain('0 / 1');
    expect(wrapper.text()).toContain('正式');
    expect(wrapper.text()).toContain('SSE 实时');
    wrapper.unmount();

    const idle = mount(ResolverMetricsRow, { props: { run: null, realtimeConnected: false } });
    expect(idle.text()).toContain('未选择');
    expect(idle.text()).toContain('—');
    expect(idle.text()).toContain('轮询校准');
    idle.unmount();
  });

  it('ResolverRunCard disables creation without snapshot sources', async () => {
    const onSelect = vi.fn();
    const wrapper = mount(ResolverRunCard, {
      props: {
        runs: [baseRun, { ...baseRun, id: 10, official: false, status: 'READY' as const }],
        sources: {
          publicSnapshot: { id: 11, version: 2, generatedAt: '', payloadSha256: 'a' },
          finalSnapshot: { id: 12, version: 3, generatedAt: '', payloadSha256: 'b' },
        },
        hasOfficial: true,
        acting: false,
        runId: 9,
        'onUpdate:runId': onSelect,
      },
    });
    expect(wrapper.text()).toContain('PUBLIC v2 · ADMIN v3');
    expect(wrapper.text()).toContain('正式');
    expect(wrapper.text()).toContain('预演');
    expect(button(wrapper, '创建正式运行').attributes('disabled')).toBeDefined();
    await button(wrapper, '创建预演').trigger('click');
    expect(wrapper.emitted('create-run')![0][0]).toBe(false);
    wrapper.unmount();

    const noSources = mount(ResolverRunCard, {
      props: {
        runs: [baseRun],
        sources: null,
        hasOfficial: false,
        acting: false,
        runId: 9,
        'onUpdate:runId': onSelect,
      },
    });
    expect(noSources.text()).toContain('尚未找到完整快照来源');
    expect(button(noSources, '创建预演').attributes('disabled')).toBeDefined();
    noSources.unmount();
  });

  it('ResolverControlCard gates single-step commands and toggles auto play label', async () => {
    const wrapper = mount(ResolverControlCard, {
      props: {
        run: baseRun,
        canNext: true,
        canPrevious: false,
        canComplete: false,
        acting: false,
        autoInterval: 3000,
      },
    });
    expect(button(wrapper, '暂停')).toBeTruthy();
    expect(button(wrapper, '回退一步').attributes('disabled')).toBeDefined();
    expect(button(wrapper, '完成 Resolver').attributes('disabled')).toBeDefined();
    await button(wrapper, '揭晓下一步').trigger('click');
    expect(wrapper.emitted('control')![0][0]).toBe('next');
    await button(wrapper, '启动自动播放').trigger('click');
    expect(wrapper.emitted('toggle-auto-play')).toHaveLength(1);
    wrapper.unmount();

    const ready = mount(ResolverControlCard, {
      props: {
        run: { ...baseRun, status: 'READY', autoPlayEnabled: true },
        canNext: false,
        canPrevious: false,
        canComplete: false,
        acting: false,
        autoInterval: 1500,
      },
    });
    await button(ready, '开始').trigger('click');
    expect(ready.emitted('control')![0][0]).toBe('start');
    expect(ready.text()).toContain('停止自动播放');
    ready.unmount();

    const paused = mount(ResolverControlCard, {
      props: {
        run: { ...baseRun, status: 'PAUSED' },
        canNext: false,
        canPrevious: true,
        canComplete: true,
        acting: false,
        autoInterval: 3000,
      },
    });
    await button(paused, '恢复').trigger('click');
    expect(paused.emitted('control')![0][0]).toBe('resume');
    expect(button(paused, '完成 Resolver').attributes('disabled')).toBeUndefined();
    paused.unmount();
  });

  it('ResolverWorkspaceRow shows the reveal focus, history, and board cells', async () => {
    const revealing: ResolverRun = {
      ...baseRun,
      currentStep: 1,
      state: {
        ...baseRun.state,
        stepIndex: 1,
        lastReveal: {
          teamId: 2,
          problemId: 1,
          before: {
            problemId: 1,
            wrongAttempts: 1,
            solved: false,
            solvedAt: null,
            penaltyMinutes: 0,
            scoreMilli: 0,
            firstBlood: false,
          },
          after: {
            problemId: 1,
            wrongAttempts: 1,
            solved: true,
            solvedAt: '2026-07-20T08:12:00Z',
            penaltyMinutes: 12,
            scoreMilli: 0,
            firstBlood: false,
          },
        },
      },
    };
    const wrapper = mount(ResolverWorkspaceRow, { props: { run: revealing, events: [event] } });
    await flushPromises();
    expect(wrapper.text()).toContain('最近揭晓');
    expect(wrapper.text()).toContain('Resolver Team');
    expect(wrapper.text()).toContain('#1');
    expect(wrapper.text()).toContain('ACCEPTED');
    expect(wrapper.text()).toContain('A +');
    expect(wrapper.text()).toContain('B -2');
    expect(wrapper.text()).toContain('揭晓下一步');
    expect(wrapper.text()).toContain('最近 1 条');
    wrapper.unmount();

    const idle = mount(ResolverWorkspaceRow, { props: { run: baseRun, events: [] } });
    await flushPromises();
    expect(idle.text()).toContain('尚未揭晓步骤');
    idle.unmount();
  });
});
