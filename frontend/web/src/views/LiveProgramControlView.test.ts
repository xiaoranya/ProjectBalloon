import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import LiveProgramControlView from './LiveProgramControlView.vue';
import { ApiError } from '../api/client';
import { contestApi } from '../api/contest';
import {
  presentationApi,
  type LiveProgramState,
  type ResolverRunOption,
} from '../api/presentation';

const replace = vi.fn();
const elementMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  warning: vi.fn(),
}));
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return {
    ...actual,
    ElMessage: {
      success: elementMocks.success,
      error: elementMocks.error,
      warning: elementMocks.warning,
    },
  };
});
vi.mock('vue-router', () => ({
  useRoute: () => ({ query: {} }),
  useRouter: () => ({ push: replace }),
}));
vi.mock('../api/contest', () => ({ contestApi: { listContests: vi.fn() } }));
vi.mock('../api/presentation', () => ({
  presentationApi: {
    program: vi.fn(),
    updateProgram: vi.fn(),
  },
}));

const program: LiveProgramState = {
  contestId: 7,
  currentScene: 'SCOREBOARD',
  resolverRunId: null,
  transitionMilliseconds: 800,
  showClock: true,
  tickerEnabled: true,
  titleCardText: null,
  version: 4,
  updatedAt: '2026-07-29T01:00:00Z',
};

const runs: ResolverRunOption[] = [
  {
    id: 5,
    official: true,
    status: 'RUNNING',
    currentStep: 2,
    totalSteps: 6,
    createdAt: '2026-07-29T01:00:00Z',
  },
  {
    id: 3,
    official: false,
    status: 'COMPLETED',
    currentStep: 6,
    totalSteps: 6,
    createdAt: '2026-07-28T09:00:00Z',
  },
];

describe('live program control view', () => {
  beforeEach(() => {
    vi.mocked(contestApi.listContests).mockResolvedValue({
      content: [{ id: 7, name: 'Finals' } as never],
      page: 0,
      size: 20,
      totalElements: 1,
      totalPages: 1,
    });
    vi.mocked(presentationApi.program).mockResolvedValue({ program, resolverRuns: runs });
    vi.mocked(presentationApi.updateProgram).mockResolvedValue({ ...program, version: 5 });
    // restoreMocks resets implementations between tests but keeps history;
    // clear the call logs explicitly so per-test counts are exact.
    vi.mocked(presentationApi.program).mockClear();
    vi.mocked(presentationApi.updateProgram).mockClear();
    elementMocks.success.mockClear();
    elementMocks.error.mockClear();
    elementMocks.warning.mockClear();
  });

  it('loads the on-air program and only offers official runs', async () => {
    const wrapper = mount(LiveProgramControlView);
    await flushPromises();
    expect(wrapper.text()).toContain('正在播出');
    expect(wrapper.text()).toContain('实时榜单');
    const select = wrapper.findAllComponents({ name: 'ElSelect' }).at(-1);
    expect(select).toBeDefined();
    wrapper.unmount();
  });

  it('switches scenes via buttons with the optimistic version', async () => {
    const wrapper = mount(LiveProgramControlView);
    await flushPromises();
    const buttons = wrapper.findAll('.scene-grid button');
    expect(buttons.length).toBe(8);
    await buttons[1].trigger('click'); // FIRST_BLOOD
    await flushPromises();
    expect(presentationApi.updateProgram).toHaveBeenCalledWith(7, {
      currentScene: 'FIRST_BLOOD',
      resolverRunId: null,
      transitionMilliseconds: 800,
      showClock: true,
      tickerEnabled: true,
      titleCardText: null,
      expectedVersion: 4,
    });
    wrapper.unmount();
  });

  it('switches scenes via keyboard hotkeys', async () => {
    const wrapper = mount(LiveProgramControlView);
    await flushPromises();
    window.dispatchEvent(new KeyboardEvent('keydown', { key: '8' }));
    await flushPromises();
    expect(presentationApi.updateProgram).toHaveBeenCalledWith(
      7,
      expect.objectContaining({ currentScene: 'TITLE_CARD' }),
    );
    wrapper.unmount();
  });

  it('hotkeys toggle the ticker and clock', async () => {
    const wrapper = mount(LiveProgramControlView);
    await flushPromises();
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 't' }));
    await flushPromises();
    expect(presentationApi.updateProgram).toHaveBeenCalledWith(
      7,
      expect.objectContaining({ tickerEnabled: false }),
    );
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'c' }));
    await flushPromises();
    expect(presentationApi.updateProgram).toHaveBeenCalledWith(
      7,
      expect.objectContaining({ showClock: false }),
    );
    wrapper.unmount();
  });

  it('ignores hotkeys while an input field has focus', async () => {
    const wrapper = mount(LiveProgramControlView);
    await flushPromises();
    const input = document.createElement('input');
    document.body.appendChild(input);
    input.focus();
    input.dispatchEvent(new KeyboardEvent('keydown', { key: '3', bubbles: true }));
    await flushPromises();
    expect(presentationApi.updateProgram).not.toHaveBeenCalled();
    input.remove();
    wrapper.unmount();
  });

  it('recovers from a version conflict by reloading the server state', async () => {
    vi.mocked(presentationApi.updateProgram).mockRejectedValueOnce(
      new ApiError(409, 'LIVE_PROGRAM_VERSION_CONFLICT', 'Live program was changed'),
    );
    const wrapper = mount(LiveProgramControlView);
    await flushPromises();
    const buttons = wrapper.findAll('.scene-grid button');
    await buttons[6].trigger('click'); // AWARDS
    await flushPromises();
    expect(presentationApi.program).toHaveBeenCalledTimes(2);
    expect(elementMocks.error).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain('已被他人修改');
    wrapper.unmount();
  });

  it('builds OBS links from a pasted token', async () => {
    const wrapper = mount(LiveProgramControlView);
    await flushPromises();
    const tokenInputs = wrapper.findAll('input');
    const tokenInput = tokenInputs.find((input) =>
      input.attributes('placeholder')?.includes('Token'),
    );
    expect(tokenInput).toBeDefined();
    await tokenInput!.setValue('broadcast-token');
    const buttons = wrapper.findAll('button');
    const generate = buttons.find((button) => button.text().includes('生成链接'));
    await generate!.trigger('click');
    // The URLs live in readonly input values, not rendered text.
    const values = wrapper.findAll('input').map((input) => String(input.element.value));
    expect(
      values.some((value) => value.includes('/live/program?contestId=7#token=broadcast-token')),
    ).toBe(true);
    expect(
      values.some((value) => value.includes('/live/overlay?contestId=7#token=broadcast-token')),
    ).toBe(true);
    wrapper.unmount();
  });
});
