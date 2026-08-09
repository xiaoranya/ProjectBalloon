<template>
  <main class="live-page" :class="templateClass" :style="liveStyle">
    <header>
      <div>
        <small v-if="showBrand">ProjectBalloon · LIVE</small>
        <h1>{{ presentation?.config.title || presentation?.contestName || 'LIVE' }}</h1>
        <p>{{ presentation?.config.subtitle }}</p>
      </div>
      <span>{{ presentation?.contestStatus }}</span>
    </header>
    <ElAlert v-if="errorMessage" type="error" :closable="false" show-icon :title="errorMessage" />
    <section v-if="presentation && view === 'scoreboard'" class="board">
      <table>
        <thead>
          <tr>
            <th>#</th>
            <th>队伍</th>
            <th>{{ presentation.scoreboard.scoringMode === 'ICPC' ? '解题' : '总分' }}</th>
            <th v-if="presentation.scoreboard.scoringMode === 'ICPC'">罚时</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="row in presentation.scoreboard.rows.slice(0, presentation.config.rowLimit)"
            :key="row.teamId"
          >
            <td>{{ row.rank }}</td>
            <td>
              <strong>{{ row.teamName }}</strong
              ><small>{{ row.school }}</small>
            </td>
            <td>
              {{
                presentation.scoreboard.scoringMode === 'ICPC'
                  ? row.solvedCount
                  : (row.totalScoreMilli / 1000).toFixed(3).replace(/\.?0+$/, '')
              }}
            </td>
            <td v-if="presentation.scoreboard.scoringMode === 'ICPC'">{{ row.penaltyMinutes }}</td>
          </tr>
        </tbody>
      </table>
      <aside v-if="announcement">
        <strong>{{ announcement.title }}</strong>
        <p>{{ announcement.body }}</p>
      </aside>
    </section>
    <section v-else-if="presentation && view === 'first-blood'" class="hero">
      <p>FIRST BLOOD</p>
      <h2>{{ firstBlood?.teamName || '等待首杀' }}</h2>
      <strong>{{ firstBloodProblem }}</strong>
    </section>
    <section v-else-if="presentation && view === 'freeze'" class="hero">
      <p>FREEZE COUNTDOWN</p>
      <h2>{{ countdown }}</h2>
    </section>
    <section v-else-if="metrics && view === 'balloons'" class="metrics">
      <article>
        <span>气球总数</span><strong>{{ metrics.balloons.total }}</strong>
      </article>
      <article>
        <span>已送达</span><strong>{{ metrics.balloons.delivered }}</strong>
      </article>
      <article>
        <span>配送中</span
        ><strong>{{ metrics.balloons.preparing + metrics.balloons.delivering }}</strong>
      </article>
      <article>
        <span>首杀</span><strong>{{ metrics.balloons.firstBlood }}</strong>
      </article>
    </section>
    <section v-else-if="metrics" class="metrics">
      <article>
        <span>提交</span><strong>{{ metrics.submissions.total }}</strong>
      </article>
      <article>
        <span>通过</span><strong>{{ metrics.submissions.accepted }}</strong>
      </article>
      <article>
        <span>通过率</span><strong>{{ acceptance }}%</strong>
      </article>
      <article>
        <span>评测中</span><strong>{{ metrics.submissions.pending }}</strong>
      </article>
    </section>
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

const props = withDefaults(
  defineProps<{ view?: 'scoreboard' | 'first-blood' | 'balloons' | 'freeze' | 'statistics' }>(),
  { view: 'scoreboard' },
);
const route = useRoute();
const contestId = Number(route.query.contestId);
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
const countdown = computed(() => {
  const target = presentation.value?.freezeAt;
  if (!target) return '--:--:--';
  const seconds = Math.max(0, Math.floor((new Date(target).getTime() - now.value) / 1000));
  return [Math.floor(seconds / 3600), Math.floor((seconds % 3600) / 60), seconds % 60]
    .map((v) => String(v).padStart(2, '0'))
    .join(':');
});
const acceptance = computed(() =>
  metrics.value?.submissions.total
    ? Math.round((metrics.value.submissions.accepted / metrics.value.submissions.total) * 100)
    : 0,
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
async function load() {
  try {
    presentation.value = await presentationApi.published(contestId, 'LIVE', token);
    if (props.view === 'balloons' || props.view === 'statistics')
      metrics.value = await presentationApi.metrics(contestId, 'LIVE', token);
    errorMessage.value = '';
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}
onMounted(async () => {
  if (!contestId || !token) {
    errorMessage.value = '缺少 contestId 或广播 Token';
    return;
  }
  await load();
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
.live-page header {
  display: flex;
  justify-content: space-between;
  border-bottom: 2px solid var(--accent);
}
.live-page h1 {
  margin: 4px 0;
}
.live-page header > span,
.hero p {
  color: var(--accent);
  letter-spacing: 0.2em;
}
.board {
  display: grid;
  grid-template-columns: 1fr 320px;
  gap: 24px;
}
.board table {
  width: 100%;
  border-collapse: collapse;
  font-size: clamp(18px, 2vw, 30px);
}
th,
td {
  padding: var(--custom-density, 14px);
  border-bottom: 1px solid #203047;
  text-align: left;
}
.board small {
  display: block;
  color: #94a3b8;
}
.board aside,
.metrics article {
  background: #101d2d;
  border-left: 4px solid var(--accent);
  padding: 24px;
}
.hero {
  display: grid;
  place-content: center;
  text-align: center;
}
.hero h2 {
  font-size: clamp(64px, 12vw, 170px);
  margin: 10px;
}
.metrics {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 24px;
  align-content: center;
}
.metrics article {
  display: grid;
}
.metrics strong {
  font-size: clamp(54px, 9vw, 120px);
  color: var(--accent);
}
.template-cinematic {
  background:
    radial-gradient(
      circle at 75% 10%,
      color-mix(in srgb, var(--accent) 25%, transparent),
      transparent 34%
    ),
    #080b14;
}
.template-cinematic h1 {
  letter-spacing: 0.06em;
  text-transform: uppercase;
}
.template-minimal {
  background: #fff;
  color: #101828;
}
.template-minimal header {
  border-bottom-width: 1px;
}
.template-minimal .board aside,
.template-minimal .metrics article {
  background: #f2f4f7;
  color: #101828;
  box-shadow: none;
}
.template-minimal .board small {
  color: #667085;
}
.template-split {
  background: linear-gradient(90deg, #07111f 0 58%, #101d2d 58%);
}
.template-split .board {
  grid-template-columns: 1.4fr 0.8fr;
}
.template-split .board aside {
  border-left-width: 8px;
}
.template-custom {
  background: var(--custom-background);
  color: var(--custom-foreground);
  font-family: var(--custom-font), sans-serif;
}
.template-custom .board aside,
.template-custom .metrics article {
  background: color-mix(in srgb, var(--custom-foreground) 8%, var(--custom-background));
}
@media (max-width: 800px) {
  .board {
    grid-template-columns: 1fr;
  }
  .metrics {
    grid-template-columns: 1fr;
  }
  .template-split {
    background: #07111f;
  }
}
</style>
