<template>
  <el-container direction="vertical" class="resolver-page">
    <el-header height="auto" class="resolver-page-heading">
      <div>
        <p class="eyebrow">Official Resolver</p>
        <h1>{{ t('Resolver 运行控制台') }}</h1>
      </div>
      <ElSpace wrap :size="10" class="resolver-heading-actions">
        <ElSelect
          v-model="contestId"
          filterable
          :placeholder="t('选择比赛')"
          @change="changeContest"
        >
          <ElOption
            v-for="contest in contests"
            :key="contest.id"
            :label="`${contest.name} · #${contest.id}`"
            :value="contest.id"
          />
        </ElSelect>
        <ElButton :icon="Refresh" :loading="loading" @click="loadContext">{{ t('刷新') }}</ElButton>
        <RouterLink
          v-if="run?.official && run.status !== 'READY'"
          :to="{ name: 'resolver-display', params: { runId: run.id } }"
          target="_blank"
          ><ElButton :icon="Monitor">{{ t('打开展示端') }}</ElButton></RouterLink
        >
      </ElSpace>
    </el-header>
    <el-main class="page-body">
      <ElAlert
        v-if="errorMessage"
        :title="errorMessage"
        type="error"
        show-icon
        :closable="false"
        class="page-alert"
      />
      <template v-if="contestId">
        <ResolverMetricsRow :run="run" :realtime-connected="realtimeConnected" />
        <ResolverRunCard
          v-model:run-id="runId"
          :runs="runs"
          :sources="sources"
          :has-official="hasOfficial"
          :acting="acting"
          @select-run="selectRun"
          @create-run="createRun"
        />
        <ResolverControlCard
          v-model:auto-interval="autoInterval"
          :run="run"
          :can-next="canNext"
          :can-previous="canPrevious"
          :can-complete="canComplete"
          :acting="acting"
          @control="control"
          @complete="completeRun"
          @toggle-auto-play="toggleAutoPlay"
        />
        <ResolverWorkspaceRow v-if="run" :run="run" :events="events" />
      </template>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Monitor, Refresh } from '@element-plus/icons-vue';
import { useRoute, useRouter } from 'vue-router';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import {
  resolverApi,
  type ResolverEvent,
  type ResolverRun,
  type ResolverSources,
} from '../api/resolver';
import type { ContestResponse } from '../api/types';
import {
  subscribeContestEvents,
  type ContestRealtimeSubscription,
} from '../realtime/contest-events';
import { useI18n } from '../i18n';
import ResolverMetricsRow from '../components/resolver-manage/ResolverMetricsRow.vue';
import ResolverRunCard from '../components/resolver-manage/ResolverRunCard.vue';
import ResolverControlCard from '../components/resolver-manage/ResolverControlCard.vue';
import ResolverWorkspaceRow from '../components/resolver-manage/ResolverWorkspaceRow.vue';

const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const contests = ref<ContestResponse[]>([]);
const contestId = ref<number | null>(null);
const runs = ref<ResolverRun[]>([]);
const runId = ref<number | null>(null);
const run = ref<ResolverRun | null>(null);
const sources = ref<ResolverSources | null>(null);
const events = ref<ResolverEvent[]>([]);
const loading = ref(false);
const acting = ref(false);
const errorMessage = ref('');
const realtimeConnected = ref(false);
const autoInterval = ref(3000);
let realtime: ContestRealtimeSubscription | undefined;
let generation = 0;
const hasOfficial = computed(() => runs.value.some((item) => item.official));
const canNext = computed(
  () => run.value?.status === 'RUNNING' && run.value.currentStep < run.value.totalSteps,
);
const canPrevious = computed(
  () =>
    !!run.value && ['RUNNING', 'PAUSED'].includes(run.value.status) && run.value.currentStep > 0,
);
const canComplete = computed(
  () =>
    !!run.value &&
    ['RUNNING', 'PAUSED'].includes(run.value.status) &&
    run.value.currentStep === run.value.totalSteps,
);

async function loadRun(id: number, silent = false) {
  try {
    const [next, history] = await Promise.all([resolverApi.get(id), resolverApi.events(id)]);
    if (runId.value !== id) return;
    run.value = next;
    events.value = history;
    autoInterval.value = next.autoPlayIntervalMilliseconds;
    if (!silent) errorMessage.value = '';
  } catch (error) {
    if (!silent) errorMessage.value = getErrorMessage(error);
  }
}
async function loadContext() {
  if (!contestId.value) return;
  const id = contestId.value;
  const request = ++generation;
  loading.value = true;
  try {
    const [nextRuns, nextSources] = await Promise.all([
      resolverApi.list(id),
      resolverApi.sources(id).catch(() => null),
    ]);
    if (request !== generation || id !== contestId.value) return;
    runs.value = nextRuns;
    sources.value = nextSources;
    const requested = Number(route.query.runId);
    runId.value = nextRuns.some((item) => item.id === requested)
      ? requested
      : (nextRuns[0]?.id ?? null);
    if (runId.value) await loadRun(runId.value);
    else {
      run.value = null;
      events.value = [];
    }
    errorMessage.value = '';
  } catch (error) {
    if (request === generation) errorMessage.value = getErrorMessage(error);
  } finally {
    if (request === generation) loading.value = false;
  }
}
function connectRealtime() {
  realtime?.stop();
  realtimeConnected.value = false;
  if (!contestId.value) return;
  realtime = subscribeContestEvents({
    contestId: contestId.value,
    scope: 'STAFF',
    eventTypes: ['RESOLVER_STATE_CHANGED'],
    onEvent: () => {
      if (runId.value) void loadRun(runId.value, true);
    },
    onConnectionChange: (value) => {
      realtimeConnected.value = value;
    },
    poll: async () => {
      if (runId.value) await loadRun(runId.value, true);
    },
  });
}
async function changeContest() {
  generation += 1;
  run.value = null;
  runs.value = [];
  sources.value = null;
  runId.value = null;
  await router.replace({ query: contestId.value ? { contestId: String(contestId.value) } : {} });
  connectRealtime();
  await loadContext();
}
async function selectRun() {
  if (!runId.value) return;
  await router.replace({
    query: { contestId: String(contestId.value), runId: String(runId.value) },
  });
  await loadRun(runId.value);
}
async function createRun(official: boolean) {
  if (!contestId.value || !sources.value) return;
  try {
    await ElMessageBox.confirm(
      official
        ? t('正式运行每场比赛只能创建一次，确认快照均已复核？')
        : t('确认使用当前最新快照创建预演运行？'),
      official ? t('创建正式 Resolver') : t('创建预演'),
      { type: 'warning' },
    );
  } catch {
    return;
  }
  acting.value = true;
  try {
    const created = await resolverApi.create(
      contestId.value,
      sources.value.publicSnapshot.id,
      sources.value.finalSnapshot.id,
      official,
    );
    runs.value = [created, ...runs.value];
    runId.value = created.id;
    run.value = created;
    events.value = await resolverApi.events(created.id);
    await selectRun();
    ElMessage.success(official ? t('正式运行已创建') : t('预演已创建'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    acting.value = false;
  }
}
async function control(action: 'start' | 'next' | 'previous' | 'pause' | 'resume') {
  if (!run.value) return;
  acting.value = true;
  try {
    run.value = await resolverApi[action](run.value.id, run.value.version);
    events.value = await resolverApi.events(run.value.id);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
    await loadRun(run.value.id, true);
  } finally {
    acting.value = false;
  }
}
async function completeRun() {
  if (!run.value || !canComplete.value) return;
  try {
    await ElMessageBox.confirm(t('确认所有步骤均已复核并完成 Resolver？'), t('完成 Resolver'), {
      type: 'warning',
    });
  } catch {
    return;
  }
  acting.value = true;
  try {
    run.value = await resolverApi.complete(run.value.id, run.value.version);
    ElMessage.success(t('Resolver 已完成'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    acting.value = false;
  }
}
async function toggleAutoPlay() {
  if (!run.value) return;
  acting.value = true;
  try {
    run.value = await resolverApi.autoPlay(
      run.value.id,
      run.value.version,
      !run.value.autoPlayEnabled,
      autoInterval.value,
    );
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    acting.value = false;
  }
}
onMounted(async () => {
  try {
    contests.value = (await contestApi.listContests()).content;
    const requested = Number(route.query.contestId);
    contestId.value = contests.value.some((item) => item.id === requested)
      ? requested
      : (contests.value[0]?.id ?? null);
    if (contestId.value) {
      connectRealtime();
      await loadContext();
    }
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
});
onUnmounted(() => realtime?.stop());
</script>

<style scoped>
.resolver-page {
  width: min(1580px, 100%);
  margin: 0 auto;
}
.resolver-page-heading {
  display: flex;
  align-items: center;
  gap: 12px;
  justify-content: space-between;
  padding: 34px 34px 0;
  margin-bottom: 24px;
  height: auto;
}
.resolver-page-heading h1 {
  margin: 4px 0 8px;
  color: #152033;
  font-size: clamp(30px, 4vw, 44px);
  letter-spacing: -0.04em;
}
.resolver-page-heading p:last-child {
  display: none;
  margin: 0;
  color: var(--muted);
}
.resolver-heading-actions .el-select {
  width: min(320px, 100%);
}
.page-body {
  padding: 0 34px 34px;
}
@media (max-width: 1100px) {
  .resolver-page-heading {
    align-items: stretch;
    flex-direction: column;
  }
}
@media (max-width: 760px) {
  .resolver-page-heading {
    padding: 24px 16px 0;
  }
  .page-body {
    padding: 0 16px 24px;
  }
}
</style>
