import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ScreenOrchestrationControl from './ScreenOrchestrationControl.vue';
import {
  screenApi,
  type ScreenGroup,
  type ScreenInstance,
  type ScreenPlaylist,
} from '../api/screen';

const elementMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  warning: vi.fn(),
  confirm: vi.fn(),
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
    ElMessageBox: { confirm: elementMocks.confirm },
  };
});
vi.mock('../api/screen', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/screen')>();
  return {
    ...actual,
    screenApi: {
      ...actual.screenApi,
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
  };
});

const instance: ScreenInstance = {
  id: 3,
  contestId: 7,
  name: 'Main Hall',
  currentView: 'SCOREBOARD',
  online: true,
  lastSeenAt: '2026-07-22T01:00:00Z',
  lastIp: '127.0.0.1',
  revokedAt: null,
  createdAt: '2026-07-22T01:00:00Z',
};
const playlist: ScreenPlaylist = {
  id: 2,
  contestId: 7,
  name: '颁奖轮播',
  loopEnabled: true,
  version: 3,
  items: [{ id: 1, targetView: 'SCOREBOARD', durationSeconds: 15, displayOrder: 1 }],
  createdAt: '2026-07-22T01:00:00Z',
  updatedAt: '2026-07-22T01:00:00Z',
};
const group: ScreenGroup = {
  id: 4,
  contestId: 7,
  name: '主屏组',
  instanceIds: [3],
  playlistId: 2,
  playbackStatus: 'PLAYING',
  playbackStartedAt: '2026-07-22T01:00:00Z',
  pausedElapsedSeconds: 0,
  lockedView: null,
  version: 5,
  createdAt: '2026-07-22T01:00:00Z',
  updatedAt: '2026-07-22T01:00:00Z',
};

function button(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((candidate) => candidate.text().includes(text))!;
}

function exactButton(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((candidate) => candidate.text().trim() === text)!;
}

function mountControl() {
  return mount(ScreenOrchestrationControl, {
    props: { contestId: 7, instances: [instance] },
  });
}

describe('ScreenOrchestrationControl', () => {
  beforeEach(() => {
    elementMocks.success.mockReset();
    elementMocks.error.mockReset();
    elementMocks.warning.mockReset();
    elementMocks.confirm.mockReset();
    vi.mocked(screenApi.playlists).mockResolvedValue([playlist]);
    vi.mocked(screenApi.groups).mockResolvedValue([group]);
    vi.mocked(screenApi.controlGroup).mockResolvedValue(group);
    vi.mocked(screenApi.deletePlaylist).mockResolvedValue(undefined);
  });

  it('loads playlists and groups and renders their content', async () => {
    const wrapper = mountControl();
    await flushPromises();
    expect(screenApi.playlists).toHaveBeenCalledWith(7);
    expect(screenApi.groups).toHaveBeenCalledWith(7);
    expect(wrapper.text()).toContain('颁奖轮播');
    expect(wrapper.text()).toContain('榜单 · 15s');
    expect(wrapper.text()).toContain('Main Hall');
    expect(wrapper.text()).toContain('PLAYING');
    expect(button(wrapper, '暂停')).toBeTruthy();
    expect(button(wrapper, '停止')).toBeTruthy();
    wrapper.unmount();
  });

  it('controls group playback with the selected playlist', async () => {
    const wrapper = mountControl();
    await flushPromises();
    await exactButton(wrapper, '播放').trigger('click');
    await flushPromises();
    expect(screenApi.controlGroup).toHaveBeenCalledWith(4, 'PLAY', 5, { playlistId: 2 });
    await button(wrapper, '暂停').trigger('click');
    await flushPromises();
    expect(screenApi.controlGroup).toHaveBeenLastCalledWith(4, 'PAUSE', 5, {});
    await exactButton(wrapper, '停止').trigger('click');
    await flushPromises();
    expect(screenApi.controlGroup).toHaveBeenLastCalledWith(4, 'STOP', 5, {});
    wrapper.unmount();
  });

  it('locks a group through the dropdown command and unlocks it', async () => {
    vi.mocked(screenApi.groups).mockResolvedValue([{ ...group, lockedView: 'AWARDS' }]);
    const wrapper = mountControl();
    await flushPromises();
    expect(wrapper.text()).toContain('锁定 颁奖');
    const dropdown = wrapper.findComponent({ name: 'ElDropdown' });
    (dropdown.vm as unknown as { $emit: (event: string, command: string) => void }).$emit(
      'command',
      'BALLOONS',
    );
    await flushPromises();
    expect(screenApi.controlGroup).toHaveBeenCalledWith(4, 'LOCK', 5, {
      targetView: 'BALLOONS',
    });
    expect(button(wrapper, '解锁')).toBeTruthy();
    await button(wrapper, '解锁').trigger('click');
    await flushPromises();
    expect(screenApi.controlGroup).toHaveBeenLastCalledWith(4, 'UNLOCK', 5, {});
    wrapper.unmount();
  });

  it('warns when playing a group without a selected playlist', async () => {
    vi.mocked(screenApi.groups).mockResolvedValue([{ ...group, playlistId: null }]);
    const wrapper = mountControl();
    await flushPromises();
    const controlCalls = vi.mocked(screenApi.controlGroup).mock.calls.length;
    await exactButton(wrapper, '播放').trigger('click');
    await flushPromises();
    expect(elementMocks.warning).toHaveBeenCalled();
    expect(vi.mocked(screenApi.controlGroup).mock.calls.length).toBe(controlCalls);
    wrapper.unmount();
  });

  it('creates and edits playlists from the dialog with optimistic versions', async () => {
    const created = { ...playlist, id: 9, name: '开屏循环', version: 1 };
    vi.mocked(screenApi.createPlaylist).mockResolvedValue(created);
    vi.mocked(screenApi.updatePlaylist).mockResolvedValue(playlist);
    const wrapper = mountControl();
    await flushPromises();
    await button(wrapper, '新建播放列表').trigger('click');
    const loadCalls = vi.mocked(screenApi.playlists).mock.calls.length;
    const nameInput = wrapper
      .findAll('input.el-input__inner')
      .find((input) => (input.element as HTMLInputElement).type !== 'number')!;
    await nameInput.setValue('开屏循环');
    await button(wrapper, '保存').trigger('click');
    await flushPromises();
    expect(screenApi.createPlaylist).toHaveBeenCalledWith(
      7,
      expect.objectContaining({ name: '开屏循环', loopEnabled: true }),
    );
    expect(vi.mocked(screenApi.playlists).mock.calls.length).toBe(loadCalls + 1);

    await button(wrapper, '编辑').trigger('click');
    await button(wrapper, '保存').trigger('click');
    await flushPromises();
    expect(screenApi.updatePlaylist).toHaveBeenCalledWith(
      2,
      expect.objectContaining({ name: '颁奖轮播', expectedVersion: 3 }),
    );
    wrapper.unmount();
  });

  it('deletes playlists and groups after confirmation', async () => {
    vi.mocked(screenApi.deleteGroup).mockResolvedValue(undefined);
    const wrapper = mountControl();
    await flushPromises();
    const playlistLoadCalls = vi.mocked(screenApi.playlists).mock.calls.length;
    await button(wrapper, '删除').trigger('click');
    await flushPromises();
    expect(elementMocks.confirm).toHaveBeenCalled();
    expect(screenApi.deletePlaylist).toHaveBeenCalledWith(2);
    expect(vi.mocked(screenApi.playlists).mock.calls.length).toBe(playlistLoadCalls + 1);

    elementMocks.confirm.mockRejectedValue('cancel');
    const deleteCalls = vi.mocked(screenApi.deleteGroup).mock.calls.length;
    const deleteButtons = wrapper.findAll('button').filter((row) => row.text().includes('删除'));
    await deleteButtons[deleteButtons.length - 1].trigger('click');
    await flushPromises();
    expect(vi.mocked(screenApi.deleteGroup).mock.calls.length).toBe(deleteCalls);
    wrapper.unmount();
  });

  it('creates and edits sync groups from the dialog', async () => {
    const createdGroup = { ...group, id: 6, name: '副屏组', version: 1 };
    vi.mocked(screenApi.createGroup).mockResolvedValue(createdGroup);
    vi.mocked(screenApi.updateGroup).mockResolvedValue(group);
    const wrapper = mountControl();
    await flushPromises();
    await button(wrapper, '新建分组').trigger('click');
    const dialogs = wrapper.findAll('.el-dialog');
    const dialog = dialogs[dialogs.length - 1];
    const groupLoadCalls = vi.mocked(screenApi.groups).mock.calls.length;
    await dialog.findAll('input.el-input__inner')[0].setValue('副屏组');
    await button(wrapper, '保存').trigger('click');
    await flushPromises();
    expect(screenApi.createGroup).toHaveBeenCalledWith(
      7,
      expect.objectContaining({ name: '副屏组', instanceIds: [] }),
    );
    expect(vi.mocked(screenApi.groups).mock.calls.length).toBe(groupLoadCalls + 1);

    const editButtons = wrapper.findAll('button').filter((row) => row.text().includes('编辑'));
    await editButtons[editButtons.length - 1].trigger('click');
    await button(wrapper, '保存').trigger('click');
    await flushPromises();
    expect(screenApi.updateGroup).toHaveBeenCalledWith(
      4,
      expect.objectContaining({ name: '主屏组', instanceIds: [3], expectedVersion: 5 }),
    );
    wrapper.unmount();
  });

  it('reports playlist save failures', async () => {
    vi.mocked(screenApi.updatePlaylist).mockRejectedValue(
      new (await import('../api/client')).ApiError(409, 'SCREEN_PLAYLIST_CONFLICT', 'conflict'),
    );
    const wrapper = mountControl();
    await flushPromises();
    await button(wrapper, '编辑').trigger('click');
    await button(wrapper, '保存').trigger('click');
    await flushPromises();
    expect(elementMocks.error).toHaveBeenCalled();
    wrapper.unmount();
  });
});
