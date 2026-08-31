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
    <section v-if="presentation?.status === 'WAITING'" class="award-display-message">
      <p>AWARD CEREMONY</p>
      <h1>{{ t('颁奖典礼即将开始') }}</h1>
      <span>{{ t('获奖名单已经锁定，请等待主持人宣布。') }}</span>
    </section>
    <section
      v-else-if="presentation?.status === 'COMPLETED'"
      class="award-display-message completed"
    >
      <p>CONGRATULATIONS</p>
      <h1>{{ t('颁奖典礼圆满结束') }}</h1>
      <span>{{ t('祝贺所有获奖队伍。') }}</span>
    </section>
    <section v-else-if="category" class="award-display-stage">
      <div class="award-display-category">
        <small>{{ category.code }}</small>
        <h1>{{ category.name }}</h1>
        <p v-if="category.groupName">{{ category.groupName }}</p>
        <span>{{ t('{count} 条获奖记录', { count: category.recipients.length }) }}</span>
      </div>
      <div class="award-display-recipients">
        <article
          v-for="recipient in category.recipients"
          :key="recipient.id"
          class="award-display-recipient"
        >
          <div class="award-display-medal">
            {{ recipient.problemAlias ? `FB ${recipient.problemAlias}` : (recipient.rank ?? '★') }}
          </div>
          <div>
            <small>{{ recipient.school || t('参赛队伍') }}</small>
            <h2>{{ recipient.teamName }}</h2>
            <p>
              <span v-if="recipient.seatNo">{{ t('座位 {seat}', { seat: recipient.seatNo }) }}</span
              ><b v-if="recipient.star">{{ t('打星队') }}</b>
            </p>
          </div>
          <div class="award-display-score">
            <strong>{{ recipient.solved ?? '—' }}</strong
            ><small>solved</small><span>{{ recipient.penaltyMinutes ?? '—' }} min</span>
          </div>
        </article>
        <div v-if="!category.recipients.length" class="award-display-empty">
          {{ t('当前奖项没有获奖队伍') }}
        </div>
      </div>
    </section>
    <section v-else class="award-display-message">
      <p>AWARD CEREMONY</p>
      <h1>{{ errorMessage || t('等待最终获奖名单') }}</h1>
      <span>{{ t('请使用 ?contestId= 指定比赛。') }}</span>
    </section>
  </main>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { useRoute } from 'vue-router';
import { awardsApi, type AwardPresentation } from '../api/awards';
import { getErrorMessage } from '../api/client';
import {
  subscribeContestEvents,
  type ContestRealtimeSubscription,
} from '../realtime/contest-events';
import { useI18n } from '../i18n';
const { t } = useI18n();
const route = useRoute();
const contestId = Number(route.query.contestId) || null;
const presentation = ref<AwardPresentation | null>(null);
const errorMessage = ref('');
const disconnected = ref(false);
const now = ref(Date.now());
let realtime: ContestRealtimeSubscription | undefined;
let clockTimer: number | undefined;
const category = computed(() => {
  const value = presentation.value;
  if (!value?.categories.length) return null;
  const base = Math.max(
    0,
    value.categories.findIndex((item) => item.id === value.currentCategoryId),
  );
  if (!value.autoRotate || value.status !== 'PRESENTING') return value.categories[base] ?? null;
  const elapsed = Math.max(0, now.value - new Date(value.stateUpdatedAt).getTime());
  return (
    value.categories[
      (base + Math.floor(elapsed / (value.intervalSeconds * 1000))) % value.categories.length
    ] ?? null
  );
});
async function load() {
  if (!contestId) return;
  try {
    presentation.value = await awardsApi.presentation(contestId);
    now.value = new Date(presentation.value.serverTime).getTime();
    disconnected.value = false;
    errorMessage.value = '';
  } catch (error) {
    if (presentation.value) disconnected.value = true;
    else errorMessage.value = getErrorMessage(error);
  }
}
async function toggleFullscreen() {
  if (document.fullscreenElement) await document.exitFullscreen();
  else await document.documentElement.requestFullscreen();
}
onMounted(async () => {
  await load();
  clockTimer = window.setInterval(() => {
    now.value += 1000;
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
