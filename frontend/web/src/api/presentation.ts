import { apiRequest } from './client';
import type { Scoreboard } from './types';

export type PresentationMode = 'SCREEN' | 'LIVE';
export interface PresentationConfig { contestId: number; mode: PresentationMode; enabled: boolean; title: string | null; subtitle: string | null; accentColor: string; rowLimit: number; showAnnouncements: boolean; announcementIntervalSeconds: number; updatedAt: string | null; }
export type PresentationConfigPayload = Omit<PresentationConfig, 'contestId' | 'mode' | 'updatedAt'>;
export interface BroadcastToken { id: number; label: string; expiresAt: string; revokedAt: string | null; lastUsedAt: string | null; createdAt: string; }
export interface BroadcastTokenCreated { id: number; label: string; token: string; expiresAt: string; createdAt: string; }
export interface PublishedPresentation { contestId: number; contestName: string; contestStatus: string; startAt: string | null; freezeAt: string | null; endAt: string | null; serverTime: string; config: PresentationConfig; scoreboard: Scoreboard; announcements: Array<{ id: number; title: string; body: string; pinned: boolean; publishedAt: string | null }>; }
export interface PresentationMetrics { balloons: { total: number; firstBlood: number; pending: number; preparing: number; delivering: number; delivered: number; cancelled: number; colors: Array<{ name: string; total: number }> }; submissions: { total: number; accepted: number; pending: number; languages: Array<{ name: string; total: number }>; trend: Array<{ bucket: string; total: number; accepted: number }> } }

export const presentationApi = {
  config(contestId: number, mode: PresentationMode) { return apiRequest<PresentationConfig>(`/api/presentation-configs/${contestId}?mode=${mode}`); },
  update(contestId: number, mode: PresentationMode, payload: PresentationConfigPayload) { return apiRequest<PresentationConfig>(`/api/presentation-configs/${contestId}/${mode.toLowerCase()}`, { method: 'PUT', body: payload }); },
  published(contestId: number, mode: PresentationMode, token?: string) { return apiRequest<PublishedPresentation>(`/api/public/presentations/${contestId}?mode=${mode}`, { suppressUnauthorizedHandler: true, ...(token ? { headers: { 'X-Broadcast-Token': token } } : {}) }); },
  metrics(contestId: number, mode: PresentationMode, token?: string) { return apiRequest<PresentationMetrics>(`/api/public/presentations/${contestId}/metrics?mode=${mode}`, { suppressUnauthorizedHandler: true, ...(token ? { headers: { 'X-Broadcast-Token': token } } : {}) }); },
  tokens(contestId: number) { return apiRequest<BroadcastToken[]>(`/api/presentation-configs/${contestId}/live/tokens`); },
  createToken(contestId: number, payload: { label: string; expiresAt: string }) { return apiRequest<BroadcastTokenCreated>(`/api/presentation-configs/${contestId}/live/tokens`, { method: 'POST', body: payload }); },
  revokeToken(contestId: number, id: number) { return apiRequest<void>(`/api/presentation-configs/${contestId}/live/tokens/${id}`, { method: 'DELETE' }); },
};
