import { apiRequest } from './client';
import type {
  AuditLog,
  Contest,
  ContestAdminScope,
  HealthResponse,
  PageResponse,
  StaffAccount,
} from './types';

export interface StaffAccountPayload {
  username: string;
  displayName: string;
  userType: StaffAccount['userType'];
  initialPassword: string;
  requirePasswordReset: boolean;
}

export interface AuditLogFilters {
  page?: number;
  size?: number;
  actorUserId?: number;
  action?: string;
  result?: string;
  from?: string;
  to?: string;
}

export const adminApi = {
  getHealth() {
    return apiRequest<HealthResponse>('/api/health', { acceptedStatuses: [503] });
  },

  listAuditLogs(filters: AuditLogFilters = {}) {
    const params = new URLSearchParams({
      page: String(filters.page ?? 0),
      size: String(filters.size ?? 25),
      sort: 'createdAt,desc',
    });
    if (filters.actorUserId !== undefined) params.set('actorUserId', String(filters.actorUserId));
    if (filters.action) params.set('action', filters.action);
    if (filters.result) params.set('result', filters.result);
    if (filters.from) params.set('from', filters.from);
    if (filters.to) params.set('to', filters.to);
    return apiRequest<PageResponse<AuditLog>>(`/api/admin/audit-logs?${params}`);
  },

  listStaffAccounts(page = 0, size = 100) {
    return apiRequest<PageResponse<StaffAccount>>(
      `/api/admin/staff-accounts?page=${page}&size=${size}&sort=username,asc`,
    );
  },

  createStaffAccount(payload: StaffAccountPayload) {
    return apiRequest<StaffAccount>('/api/admin/staff-accounts', {
      method: 'POST',
      body: payload,
    });
  },

  updateStaffAccount(
    userId: number,
    payload: Partial<Pick<StaffAccount, 'displayName' | 'userType' | 'enabled'>>,
  ) {
    return apiRequest<StaffAccount>(`/api/admin/staff-accounts/${userId}`, {
      method: 'PATCH',
      body: payload,
    });
  },

  resetStaffPassword(userId: number, newPassword: string, requirePasswordReset = true) {
    return apiRequest<StaffAccount>(`/api/admin/staff-accounts/${userId}/reset-password`, {
      method: 'POST',
      body: { newPassword, requirePasswordReset },
    });
  },

  listContestAdminScopes() {
    return apiRequest<ContestAdminScope[]>('/api/admin/contest-admins');
  },

  updateContestAdminScope(userId: number, contestIds: number[]) {
    return apiRequest<ContestAdminScope>(`/api/admin/contest-admins/${userId}/contests`, {
      method: 'PUT',
      body: { contestIds },
    });
  },

  listContests(page = 0, size = 100, manageableOnly = false) {
    const params = new URLSearchParams({
      page: String(page),
      size: String(size),
      sort: 'updatedAt,desc',
    });
    if (manageableOnly) params.set('manageableOnly', 'true');
    return apiRequest<PageResponse<Contest>>(`/api/contests?${params}`);
  },

  async listAllManageableContests() {
    const firstPage = await this.listContests(0, 500, true);
    const remaining = await Promise.all(
      Array.from({ length: Math.max(0, firstPage.totalPages - 1) }, (_, index) =>
        this.listContests(index + 1, 500, true),
      ),
    );
    return [firstPage, ...remaining].flatMap((page) => page.content);
  },
};
