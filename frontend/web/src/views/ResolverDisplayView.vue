<template>
  <main class="resolver-display">
    <header class="resolver-display-header">
      <div>
        <span>ProjectBalloon · OFFICIAL RESOLVER</span><strong>{{ contestTitle }}</strong>
      </div>
      <div class="resolver-display-status">
        <span :class="run?.status.toLowerCase()">{{ statusLabel }}</span
        ><small>{{ run ? `${run.currentStep} / ${run.totalSteps}` : t('等待连接') }}</small
        ><button type="button" @click="toggleFullscreen">{{ t('全屏') }}</button>
      </div>
    </header>
    <section v-if="run" class="resolver-display-stage">
      <div v-if="focusRow && run.state.lastReveal" class="resolver-display-focus">
        <div class="resolver-display-rank">
          <small>CURRENT RANK</small><strong>#{{ focusRow.rank }}</strong
          ><span>{{ focusRow.solvedCount }} solved</span>
        </div>
        <div class="resolver-display-team">
          <p>{{ focusRow.school ?? t('学校未填写') }}</p>
          <h1>{{ focusRow.teamName }}</h1>
          <div>
            <span>{{ focusRow.penaltyMinutes }} penalty</span
            ><span>Team #{{ focusRow.teamId }}</span>
          </div>
        </div>
        <div
          class="resolver-display-verdict"
          :class="run.state.lastReveal.after.solved ? 'accepted' : 'rejected'"
        >
          <small>PROBLEM {{ focusProblem?.alias ?? run.state.lastReveal.problemId }}</small
          ><strong>{{ run.state.lastReveal.after.solved ? 'ACCEPTED' : 'REVEALED' }}</strong
          ><span>Step {{ run.currentStep }}</span>
        </div>
      </div>
      <div v-else class="resolver-display-complete">
        <small>{{ run.status === 'COMPLETED' ? 'RESOLVER COMPLETE' : 'WAITING FOR REVEAL' }}</small>
        <h1>{{ run.status === 'COMPLETED' ? t('最终排名') : t('等待下一步') }}</h1>
      </div>
      <div class="resolver-display-board">
        <article
          v-for="row in run.state.board.rows.slice(0, 12)"
          :key="row.teamId"
          :class="{ focused: row.teamId === focusRow?.teamId }"
        >
          <strong>#{{ row.rank }}</strong>
          <div>
            <b>{{ row.teamName }}</b
            ><span>{{ row.school ?? '—' }}</span>
          </div>
          <span>{{ row.solvedCount }}</span
          ><span>{{ row.penaltyMinutes }}</span>
        </article>
      </div>
    </section>
    <section v-else class="resolver-display-waiting">
      <p>OFFICIAL RESOLVER</p>
      <h1>{{ errorMessage || t('等待正式运行开始') }}</h1>
      <small>{{ t('运行必须为正式运行且已经开始。') }}</small>
    </section>
  </main>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { useRoute } from 'vue-router';
import { getErrorMessage } from '../api/client';
import { contestApi } from '../api/contest';
import { resolverApi, type ResolverPublicRun } from '../api/resolver';
import type { ContestResponse } from '../api/types';
import {
  subscribeContestEvents,
  type ContestRealtimeSubscription,
} from '../realtime/contest-events';
import { useI18n } from '../i18n';

const { t } = useI18n();

const route = useRoute();
const run = ref<ResolverPublicRun | null>(null);
const contest = ref<ContestResponse | null>(null);
const errorMessage = ref('');
let realtime: ContestRealtimeSubscription | undefined;
let generation = 0;
const contestTitle = computed(
  () => contest.value?.name ?? (run.value ? `Contest #${run.value.contestId}` : 'Resolver Display'),
);
const statusLabel = computed(() =>
  run.value
    ? { RUNNING: 'LIVE', PAUSED: 'PAUSED', COMPLETED: 'COMPLETE', READY: 'READY' }[run.value.status]
    : 'OFFLINE',
);
const focusRow = computed(
  () =>
    run.value?.state.board.rows.find((row) => row.teamId === run.value?.state.lastReveal?.teamId) ??
    null,
);
const focusProblem = computed(
  () =>
    run.value?.state.board.problems.find(
      (problem) => problem.problemId === run.value?.state.lastReveal?.problemId,
    ) ?? null,
);

async function load(id: number, request = generation): Promise<ResolverPublicRun | null> {
  try {
    const next = await resolverApi.publicState(id);
    if (request !== generation) return null;
    run.value = next;
    errorMessage.value = '';
    return next;
  } catch (error) {
    if (request === generation) {
      run.value = null;
      errorMessage.value = getErrorMessage(error);
    }
    return null;
  }
}
async function activate() {
  const request = ++generation;
  realtime?.stop();
  realtime = undefined;
  run.value = null;
  contest.value = null;
  errorMessage.value = '';
  const id = Number(route.params.runId);
  if (!Number.isSafeInteger(id) || id <= 0) {
    errorMessage.value = t('Resolver 运行 ID 不正确');
    return;
  }
  const activeRun = await load(id, request);
  if (request !== generation || !activeRun) return;
  try {
    contest.value = await contestApi.getContest(activeRun.contestId);
  } catch {
    contest.value = null;
  }
  if (request !== generation) return;
  realtime = subscribeContestEvents({
    contestId: activeRun.contestId,
    scope: 'PUBLIC',
    eventTypes: ['RESOLVER_STATE_CHANGED'],
    onEvent: () => {
      void load(id, request);
    },
    poll: async () => {
      await load(id, request);
    },
    pollIntervalMs: 2000,
  });
}
async function toggleFullscreen() {
  if (document.fullscreenElement) await document.exitFullscreen();
  else await document.documentElement.requestFullscreen();
}
watch(
  () => route.params.runId,
  () => {
    void activate();
  },
  { immediate: true },
);
onUnmounted(() => {
  generation += 1;
  realtime?.stop();
});
</script>
