import type { RouteLocationRaw } from 'vue-router';
import type { UserType } from '../api/types';

export const staffHomeByUserType: Record<Exclude<UserType, 'TEAM'>, string> = {
  SUPER_ADMIN: '/admin',
  CONTEST_ADMIN: '/admin',
  JUDGE: '/judge',
  PRINTER: '/printer',
  BALLOON_STAFF: '/balloon',
  AWARD_OPERATOR: '/awards',
  RESOLVER_OPERATOR: '/resolver',
  SCREEN_OPERATOR: '/screen/manage',
  LIVE_OPERATOR: '/live/manage',
};

export function homeForUserType(userType: UserType | undefined): RouteLocationRaw {
  if (!userType || userType === 'TEAM') return '/contests';
  return staffHomeByUserType[userType];
}
