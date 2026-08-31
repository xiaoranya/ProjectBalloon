import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ResolverDisplayView from './ResolverDisplayView.vue';
import ResolverManageView from './ResolverManageView.vue';
import { contestApi } from '../api/contest';
import { ApiError } from '../api/client';
import { resolverApi } from '../api/resolver';
import { subscribeContestEvents } from '../realtime/contest-events';
import { setLocale } from '../i18n';

const elementMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  confirm: vi.fn(),
}));
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return {
    ...actual,
    ElMessage: { success: elementMocks.success, error: elementMocks.error },
    ElMessageBox: { confirm: elementMocks.confirm },
  };
});

const replace = vi.fn();
let route = { params: { runId: '9' }, query: { contestId: '7', runId: '9' } };
vi.mock('vue-router', () => ({ useRoute: () => route, useRouter: () => ({ replace }) }));
vi.mock('../api/contest', () => ({ contestApi: { listContests: vi.fn(), getContest: vi.fn() } }));
vi.mock('../api/resolver', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/resolver')>();
  return {
    ...actual,
    resolverApi: {
      list: vi.fn(),
      sources: vi.fn(),
      create: vi.fn(),
      get: vi.fn(),
      publicState: vi.fn(),
      events: vi.fn(),
      start: vi.fn(),
      next: vi.fn(),
      previous: vi.fn(),
      pause: vi.fn(),
      resume: vi.fn(),
      complete: vi.fn(),
      autoPlay: vi.fn(),
    },
  };
});
vi.mock('../realtime/contest-events', () => ({
  subscribeContestEvents: vi.fn(() => ({ stop: vi.fn() })),
}));

const run = {
  id: 9,
  contestId: 7,
  official: true,
  status: 'RUNNING' as const,
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
      scoringMode: 'ICPC' as const,
      scoreAggregation: 'BEST' as const,
      generatedAt: '2026-07-20T08:00:00Z',
      problems: [
        { problemId: 1, alias: 'A', displayOrder: 1, firstBloodTeamId: null, firstBloodAt: null },
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
          solvedCount: 0,
          penaltyMinutes: 0,
          totalScoreMilli: 0,
          lastSolvedAt: null,
          problems: [
            {
              problemId: 1,
              wrongAttempts: 0,
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

describe('Resolver views', () => {
  beforeEach(() => {
    setLocale('zh-CN');
    route = { params: { runId: '9' }, query: { contestId: '7', runId: '9' } };
    replace.mockReset();
    elementMocks.success.mockReset();
    elementMocks.error.mockReset();
    elementMocks.confirm.mockReset();
    vi.mocked(contestApi.listContests).mockResolvedValue({
      content: [{ id: 7, name: 'Contest 7' }],
    } as Awaited<ReturnType<typeof contestApi.listContests>>);
    vi.mocked(contestApi.getContest).mockResolvedValue({ id: 7, name: 'Contest 7' } as Awaited<
      ReturnType<typeof contestApi.getContest>
    >);
    vi.mocked(resolverApi.list).mockResolvedValue([run]);
    vi.mocked(resolverApi.sources).mockResolvedValue({
      publicSnapshot: { id: 11, version: 2, generatedAt: '', payloadSha256: 'a' },
      finalSnapshot: { id: 12, version: 3, generatedAt: '', payloadSha256: 'b' },
    });
    vi.mocked(resolverApi.get).mockResolvedValue(run);
    vi.mocked(resolverApi.publicState).mockResolvedValue(run);
    vi.mocked(resolverApi.events).mockResolvedValue([]);
    vi.mocked(resolverApi.next).mockResolvedValue({ ...run, currentStep: 1, version: 3 });
    vi.mocked(resolverApi.pause).mockResolvedValue({ ...run, status: 'PAUSED', version: 3 });
    vi.mocked(resolverApi.complete).mockResolvedValue({ ...run, status: 'COMPLETED', version: 3 });
    vi.mocked(resolverApi.autoPlay).mockResolvedValue({
      ...run,
      autoPlayEnabled: true,
      version: 3,
    });
  });

  it('recovers an existing run by contest and uses its current version for the next command', async () => {
    const wrapper = mount(ResolverManageView);
    await flushPromises();
    expect(resolverApi.list).toHaveBeenCalledWith(7);
    expect(resolverApi.sources).toHaveBeenCalledWith(7);
    const next = wrapper.findAll('button').find((button) => button.text().includes('揭晓下一步'))!;
    await next.trigger('click');
    await flushPromises();
    expect(resolverApi.next).toHaveBeenCalledWith(9, 2);
    wrapper.unmount();
  });

  it('renders run controls in English', async () => {
    setLocale('en');
    const wrapper = mount(ResolverManageView);
    await flushPromises();

    expect(wrapper.text()).toContain('Resolver Run Console');
    expect(wrapper.text()).toContain('Reveal Next Step');
    expect(wrapper.text()).not.toContain('运行控制台');
    wrapper.unmount();
  });

  it('loads only the public run state and subscribes to public Resolver events', async () => {
    const wrapper = mount(ResolverDisplayView);
    await flushPromises();
    expect(resolverApi.publicState).toHaveBeenCalledWith(9);
    expect(wrapper.text()).toContain('Resolver Team');
    expect(subscribeContestEvents).toHaveBeenCalledWith(
      expect.objectContaining({
        contestId: 7,
        scope: 'PUBLIC',
        eventTypes: ['RESOLVER_STATE_CHANGED'],
      }),
    );
    wrapper.unmount();
  });

  it('guards completion until every step is revealed, then completes after confirmation', async () => {
    const wrapper = mount(ResolverManageView);
    await flushPromises();
    const locked = wrapper
      .findAll('button')
      .find((button) => button.text().includes('完成 Resolver'))!;
    expect(locked.attributes('disabled')).toBeDefined();
    wrapper.unmount();

    vi.mocked(resolverApi.get).mockResolvedValue({ ...run, currentStep: 1 });
    const ready = mount(ResolverManageView);
    await flushPromises();
    const complete = ready
      .findAll('button')
      .find((button) => button.text().includes('完成 Resolver'))!;
    expect(complete.attributes('disabled')).toBeUndefined();
    elementMocks.confirm.mockResolvedValue(undefined);
    await complete.trigger('click');
    await flushPromises();
    expect(elementMocks.confirm).toHaveBeenCalled();
    expect(resolverApi.complete).toHaveBeenCalledWith(9, 2);
    expect(elementMocks.success).toHaveBeenCalled();
    ready.unmount();
  });

  it('keeps the run open when completion is dismissed', async () => {
    vi.mocked(resolverApi.get).mockResolvedValue({ ...run, currentStep: 1 });
    elementMocks.confirm.mockRejectedValue('cancel');
    const wrapper = mount(ResolverManageView);
    await flushPromises();
    const completeCalls = vi.mocked(resolverApi.complete).mock.calls.length;
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('完成 Resolver'))!
      .trigger('click');
    await flushPromises();
    expect(vi.mocked(resolverApi.complete).mock.calls.length).toBe(completeCalls);
    expect(elementMocks.error).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it('reports command failures and silently resyncs the run', async () => {
    vi.mocked(resolverApi.next).mockRejectedValueOnce(
      new ApiError(409, 'RESOLVER_VERSION_CONFLICT', 'resolver version conflict'),
    );
    const wrapper = mount(ResolverManageView);
    await flushPromises();
    const getCalls = vi.mocked(resolverApi.get).mock.calls.length;
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('揭晓下一步'))!
      .trigger('click');
    await flushPromises();
    expect(elementMocks.error).toHaveBeenCalled();
    expect(vi.mocked(resolverApi.get).mock.calls.length).toBe(getCalls + 1);
    expect(
      wrapper
        .findAll('button')
        .find((button) => button.text().includes('揭晓下一步'))!
        .attributes('disabled'),
    ).toBeUndefined();
    wrapper.unmount();
  });

  it('toggles auto play and pauses with the current version', async () => {
    const wrapper = mount(ResolverManageView);
    await flushPromises();
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('启动自动播放'))!
      .trigger('click');
    await flushPromises();
    expect(resolverApi.autoPlay).toHaveBeenCalledWith(9, 2, true, 3000);
    expect(wrapper.text()).toContain('停止自动播放');
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('暂停'))!
      .trigger('click');
    await flushPromises();
    expect(resolverApi.pause).toHaveBeenCalledWith(9, 3);
    expect(wrapper.text()).toContain('恢复');
    wrapper.unmount();
  });

  it('creates a rehearsal run after confirmation and selects it', async () => {
    vi.mocked(resolverApi.create).mockResolvedValue({
      ...run,
      id: 13,
      official: false,
      status: 'READY',
      version: 1,
    });
    elementMocks.confirm.mockResolvedValue(undefined);
    const wrapper = mount(ResolverManageView);
    await flushPromises();
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('创建预演'))!
      .trigger('click');
    await flushPromises();
    expect(resolverApi.create).toHaveBeenCalledWith(7, 11, 12, false);
    expect(replace).toHaveBeenCalledWith(
      expect.objectContaining({ query: expect.objectContaining({ runId: '13' }) }),
    );
    expect(elementMocks.success).toHaveBeenCalled();
    wrapper.unmount();
  });

  it('aborts run creation when the confirmation is dismissed', async () => {
    elementMocks.confirm.mockRejectedValue('cancel');
    const wrapper = mount(ResolverManageView);
    await flushPromises();
    const createCalls = vi.mocked(resolverApi.create).mock.calls.length;
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('创建预演'))!
      .trigger('click');
    await flushPromises();
    expect(vi.mocked(resolverApi.create).mock.calls.length).toBe(createCalls);
    expect(elementMocks.error).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
