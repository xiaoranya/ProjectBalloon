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

const RELOAD_HINT_ID = 'app-reload-hint';
const CHUNK_LOAD_FAILURE =
  /Failed to fetch dynamically imported module|Importing a module script failed|ChunkLoadError|Loading chunk \d+ failed/i;

// The outlet stays empty when a lazy chunk 404s after a redeploy, and a render
// error blanks the page the same way. Say so, and offer the only fix there is.
function showReloadHint(message: string) {
  if (document.getElementById(RELOAD_HINT_ID)) return;
  const banner = document.createElement('div');
  banner.id = RELOAD_HINT_ID;
  banner.setAttribute('role', 'alert');
  banner.style.cssText = [
    'position:fixed',
    'inset-block-start:0',
    'inset-inline:0',
    'z-index:9999',
    'display:flex',
    'gap:12px',
    'align-items:center',
    'justify-content:center',
    'padding:12px 16px',
    'background:#f56c6c',
    'color:#fff',
    'font-size:14px',
  ].join(';');
  const text = document.createElement('span');
  text.textContent = message;
  const button = document.createElement('button');
  button.type = 'button';
  button.textContent = '重新加载';
  button.style.cssText =
    'padding:4px 12px;border:1px solid #fff;border-radius:4px;background:transparent;color:#fff;cursor:pointer';
  button.addEventListener('click', () => window.location.reload());
  banner.append(text, button);
  document.body.append(banner);
}

router.onError((error) => {
  console.error('router error', error);
  const message = error instanceof Error ? error.message : String(error);
  if (CHUNK_LOAD_FAILURE.test(message)) showReloadHint('页面资源已更新，请重新加载');
});

// A render error used to leave a blank screen with nothing in the console that
// pointed at the failing component; keep the report and a visible hint instead.
app.config.errorHandler = (error, _instance, info) => {
  console.error('unhandled component error', info, error);
  showReloadHint('页面出现异常，可尝试重新加载');
};

app.use(router).mount('#app');
