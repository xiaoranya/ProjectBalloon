<template>
  <main
    class="award-display"
    :class="`award-display-${presentation?.status.toLowerCase() || 'offline'}`"
  >
    <div class="award-display-decor" aria-hidden="true"></div>
    <div v-if="disconnected && presentation" class="award-display-connection">
      {{ t('连接中断，继续展示最后一次同步结果') }}
    </div>
    <header v-if="presentation" class="award-display-header">
      <div>
        <span>ProjectBalloon · AWARD CEREMONY</span><strong>{{ presentation.contestName }}</strong>
      </div>
      <button type="button" @click="toggleFullscreen">{{ t('全屏') }}</button>
    </header>
    <AwardStage :presentation="presentation" :now="now" :error-message="errorMessage" />
  </main>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { useRoute } from 'vue-router';
import { awardsApi, type AwardPresentation } from '../api/awards';
import { getErrorMessage } from '../api/client';
import AwardStage from '../components/awards/AwardStage.vue';
import {
  subscribeContestEvents,
  type ContestRealtimeSubscription,
} from '../realtime/contest-events';
import { useI18n } from '../i18n';
import { numericQueryId } from '../utils/route-params';
const { t } = useI18n();
const route = useRoute();
const contestId = numericQueryId(route.query.contestId);
const presentation = ref<AwardPresentation | null>(null);
const errorMessage = ref('');
const disconnected = ref(false);
const now = ref(Date.now());
// The rotation clock is anchored to the last server sync instead of ticking a
// local counter: `+= 1000` drifts whenever SSE stalls and timers throttle, so
// each tick recomputes elapsed time from performance.now() against the anchor.
let serverTimeAnchor = Date.now();
let performanceAnchor = 0;
let realtime: ContestRealtimeSubscription | undefined;
let clockTimer: number | undefined;
async function load() {
  if (!contestId) return;
  try {
    presentation.value = await awardsApi.presentation(contestId);
    resyncClock(new Date(presentation.value.serverTime).getTime());
    disconnected.value = false;
    errorMessage.value = '';
  } catch (error) {
    if (presentation.value) disconnected.value = true;
    else errorMessage.value = getErrorMessage(error);
  }
}

/** Re-anchors the monotonic rotation clock to the latest server time. */
function resyncClock(serverTime: number) {
  serverTimeAnchor = serverTime;
  performanceAnchor = performance.now();
  now.value = serverTime;
}
async function toggleFullscreen() {
  if (document.fullscreenElement) await document.exitFullscreen();
  else await document.documentElement.requestFullscreen();
}
onMounted(async () => {
  await load();
  clockTimer = window.setInterval(() => {
    now.value = serverTimeAnchor + (performance.now() - performanceAnchor);
  }, 1000);
  if (contestId)
    realtime = subscribeContestEvents({
      contestId,
      scope: 'PUBLIC',
      eventTypes: ['AWARDS_UPDATED'],
      onEvent: () => void load(),
      onConnectionChange: (value) => {
        disconnected.value = !value;
      },
      poll: load,
      pollIntervalMs: 5000,
    });
});
onBeforeUnmount(() => {
  realtime?.stop();
  if (clockTimer) window.clearInterval(clockTimer);
});
</script>
