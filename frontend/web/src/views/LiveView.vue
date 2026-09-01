<template>
  <main class="live-page" :class="templateClass" :style="liveStyle">
    <LiveHeaderBar
      :title="presentation?.config.title || presentation?.contestName || 'LIVE'"
      :subtitle="presentation?.config.subtitle"
      :status="presentation?.contestStatus"
      :show-brand="showBrand"
      :template="presentation?.config.template || 'DEFAULT'"
    />
    <ElAlert v-if="errorMessage" type="error" :closable="false" show-icon :title="errorMessage" />
    <LiveScoreboardPanel
      v-if="presentation && view === 'scoreboard'"
      :scoreboard="presentation.scoreboard"
      :row-limit="presentation.config.rowLimit"
      :announcement="announcement"
      :template="presentation.config.template"
    />
    <LiveHeroPanel
      v-else-if="presentation && view === 'first-blood'"
      variant="first-blood"
      :title="firstBlood?.teamName || t('等待首杀')"
      :detail="firstBloodProblem"
      :pulse-key="firstBloodKey"
      :template="presentation.config.template"
    />
    <LiveHeroPanel
      v-else-if="presentation && view === 'freeze'"
      variant="freeze"
      :title="countdown"
      :template="presentation.config.template"
    />
    <LiveMetricsPanel
      v-else-if="metrics && view === 'balloons'"
      :metrics="metrics"
      variant="balloons"
      :template="presentation?.config.template"
    />
    <LiveMetricsPanel
      v-else-if="metrics"
      :metrics="metrics"
      variant="statistics"
      :template="presentation?.config.template"
    />
  </main>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { useRoute } from 'vue-router';
import { getErrorMessage } from '../api/client';
import {
  presentationApi,
  type PresentationMetrics,
  type PublishedPresentation,
} from '../api/presentation';
import LiveHeaderBar from '../components/live/LiveHeaderBar.vue';
import LiveHeroPanel from '../components/live/LiveHeroPanel.vue';
import LiveMetricsPanel from '../components/live/LiveMetricsPanel.vue';
import LiveScoreboardPanel from '../components/live/LiveScoreboardPanel.vue';
import { useI18n } from '../i18n';
import { numericQueryId } from '../utils/route-params';
const { t } = useI18n();

const props = withDefaults(
  defineProps<{ view?: 'scoreboard' | 'first-blood' | 'balloons' | 'freeze' | 'statistics' }>(),
  { view: 'scoreboard' },
);
const route = useRoute();
const contestId = numericQueryId(route.query.contestId);
const token = new URLSearchParams(location.hash.slice(1)).get('token') || '';
const presentation = ref<PublishedPresentation | null>(null);
const metrics = ref<PresentationMetrics | null>(null);
const errorMessage = ref('');
const announcementIndex = ref(0);
const now = ref(Date.now());
let timer: number | undefined;
const announcement = computed(
  () =>
    presentation.value?.announcements[
      announcementIndex.value % Math.max(1, presentation.value.announcements.length)
    ],
);
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
const countdown = computed(() => {
  const target = presentation.value?.freezeAt;
  if (!target) return '--:--:--';
  const seconds = Math.max(0, Math.floor((new Date(target).getTime() - now.value) / 1000));
  return [Math.floor(seconds / 3600), Math.floor((seconds % 3600) / 60), seconds % 60]
    .map((v) => String(v).padStart(2, '0'))
    .join(':');
});
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
  '--custom-font': presentation.value?.config.customFontFamily || 'Inter',
  '--custom-density':
    presentation.value?.config.customDensity === 'COMPACT'
      ? '8px'
      : presentation.value?.config.customDensity === 'SPACIOUS'
        ? '22px'
        : '14px',
}));
const showBrand = computed(
  () =>
    presentation.value?.config.template !== 'CUSTOM' || presentation.value?.config.customShowLogo,
);
// Stale-response guard: only the newest load() may commit state, mirroring
// ScoreboardView, so a slow response cannot overwrite a newer one on a big screen.
let loadGeneration = 0;
async function load() {
  if (!contestId) return;
  const generation = ++loadGeneration;
  try {
    const nextPresentation = await presentationApi.published(contestId, 'LIVE', token);
    if (generation !== loadGeneration) return;
    presentation.value = nextPresentation;
    if (props.view === 'balloons' || props.view === 'statistics') {
      const nextMetrics = await presentationApi.metrics(contestId, 'LIVE', token);
      if (generation !== loadGeneration) return;
      metrics.value = nextMetrics;
    }
    if (generation !== loadGeneration) return;
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
  // Do not await the first load: a hung or slow first response must not
  // prevent the 10s poller from ever starting.
  void load();
  timer = window.setInterval(() => {
    now.value = Date.now();
    if (presentation.value?.announcements.length) announcementIndex.value += 1;
    void load();
  }, 10_000);
});
onBeforeUnmount(() => {
  if (timer) window.clearInterval(timer);
});
</script>

<style scoped>
.live-page {
  --accent: #22c55e;
  min-height: 100vh;
  background: #07111f;
  color: #fff;
  padding: 32px;
  display: grid;
  grid-template-rows: auto 1fr;
  gap: 24px;
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
}
</style>
