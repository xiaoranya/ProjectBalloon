import { createApp } from 'vue';
import { createRouter, createWebHistory } from 'vue-router';
import * as ElementPlusIcons from '@element-plus/icons-vue';
import App from './App.vue';
import { routes } from './routes';
import { homeForUser } from './auth/access';
import { useSession } from './auth/session';
import { setUnauthorizedHandler } from './api/client';
import 'element-plus/theme-chalk/index.css';
import './styles/base.css';
import './styles/shells.css';
import './styles/display.css';
import './styles/media.css';

const router = createRouter({
  history: createWebHistory(),
  routes,
});

const app = createApp(App);

for (const [name, component] of Object.entries(ElementPlusIcons)) {
  app.component(name, component);
}

const session = useSession();

setUnauthorizedHandler(() => {
  session.clearSession();
  const current = router.currentRoute.value;
  if (!current.matched.some((record) => record.meta.requiresAuth)) return;
  const staffRoute = current.matched.some(
    (record) =>
      record.meta.requiresStaff || record.meta.requiresSuperAdmin || record.meta.requiredPermission,
  );
  void router.replace({
    name: staffRoute ? 'admin-login' : 'login',
    query: { redirect: current.fullPath },
  });
});

router.beforeEach(async (to) => {
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
});

app.use(router).mount('#app');
