import { apiRequest } from './client';
import type { ScoreboardResponse, ScoreboardCell } from './types';

export type ResolverRunStatus = 'READY' | 'RUNNING' | 'PAUSED' | 'COMPLETED';

export interface ResolverReveal {
  teamId: number;
  problemId: number;
  before: ScoreboardCell;
  after: ScoreboardCell;
}

export interface ResolverState {
  stepIndex: number;
  totalSteps: number;
  board: ScoreboardResponse;
  lastReveal: ResolverReveal | null;
}

export interface ResolverRun {
  id: number;
  contestId: number;
  official: boolean;
  status: ResolverRunStatus;
  currentStep: number;
  totalSteps: number;
  sourcePublicSnapshotId: number;
  sourceFinalSnapshotId: number;
  planSha256: string;
  createdByUserId: number;
  startedAt: string | null;
  completedAt: string | null;
  autoPlayEnabled: boolean;
  autoPlayIntervalMilliseconds: number;
  nextAutoAt: string | null;
  createdAt: string;
  updatedAt: string;
  version: number;
  state: ResolverState;
}

export interface ResolverPublicRun {
  id: number;
  contestId: number;
  status: Exclude<ResolverRunStatus, 'READY'>;
  currentStep: number;
  totalSteps: number;
  updatedAt: string;
  state: ResolverState;
}

export interface ResolverEvent {
  id: number;
  eventType: string;
  payload: Record<string, unknown>;
  sequence: number;
  actorUserId: number | null;
  createdAt: string;
}

export interface ResolverSourceSnapshot {
  id: number;
  version: number;
  generatedAt: string;
  payloadSha256: string;
}

export interface ResolverSources {
  publicSnapshot: ResolverSourceSnapshot;
  finalSnapshot: ResolverSourceSnapshot;
}

function runPath(id: number, suffix = '') {
  return `/api/admin/resolver-runs/${id}${suffix}`;
}
function command(id: number, action: string, expectedVersion: number) {
  return apiRequest<ResolverRun>(runPath(id, `/${action}`), {
    method: 'POST',
    body: { expectedVersion },
  });
}

export const resolverApi = {
  list(contestId: number) {
    return apiRequest<ResolverRun[]>(`/api/admin/contests/${contestId}/resolver-runs`);
  },
  sources(contestId: number) {
    return apiRequest<ResolverSources>(`/api/admin/contests/${contestId}/resolver-sources`);
  },
  create(contestId: number, publicSnapshotId: number, finalSnapshotId: number, official: boolean) {
    return apiRequest<ResolverRun>(`/api/admin/contests/${contestId}/resolver-runs`, {
      method: 'POST',
      body: { publicSnapshotId, finalSnapshotId, official },
    });
  },
  get(id: number) {
    return apiRequest<ResolverRun>(runPath(id));
  },
  publicState(id: number) {
    return apiRequest<ResolverPublicRun>(`/api/public/resolver-runs/${id}/state`);
  },
  events(id: number) {
    return apiRequest<ResolverEvent[]>(runPath(id, '/events'));
  },
  start(id: number, version: number) {
    return command(id, 'start', version);
  },
  next(id: number, version: number) {
    return command(id, 'next', version);
  },
  previous(id: number, version: number) {
    return command(id, 'previous', version);
  },
  pause(id: number, version: number) {
    return command(id, 'pause', version);
  },
  resume(id: number, version: number) {
    return command(id, 'resume', version);
  },
  complete(id: number, version: number) {
    return command(id, 'complete', version);
  },
  autoPlay(id: number, expectedVersion: number, enabled: boolean, intervalMilliseconds: number) {
    return apiRequest<ResolverRun>(runPath(id, '/auto-play'), {
      method: 'POST',
      body: { expectedVersion, enabled, intervalMilliseconds },
    });
  },
};
