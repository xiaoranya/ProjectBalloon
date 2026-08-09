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
const { t } = useI18n();
const route = useRoute();
const contestId = Number(route.query.contestId) || null;
const screenName = String(route.query.name || 'ProjectBalloon Screen');
const registration = ref<ScreenRegistration | null>(null);
const currentView = ref<ScreenViewTarget>('SCOREBOARD');
const online = ref(false);
const errorMessage = ref('');
const heartbeatIntervalMs = 10_000;
const storageKey = contestId ? `project-balloon:screen:${contestId}` : '';
let heartbeatTimer: number | undefined;
let boundaryTimer: number | undefined;
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
    if (error instanceof ApiError && error.code === 'SCREEN_TOKEN_INVALID') {
      localStorage.removeItem(storageKey);
      registration.value = null;
    }
    online.value = false;
    errorMessage.value = getErrorMessage(error);
  } finally {
    heartbeatInFlight = false;
    scheduleHeartbeat();
  }
}

onMounted(async () => {
  if (!contestId) {
    errorMessage.value = t('请使用 ?contestId= 指定比赛');
    return;
  }
  try {
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
          registration.value = parsed as ScreenRegistration;
        } else {
          localStorage.removeItem(storageKey);
        }
      } catch {
        localStorage.removeItem(storageKey);
      }
    }
    if (!registration.value) {
      const registered = await screenApi.register(contestId, screenName);
      if (stopped) return;
      registration.value = registered;
      localStorage.setItem(storageKey, JSON.stringify(registration.value));
    }
    if (stopped) return;
    currentView.value = registration.value.currentView;
    await heartbeat();
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
});

onBeforeUnmount(() => {
  stopped = true;
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
</style>
