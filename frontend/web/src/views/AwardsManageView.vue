<template>
  <el-container direction="vertical" class="awards-page">
    <el-header height="auto" class="awards-page-header">
      <div>
        <p class="eyebrow">Awards Operations</p>
        <h1>{{ t('奖项名单管理') }}</h1>
      </div>
      <ElSpace wrap :size="10" class="awards-header-actions">
        <ElSelect
          v-model="contestId"
          filterable
          :placeholder="t('选择比赛')"
          @change="changeContest"
        >
          <ElOption
            v-for="contest in contests"
            :key="contest.id"
            :value="contest.id"
            :label="contest.name"
          />
        </ElSelect>
        <ElButton :icon="Refresh" :loading="loading" @click="loadWorkspace">{{
          t('刷新')
        }}</ElButton>
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
      <AwardMetricsRow :categories="categories" :award-set="awardSet" />
      <AwardsCommandBar
        v-model:resolver-run-id="resolverRunId"
        :categories="categories"
        :award-set="awardSet"
        :completed-runs="completedRuns"
        :mutating="mutating"
        :exporting="exporting"
        @generate="generateAwards"
        @freeze="freezeAwards"
        @unfreeze="unfreezeAwards"
        @export-csv="downloadCsv"
        @export-certificates="downloadCertificates"
      />
      <div v-if="awardSet?.conflicts.length" class="award-warning-stack">
        <ElAlert
          v-for="conflict in awardSet.conflicts"
          :key="conflict.teamId"
          type="warning"
          show-icon
          :closable="false"
          :title="
            t('{team} 同时出现在 {categories}', {
              team: conflict.teamName,
              categories: conflict.categoryCodes.join('、'),
            })
          "
        />
      </div>
      <ElRow :gutter="18" class="awards-workspace">
        <ElCol :xs="24" :md="8">
          <AwardCategoriesPanel
            :contest-id="contestId"
            :categories="categories"
            :award-set="awardSet"
            @update:categories="categories = $event"
            @refresh="loadWorkspace"
          />
        </ElCol>
        <ElCol :xs="24" :md="16">
          <AwardRecipientsPanel
            :contest-id="contestId"
            :award-set="awardSet"
            :categories="categories"
            :candidates="candidates"
            :mutating="mutating"
            @update:award-set="awardSet = $event"
          />
        </ElCol>
      </ElRow>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Refresh } from '@element-plus/icons-vue';
import { useRoute, useRouter } from 'vue-router';
import { awardsApi, type AwardCategory, type AwardResolverRun, type AwardSet } from '../api/awards';
import { ApiError, getErrorMessage } from '../api/client';
import { contestApi } from '../api/contest';
import type { Contest } from '../api/types';
import { useI18n } from '../i18n';
import AwardMetricsRow from '../components/awards-manage/AwardMetricsRow.vue';
import AwardsCommandBar from '../components/awards-manage/AwardsCommandBar.vue';
import AwardCategoriesPanel from '../components/awards-manage/AwardCategoriesPanel.vue';
import AwardRecipientsPanel from '../components/awards-manage/AwardRecipientsPanel.vue';

const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const contests = ref<Contest[]>([]);
const contestId = ref<number | null>(null);
const categories = ref<AwardCategory[]>([]);
const awardSet = ref<AwardSet | null>(null);
const completedRuns = ref<AwardResolverRun[]>([]);
const candidates = ref<Awaited<ReturnType<typeof awardsApi.candidates>>>([]);
const resolverRunId = ref<number | null>(null);
const loading = ref(false);
const mutating = ref(false);
const exporting = ref(false);
const errorMessage = ref('');
let generation = 0;

async function loadWorkspace() {
  if (!contestId.value) return;
  const activeContest = contestId.value;
  const request = ++generation;
  loading.value = true;
  try {
    const [nextCategories, runs] = await Promise.all([
      awardsApi.listCategories(activeContest),
      awardsApi.completedRuns(activeContest),
    ]);
    if (request !== generation || activeContest !== contestId.value) return;
    categories.value = nextCategories;
    completedRuns.value = runs;
    resolverRunId.value = awardSet.value?.resolverRunId ?? runs[0]?.id ?? null;
    try {
      const nextSet = await awardsApi.get(activeContest);
      const nextCandidates = await awardsApi.candidates(activeContest);
      if (request !== generation || activeContest !== contestId.value) return;
      awardSet.value = nextSet;
      candidates.value = nextCandidates;
      resolverRunId.value = nextSet.resolverRunId;
    } catch (error) {
      if (!(error instanceof ApiError && error.status === 404)) throw error;
      if (request !== generation || activeContest !== contestId.value) return;
      awardSet.value = null;
      candidates.value = [];
    }
    errorMessage.value = '';
  } catch (error) {
    if (request === generation && activeContest === contestId.value)
      errorMessage.value = getErrorMessage(error);
  } finally {
    if (request === generation) loading.value = false;
  }
}
async function changeContest() {
  generation += 1;
  awardSet.value = null;
  categories.value = [];
  candidates.value = [];
  await router.replace({ query: contestId.value ? { contestId: String(contestId.value) } : {} });
  await loadWorkspace();
}
async function generateAwards() {
  if (!contestId.value || !resolverRunId.value) return;
  try {
    await ElMessageBox.confirm(
      t('重新生成会替换自动生成的名单，但保留手工添加项。是否继续？'),
      t('生成奖项名单'),
      { type: 'warning' },
    );
  } catch {
    return;
  }
  mutating.value = true;
  try {
    awardSet.value = await awardsApi.generate(contestId.value, resolverRunId.value);
    candidates.value = await awardsApi.candidates(contestId.value);
    ElMessage.success(t('奖项名单已生成'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    mutating.value = false;
  }
}
async function freezeAwards() {
  if (!contestId.value || !awardSet.value) return;
  if (awardSet.value.conflicts.length) {
    ElMessage.warning(t('名单仍有重复获奖冲突，请复核后再锁定'));
    return;
  }
  mutating.value = true;
  try {
    awardSet.value = await awardsApi.freeze(contestId.value, awardSet.value.version);
    ElMessage.success(t('奖项名单已锁定'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    mutating.value = false;
  }
}
async function unfreezeAwards() {
  if (!contestId.value || !awardSet.value) return;
  mutating.value = true;
  try {
    awardSet.value = await awardsApi.unfreeze(contestId.value, awardSet.value.version);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    mutating.value = false;
  }
}
async function downloadCsv() {
  if (!contestId.value) return;
  exporting.value = true;
  let url: string | null = null;
  let anchor: HTMLAnchorElement | null = null;
  try {
    url = URL.createObjectURL(await awardsApi.csv(contestId.value));
    anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `contest-${contestId.value}-awards.csv`;
    document.body.append(anchor);
    anchor.click();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    anchor?.remove();
    if (url) window.setTimeout(() => URL.revokeObjectURL(url!), 0);
    exporting.value = false;
  }
}
async function downloadCertificates() {
  if (!contestId.value || awardSet.value?.status !== 'FROZEN') return;
  exporting.value = true;
  let url: string | null = null;
  let anchor: HTMLAnchorElement | null = null;
  try {
    url = URL.createObjectURL(await awardsApi.certificates(contestId.value));
    anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `contest-${contestId.value}-certificates.csv`;
    document.body.append(anchor);
    anchor.click();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    anchor?.remove();
    if (url) window.setTimeout(() => URL.revokeObjectURL(url!), 0);
    exporting.value = false;
  }
}
onMounted(async () => {
  try {
    contests.value = (await contestApi.listContests()).content;
    const requested = Number(route.query.contestId);
    contestId.value = contests.value.some((item) => item.id === requested)
      ? requested
      : (contests.value[0]?.id ?? null);
    await loadWorkspace();
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
});
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
.page-body {
  padding: 0 38px 38px;
}
.award-warning-stack {
  display: grid;
  gap: 10px;
  margin-bottom: 18px;
}
.awards-workspace > .el-col {
  align-self: flex-start;
}
@media (max-width: 1180px) {
  .awards-workspace > .el-col {
    flex: 0 0 100%;
    max-width: 100%;
  }
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
