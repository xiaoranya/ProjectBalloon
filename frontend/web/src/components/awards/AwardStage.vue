<template>
  <section v-if="presentation?.status === 'WAITING'" class="award-display-message">
    <p>AWARD CEREMONY</p>
    <h1>{{ t('颁奖典礼即将开始') }}</h1>
    <span>{{ t('获奖名单已经锁定，请等待主持人宣布。') }}</span>
  </section>
  <section v-else-if="presentation?.status === 'COMPLETED'" class="award-display-message completed">
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
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { AwardPresentation } from '../../api/awards';
import { useI18n } from '../../i18n';

const props = withDefaults(
  defineProps<{
    presentation: AwardPresentation | null;
    /** Live rotation clock; recompute the auto-rotated category from it. */
    now?: number;
    errorMessage?: string;
  }>(),
  { presentation: null, now: 0, errorMessage: '' },
);
const { t } = useI18n();
const category = computed(() => {
  const value = props.presentation;
  if (!value?.categories.length) return null;
  const base = Math.max(
    0,
    value.categories.findIndex((item) => item.id === value.currentCategoryId),
  );
  if (!value.autoRotate || value.status !== 'PRESENTING' || !props.now) {
    return value.categories[base] ?? null;
  }
  const elapsed = Math.max(0, props.now - new Date(value.stateUpdatedAt).getTime());
  return (
    value.categories[
      (base + Math.floor(elapsed / (value.intervalSeconds * 1000))) % value.categories.length
    ] ?? null
  );
});
</script>
