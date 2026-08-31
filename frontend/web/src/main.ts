import { createApp } from 'vue';
import { createRouter, createWebHistory } from 'vue-router';
import * as ElementPlusIcons from '@element-plus/icons-vue';
import App from './App.vue';
import { routes } from './routes';
import { resolveRouteGuard } from './router/guards';
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

router.beforeEach((to) => resolveRouteGuard(to, session));

app.use(router).mount('#app');
