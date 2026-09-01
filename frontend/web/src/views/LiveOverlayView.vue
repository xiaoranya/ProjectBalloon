<template>
  <main class="live-overlay" :class="templateClass" :style="liveStyle">
    <ElAlert v-if="errorMessage" type="error" :closable="false" show-icon :title="errorMessage" />
    <LiveFirstBloodPopup
      v-if="parts.popup && firstBlood"
      :team-name="firstBlood.teamName"
      :problem-alias="firstBloodProblem"
      :play-key="firstBloodKey"
      transient
      :hide-after-ms="12_000"
      :template="presentation?.config.template"
    />
    <LiveClock
      v-if="parts.clock"
      :server-time="presentation?.serverTime ?? null"
      class="live-overlay-clock"
    />
    <LiveTickerBar
      v-if="parts.ticker"
      :announcements="presentation?.announcements ?? []"
      :interval-seconds="presentation?.config.announcementIntervalSeconds"
      :template="presentation?.config.template"
    />
  </main>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { useRoute } from 'vue-router';
import { getErrorMessage } from '../api/client';
import { presentationApi, type PublishedPresentation } from '../api/presentation';
import LiveClock from '../components/live/LiveClock.vue';
import LiveFirstBloodPopup from '../components/live/LiveFirstBloodPopup.vue';
import LiveTickerBar from '../components/live/LiveTickerBar.vue';
import {
  subscribeContestEvents,
  type ContestRealtimeSubscription,
} from '../realtime/contest-events';
import { useI18n } from '../i18n';
import { numericQueryId } from '../utils/route-params';
const { t } = useI18n();

const route = useRoute();
const contestId = numericQueryId(route.query.contestId);
const token = new URLSearchParams(location.hash.slice(1)).get('token') || '';
const partsParam = String(route.query.parts || 'ticker,popup,clock');
const parts = reactive({
  ticker: partsParam.includes('ticker'),
  popup: partsParam.includes('popup'),
  clock: partsParam.includes('clock'),
});
const presentation = ref<PublishedPresentation | null>(null);
const errorMessage = ref('');
let realtime: ContestRealtimeSubscription | undefined;
let loadGeneration = 0;
let timer: number | undefined;

const firstBlood = computed(() =>
  presentation.value?.scoreboard.rows.find((row) => row.problems.some((cell) => cell.firstBlood)),
);
const firstBloodProblem = computed(() => {
  const row = firstBlood.value;
  const board = presentation.value?.scoreboard;
  if (!row || !board) return '';
  const id = row.problems.find((cell) => cell.firstBlood)?.problemId;
  return board.problems.find((item) => item.problemId === id)?.alias ?? '';
});
const firstBloodKey = computed(() =>
  firstBlood.value ? `${firstBlood.value.teamId}:${firstBloodProblem.value}` : null,
);
const templateClass = computed(
  () => `overlay-${(presentation.value?.config.template || 'DEFAULT').toLowerCase()}`,
);
const liveStyle = computed(() => ({
  '--accent':
    presentation.value?.config.customAccentColor ||
    presentation.value?.config.accentColor ||
    '#22c55e',
  '--custom-background': presentation.value?.config.customBackgroundColor || '#07111f',
  '--custom-foreground': presentation.value?.config.customForegroundColor || '#fff',
  '--custom-font': presentation.value?.config.customFontFamily || 'Inter',
}));

// Stale-response guard: only the newest load may commit state.
async function load() {
  if (!contestId) return;
  const generation = ++loadGeneration;
  try {
    const next = await presentationApi.published(contestId, 'LIVE', token);
    if (generation !== loadGeneration) return;
    presentation.value = next;
    errorMessage.value = '';
  } catch (error) {
    if (generation !== loadGeneration) return;
    errorMessage.value = getErrorMessage(error);
  }
}

onMounted(() => {
  if (!contestId || !token) {
    errorMessage.value = t('缺少 contestId 或广播 Token');
    return;
  }
  void load();
  timer = window.setInterval(() => void load(), 10_000);
  realtime = subscribeContestEvents({
    contestId,
    scope: 'PUBLIC',
    eventTypes: ['ANNOUNCEMENT_UPDATED', 'PRESENTATION_UPDATED'],
    onEvent: () => void load(),
    poll: load,
    pollIntervalMs: 10_000,
  });
  // OBS browser sources need a truly transparent page; the class is removed
  // again on unmount so in-app navigation never inherits it.
  document.documentElement.classList.add('live-overlay-page');
});
onBeforeUnmount(() => {
  realtime?.stop();
  if (timer) window.clearInterval(timer);
  document.documentElement.classList.remove('live-overlay-page');
});
</script>

<style>
html.live-overlay-page,
html.live-overlay-page body {
  background: transparent !important;
}
</style>

<style scoped>
.live-overlay {
  min-height: 100vh;
  padding: 24px 32px;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  gap: 14px;
  position: relative;
  background: transparent;
  color: var(--custom-foreground, #fff);
}
.live-overlay-clock {
  position: absolute;
  top: 24px;
  right: 32px;
  z-index: 3;
  /* On a transparent page the accent clock needs its own readable plate. */
  background: color-mix(in srgb, var(--accent, #22c55e) 12%, rgb(7 17 31 / 72%));
  padding: 8px 14px;
  border-radius: 10px;
}
.live-overlay :deep(.fb-popup) {
  position: static;
}
</style>
