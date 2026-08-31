import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ScreenClientView from './ScreenClientView.vue';
import ScreenManageView from './ScreenManageView.vue';
import { contestApi } from '../api/contest';
import { presentationApi } from '../api/presentation';
import { screenApi } from '../api/screen';
import { ApiError } from '../api/client';
const replace = vi.fn();
const stored = new Map<string, string>();
const elementMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  confirm: vi.fn(),
}));
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return {
    ...actual,
    ElMessage: {
      success: elementMocks.success,
      error: elementMocks.error,
      warning: vi.fn(),
    },
    ElMessageBox: { confirm: elementMocks.confirm },
  };
});
vi.stubGlobal('localStorage', {
  clear: () => stored.clear(),
  getItem: (key: string) => stored.get(key) ?? null,
  setItem: (key: string, value: string) => stored.set(key, value),
  removeItem: (key: string) => stored.delete(key),
});
let route: { query: Record<string, string> } = { query: { contestId: '7', name: 'Main Hall' } };
vi.mock('vue-router', () => ({
  useRoute: () => route,
  useRouter: () => ({ replace }),
}));
vi.mock('../api/contest', () => ({ contestApi: { listContests: vi.fn() } }));
vi.mock('../api/presentation', () => ({ presentationApi: { config: vi.fn(), update: vi.fn() } }));
vi.mock('../api/screen', () => ({
  screenApi: {
    register: vi.fn(),
    heartbeat: vi.fn(),
    list: vi.fn(),
    command: vi.fn(),
    revoke: vi.fn(),
    playlists: vi.fn(),
    groups: vi.fn(),
    createPlaylist: vi.fn(),
    updatePlaylist: vi.fn(),
    deletePlaylist: vi.fn(),
    createGroup: vi.fn(),
    updateGroup: vi.fn(),
    deleteGroup: vi.fn(),
    controlGroup: vi.fn(),
  },
}));
const config = {
  contestId: 7,
  mode: 'SCREEN' as const,
  enabled: true,
  title: 'Finals',
  subtitle: null,
  accentColor: '#22c55e',
  rowLimit: 12,
  showAnnouncements: true,
  announcementIntervalSeconds: 10,
  template: 'DEFAULT' as const,
  updatedAt: null,
};
const instance = {
  id: 3,
  contestId: 7,
  name: 'Main Hall',
  currentView: 'SCOREBOARD' as const,
  online: true,
  lastSeenAt: '2026-07-22T01:00:00Z',
  lastIp: '127.0.0.1',
  revokedAt: null,
  createdAt: '2026-07-22T01:00:00Z',
};
describe('screen views', () => {
  beforeEach(() => {
    route = { query: { contestId: '7', name: 'Main Hall' } };
    localStorage.clear();
    replace.mockReset();
    elementMocks.success.mockReset();
    elementMocks.error.mockReset();
    elementMocks.confirm.mockReset();
    vi.mocked(contestApi.listContests).mockResolvedValue({
      content: [{ id: 7, name: 'Finals' }],
    } as Awaited<ReturnType<typeof contestApi.listContests>>);
    vi.mocked(presentationApi.config).mockResolvedValue(config);
    vi.mocked(presentationApi.update).mockResolvedValue(config);
    vi.mocked(screenApi.list).mockResolvedValue([instance]);
    vi.mocked(screenApi.command).mockResolvedValue({});
    vi.mocked(screenApi.revoke).mockResolvedValue();
    vi.mocked(screenApi.playlists).mockResolvedValue([]);
    vi.mocked(screenApi.groups).mockResolvedValue([]);
    vi.mocked(screenApi.register).mockResolvedValue({
      instanceId: 3,
      contestId: 7,
      name: 'Main Hall',
      clientToken: 'secret',
      currentView: 'SCOREBOARD',
      registeredAt: '2026-07-22T01:00:00Z',
    });
    vi.mocked(screenApi.heartbeat).mockResolvedValue({
      instanceId: 3,
      serverTime: '2026-07-22T01:00:00Z',
      commandId: 4,
      targetView: 'AWARDS',
      groupPlayback: null,
    });
  });
  it('loads published config and sends a remote screen command', async () => {
    const wrapper = mount(ScreenManageView);
    await flushPromises();
    expect(presentationApi.config).toHaveBeenCalledWith(7, 'SCREEN');
    expect(wrapper.text()).toContain('Main Hall');
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('发送'))!
      .trigger('click');
    await flushPromises();
    expect(screenApi.command).toHaveBeenCalledWith(7, 3, 'SCOREBOARD');
    wrapper.unmount();
  });
  it('registers once, stores the token locally, and applies the newest heartbeat command', async () => {
    const wrapper = mount(ScreenClientView);
    await flushPromises();
    expect(screenApi.register).toHaveBeenCalledWith(7, 'Main Hall');
    expect(screenApi.heartbeat).toHaveBeenCalledWith(3, 'secret', 'SCOREBOARD');
    expect(wrapper.text()).toContain('AWARDS');
    expect(localStorage.getItem('project-balloon:screen:7')).toContain('secret');
    wrapper.unmount();
  });
  it('clears malformed cached registration and registers again', async () => {
    localStorage.setItem('project-balloon:screen:7', '{not-json');
    const wrapper = mount(ScreenClientView);
    await flushPromises();
    expect(screenApi.register).toHaveBeenCalledWith(7, 'Main Hall');
    expect(localStorage.getItem('project-balloon:screen:7')).toContain('secret');
    wrapper.unmount();
  });
  it('clears a revoked cached registration token', async () => {
    localStorage.setItem(
      'project-balloon:screen:7',
      JSON.stringify({
        instanceId: 3,
        contestId: 7,
        name: 'Main Hall',
        clientToken: 'revoked',
        currentView: 'SCOREBOARD',
        registeredAt: '2026-07-22T01:00:00Z',
      }),
    );
    vi.mocked(screenApi.heartbeat).mockRejectedValueOnce(
      new ApiError(401, 'SCREEN_TOKEN_INVALID', 'Screen token is invalid'),
    );
    const wrapper = mount(ScreenClientView);
    await flushPromises();
    expect(localStorage.getItem('project-balloon:screen:7')).toBeNull();
    wrapper.unmount();
  });
  it('does not overlap screen heartbeats while a request is pending', async () => {
    vi.useFakeTimers();
    vi.mocked(screenApi.heartbeat).mockClear();
    let release!: (value: Awaited<ReturnType<typeof screenApi.heartbeat>>) => void;
    const pending = new Promise<Awaited<ReturnType<typeof screenApi.heartbeat>>>((resolve) => {
      release = resolve;
    });
    vi.mocked(screenApi.heartbeat).mockReturnValueOnce(pending);
    const wrapper = mount(ScreenClientView);
    await flushPromises();
    expect(screenApi.heartbeat).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(30_000);
    expect(screenApi.heartbeat).toHaveBeenCalledTimes(1);

    release({
      instanceId: 3,
      serverTime: '2026-07-22T01:00:00Z',
      commandId: null,
      targetView: 'SCOREBOARD',
      groupPlayback: null,
    });
    await flushPromises();
    vi.advanceTimersByTime(10_000);
    expect(screenApi.heartbeat).toHaveBeenCalledTimes(2);
    wrapper.unmount();
    vi.useRealTimers();
  });

  it('saves the trimmed config and reports save failures', async () => {
    const wrapper = mount(ScreenManageView);
    await flushPromises();
    const updateCalls = vi.mocked(presentationApi.update).mock.calls.length;
    await wrapper
      .findAll('input')
      .find((input) => (input.element as HTMLInputElement).value === 'Finals')!
      .setValue('  Finals  ');
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('保存配置'))!
      .trigger('click');
    await flushPromises();
    expect(presentationApi.update).toHaveBeenCalledWith(
      7,
      'SCREEN',
      expect.objectContaining({ title: 'Finals' }),
    );
    expect(elementMocks.success).toHaveBeenCalled();
    vi.mocked(presentationApi.update).mockRejectedValueOnce(
      new ApiError(400, 'PRESENTATION_INVALID', 'presentation invalid'),
    );
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('保存配置'))!
      .trigger('click');
    await flushPromises();
    expect(vi.mocked(presentationApi.update).mock.calls.length).toBe(updateCalls + 2);
    expect(elementMocks.error).toHaveBeenCalled();
    wrapper.unmount();
  });

  it('surfaces config load failures as an alert', async () => {
    vi.mocked(presentationApi.config).mockRejectedValueOnce(
      new ApiError(404, 'PRESENTATION_NOT_FOUND', 'presentation not found'),
    );
    const wrapper = mount(ScreenManageView);
    await flushPromises();
    expect(wrapper.text()).toContain('presentation not found');
    wrapper.unmount();
  });

  it('revokes an instance after confirmation and reloads the instances', async () => {
    elementMocks.confirm.mockResolvedValue(undefined);
    const wrapper = mount(ScreenManageView);
    await flushPromises();
    const listCalls = vi.mocked(screenApi.list).mock.calls.length;
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('撤销'))!
      .trigger('click');
    await flushPromises();
    expect(elementMocks.confirm).toHaveBeenCalled();
    expect(screenApi.revoke).toHaveBeenCalledWith(7, 3);
    expect(vi.mocked(screenApi.list).mock.calls.length).toBe(listCalls + 1);
    wrapper.unmount();
  });

  it('keeps the instance when the revoke confirmation is dismissed', async () => {
    elementMocks.confirm.mockRejectedValue('cancel');
    const wrapper = mount(ScreenManageView);
    await flushPromises();
    const revokeCalls = vi.mocked(screenApi.revoke).mock.calls.length;
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('撤销'))!
      .trigger('click');
    await flushPromises();
    expect(vi.mocked(screenApi.revoke).mock.calls.length).toBe(revokeCalls);
    expect(elementMocks.error).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it('reports command failures without clearing the target', async () => {
    vi.mocked(screenApi.command).mockRejectedValueOnce(
      new ApiError(409, 'SCREEN_INSTANCE_OFFLINE', 'screen instance offline'),
    );
    const wrapper = mount(ScreenManageView);
    await flushPromises();
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('发送'))!
      .trigger('click');
    await flushPromises();
    expect(elementMocks.error).toHaveBeenCalled();
    expect(wrapper.text()).toContain('Main Hall');
    wrapper.unmount();
  });

  it('asks for a contest when the client is opened without a contestId', async () => {
    route = { query: { name: 'Lobby' } };
    vi.mocked(screenApi.register).mockClear();
    const wrapper = mount(ScreenClientView);
    await flushPromises();
    expect(screenApi.register).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain('请使用 ?contestId= 指定比赛');
    wrapper.unmount();
  });

  it('reports registration failures and stores nothing locally', async () => {
    vi.mocked(screenApi.register).mockRejectedValueOnce(
      new ApiError(404, 'CONTEST_NOT_FOUND', 'contest not found'),
    );
    const wrapper = mount(ScreenClientView);
    await flushPromises();
    expect(wrapper.text()).toContain('比赛不存在或不可访问');
    expect(localStorage.getItem('project-balloon:screen:7')).toBeNull();
    wrapper.unmount();
  });

  it('switches to the next playlist entry at the group playback boundary', async () => {
    vi.useFakeTimers();
    vi.mocked(screenApi.heartbeat).mockClear();
    const playback = {
      groupId: 5,
      groupName: 'Main Hall',
      playlistId: 2,
      loopEnabled: true,
      status: 'PLAYING' as const,
      startedAt: '2026-07-22T00:59:30Z',
      pausedElapsedSeconds: 0,
      lockedView: null,
      version: 1,
      items: [
        { id: 11, targetView: 'SCOREBOARD' as const, durationSeconds: 60, displayOrder: 1 },
        { id: 12, targetView: 'AWARDS' as const, durationSeconds: 60, displayOrder: 2 },
      ],
    };
    vi.mocked(screenApi.heartbeat)
      .mockResolvedValueOnce({
        instanceId: 3,
        serverTime: '2026-07-22T01:00:00Z',
        commandId: null,
        targetView: null,
        groupPlayback: playback,
      })
      .mockResolvedValueOnce({
        instanceId: 3,
        serverTime: '2026-07-22T01:00:30Z',
        commandId: null,
        targetView: null,
        groupPlayback: playback,
      });
    const wrapper = mount(ScreenClientView);
    await flushPromises();
    expect(wrapper.text()).toContain('SCOREBOARD');
    vi.advanceTimersByTime(30_000);
    await flushPromises();
    expect(wrapper.text()).toContain('AWARDS');
    expect(screenApi.heartbeat).toHaveBeenCalledTimes(2);
    wrapper.unmount();
    vi.useRealTimers();
  });
});
