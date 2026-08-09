import type { RouteRecordRaw } from 'vue-router';

export const routes: RouteRecordRaw[] = [
  { path: '/', redirect: '/contests' },
  {
    path: '/problem-bank',
    name: 'problem-bank',
    component: () => import('./views/ProblemBankView.vue'),
    meta: { dailyOnly: true },
  },
  {
    path: '/practice',
    name: 'practice',
    component: () => import('./views/PracticeView.vue'),
    meta: { requiresAuth: true, dailyOnly: true },
  },
  {
    path: '/practice/virtual',
    name: 'practice-virtual',
    component: () => import('./views/VirtualPracticeView.vue'),
    meta: { requiresAuth: true, dailyOnly: true },
  },
  {
    path: '/training',
    name: 'training',
    component: () => import('./views/TrainingView.vue'),
    meta: { requiresAuth: true, dailyOnly: true },
  },
  {
    path: '/login',
    name: 'login',
    component: () => import('./views/LoginView.vue'),
    meta: { guestOnly: true },
  },
  {
    path: '/register',
    name: 'register',
    component: () => import('./views/RegisterView.vue'),
    meta: { guestOnly: true, dailyOnly: true },
  },
  {
    path: '/change-password',
    name: 'change-password',
    component: () => import('./views/ChangePasswordView.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/profile',
    name: 'profile',
    component: () => import('./views/ProfileView.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/contests',
    name: 'contests',
    component: () => import('./views/ContestListView.vue'),
    meta: { requiresAuth: true, requiresTeam: true },
  },
  {
    path: '/admin/login',
    name: 'admin-login',
    component: () => import('./views/AdminLoginView.vue'),
    meta: { guestOnly: true },
  },
  {
    path: '/judge',
    component: () => import('./layouts/JudgeLayout.vue'),
    meta: { requiresAuth: true, requiresStaff: true, requiredPermission: 'CLARIFICATION_MANAGE' },
    children: [
      {
        path: '',
        name: 'judge-home',
        component: () => import('./views/JudgeClarificationView.vue'),
      },
    ],
  },
  {
    path: '/printer',
    component: () => import('./layouts/PrinterLayout.vue'),
    meta: { requiresAuth: true, requiresStaff: true, requiredPermission: 'PRINTING_MANAGE' },
    children: [
      {
        path: '',
        name: 'printer-home',
        component: () => import('./views/PrinterRequestsView.vue'),
      },
    ],
  },
  {
    path: '/balloon',
    component: () => import('./layouts/BalloonLayout.vue'),
    meta: { requiresAuth: true, requiresStaff: true, requiredPermission: 'BALLOON_MANAGE' },
    children: [
      { path: '', name: 'balloon-home', component: () => import('./views/BalloonTasksView.vue') },
    ],
  },
  {
    path: '/resolver',
    component: () => import('./layouts/ResolverLayout.vue'),
    meta: { requiresAuth: true, requiresStaff: true, requiredPermission: 'RESOLVER_MANAGE' },
    children: [
      {
        path: '',
        name: 'resolver-home',
        component: () => import('./views/ResolverManageView.vue'),
      },
    ],
  },
  {
    path: '/awards',
    component: () => import('./layouts/AwardsLayout.vue'),
    meta: { requiresAuth: true, requiresStaff: true, requiredPermission: 'AWARD_MANAGE' },
    children: [
      { path: '', name: 'awards-home', component: () => import('./views/AwardsManageView.vue') },
      {
        path: 'presentation',
        name: 'awards-presentation',
        component: () => import('./views/AwardPresentationControlView.vue'),
      },
      {
        path: 'host-script',
        name: 'awards-host-script',
        component: () => import('./views/AwardHostScriptView.vue'),
      },
    ],
  },
  {
    path: '/awards/display',
    name: 'awards-display',
    component: () => import('./views/AwardDisplayView.vue'),
  },
  {
    path: '/resolver/display/:runId',
    name: 'resolver-display',
    component: () => import('./views/ResolverDisplayView.vue'),
  },
  {
    path: '/screen/manage',
    name: 'screen-manage',
    component: () => import('./views/ScreenManageView.vue'),
    meta: { requiresAuth: true, requiresStaff: true, requiredPermission: 'SCREEN_MANAGE' },
  },
  {
    path: '/screen',
    name: 'screen-client',
    component: () => import('./views/ScreenClientView.vue'),
  },
  {
    path: '/live/manage',
    name: 'live-manage',
    component: () => import('./views/LiveManageView.vue'),
    meta: { requiresAuth: true, requiresStaff: true, requiredPermission: 'LIVE_MANAGE' },
  },
  {
    path: '/live',
    name: 'live',
    component: () => import('./views/LiveView.vue'),
    props: { view: 'scoreboard' },
  },
  {
    path: '/live/first-blood',
    name: 'live-first-blood',
    component: () => import('./views/LiveView.vue'),
    props: { view: 'first-blood' },
  },
  {
    path: '/live/balloons',
    name: 'live-balloons',
    component: () => import('./views/LiveView.vue'),
    props: { view: 'balloons' },
  },
  {
    path: '/live/freeze-countdown',
    name: 'live-freeze',
    component: () => import('./views/LiveView.vue'),
    props: { view: 'freeze' },
  },
  {
    path: '/live/statistics',
    name: 'live-statistics',
    component: () => import('./views/LiveView.vue'),
    props: { view: 'statistics' },
  },
  {
    path: '/admin',
    component: () => import('./layouts/AdminLayout.vue'),
    meta: { requiresAuth: true, requiresStaff: true },
    children: [
      { path: '', name: 'admin-home', component: () => import('./views/AdminView.vue') },
      {
        path: 'contests',
        name: 'admin-contests',
        component: () => import('./views/AdminContestListView.vue'),
        meta: { requiredPermission: 'CONTEST_MANAGE' },
      },
      {
        path: 'contests/:contestId',
        name: 'admin-contest-detail',
        component: () => import('./views/AdminContestDetailView.vue'),
        meta: { requiredPermission: 'CONTEST_MANAGE' },
      },
      {
        path: 'contests/:contestId/rejudge-tasks',
        name: 'admin-contest-rejudge-tasks',
        component: () => import('./views/AdminBulkRejudgeView.vue'),
        meta: { requiredPermission: 'CONTEST_MANAGE' },
      },
      {
        path: 'contests/:contestId/announcements',
        name: 'admin-contest-announcements',
        component: () => import('./views/AdminAnnouncementsView.vue'),
        meta: { requiredPermission: 'CONTEST_MANAGE' },
      },
      {
        path: 'team-import',
        name: 'admin-team-import',
        component: () => import('./views/AdminTeamImportView.vue'),
        meta: { requiredPermission: 'CONTEST_MANAGE' },
      },
      {
        path: 'problems',
        name: 'admin-problems',
        component: () => import('./views/AdminProblemListView.vue'),
        meta: { requiresSuperAdmin: true },
      },
      {
        path: 'problems/new',
        name: 'admin-problem-new',
        component: () => import('./views/AdminProblemEditorView.vue'),
        meta: { requiresSuperAdmin: true },
      },
      {
        path: 'competition',
        name: 'admin-competition',
        component: () => import('./views/AdminCompetitionView.vue'),
        meta: { requiredPermission: 'CONTEST_MANAGE', competitionOnly: true },
      },
      {
        path: 'practice',
        name: 'admin-practice',
        component: () => import('./views/AdminPracticeView.vue'),
        meta: { requiresSuperAdmin: true, dailyOnly: true },
      },
      {
        path: 'problems/:problemId',
        name: 'admin-problem-editor',
        component: () => import('./views/AdminProblemEditorView.vue'),
        meta: { requiredPermission: 'CONTEST_MANAGE' },
      },
      {
        path: 'staff-accounts',
        name: 'admin-staff-accounts',
        component: () => import('./views/AdminStaffAccountsView.vue'),
        meta: { requiresSuperAdmin: true },
      },
      {
        path: 'permissions',
        name: 'admin-permissions',
        component: () => import('./views/AdminPermissionsView.vue'),
        meta: { requiresSuperAdmin: true },
      },
    ],
  },
  {
    path: '/contests/:contestId',
    component: () => import('./layouts/ContestantLayout.vue'),
    meta: { requiresAuth: true, requiresTeam: true },
    children: [
      { path: '', redirect: { name: 'problems' } },
      {
        path: 'problems',
        name: 'problems',
        component: () => import('./views/ProblemListView.vue'),
      },
      {
        path: 'problems/:problemId',
        name: 'problem-detail',
        component: () => import('./views/ProblemDetailView.vue'),
      },
      {
        path: 'submissions',
        name: 'submissions',
        component: () => import('./views/SubmissionListView.vue'),
      },
      {
        path: 'submissions/:submissionId',
        name: 'submission-detail',
        component: () => import('./views/SubmissionDetailView.vue'),
      },
      {
        path: 'clarifications',
        name: 'contest-clarifications',
        component: () => import('./views/ContestClarificationView.vue'),
      },
      {
        path: 'printing',
        name: 'contest-printing',
        component: () => import('./views/ContestPrintingView.vue'),
      },
      {
        path: 'scoreboard',
        name: 'scoreboard',
        component: () => import('./views/ScoreboardView.vue'),
      },
    ],
  },
  { path: '/forbidden', name: 'forbidden', component: () => import('./views/ForbiddenView.vue') },
  {
    path: '/:pathMatch(.*)*',
    name: 'not-found',
    component: () => import('./views/NotFoundView.vue'),
  },
];
