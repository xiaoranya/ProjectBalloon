<template>
  <main class="live-program" :class="templateClass" :style="liveStyle">
    <LiveHeaderBar
      :title="presentation?.config.title || presentation?.contestName || 'LIVE'"
      :subtitle="presentation?.config.subtitle"
      :status="onAirLabel"
      :show-brand="showBrand"
      :template="presentation?.config.template || 'DEFAULT'"
    />
    <ElAlert v-if="errorMessage" type="error" :closable="false" show-icon :title="errorMessage" />
    <div v-if="program?.showClock" class="live-program-clock">
      <LiveClock :server-time="serverTime" />
    </div>
    <div
      class="live-program-stage"
      :style="{ '--scene-duration': `${program?.transitionMilliseconds ?? 800}ms` }"
    >
      <Transition name="program-scene" mode="out-in">
        <div :key="program?.currentScene || 'SCOREBOARD'" class="live-program-scene">
          <LiveScoreboardPanel
            v-if="scene === 'SCOREBOARD' && presentation"
            :scoreboard="presentation.scoreboard"
            :row-limit="presentation.config.rowLimit"
            :announcement="announcement"
            :template="presentation.config.template"
          />
          <LiveHeroPanel
            v-else-if="scene === 'FIRST_BLOOD' && presentation"
            variant="first-blood"
            :title="firstBlood?.teamName || t('等待首杀')"
            :detail="firstBloodProblem"
            :pulse-key="firstBloodKey"
            :template="presentation.config.template"
          />
          <LiveMetricsPanel
            v-else-if="scene === 'BALLOONS' && metrics"
            :metrics="metrics"
            variant="balloons"
            :template="presentation?.config.template"
          />
          <LiveHeroPanel
            v-else-if="scene === 'FREEZE_COUNTDOWN' && presentation"
            variant="freeze"
            :title="countdown"
            :template="presentation.config.template"
          />
          <LiveMetricsPanel
            v-else-if="scene === 'STATISTICS' && metrics"
            :metrics="metrics"
            variant="statistics"
            :template="presentation?.config.template"
          />
          <div v-else-if="scene === 'RESOLVER'" class="live-program-embedded">
            <ResolverStage :run="resolverRun" :error-message="resolverError" />
          </div>
          <div v-else-if="scene === 'AWARDS'" class="live-program-embedded">
            <AwardStage
              :presentation="awardPresentation"
              :now="awardNow"
              :error-message="awardsError"
            />
          </div>
          <div v-else-if="scene === 'TITLE_CARD'" class="live-program-title-card">
            <p>PROJECTBALLOON</p>
            <h1>{{ titleCardTitle }}</h1>
            <p v-if="titleCardSubtitle" class="live-program-title-sub">{{ titleCardSubtitle }}</p>
          </div>
          <div v-else class="live-program-placeholder">
            <p>{{ t('等待导播指令') }}</p>
            <h1>{{ t('缺少比赛数据') }}</h1>
          </div>
        </div>
      </Transition>
    </div>
    <LiveFirstBloodPopup
      v-if="firstBlood"
      :team-name="firstBlood.teamName"
      :problem-alias="firstBloodProblem"
      :play-key="firstBloodKey"
      transient
      :hide-after-ms="12_000"
      :template="presentation?.config.template"
      class="live-program-popup"
    />
    <LiveTickerBar
      v-if="program?.tickerEnabled"
      :announcements="presentation?.announcements ?? []"
      :interval-seconds="presentation?.config.announcementIntervalSeconds"
      :template="presentation?.config.template"
      class="live-program-ticker"
    />
  </main>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { useRoute } from 'vue-router';
import { awardsApi, type AwardPresentation } from '../api/awards';
import { getErrorMessage } from '../api/client';
import {
  presentationApi,
  type LiveScene,
  type PresentationMetrics,
  type PublishedLiveProgram,
  type PublishedPresentation,
} from '../api/presentation';
import { resolverApi, type ResolverPublicRun } from '../api/resolver';
import AwardStage from '../components/awards/AwardStage.vue';
import LiveClock from '../components/live/LiveClock.vue';
import LiveFirstBloodPopup from '../components/live/LiveFirstBloodPopup.vue';
import LiveHeaderBar from '../components/live/LiveHeaderBar.vue';
import LiveHeroPanel from '../components/live/LiveHeroPanel.vue';
import LiveMetricsPanel from '../components/live/LiveMetricsPanel.vue';
import LiveScoreboardPanel from '../components/live/LiveScoreboardPanel.vue';
import LiveTickerBar from '../components/live/LiveTickerBar.vue';
import ResolverStage from '../components/resolver/ResolverStage.vue';
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
const presentation = ref<PublishedPresentation | null>(null);
const metrics = ref<PresentationMetrics | null>(null);
const program = ref<PublishedLiveProgram | null>(null);
const awardPresentation = ref<AwardPresentation | null>(null);
const awardsError = ref('');
const awardNow = ref(0);
const resolverRun = ref<ResolverPublicRun | null>(null);
const resolverError = ref('');
const errorMessage = ref('');
const now = ref(Date.now());
let realtime: ContestRealtimeSubscription | undefined;
let loadGeneration = 0;
let clockTimer: number | undefined;
let awardsClockTimer: number | undefined;
// Award rotation clock is anchored like AwardDisplayView: elapsed time is
// recomputed from performance.now() instead of += 1000, so SSE outages cannot
// make the rotation drift.
let awardsServerAnchor = 0;
let awardsPerformanceAnchor = 0;

const scene = computed<LiveScene>(() => program.value?.currentScene ?? 'SCOREBOARD');
const onAirLabel = computed(() => `${t('播出中')} · ${SCENE_LABELS[scene.value]}`);
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
const announcement = computed(() => presentation.value?.announcements[0] ?? null);
const countdown = computed(() => {
  const target = presentation.value?.freezeAt;
  if (!target) return '--:--:--';
  const seconds = Math.max(0, Math.floor((new Date(target).getTime() - now.value) / 1000));
  return [Math.floor(seconds / 3600), Math.floor((seconds % 3600) / 60), seconds % 60]
    .map((v) => String(v).padStart(2, '0'))
    .join(':');
});
const serverTime = computed(
  () => program.value?.serverTime ?? presentation.value?.serverTime ?? null,
);
const templateClass = computed(
  () => `template-${(presentation.value?.config.template || 'DEFAULT').toLowerCase()}`,
);
const liveStyle = computed(() => ({
  '--accent':
    presentation.value?.config.customAccentColor ||
    presentation.value?.config.accentColor ||
    '#22c55e',
  '--custom-background': presentation.value?.config.customBackgroundColor || '#07111f',
  '--custom-foreground': presentation.value?.config.customForegroundColor || '#fff',
  '--custom-accent': presentation.value?.config.customAccentColor || undefined,
  '--custom-font': presentation.value?.config.customFontFamily || 'Inter',
}));
const showBrand = computed(
  () =>
    presentation.value?.config.template !== 'CUSTOM' || presentation.value?.config.customShowLogo,
);
const titleCardTitle = computed(
  () =>
    program.value?.titleCardText ||
    presentation.value?.config.title ||
    presentation.value?.contestName ||
    'LIVE',
);
const titleCardSubtitle = computed(() => presentation.value?.config.subtitle ?? '');

const SCENE_LABELS: Record<LiveScene, string> = {
  SCOREBOARD: t('实时榜单'),
  FIRST_BLOOD: 'FIRST BLOOD',
  BALLOONS: t('气球状态'),
  FREEZE_COUNTDOWN: t('封榜倒计时'),
  STATISTICS: t('比赛统计'),
  RESOLVER: t('滚榜仪式'),
  AWARDS: t('颁奖典礼'),
  TITLE_CARD: t('标题卡'),
};

async function loadPublished() {
  if (!contestId) return;
  const generation = ++loadGeneration;
  try {
    const next = await presentationApi.published(contestId, 'LIVE', token);
    if (generation !== loadGeneration) return;
    presentation.value = next;
    errorMessage.value = '';
  } catch (error) {
    if (generation === loadGeneration) errorMessage.value = getErrorMessage(error);
  }
}

async function loadMetrics() {
  if (!contestId) return;
  const generation = loadGeneration;
  try {
    const next = await presentationApi.metrics(contestId, 'LIVE', token);
    if (generation !== loadGeneration) return;
    metrics.value = next;
  } catch {
    /* metrics failures keep the last values on air */
  }
}

async function loadProgram() {
  if (!contestId) return;
  try {
    program.value = await presentationApi.publishedProgram(contestId, token);
  } catch {
    /* keep the last known program while the token/visibility is being fixed */
  }
}

async function loadSceneData() {
  if (scene.value === 'BALLOONS' || scene.value === 'STATISTICS') await loadMetrics();
  if (scene.value === 'RESOLVER') await loadResolver();
  if (scene.value === 'AWARDS') await loadAwards();
}

async function loadResolver() {
  const runId = program.value?.resolverRunId;
  if (!runId) {
    resolverRun.value = null;
    return;
  }
  try {
    resolverRun.value = await resolverApi.publicState(runId);
    resolverError.value = '';
  } catch (error) {
    resolverRun.value = null;
    resolverError.value = getErrorMessage(error);
  }
}

async function loadAwards() {
  if (!contestId) return;
  try {
    const next = await awardsApi.presentation(contestId);
    awardPresentation.value = next;
    awardsServerAnchor = new Date(next.serverTime).getTime();
    awardsPerformanceAnchor = performance.now();
    awardNow.value = awardsServerAnchor;
    awardsError.value = '';
  } catch (error) {
    awardPresentation.value = null;
    awardsError.value = getErrorMessage(error);
  }
}

async function refreshAll(reloadProgram: boolean) {
  await loadPublished();
  if (reloadProgram) await loadProgram();
  await loadSceneData();
}

onMounted(() => {
  if (!contestId || !token) {
    errorMessage.value = t('缺少 contestId 或广播 Token');
    return;
  }
  void refreshAll(true);
  clockTimer = window.setInterval(() => {
    now.value = Date.now();
  }, 1000);
  awardsClockTimer = window.setInterval(() => {
    if (awardPresentation.value) {
      awardNow.value = awardsServerAnchor + (performance.now() - awardsPerformanceAnchor);
    }
  }, 1000);
  realtime = subscribeContestEvents({
    contestId,
    scope: 'PUBLIC',
    eventTypes: [
      'LIVE_PROGRAM_UPDATED',
      'PRESENTATION_UPDATED',
      'ANNOUNCEMENT_UPDATED',
      'AWARDS_UPDATED',
      'RESOLVER_STATE_CHANGED',
    ],
    onEvent: (event) => {
      void refreshAll(event.type === 'LIVE_PROGRAM_UPDATED');
    },
    poll: async () => {
      await refreshAll(true);
    },
    pollIntervalMs: 10_000,
  });
});
onBeforeUnmount(() => {
  realtime?.stop();
  if (clockTimer) window.clearInterval(clockTimer);
  if (awardsClockTimer) window.clearInterval(awardsClockTimer);
});
</script>

<style scoped>
.live-program {
  --accent: #22c55e;
  min-height: 100vh;
  background: #07111f;
  color: #fff;
  padding: 32px;
  display: grid;
  grid-template-rows: auto 1fr auto;
  gap: 18px;
  position: relative;
}
.live-program-clock {
  position: absolute;
  top: 34px;
  right: 36px;
  z-index: 5;
}
.live-program-stage {
  min-height: 0;
  display: grid;
  align-items: center;
}
.live-program-scene {
  min-height: 0;
}
.live-program-embedded {
  min-height: 52vh;
  display: grid;
}
.live-program-title-card {
  display: grid;
  place-content: center;
  text-align: center;
  gap: 10px;
  animation: title-card-in 1.6s ease-out;
}
.live-program-title-card p:first-of-type {
  letter-spacing: 0.4em;
  color: var(--accent);
  font-size: clamp(14px, 1.6vw, 22px);
}
.live-program-title-card h1 {
  font-size: clamp(52px, 9vw, 150px);
  margin: 0;
}
.live-program-title-sub {
  color: var(--muted-light, #94a3b8);
  font-size: clamp(18px, 2.4vw, 34px);
}
.live-program-placeholder {
  display: grid;
  place-content: center;
  text-align: center;
  color: var(--muted-light, #94a3b8);
}
.live-program-popup {
  position: absolute;
  left: 50%;
  bottom: 108px;
  transform: translateX(-50%);
  z-index: 6;
}
.live-program-ticker {
  padding-bottom: 4px;
}
.program-scene-enter-active,
.program-scene-leave-active {
  transition:
    opacity var(--scene-duration, 800ms) ease,
    transform var(--scene-duration, 800ms) ease;
}
.program-scene-enter-from {
  opacity: 0;
  transform: translateY(24px) scale(0.985);
}
.program-scene-leave-to {
  opacity: 0;
  transform: translateY(-24px) scale(0.985);
}
@keyframes title-card-in {
  0% {
    opacity: 0;
    transform: scale(0.92);
    letter-spacing: 0.2em;
  }
  100% {
    opacity: 1;
    transform: scale(1);
  }
}
.template-cinematic {
  background: #080b14;
  background:
    radial-gradient(
      circle at 75% 10%,
      color-mix(in srgb, var(--accent) 25%, transparent),
      transparent 34%
    ),
    #080b14;
}
.template-minimal {
  background: #fff;
  color: #101828;
}
.template-split {
  background: linear-gradient(90deg, #07111f 0 58%, #101d2d 58%);
}
.template-custom {
  background: var(--custom-background);
  color: var(--custom-foreground);
  font-family: var(--custom-font), sans-serif;
}
@media (max-width: 800px) {
  .template-split {
    background: #07111f;
  }
  .live-program-popup {
    bottom: 140px;
  }
}
@media (prefers-reduced-motion: reduce) {
  .program-scene-enter-active,
  .program-scene-leave-active,
  .live-program-title-card {
    animation: none;
    transition: opacity 0.2s ease;
  }
  .program-scene-enter-from,
  .program-scene-leave-to {
    opacity: 0;
    transform: none;
  }
}
</style>
