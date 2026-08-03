import { apiRequest } from './client';

export type ScreenViewTarget =
  | 'SCOREBOARD'
  | 'FIRST_BLOOD'
  | 'BALLOONS'
  | 'FREEZE_COUNTDOWN'
  | 'STATISTICS'
  | 'RESOLVER'
  | 'AWARDS';
export interface ScreenInstance {
  id: number;
  contestId: number;
  name: string;
  currentView: ScreenViewTarget;
  online: boolean;
  lastSeenAt: string | null;
  lastIp: string | null;
  revokedAt: string | null;
  createdAt: string;
}
export interface ScreenRegistration {
  instanceId: number;
  contestId: number;
  name: string;
  clientToken: string;
  currentView: ScreenViewTarget;
  registeredAt: string;
}
export interface ScreenPlaylistItem {
  id: number;
  targetView: ScreenViewTarget;
  durationSeconds: number;
  displayOrder: number;
}
export interface ScreenPlaylist {
  id: number;
  contestId: number;
  name: string;
  loopEnabled: boolean;
  version: number;
  items: ScreenPlaylistItem[];
  createdAt: string;
  updatedAt: string;
}
export interface ScreenGroup {
  id: number;
  contestId: number;
  name: string;
  instanceIds: number[];
  playlistId: number | null;
  playbackStatus: 'STOPPED' | 'PLAYING' | 'PAUSED';
  playbackStartedAt: string | null;
  pausedElapsedSeconds: number;
  lockedView: ScreenViewTarget | null;
  version: number;
  createdAt: string;
  updatedAt: string;
}
export interface ScreenGroupPlayback {
  groupId: number;
  groupName: string;
  playlistId: number | null;
  loopEnabled: boolean;
  status: ScreenGroup['playbackStatus'];
  startedAt: string | null;
  pausedElapsedSeconds: number;
  lockedView: ScreenViewTarget | null;
  version: number;
  items: ScreenPlaylistItem[];
}
export interface ScreenHeartbeat {
  instanceId: number;
  serverTime: string;
  commandId: number | null;
  targetView: ScreenViewTarget | null;
  groupPlayback: ScreenGroupPlayback | null;
}
export interface ScreenPlaylistInput {
  name: string;
  loopEnabled: boolean;
  items: Array<{ targetView: ScreenViewTarget; durationSeconds: number }>;
  expectedVersion?: number;
}
export interface ScreenGroupInput {
  name: string;
  instanceIds: number[];
  expectedVersion?: number;
}
export type ScreenGroupAction = 'PLAY' | 'PAUSE' | 'RESUME' | 'STOP' | 'LOCK' | 'UNLOCK';

export const screenApi = {
  register(contestId: number, name: string) {
    return apiRequest<ScreenRegistration>('/api/public/screens/register', {
      method: 'POST',
      body: { contestId, name },
    });
  },
  heartbeat(instanceId: number, clientToken: string, currentView: ScreenViewTarget) {
    return apiRequest<ScreenHeartbeat>(`/api/public/screens/${instanceId}/heartbeat`, {
      method: 'POST',
      body: { clientToken, currentView },
    });
  },
  list(contestId: number) {
    return apiRequest<ScreenInstance[]>(`/api/screen-instances/${contestId}`);
  },
  command(contestId: number, instanceId: number, targetView: ScreenViewTarget) {
    return apiRequest(`/api/screen-instances/${contestId}/${instanceId}/commands`, {
      method: 'POST',
      body: { targetView },
    });
  },
  revoke(contestId: number, instanceId: number) {
    return apiRequest<void>(`/api/screen-instances/${contestId}/${instanceId}`, {
      method: 'DELETE',
    });
  },
  playlists(contestId: number) {
    return apiRequest<ScreenPlaylist[]>(`/api/contests/${contestId}/screen-playlists`);
  },
  createPlaylist(contestId: number, input: ScreenPlaylistInput) {
    return apiRequest<ScreenPlaylist>(`/api/contests/${contestId}/screen-playlists`, {
      method: 'POST',
      body: input,
    });
  },
  updatePlaylist(id: number, input: ScreenPlaylistInput) {
    return apiRequest<ScreenPlaylist>(`/api/screen-playlists/${id}`, {
      method: 'PUT',
      body: input,
    });
  },
  deletePlaylist(id: number) {
    return apiRequest<void>(`/api/screen-playlists/${id}`, { method: 'DELETE' });
  },
  groups(contestId: number) {
    return apiRequest<ScreenGroup[]>(`/api/contests/${contestId}/screen-groups`);
  },
  createGroup(contestId: number, input: ScreenGroupInput) {
    return apiRequest<ScreenGroup>(`/api/contests/${contestId}/screen-groups`, {
      method: 'POST',
      body: input,
    });
  },
  updateGroup(id: number, input: ScreenGroupInput) {
    return apiRequest<ScreenGroup>(`/api/screen-groups/${id}`, { method: 'PUT', body: input });
  },
  deleteGroup(id: number) {
    return apiRequest<void>(`/api/screen-groups/${id}`, { method: 'DELETE' });
  },
  controlGroup(
    id: number,
    action: ScreenGroupAction,
    expectedVersion: number,
    options: { playlistId?: number; targetView?: ScreenViewTarget } = {},
  ) {
    return apiRequest<ScreenGroup>(`/api/screen-groups/${id}/control`, {
      method: 'POST',
      body: { action, expectedVersion, ...options },
    });
  },
};
