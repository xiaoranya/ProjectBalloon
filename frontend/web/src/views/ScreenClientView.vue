<template>
  <main class="screen-client">
    <header>
      <div>
        <small>ProjectBalloon · SCREEN #{{ registration?.instanceId ?? '—' }}</small
        ><strong>{{ registration?.name || screenName }}</strong>
      </div>
      <span :class="{ online }">{{ online ? 'CONTROL CONNECTED' : 'CONNECTING' }}</span>
    </header>
    <section>
      <p>{{ viewLabel(currentView) }}</p>
      <h1>{{ currentView }}</h1>
      <span v-if="errorMessage">{{ errorMessage }}</span
      ><span v-else>{{ t('等待控制台下发画面切换命令') }}</span>
      <span v-if="retryDelaySeconds > 0" class="retrying">{{
        t('连接中断，{seconds} 秒后自动重试', { seconds: retryDelaySeconds })
      }}</span>
    </section>
  </main>
</template>
<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { useRoute } from 'vue-router';
import { ApiError, getErrorMessage } from '../api/client';
import { screenApi, type ScreenRegistration, type ScreenViewTarget } from '../api/screen';
import { resolveScreenPlayback } from '../composables/screenPlayback';
import { useI18n } from '../i18n';
import { numericQueryId } from '../utils/route-params';
const { t } = useI18n();
const route = useRoute();
const contestId = numericQueryId(route.query.contestId);
const screenName = String(route.query.name || 'ProjectBalloon Screen');
const registration = ref<ScreenRegistration | null>(null);
const currentView = ref<ScreenViewTarget>('SCOREBOARD');
const online = ref(false);
const errorMessage = ref('');
const retryDelaySeconds = ref(0);
const heartbeatIntervalMs = 10_000;
const retryInitialMs = 2_000;
const retryMaxMs = 60_000;
const storageKey = contestId ? `project-balloon:screen:${contestId}` : '';
let heartbeatTimer: number | undefined;
let boundaryTimer: number | undefined;
let retryTimer: number | undefined;
let retryAttempt = 0;
let heartbeatInFlight = false;
let stopped = false;

function viewLabel(value: ScreenViewTarget) {
  return t(
    {
      SCOREBOARD: '实时榜单',
      FIRST_BLOOD: 'First Blood',
      BALLOONS: '气球状态',
      FREEZE_COUNTDOWN: '封榜倒计时',
      STATISTICS: '比赛统计',
      RESOLVER: '滚榜仪式',
      AWARDS: '颁奖典礼',
    }[value],
  );
}

function scheduleHeartbeat() {
  if (stopped || !registration.value || heartbeatTimer !== undefined) return;
  heartbeatTimer = window.setTimeout(() => {
    heartbeatTimer = undefined;
    void heartbeat();
  }, heartbeatIntervalMs);
}

function clearRetryTimer() {
  if (retryTimer !== undefined) {
    window.clearTimeout(retryTimer);
    retryTimer = undefined;
  }
  retryDelaySeconds.value = 0;
}

/**
 * Retries unattended boot/re-registration with capped exponential backoff
 * (2s, 4s, 8s, ... capped at 60s) so a screen powered on before the network
 * is up, or whose token the server invalidated, recovers without a human.
 */
function scheduleRetry(retry: () => void) {
  if (stopped || retryTimer !== undefined) return;
  const delayMs = Math.min(retryInitialMs * 2 ** retryAttempt, retryMaxMs);
  retryAttempt += 1;
  retryDelaySeconds.value = Math.round(delayMs / 1000);
  retryTimer = window.setTimeout(() => {
    retryTimer = undefined;
    retryDelaySeconds.value = 0;
    if (!stopped) void retry();
  }, delayMs);
}

async function heartbeat() {
  if (stopped || heartbeatInFlight) return;
  const value = registration.value;
  if (!value) return;
  heartbeatInFlight = true;
  const started = Date.now();
  try {
    const response = await screenApi.heartbeat(
      value.instanceId,
      value.clientToken,
      currentView.value,
    );
    if (boundaryTimer !== undefined) window.clearTimeout(boundaryTimer);
    const serverNow = new Date(response.serverTime).getTime() + (Date.now() - started) / 2;
    const playback = resolveScreenPlayback(response.groupPlayback, serverNow);
    currentView.value = playback.targetView ?? response.targetView ?? currentView.value;
    if (playback.nextTargetView && playback.msUntilNext !== null) {
      boundaryTimer = window.setTimeout(() => {
        boundaryTimer = undefined;
        if (heartbeatTimer !== undefined) {
          window.clearTimeout(heartbeatTimer);
          heartbeatTimer = undefined;
        }
        currentView.value = playback.nextTargetView!;
        void heartbeat();
      }, playback.msUntilNext);
    } else {
      boundaryTimer = undefined;
    }
    online.value = true;
    errorMessage.value = '';
  } catch (error) {
    online.value = false;
    errorMessage.value = getErrorMessage(error);
    if (error instanceof ApiError && error.code === 'SCREEN_TOKEN_INVALID') {
      localStorage.removeItem(storageKey);
      registration.value = null;
      scheduleRetry(start);
    }
  } finally {
    heartbeatInFlight = false;
    scheduleHeartbeat();
  }
}

async function ensureRegistration(): Promise<ScreenRegistration> {
  if (!contestId) {
    // Unreachable: start() is only called after the contestId check in onMounted.
    throw new Error('contestId is required');
  }
  const stored = localStorage.getItem(storageKey);
  if (stored) {
    try {
      const parsed = JSON.parse(stored) as Partial<ScreenRegistration>;
      if (
        parsed.contestId === contestId &&
        typeof parsed.instanceId === 'number' &&
        typeof parsed.clientToken === 'string' &&
        parsed.clientToken
      ) {
        return parsed as ScreenRegistration;
      }
      localStorage.removeItem(storageKey);
    } catch {
      localStorage.removeItem(storageKey);
    }
  }
  const registered = await screenApi.register(contestId, screenName);
  if (stopped) return registered;
  registration.value = registered;
  localStorage.setItem(storageKey, JSON.stringify(registered));
  return registered;
}

async function start() {
  if (stopped) return;
  try {
    const value = await ensureRegistration();
    if (stopped) return;
    registration.value = value;
    retryAttempt = 0;
    currentView.value = value.currentView;
    await heartbeat();
  } catch (error) {
    if (stopped) return;
    online.value = false;
    errorMessage.value = getErrorMessage(error);
    scheduleRetry(start);
  }
}

onMounted(async () => {
  if (!contestId) {
    errorMessage.value = t('请使用 ?contestId= 指定比赛');
    return;
  }
  await start();
});

onBeforeUnmount(() => {
  stopped = true;
  clearRetryTimer();
  if (heartbeatTimer !== undefined) window.clearTimeout(heartbeatTimer);
  if (boundaryTimer !== undefined) window.clearTimeout(boundaryTimer);
});
</script>
<style scoped>
.screen-client {
  min-height: 100vh;
  background: #07111f;
  color: #fff;
  padding: 32px;
  display: grid;
  grid-template-rows: auto 1fr;
}
.screen-client header {
  display: flex;
  justify-content: space-between;
}
.screen-client header div {
  display: grid;
}
.screen-client header span {
  color: #f59e0b;
}
.screen-client header span.online {
  color: #22c55e;
}
.screen-client section {
  display: grid;
  place-content: center;
  text-align: center;
}
.screen-client section p {
  color: #22c55e;
  letter-spacing: 0.3em;
}
.screen-client section h1 {
  font-size: clamp(56px, 10vw, 150px);
  margin: 12px;
}
.screen-client section .retrying {
  color: #f59e0b;
}
</style>
