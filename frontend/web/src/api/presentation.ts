import { apiRequest } from './client';
import type { ScoreboardResponse } from './types';

export type PresentationMode = 'SCREEN' | 'LIVE';
export interface PresentationConfig {
  contestId: number;
  mode: PresentationMode;
  enabled: boolean;
  title: string | null;
  subtitle: string | null;
  accentColor: string;
  rowLimit: number;
  showAnnouncements: boolean;
  announcementIntervalSeconds: number;
  template: 'DEFAULT' | 'CINEMATIC' | 'MINIMAL' | 'SPLIT' | 'CUSTOM';
  customTemplateId?: number | null;
  customTemplateName?: string | null;
  customBackgroundColor?: string | null;
  customForegroundColor?: string | null;
  customAccentColor?: string | null;
  customFontFamily?: string | null;
  customDensity?: 'COMPACT' | 'COMFORTABLE' | 'SPACIOUS' | null;
  customShowClock?: boolean | null;
  customShowLogo?: boolean | null;
  customLogoObjectKey?: string | null;
  updatedAt: string | null;
}
export type PresentationConfigPayload = Pick<
  PresentationConfig,
  | 'enabled'
  | 'title'
  | 'subtitle'
  | 'accentColor'
  | 'rowLimit'
  | 'showAnnouncements'
  | 'announcementIntervalSeconds'
  | 'template'
> & { customTemplateId?: number | null };
export interface PresentationTemplate {
  id: number;
  name: string;
  description: string;
  backgroundColor: string;
  foregroundColor: string;
  accentColor: string;
  fontFamily: string;
  density: 'COMPACT' | 'COMFORTABLE' | 'SPACIOUS';
  showClock: boolean;
  showLogo: boolean;
  logoObjectKey: string | null;
  updatedAt: string;
}
export type PresentationTemplatePayload = Omit<PresentationTemplate, 'id' | 'updatedAt'>;
export interface BroadcastToken {
  id: number;
  label: string;
  expiresAt: string;
  revokedAt: string | null;
  lastUsedAt: string | null;
  createdAt: string;
}
export interface BroadcastTokenCreated {
  id: number;
  label: string;
  token: string;
  expiresAt: string;
  createdAt: string;
}
export interface PublishedPresentation {
  contestId: number;
  contestName: string;
  contestStatus: string;
  startAt: string | null;
  freezeAt: string | null;
  endAt: string | null;
  serverTime: string;
  config: PresentationConfig;
  scoreboard: ScoreboardResponse;
  announcements: Array<{
    id: number;
    title: string;
    body: string;
    pinned: boolean;
    publishedAt: string | null;
  }>;
}
export interface PresentationMetrics {
  balloons: {
    total: number;
    firstBlood: number;
    pending: number;
    preparing: number;
    delivering: number;
    delivered: number;
    cancelled: number;
    colors: Array<{ name: string; total: number }>;
  };
  submissions: {
    total: number;
    accepted: number;
    pending: number;
    languages: Array<{ name: string; total: number }>;
    trend: Array<{ bucket: string; total: number; accepted: number }>;
  };
}
export type LiveScene =
  | 'SCOREBOARD'
  | 'FIRST_BLOOD'
  | 'BALLOONS'
  | 'FREEZE_COUNTDOWN'
  | 'STATISTICS'
  | 'RESOLVER'
  | 'AWARDS'
  | 'TITLE_CARD';
export interface LiveProgramState {
  contestId: number;
  currentScene: LiveScene;
  resolverRunId: number | null;
  transitionMilliseconds: number;
  showClock: boolean;
  tickerEnabled: boolean;
  titleCardText: string | null;
  version: number;
  updatedAt: string | null;
}
export interface ResolverRunOption {
  id: number;
  official: boolean;
  status: string;
  currentStep: number;
  totalSteps: number;
  createdAt: string;
}
export interface StaffLiveProgram {
  program: LiveProgramState;
  resolverRuns: ResolverRunOption[];
}
export interface PublishedLiveProgram {
  contestId: number;
  currentScene: LiveScene;
  resolverRunId: number | null;
  transitionMilliseconds: number;
  showClock: boolean;
  tickerEnabled: boolean;
  titleCardText: string | null;
  serverTime: string;
  version: number;
}
export interface LiveProgramUpdatePayload {
  currentScene: LiveScene;
  resolverRunId: number | null;
  transitionMilliseconds: number;
  showClock: boolean;
  tickerEnabled: boolean;
  titleCardText: string | null;
  expectedVersion: number;
}

export const presentationApi = {
  config(contestId: number, mode: PresentationMode) {
    return apiRequest<PresentationConfig>(`/api/presentation-configs/${contestId}?mode=${mode}`);
  },
  update(contestId: number, mode: PresentationMode, payload: PresentationConfigPayload) {
    return apiRequest<PresentationConfig>(
      `/api/presentation-configs/${contestId}/${mode.toLowerCase()}`,
      { method: 'PUT', body: payload },
    );
  },
  published(contestId: number, mode: PresentationMode, token?: string) {
    return apiRequest<PublishedPresentation>(
      `/api/public/presentations/${contestId}?mode=${mode}`,
      {
        suppressUnauthorizedHandler: true,
        ...(token ? { headers: { 'X-Broadcast-Token': token } } : {}),
      },
    );
  },
  metrics(contestId: number, mode: PresentationMode, token?: string) {
    return apiRequest<PresentationMetrics>(
      `/api/public/presentations/${contestId}/metrics?mode=${mode}`,
      {
        suppressUnauthorizedHandler: true,
        ...(token ? { headers: { 'X-Broadcast-Token': token } } : {}),
      },
    );
  },
  tokens(contestId: number) {
    return apiRequest<BroadcastToken[]>(`/api/presentation-configs/${contestId}/live/tokens`);
  },
  createToken(contestId: number, payload: { label: string; expiresAt: string }) {
    return apiRequest<BroadcastTokenCreated>(`/api/presentation-configs/${contestId}/live/tokens`, {
      method: 'POST',
      body: payload,
    });
  },
  revokeToken(contestId: number, id: number) {
    return apiRequest<void>(`/api/presentation-configs/${contestId}/live/tokens/${id}`, {
      method: 'DELETE',
    });
  },
  templates() {
    return apiRequest<PresentationTemplate[]>('/api/presentation-templates');
  },
  program(contestId: number) {
    return apiRequest<StaffLiveProgram>(`/api/presentation-configs/${contestId}/live/program`);
  },
  updateProgram(contestId: number, payload: LiveProgramUpdatePayload) {
    return apiRequest<LiveProgramState>(`/api/presentation-configs/${contestId}/live/program`, {
      method: 'PUT',
      body: payload,
    });
  },
  publishedProgram(contestId: number, token?: string) {
    return apiRequest<PublishedLiveProgram>(`/api/public/presentations/${contestId}/program`, {
      suppressUnauthorizedHandler: true,
      ...(token ? { headers: { 'X-Broadcast-Token': token } } : {}),
    });
  },
  createTemplate(payload: PresentationTemplatePayload) {
    return apiRequest<PresentationTemplate>('/api/presentation-templates', {
      method: 'POST',
      body: payload,
    });
  },
  updateTemplate(id: number, payload: PresentationTemplatePayload) {
    return apiRequest<PresentationTemplate>(`/api/presentation-templates/${id}`, {
      method: 'PUT',
      body: payload,
    });
  },
};
