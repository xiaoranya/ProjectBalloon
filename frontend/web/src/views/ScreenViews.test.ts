import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ScreenClientView from './ScreenClientView.vue';
import ScreenManageView from './ScreenManageView.vue';
import { contestApi } from '../api/contest';
import { presentationApi } from '../api/presentation';
import { screenApi } from '../api/screen';
const replace = vi.fn();
const stored = new Map<string, string>();
vi.stubGlobal('localStorage', { clear: () => stored.clear(), getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => stored.set(key, value), removeItem: (key: string) => stored.delete(key) });
vi.mock('vue-router', () => ({ useRoute: () => ({ query: { contestId: '7', name: 'Main Hall' } }), useRouter: () => ({ replace }) }));
vi.mock('../api/contest', () => ({ contestApi: { listContests: vi.fn() } }));
vi.mock('../api/presentation', () => ({ presentationApi: { config: vi.fn(), update: vi.fn() } }));
vi.mock('../api/screen', () => ({ screenApi: { register: vi.fn(), heartbeat: vi.fn(), list: vi.fn(), command: vi.fn(), revoke: vi.fn(), playlists: vi.fn(), groups: vi.fn(), createPlaylist: vi.fn(), updatePlaylist: vi.fn(), deletePlaylist: vi.fn(), createGroup: vi.fn(), updateGroup: vi.fn(), deleteGroup: vi.fn(), controlGroup: vi.fn() } }));
const config = { contestId: 7, mode: 'SCREEN' as const, enabled: true, title: 'Finals', subtitle: null, accentColor: '#22c55e', rowLimit: 12, showAnnouncements: true, announcementIntervalSeconds: 10, updatedAt: null };
const instance = { id: 3, contestId: 7, name: 'Main Hall', currentView: 'SCOREBOARD' as const, online: true, lastSeenAt: '2026-07-22T01:00:00Z', lastIp: '127.0.0.1', revokedAt: null, createdAt: '2026-07-22T01:00:00Z' };
describe('screen views', () => {
  beforeEach(() => {
    localStorage.clear(); replace.mockReset();
    vi.mocked(contestApi.listContests).mockResolvedValue({ content: [{ id: 7, name: 'Finals' }] } as Awaited<ReturnType<typeof contestApi.listContests>>);
    vi.mocked(presentationApi.config).mockResolvedValue(config); vi.mocked(presentationApi.update).mockResolvedValue(config);
    vi.mocked(screenApi.list).mockResolvedValue([instance]); vi.mocked(screenApi.command).mockResolvedValue({}); vi.mocked(screenApi.revoke).mockResolvedValue();
    vi.mocked(screenApi.playlists).mockResolvedValue([]); vi.mocked(screenApi.groups).mockResolvedValue([]);
    vi.mocked(screenApi.register).mockResolvedValue({ instanceId: 3, contestId: 7, name: 'Main Hall', clientToken: 'secret', currentView: 'SCOREBOARD', registeredAt: '2026-07-22T01:00:00Z' });
    vi.mocked(screenApi.heartbeat).mockResolvedValue({ instanceId: 3, serverTime: '2026-07-22T01:00:00Z', commandId: 4, targetView: 'AWARDS', groupPlayback: null });
  });
  it('loads published config and sends a remote screen command', async () => {
    const wrapper = mount(ScreenManageView); await flushPromises();
    expect(presentationApi.config).toHaveBeenCalledWith(7, 'SCREEN'); expect(wrapper.text()).toContain('Main Hall');
    await wrapper.findAll('button').find((button) => button.text().includes('发送'))!.trigger('click'); await flushPromises();
    expect(screenApi.command).toHaveBeenCalledWith(7, 3, 'SCOREBOARD'); wrapper.unmount();
  });
  it('registers once, stores the token locally, and applies the newest heartbeat command', async () => {
    const wrapper = mount(ScreenClientView); await flushPromises();
    expect(screenApi.register).toHaveBeenCalledWith(7, 'Main Hall'); expect(screenApi.heartbeat).toHaveBeenCalledWith(3, 'secret', 'SCOREBOARD');
    expect(wrapper.text()).toContain('AWARDS'); expect(localStorage.getItem('project-balloon:screen:7')).toContain('secret'); wrapper.unmount();
  });
});
