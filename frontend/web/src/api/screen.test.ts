import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiRequest } from './client';
import { screenApi } from './screen';
import { presentationApi } from './presentation';
vi.mock('./client', () => ({ apiRequest: vi.fn() }));
describe('screen and presentation APIs', () => {
  beforeEach(() => vi.mocked(apiRequest).mockReset());
  it('matches Java screen registration and heartbeat paths', async () => {
    await screenApi.register(7, 'Main'); await screenApi.heartbeat(3, 'token', 'SCOREBOARD');
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/public/screens/register', { method: 'POST', body: { contestId: 7, name: 'Main' } });
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/public/screens/3/heartbeat', { method: 'POST', body: { clientToken: 'token', currentView: 'SCOREBOARD' } });
  });
  it('uses mode-scoped config and operator control paths', async () => {
    await presentationApi.config(7, 'SCREEN'); await screenApi.command(7, 3, 'AWARDS'); await screenApi.revoke(7, 3);
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/presentation-configs/7?mode=SCREEN');
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/screen-instances/7/3/commands', { method: 'POST', body: { targetView: 'AWARDS' } });
    expect(apiRequest).toHaveBeenNthCalledWith(3, '/api/screen-instances/7/3', { method: 'DELETE' });
  });
  it('matches Java playlist and group orchestration paths', async () => {
    await screenApi.playlists(7); await screenApi.groups(7); await screenApi.controlGroup(4, 'PLAY', 2, { playlistId: 9 });
    expect(apiRequest).toHaveBeenNthCalledWith(1, '/api/contests/7/screen-playlists');
    expect(apiRequest).toHaveBeenNthCalledWith(2, '/api/contests/7/screen-groups');
    expect(apiRequest).toHaveBeenNthCalledWith(3, '/api/screen-groups/4/control', { method: 'POST', body: { action: 'PLAY', expectedVersion: 2, playlistId: 9 } });
  });
});
