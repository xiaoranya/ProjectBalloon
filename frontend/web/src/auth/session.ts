import { computed, reactive, readonly } from 'vue';
import { apiRequest, clearCsrfToken, setUnauthorizedHandler } from '../api/client';
import type { CurrentUser } from '../api/types';

const state = reactive({
  user: null as CurrentUser | null,
  initialized: false,
  loading: false,
});

let initialization: Promise<void> | null = null;

async function initialize() {
  if (state.initialized) {
    return;
  }
  if (initialization) {
    return initialization;
  }
  initialization = (async () => {
    state.loading = true;
    try {
      state.user = await apiRequest<CurrentUser>('/api/auth/me');
    } catch {
      state.user = null;
    } finally {
      state.initialized = true;
      state.loading = false;
      initialization = null;
    }
  })();
  return initialization;
}

async function login(username: string, password: string) {
  state.loading = true;
  try {
    state.user = await apiRequest<CurrentUser>('/api/auth/login', {
      method: 'POST',
      body: { username, password },
    });
    state.initialized = true;
    return state.user;
  } finally {
    state.loading = false;
  }
}

async function register(username: string, password: string, displayName: string) {
  state.loading = true;
  try {
    state.user = await apiRequest<CurrentUser>('/api/auth/register', { method: 'POST', body: { username, password, displayName } });
    state.initialized = true;
    return state.user;
  } finally { state.loading = false; }
}

async function logout() {
  try {
    await apiRequest<void>('/api/auth/logout', { method: 'POST' });
  } finally {
    state.user = null;
    state.initialized = true;
    clearCsrfToken();
  }
}

async function changePassword(currentPassword: string, newPassword: string) {
  state.user = await apiRequest<CurrentUser>('/api/auth/password', {
    method: 'POST',
    body: { currentPassword, newPassword },
  });
  return state.user;
}

function clearSession() {
  state.user = null;
  state.initialized = true;
  clearCsrfToken();
}

setUnauthorizedHandler(clearSession);

function hasPermission(role: string) {
  return state.user?.userType === 'SUPER_ADMIN'
    || state.user?.userType === role
    || state.user?.roles.includes(role) === true;
}

export function useSession() {
  return {
    state: readonly(state),
    isAuthenticated: computed(() => state.user !== null),
    isTeam: computed(() => state.user?.userType === 'TEAM'),
    isStaff: computed(() => state.user !== null && !['TEAM', 'INDIVIDUAL'].includes(state.user.userType)),
    isIndividual: computed(() => state.user?.userType === 'INDIVIDUAL'),
    isAdmin: computed(() => ['SUPER_ADMIN', 'CONTEST_ADMIN'].includes(state.user?.userType ?? '')),
    isSuperAdmin: computed(() => state.user?.userType === 'SUPER_ADMIN'),
    isContestAdmin: computed(() => state.user?.userType === 'CONTEST_ADMIN'),
    isJudge: computed(() => hasPermission('JUDGE')),
    isPrinter: computed(() => hasPermission('PRINTER')),
    isBalloonStaff: computed(() => hasPermission('BALLOON_STAFF')),
    isAwardOperator: computed(() => hasPermission('AWARD_OPERATOR')),
    isResolverOperator: computed(() => hasPermission('RESOLVER_OPERATOR')),
    isScreenOperator: computed(() => hasPermission('SCREEN_OPERATOR')),
    isLiveOperator: computed(() => hasPermission('LIVE_OPERATOR')),
    initialize,
    login,
    register,
    changePassword,
    logout,
    clearSession,
  };
}
