<template>
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
          <span>{{ focusRow.penaltyMinutes }} penalty</span><span>Team #{{ focusRow.teamId }}</span>
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
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { ResolverPublicRun } from '../../api/resolver';
import { useI18n } from '../../i18n';

const props = withDefaults(
  defineProps<{ run: ResolverPublicRun | null; errorMessage?: string }>(),
  { run: null, errorMessage: '' },
);
const { t } = useI18n();
const focusRow = computed(
  () =>
    props.run?.state.board.rows.find((row) => row.teamId === props.run?.state.lastReveal?.teamId) ??
    null,
);
const focusProblem = computed(
  () =>
    props.run?.state.board.problems.find(
      (problem) => problem.problemId === props.run?.state.lastReveal?.problemId,
    ) ?? null,
);
</script>
