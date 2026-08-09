import { apiRequest } from './client';

export interface Workstation {
  id: number;
  ipAddress: string;
  seatNo: string;
  label: string | null;
  enabled: boolean;
  lastSeenAt: string | null;
  version: number;
}

export interface WorkstationBinding {
  id: number;
  contestId: number;
  workstationId: number;
  ipAddress: string;
  seatNo: string;
  teamId: number;
  teamName: string;
  pairingCode?: string;
  boundAt: string;
  revokedAt: string | null;
}

export const competitionApi = {
  workstations: () => apiRequest<Workstation[]>('/api/admin/competition/workstations'),
  createWorkstation: (body: { ipAddress: string; seatNo: string; label?: string }) =>
    apiRequest<Workstation>('/api/admin/competition/workstations', { method: 'POST', body }),
  bindings: (contestId: number) =>
    apiRequest<WorkstationBinding[]>(`/api/admin/contests/${contestId}/workstation-bindings`),
  bind: (contestId: number, workstationId: number, teamId: number) =>
    apiRequest<WorkstationBinding>(`/api/admin/contests/${contestId}/workstation-bindings`, {
      method: 'POST',
      body: { workstationId, teamId },
    }),
  rotate: (contestId: number, bindingId: number) =>
    apiRequest<WorkstationBinding>(
      `/api/admin/contests/${contestId}/workstation-bindings/${bindingId}/rotate`,
      { method: 'POST' },
    ),
  revoke: (contestId: number, bindingId: number) =>
    apiRequest<void>(`/api/admin/contests/${contestId}/workstation-bindings/${bindingId}`, {
      method: 'DELETE',
    }),
};
