import type { RouteLocationRaw } from 'vue-router';
import type { PermissionCode, UserType } from '../api/types';

const permissionHomes: Array<[PermissionCode, string]> = [
  ['CONTEST_MANAGE', '/admin'],
  ['CLARIFICATION_MANAGE', '/judge'],
  ['PRINTING_MANAGE', '/printer'],
  ['BALLOON_MANAGE', '/balloon'],
  ['RESOLVER_MANAGE', '/resolver'],
  ['AWARD_MANAGE', '/awards'],
  ['SCREEN_MANAGE', '/screen/manage'],
  ['LIVE_MANAGE', '/live/manage'],
];

export function homeForUser(
  user: { userType: UserType; permissions: readonly PermissionCode[] } | null | undefined,
): RouteLocationRaw {
  if (!user || user.userType === 'TEAM') return '/contests';
  if (user.userType === 'INDIVIDUAL') return '/practice';
  if (user.userType === 'SUPER_ADMIN') return '/admin';
  return permissionHomes.find(([code]) => user.permissions.includes(code))?.[1] ?? '/admin';
}
