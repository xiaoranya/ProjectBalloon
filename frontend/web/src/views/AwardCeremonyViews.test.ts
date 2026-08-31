import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AwardDisplayView from './AwardDisplayView.vue';
import AwardHostScriptView from './AwardHostScriptView.vue';
import AwardPresentationControlView from './AwardPresentationControlView.vue';
import { awardsApi, type AwardHostScript, type AwardPresentation } from '../api/awards';
import { contestApi } from '../api/contest';
import { ApiError } from '../api/client';
import { subscribeContestEvents } from '../realtime/contest-events';

const elementMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
}));
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return {
    ...actual,
    ElMessage: { success: elementMocks.success, error: elementMocks.error },
  };
});

const replace = vi.fn();
vi.mock('vue-router', () => ({
  useRoute: () => ({ query: { contestId: '7' } }),
  useRouter: () => ({ replace }),
}));
vi.mock('../api/contest', () => ({ contestApi: { listContests: vi.fn() } }));
vi.mock('../api/awards', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/awards')>();
  return {
    ...actual,
    awardsApi: {
      ...actual.awardsApi,
      presentation: vi.fn(),
      updatePresentation: vi.fn(),
      hostScript: vi.fn(),
      saveHostScript: vi.fn(),
    },
  };
});
vi.mock('../realtime/contest-events', () => ({
  subscribeContestEvents: vi.fn(() => ({ stop: vi.fn() })),
}));

const presentation: AwardPresentation = {
  contestId: 7,
  contestName: 'Regional',
  contestStatus: 'ENDED',
  serverTime: '2026-07-22T01:00:00Z',
  status: 'WAITING',
  currentCategoryId: 1,
  autoRotate: false,
  intervalSeconds: 15,
  stateUpdatedAt: '2026-07-22T01:00:00Z',
  categories: [
    {
      id: 1,
      code: 'GOLD',
      name: '金奖',
      displayOrder: 1,
      groupName: null,
      firstBlood: false,
      recipients: [
        {
          id: 3,
          problemId: null,
          problemAlias: null,
          teamId: 8,
          teamName: 'Team Eight',
          school: 'School',
          seatNo: 'A08',
          groupName: null,
          participationType: 'OFFICIAL',
          star: false,
          rank: 1,
          solved: 8,
          penaltyMinutes: 600,
        },
      ],
    },
  ],
};
const hostScript: AwardHostScript = {
  contestId: 7,
  contestName: 'Regional',
  serverTime: presentation.serverTime,
  presentationStatus: 'PRESENTING',
  currentCategoryId: 1,
  nextCategoryId: null,
  autoRotate: false,
  intervalSeconds: 15,
  stateUpdatedAt: presentation.stateUpdatedAt,
  version: 2,
  updatedAt: presentation.serverTime,
  openingText: '欢迎',
  closingText: '结束',
  sections: [
    {
      categoryId: 1,
      code: 'GOLD',
      name: '金奖',
      firstBlood: false,
      current: true,
      cueText: '请上台',
      recipients: presentation.categories[0].recipients,
    },
  ],
};

describe('award ceremony views', () => {
  beforeEach(() => {
    replace.mockReset();
    elementMocks.success.mockReset();
    elementMocks.error.mockReset();
    vi.mocked(contestApi.listContests).mockResolvedValue({
      content: [{ id: 7, name: 'Regional' }],
    } as Awaited<ReturnType<typeof contestApi.listContests>>);
    vi.mocked(awardsApi.presentation).mockResolvedValue(presentation);
    vi.mocked(awardsApi.updatePresentation).mockResolvedValue({
      ...presentation,
      status: 'PRESENTING',
    });
    vi.mocked(awardsApi.hostScript).mockResolvedValue(hostScript);
    vi.mocked(awardsApi.saveHostScript).mockResolvedValue({ ...hostScript, version: 3 });
  });

  it('controls the public presentation and subscribes to award invalidations', async () => {
    const wrapper = mount(AwardPresentationControlView);
    await flushPromises();
    expect(awardsApi.presentation).toHaveBeenCalledWith(7);
    expect(subscribeContestEvents).toHaveBeenCalledWith(
      expect.objectContaining({ contestId: 7, scope: 'PUBLIC', eventTypes: ['AWARDS_UPDATED'] }),
    );
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('开始/继续'))!
      .trigger('click');
    await flushPromises();
    expect(awardsApi.updatePresentation).toHaveBeenCalledWith(
      7,
      expect.objectContaining({ status: 'PRESENTING', currentCategoryId: 1 }),
    );
    wrapper.unmount();
  });

  it('renders the frozen public recipient list without authentication', async () => {
    vi.mocked(awardsApi.presentation).mockResolvedValueOnce({
      ...presentation,
      status: 'PRESENTING',
    });
    const wrapper = mount(AwardDisplayView);
    await flushPromises();
    expect(wrapper.text()).toContain('Team Eight');
    expect(wrapper.text()).toContain('Regional');
    wrapper.unmount();
  });

  it('loads the host cue sheet and saves with its displayed version', async () => {
    const wrapper = mount(AwardHostScriptView);
    await flushPromises();
    expect(wrapper.text()).toContain('请上台');
    const textarea = wrapper.find('textarea');
    await textarea.setValue('新的开场');
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('保存脚本'))!
      .trigger('click');
    await flushPromises();
    expect(awardsApi.saveHostScript).toHaveBeenCalledWith(
      7,
      expect.objectContaining({ openingText: '新的开场', expectedVersion: 2 }),
    );
    wrapper.unmount();
  });

  it('surfaces a presentation load failure instead of the control card', async () => {
    vi.mocked(awardsApi.presentation).mockRejectedValueOnce(
      new ApiError(503, 'AWARDS_UNAVAILABLE', 'awards unavailable'),
    );
    const wrapper = mount(AwardPresentationControlView);
    await flushPromises();
    expect(wrapper.text()).toContain('awards unavailable');
    expect(wrapper.findAll('button').find((button) => button.text().includes('开始/继续'))).toBe(
      undefined,
    );
    wrapper.unmount();
  });

  it('moves through categories and disables previous at the first award', async () => {
    const twoCategories: AwardPresentation = {
      ...presentation,
      categories: [
        presentation.categories[0],
        {
          id: 2,
          code: 'SILVER',
          name: '银奖',
          displayOrder: 2,
          groupName: null,
          firstBlood: false,
          recipients: [],
        },
      ],
    };
    vi.mocked(awardsApi.presentation).mockResolvedValueOnce(twoCategories);
    vi.mocked(awardsApi.updatePresentation).mockResolvedValueOnce({
      ...twoCategories,
      status: 'PRESENTING',
      currentCategoryId: 2,
    });
    const wrapper = mount(AwardPresentationControlView);
    await flushPromises();
    const previous = wrapper.findAll('button').find((button) => button.text().includes('上一项'))!;
    expect(previous.attributes('disabled')).toBeDefined();
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('下一项'))!
      .trigger('click');
    await flushPromises();
    expect(awardsApi.updatePresentation).toHaveBeenCalledWith(
      7,
      expect.objectContaining({ status: 'WAITING', currentCategoryId: 2 }),
    );
    expect(previous.attributes('disabled')).toBeUndefined();
    await previous.trigger('click');
    await flushPromises();
    expect(awardsApi.updatePresentation).toHaveBeenLastCalledWith(
      7,
      expect.objectContaining({ currentCategoryId: 1 }),
    );
    wrapper.unmount();
  });

  it('reports update failures, reloads the presentation, and keeps the draft', async () => {
    vi.mocked(awardsApi.updatePresentation).mockRejectedValueOnce(
      new ApiError(409, 'AWARDS_STATE_CONFLICT', 'awards state conflict'),
    );
    const wrapper = mount(AwardPresentationControlView);
    await flushPromises();
    const loadCalls = vi.mocked(awardsApi.presentation).mock.calls.length;
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('开始/继续'))!
      .trigger('click');
    await flushPromises();
    expect(elementMocks.error).toHaveBeenCalled();
    expect(vi.mocked(awardsApi.presentation).mock.calls.length).toBe(loadCalls + 1);
    wrapper.unmount();
  });

  it('opens the public display page for the selected contest', async () => {
    const open = vi.spyOn(window, 'open').mockReturnValue(null);
    const wrapper = mount(AwardPresentationControlView);
    await flushPromises();
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('打开展示页'))!
      .trigger('click');
    expect(open).toHaveBeenCalledWith('/awards/display?contestId=7', '_blank', 'noopener');
    wrapper.unmount();
  });

  it('shows the waiting and completed ceremony messages', async () => {
    vi.mocked(awardsApi.presentation).mockResolvedValueOnce({
      ...presentation,
      status: 'COMPLETED',
    });
    const completed = mount(AwardDisplayView);
    await flushPromises();
    expect(completed.text()).toContain('颁奖典礼圆满结束');
    completed.unmount();
    const waiting = mount(AwardDisplayView);
    await flushPromises();
    expect(waiting.text()).toContain('颁奖典礼即将开始');
    waiting.unmount();
  });

  it('renders first blood, star teams, and school fallbacks for recipients', async () => {
    vi.mocked(awardsApi.presentation).mockResolvedValueOnce({
      ...presentation,
      status: 'PRESENTING',
      categories: [
        {
          ...presentation.categories[0],
          groupName: '邀请队',
          recipients: [
            {
              ...presentation.categories[0].recipients[0],
              school: null,
              seatNo: 'B12',
              star: true,
              problemId: 4,
              problemAlias: 'A',
            },
          ],
        },
      ],
    });
    const wrapper = mount(AwardDisplayView);
    await flushPromises();
    expect(wrapper.text()).toContain('FB A');
    expect(wrapper.text()).toContain('打星队');
    expect(wrapper.text()).toContain('邀请队');
    expect(wrapper.text()).toContain('参赛队伍');
    expect(wrapper.text()).toContain('座位 B12');
    wrapper.unmount();
  });

  it('rotates categories over time and flags empty recipient lists', async () => {
    vi.mocked(awardsApi.presentation).mockResolvedValueOnce({
      ...presentation,
      status: 'PRESENTING',
      autoRotate: true,
      intervalSeconds: 15,
      stateUpdatedAt: '2026-07-21T23:59:40Z',
      categories: [
        presentation.categories[0],
        {
          id: 2,
          code: 'SILVER',
          name: '银奖',
          displayOrder: 2,
          groupName: null,
          firstBlood: false,
          recipients: [],
        },
      ],
    });
    const wrapper = mount(AwardDisplayView);
    await flushPromises();
    expect(wrapper.text()).toContain('银奖');
    expect(wrapper.text()).toContain('当前奖项没有获奖队伍');
    wrapper.unmount();
  });

  it('falls back to the waiting message when the public presentation fails', async () => {
    vi.mocked(awardsApi.presentation).mockRejectedValueOnce(
      new ApiError(500, 'AWARDS_UNAVAILABLE', 'awards unavailable'),
    );
    const wrapper = mount(AwardDisplayView);
    await flushPromises();
    expect(wrapper.text()).toContain('awards unavailable');
    expect(wrapper.text()).toContain('请使用 ?contestId= 指定比赛');
    wrapper.unmount();
  });

  it('keeps showing the last synced recipients when the connection drops', async () => {
    vi.mocked(awardsApi.presentation).mockResolvedValueOnce({
      ...presentation,
      status: 'PRESENTING',
    });
    const wrapper = mount(AwardDisplayView);
    await flushPromises();
    const config = vi.mocked(subscribeContestEvents).mock.calls.at(-1)![0] as Parameters<
      typeof subscribeContestEvents
    >[0];
    config.onConnectionChange?.(false);
    await flushPromises();
    expect(wrapper.text()).toContain('连接中断，继续展示最后一次同步结果');
    expect(wrapper.text()).toContain('Team Eight');
    wrapper.unmount();
  });

  it('surfaces host script load failures and keeps saving disabled', async () => {
    vi.mocked(awardsApi.hostScript).mockRejectedValueOnce(
      new ApiError(500, 'AWARDS_SCRIPT_UNAVAILABLE', 'host script unavailable'),
    );
    const wrapper = mount(AwardHostScriptView);
    await flushPromises();
    expect(wrapper.text()).toContain('host script unavailable');
    expect(
      wrapper
        .findAll('button')
        .find((button) => button.text().includes('保存脚本'))!
        .attributes('disabled'),
    ).toBeDefined();
    wrapper.unmount();
  });

  it('reports host script save failures and reloads the cue sheet', async () => {
    vi.mocked(awardsApi.saveHostScript).mockRejectedValueOnce(
      new ApiError(409, 'AWARDS_SCRIPT_CONFLICT', 'host script conflict'),
    );
    const wrapper = mount(AwardHostScriptView);
    await flushPromises();
    const loadCalls = vi.mocked(awardsApi.hostScript).mock.calls.length;
    await wrapper.find('textarea').setValue('冲突的开场');
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('保存脚本'))!
      .trigger('click');
    await flushPromises();
    expect(elementMocks.error).toHaveBeenCalled();
    expect(vi.mocked(awardsApi.hostScript).mock.calls.length).toBe(loadCalls + 1);
    wrapper.unmount();
  });

  it('advances the live cue by elapsed rotation time and flags the final award', async () => {
    vi.mocked(awardsApi.hostScript).mockResolvedValueOnce({
      ...hostScript,
      presentationStatus: 'PRESENTING',
      autoRotate: true,
      intervalSeconds: 15,
      stateUpdatedAt: '2026-07-21T23:59:40Z',
      currentCategoryId: 1,
      sections: [
        hostScript.sections[0],
        {
          categoryId: 2,
          code: 'SILVER',
          name: '银奖',
          firstBlood: false,
          current: false,
          cueText: '银奖上台',
          recipients: [
            {
              id: 9,
              problemId: null,
              problemAlias: null,
              teamId: 10,
              teamName: 'Team Ten',
              school: null,
              seatNo: null,
              groupName: null,
              participationType: 'OFFICIAL',
              star: false,
              rank: 2,
              solved: 7,
              penaltyMinutes: 720,
            },
          ],
        },
      ],
    });
    const wrapper = mount(AwardHostScriptView);
    await flushPromises();
    expect(wrapper.text()).toContain('银奖');
    expect(wrapper.text()).toContain('这是最后一个奖项');
    expect(wrapper.text()).toContain('未填写学校');
    wrapper.unmount();
  });
});
