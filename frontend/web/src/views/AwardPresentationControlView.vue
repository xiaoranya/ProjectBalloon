<template>
  <el-container direction="vertical" class="awards-page">
    <el-header height="auto" class="awards-page-header">
      <div>
        <p class="eyebrow">Ceremony Control</p>
        <h1>{{ t('颁奖控制台') }}</h1>
      </div>
      <ElSpace wrap :size="10" class="awards-header-actions">
        <ElSelect
          v-model="contestId"
          filterable
          :placeholder="t('选择比赛')"
          @change="changeContest"
          ><ElOption
            v-for="contest in contests"
            :key="contest.id"
            :label="contest.name"
            :value="contest.id"
        /></ElSelect>
        <ElButton :icon="Refresh" :loading="loading" @click="load">{{ t('刷新') }}</ElButton>
        <ElButton :disabled="!contestId" @click="openDisplay">{{ t('打开展示页') }}</ElButton>
      </ElSpace>
    </el-header>
    <el-main class="page-body">
      <ElAlert
        v-if="errorMessage"
        type="warning"
        show-icon
        :closable="false"
        :title="errorMessage"
      />
      <ElCard v-if="presentation" shadow="never" class="award-presentation-control">
        <ElRow :gutter="20" class="award-presentation-control-grid">
          <ElCol :xs="24" :md="6"
            ><div class="award-presentation-status">
              <span>{{ t('典礼状态') }}</span
              ><ElTag>{{ statusLabel }}</ElTag>
              <div>
                <ElButton @click="setStatus('WAITING')">{{ t('等待') }}</ElButton
                ><ElButton type="success" @click="setStatus('PRESENTING')">{{
                  t('开始/继续')
                }}</ElButton
                ><ElButton type="danger" @click="setStatus('COMPLETED')">{{ t('结束') }}</ElButton>
              </div>
            </div></ElCol
          >
          <ElCol :xs="24" :md="12"
            ><div class="award-presentation-category-control">
              <span>{{ t('当前奖项') }}</span
              ><ElSelect v-model="draft.currentCategoryId"
                ><ElOption
                  v-for="category in presentation.categories"
                  :key="category.id"
                  :label="`${category.code} · ${category.name}`"
                  :value="category.id"
              /></ElSelect>
              <div>
                <ElButton :disabled="currentIndex <= 0" @click="move(-1)">{{
                  t('上一项')
                }}</ElButton
                ><ElButton
                  type="primary"
                  :disabled="currentIndex >= presentation.categories.length - 1"
                  @click="move(1)"
                  >{{ t('下一项') }}</ElButton
                >
              </div>
            </div></ElCol
          >
          <ElCol :xs="24" :md="6"
            ><div class="award-presentation-rotation">
              <span>{{ t('自动轮播') }}</span
              ><ElSwitch v-model="draft.autoRotate" /><ElInputNumber
                v-model="draft.intervalSeconds"
                :min="5"
                :max="120"
              /><small>{{ t('秒/页') }}</small>
            </div></ElCol
          >
        </ElRow>
        <ElButton type="primary" :loading="saving" @click="save">{{ t('应用到大屏') }}</ElButton>
      </ElCard>
      <ElEmpty
        v-else-if="!loading && !errorMessage"
        :description="t('锁定获奖名单后即可控制颁奖展示')"
      />
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { Refresh } from '@element-plus/icons-vue';
import { ElMessage } from 'element-plus';
import { useRoute, useRouter } from 'vue-router';
import { contestApi } from '../api/contest';
import { awardsApi, type AwardPresentation, type AwardPresentationStatus } from '../api/awards';
import { getErrorMessage } from '../api/client';
import { useI18n } from '../i18n';
const { t } = useI18n();
import type { Contest } from '../api/types';
import {
  subscribeContestEvents,
  type ContestRealtimeSubscription,
} from '../realtime/contest-events';

const route = useRoute();
const router = useRouter();
const contests = ref<Contest[]>([]);
const contestId = ref<number | null>(null);
const presentation = ref<AwardPresentation | null>(null);
const loading = ref(false);
const saving = ref(false);
const errorMessage = ref('');
let generation = 0;
let realtime: ContestRealtimeSubscription | undefined;
const draft = reactive({
  currentCategoryId: null as number | null,
  status: 'WAITING' as AwardPresentationStatus,
  autoRotate: false,
  intervalSeconds: 15,
});
const currentIndex = computed(
  () =>
    presentation.value?.categories.findIndex((item) => item.id === draft.currentCategoryId) ?? -1,
);
const statusLabel = computed(() =>
  t({ WAITING: '等待开始', PRESENTING: '进行中', COMPLETED: '已结束' }[draft.status]),
);

function apply(value: AwardPresentation) {
  presentation.value = value;
  draft.currentCategoryId = value.currentCategoryId;
  draft.status = value.status;
  draft.autoRotate = value.autoRotate;
  draft.intervalSeconds = value.intervalSeconds;
}
async function load() {
  if (!contestId.value) return;
  const id = contestId.value;
  const token = ++generation;
  loading.value = true;
  try {
    const value = await awardsApi.presentation(id);
    if (token === generation && id === contestId.value) {
      apply(value);
      errorMessage.value = '';
    }
  } catch (error) {
    if (token === generation) {
      presentation.value = null;
      errorMessage.value = getErrorMessage(error);
    }
  } finally {
    if (token === generation) loading.value = false;
  }
}
function connect() {
  realtime?.stop();
  if (!contestId.value) return;
  realtime = subscribeContestEvents({
    contestId: contestId.value,
    scope: 'PUBLIC',
    eventTypes: ['AWARDS_UPDATED'],
    onEvent: () => void load(),
    poll: load,
  });
}
async function changeContest() {
  generation += 1;
  presentation.value = null;
  await router.replace({ query: contestId.value ? { contestId: String(contestId.value) } : {} });
  connect();
  await load();
}
async function save() {
  if (!contestId.value) return;
  saving.value = true;
  try {
    apply(await awardsApi.updatePresentation(contestId.value, { ...draft }));
    errorMessage.value = '';
    ElMessage.success(t('颁奖展示已更新'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
    await load();
  } finally {
    saving.value = false;
  }
}
async function setStatus(status: AwardPresentationStatus) {
  draft.status = status;
  await save();
}
async function move(offset: number) {
  const category = presentation.value?.categories[currentIndex.value + offset];
  if (category) {
    draft.currentCategoryId = category.id;
    await save();
  }
}
function openDisplay() {
  if (contestId.value)
    window.open(`/awards/display?contestId=${contestId.value}`, '_blank', 'noopener');
}
onMounted(async () => {
  try {
    contests.value = (await contestApi.listContests()).content;
    const requested = Number(route.query.contestId);
    contestId.value = contests.value.some((item) => item.id === requested)
      ? requested
      : (contests.value[0]?.id ?? null);
    connect();
    await load();
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
});
onBeforeUnmount(() => realtime?.stop());
</script>

<style scoped>
.awards-page {
  width: min(1540px, 100%);
  margin: 0 auto;
}

.awards-page-header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 24px;
  padding: 38px 38px 0;
  margin-bottom: 26px;
  height: auto;
}

.awards-page-header h1 {
  margin: 4px 0 8px;
  color: #172033;
  font-size: clamp(30px, 4vw, 44px);
  letter-spacing: -0.035em;
}

.awards-page-header > div > p:last-child {
  display: none;
  margin: 4px 0 0;
  color: var(--muted);
}

.page-body {
  padding: 0 38px 38px;
}

.award-presentation-control {
  margin-top: 18px;
  border-radius: 0;
}

.award-presentation-control-grid > .el-col {
  align-self: flex-end;
}

.award-presentation-status,
.award-presentation-rotation {
  display: flex;
  align-items: center;
  gap: 10px;
}

.award-presentation-status {
  align-items: stretch;
  flex-direction: column;
}

.award-presentation-status > span {
  color: var(--muted);
  font-size: 13px;
}

.award-presentation-category-control {
  display: grid;
  grid-template-columns: auto minmax(180px, 1fr) auto;
  gap: 8px;
}

.award-presentation-rotation .el-input-number {
  width: 100px;
}

@media (max-width: 760px) {
  .awards-page-header {
    align-items: stretch;
    flex-direction: column;
    padding: 24px 16px 0;
  }

  .page-body {
    padding: 0 16px 24px;
  }
}
</style>
