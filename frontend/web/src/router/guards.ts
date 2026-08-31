import type { ComputedRef } from 'vue';
import type { RouteLocationNormalized, RouteLocationRaw } from 'vue-router';
import { homeForUser } from '../auth/access';
import type { CurrentUserResponse, DeploymentInfo, PermissionCode } from '../api/types';

export interface RouterGuardSession {
  initialize(): Promise<void>;
  state: {
    deployment: DeploymentInfo;
    user: {
      readonly userType: CurrentUserResponse['userType'];
      readonly permissions: readonly PermissionCode[];
      readonly passwordResetRequired: boolean;
    } | null;
  };
  isAuthenticated: ComputedRef<boolean>;
  isTeam: ComputedRef<boolean>;
  isStaff: ComputedRef<boolean>;
  isSuperAdmin: ComputedRef<boolean>;
  hasPermission(permission: string): boolean;
}

export type RouteGuardResult = RouteLocationRaw | boolean;

export async function resolveRouteGuard(
  to: RouteLocationNormalized,
  session: RouterGuardSession,
): Promise<RouteGuardResult> {
  await session.initialize();
  if (to.meta.dailyOnly && session.state.deployment.mode === 'competition') {
    return session.isAuthenticated.value ? { name: 'contests' } : { name: 'login' };
  }
  if (to.meta.competitionOnly && session.state.deployment.mode !== 'competition') {
    return { name: 'admin-home' };
  }
  if (to.meta.requiresAuth && !session.isAuthenticated.value) {
    const staffRoute =
      to.meta.requiresStaff || to.meta.requiresSuperAdmin || to.meta.requiredPermission;
    return { name: staffRoute ? 'admin-login' : 'login', query: { redirect: to.fullPath } };
  }
  if (
    session.isAuthenticated.value &&
    session.state.user?.passwordResetRequired &&
    to.name !== 'change-password'
  ) {
    return { name: 'change-password' };
  }
  if (to.meta.requiresTeam && !session.isTeam.value) {
    return { name: 'forbidden' };
  }
  if (to.meta.requiresStaff && !session.isStaff.value) {
    return { name: 'forbidden' };
  }
  if (to.meta.requiresSuperAdmin && !session.isSuperAdmin.value) {
    return { name: 'forbidden' };
  }
  if (
    typeof to.meta.requiredPermission === 'string' &&
    !session.hasPermission(to.meta.requiredPermission)
  ) {
    return { name: 'forbidden' };
  }
  if (to.meta.guestOnly && session.isAuthenticated.value) {
    return homeForUser(session.state.user);
  }
  return true;
}
