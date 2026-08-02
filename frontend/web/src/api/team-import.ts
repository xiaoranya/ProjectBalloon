import { apiRequest } from './client';

export type ParticipationType = 'OFFICIAL' | 'STAR' | 'PRACTICE';

export interface TeamBatchRowRequest {
  name: string;
  school: string | null;
  seatNo: string | null;
  groupName: string | null;
  star: boolean;
  username: string | null;
  initialPassword: string | null;
}

export interface TeamBatchImportRequest {
  teams: TeamBatchRowRequest[];
  contestId: number | null;
  participationType: ParticipationType | null;
  requirePasswordReset: boolean;
  idempotencyKey: string;
}

export interface TeamBatchCreatedRow {
  index: number;
  teamId: number;
  userId: number | null;
  username: string | null;
}

export interface TeamBatchImportResponse {
  batchId: string;
  totalRequested: number;
  created: TeamBatchCreatedRow[];
}

export interface TeamMemberRequest {
  name: string;
  email: string | null;
  phone: string | null;
  roleName: string | null;
}

export interface TeamMemberResponse extends TeamMemberRequest {
  id: number;
  teamId: number;
  createdAt: string;
}

export const teamImportApi = {
  importTeams(payload: TeamBatchImportRequest) {
    if (payload.teams.length < 1 || payload.teams.length > 100) {
      throw new RangeError('每个后台批次必须包含 1–100 支队伍');
    }
    if (!payload.idempotencyKey.trim() || payload.idempotencyKey.length > 128) {
      throw new RangeError('幂等键必须为 1–128 个字符');
    }
    return apiRequest<TeamBatchImportResponse>('/api/teams/batch', {
      method: 'POST',
      body: payload,
    });
  },

  addMember(teamId: number, payload: TeamMemberRequest) {
    return apiRequest<TeamMemberResponse>(`/api/teams/${teamId}/members`, {
      method: 'POST',
      body: payload,
    });
  },
};
