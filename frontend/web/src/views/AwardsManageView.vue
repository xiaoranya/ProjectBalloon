<template>
  <el-container direction="vertical" class="awards-page">
    <el-header height="auto" class="awards-page-header"
      ><div>
        <p class="eyebrow">Awards Operations</p>
        <h1>{{ t('奖项名单管理') }}</h1>
      </div>
      <ElSpace wrap :size="10" class="awards-header-actions"
        ><ElSelect
          v-model="contestId"
          filterable
          :placeholder="t('选择比赛')"
          @change="changeContest"
          ><ElOption
            v-for="contest in contests"
            :key="contest.id"
            :value="contest.id"
            :label="contest.name" /></ElSelect
        ><ElButton :icon="Refresh" :loading="loading" @click="loadWorkspace">{{
          t('刷新')
        }}</ElButton></ElSpace
      ></el-header
    >
    <el-main class="page-body">
      <ElAlert
        v-if="errorMessage"
        :title="errorMessage"
        type="error"
        show-icon
        :closable="false"
        class="page-alert"
      />
      <ElRow :gutter="14" class="award-metrics"
        ><ElCol :xs="12" :md="6"
          ><div class="award-metric">
            <span>{{ t('奖项类别') }}</span
            ><strong>{{ categories.length }}</strong>
          </div></ElCol
        ><ElCol :xs="12" :md="6"
          ><div class="award-metric">
            <span>{{ t('获奖记录') }}</span
            ><strong>{{ awardSet?.recipients.length ?? 0 }}</strong>
          </div></ElCol
        ><ElCol :xs="12" :md="6"
          ><div class="award-metric">
            <span>{{ t('获奖队伍') }}</span
            ><strong>{{ uniqueTeams }}</strong>
          </div></ElCol
        ><ElCol :xs="12" :md="6"
          ><div class="award-metric">
            <span>{{ t('名单状态') }}</span
            ><strong :class="{ locked: awardSet?.status === 'FROZEN' }">{{
              awardSet ? (awardSet.status === 'FROZEN' ? t('已锁定') : t('草稿')) : t('未生成')
            }}</strong>
          </div></ElCol
        ></ElRow
      >
      <ElCard shadow="never" class="award-command-card"
        ><div class="award-command-bar">
          <div>
            <strong>{{ t('名单操作') }}</strong
            ><small>{{ t('只有已完成的正式 Resolver 可以生成奖项名单。') }}</small>
          </div>
          <ElSpace wrap :size="10"
            ><ElSelect v-model="resolverRunId" :placeholder="t('选择正式 Resolver')"
              ><ElOption
                v-for="item in completedRuns"
                :key="item.id"
                :value="item.id"
                :label="
                  t('运行 #{id} · {time}', { id: item.id, time: formatDateTime(item.completedAt) })
                " /></ElSelect
            ><ElButton
              type="primary"
              :icon="MagicStick"
              :disabled="!resolverRunId || !categories.length || awardSet?.status === 'FROZEN'"
              :loading="mutating"
              @click="generateAwards"
              >{{ t('生成名单') }}</ElButton
            ><ElButton
              v-if="awardSet?.status === 'DRAFT'"
              type="warning"
              :icon="Lock"
              :loading="mutating"
              @click="freezeAwards"
              >{{ t('锁定名单') }}</ElButton
            ><ElButton
              v-if="awardSet?.status === 'FROZEN'"
              :icon="Unlock"
              :loading="mutating"
              @click="unfreezeAwards"
              >{{ t('解除锁定') }}</ElButton
            ><ElButton
              :icon="Download"
              :disabled="!awardSet"
              :loading="exporting"
              @click="downloadCsv"
              >{{ t('导出名单') }}</ElButton
            ><ElButton
              :icon="Download"
              :disabled="awardSet?.status !== 'FROZEN'"
              :loading="exporting"
              @click="downloadCertificates"
              >{{ t('导出证书数据') }}</ElButton
            ></ElSpace
          >
        </div></ElCard
      >
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
          <ElCard shadow="never" class="award-categories-card"
            ><template #header
              ><div class="card-header">
                <div>
                  <strong>{{ t('奖项类别') }}</strong
                  ><small>{{ t('显示顺序必须唯一') }}</small>
                </div>
                <ElButton
                  type="primary"
                  :icon="Plus"
                  :disabled="awardSet?.status === 'FROZEN'"
                  @click="openCreate"
                  >{{ t('新增类别') }}</ElButton
                >
              </div></template
            ><ElEmpty v-if="!categories.length" :description="t('尚未配置奖项类别')" />
            <div v-else class="award-category-list">
              <article
                v-for="category in categories"
                :key="category.id"
                class="award-category-item"
              >
                <div class="award-order-controls">{{ category.displayOrder }}</div>
                <div class="award-category-content">
                  <div class="award-category-title">
                    <div>
                      <code>{{ category.code }}</code
                      ><strong>{{ category.name }}</strong>
                    </div>
                  </div>
                  <p>{{ ruleLabel(category) }}</p>
                  <div class="award-category-tags">
                    <ElTag v-if="category.firstBlood" type="danger">First Blood</ElTag
                    ><ElTag v-if="category.includeStar" type="warning">{{ t('含打星队') }}</ElTag
                    ><span>{{ t('{count} 支队伍', { count: recipientCount(category.id) }) }}</span>
                  </div>
                </div>
                <div class="award-category-actions">
                  <ElButton
                    link
                    type="primary"
                    :disabled="awardSet?.status === 'FROZEN'"
                    @click="openEdit(category)"
                    >{{ t('编辑') }}</ElButton
                  ><ElButton
                    link
                    type="danger"
                    :disabled="awardSet?.status === 'FROZEN'"
                    @click="deleteCategory(category)"
                    >{{ t('删除') }}</ElButton
                  >
                </div>
              </article>
            </div></ElCard
          >
        </ElCol>
        <ElCol :xs="24" :md="16">
          <ElCard shadow="never" class="award-recipients-card"
            ><template #header
              ><div class="card-header">
                <div>
                  <strong>{{ t('获奖名单') }}</strong
                  ><small>{{
                    t('{count} 条记录', { count: awardSet?.recipients.length ?? 0 })
                  }}</small>
                </div>
                <ElButton
                  :icon="Plus"
                  :disabled="!awardSet || awardSet.status === 'FROZEN'"
                  @click="openManual"
                  >{{ t('手工添加') }}</ElButton
                >
              </div></template
            ><ElTable :data="awardSet?.recipients ?? []" row-key="id"
              ><ElTableColumn :label="t('奖项')" min-width="150"
                ><template #default="{ row }"
                  ><ElTag effect="plain">{{ row.categoryCode }}</ElTag
                  ><span> {{ row.categoryName }}</span></template
                ></ElTableColumn
              ><ElTableColumn :label="t('队伍')" min-width="220"
                ><template #default="{ row }"
                  ><div class="award-team-cell">
                    <strong
                      >{{ row.teamName
                      }}<ElTag v-if="row.isManual" size="small" type="warning">{{
                        t('手工')
                      }}</ElTag></strong
                    ><span>{{ row.school ?? '—' }} · Team #{{ row.teamId }}</span>
                  </div></template
                ></ElTableColumn
              ><ElTableColumn prop="rank" :label="t('排名')" width="80" /><ElTableColumn
                prop="solved"
                :label="t('解题')"
                width="80" /><ElTableColumn
                prop="penaltyMinutes"
                :label="t('罚时')"
                width="90" /><ElTableColumn :label="t('操作')" width="90"
                ><template #default="{ row }"
                  ><ElButton
                    v-if="row.isManual"
                    link
                    type="danger"
                    :disabled="awardSet?.status === 'FROZEN'"
                    @click="removeRecipient(row)"
                    >{{ t('移除') }}</ElButton
                  ></template
                ></ElTableColumn
              ><template #empty><ElEmpty :description="t('尚未生成奖项名单')" /></template></ElTable
          ></ElCard>
        </ElCol>
      </ElRow>
      <ElDialog
        v-model="categoryVisible"
        :title="editing ? t('编辑奖项类别') : t('新增奖项类别')"
        width="min(720px, 94vw)"
        ><ElForm label-position="top"
          ><ElRow :gutter="16" class="award-form-grid"
            ><ElCol :xs="12" :sm="12"
              ><ElFormItem :label="t('代码')"
                ><ElInput v-model="categoryForm.code" maxlength="64" /></ElFormItem></ElCol
            ><ElCol :xs="12" :sm="12"
              ><ElFormItem :label="t('名称')"
                ><ElInput v-model="categoryForm.name" maxlength="128" /></ElFormItem></ElCol
            ><ElCol :xs="12" :sm="12"
              ><ElFormItem :label="t('显示顺序')"
                ><ElInputNumber
                  v-model="categoryForm.displayOrder"
                  :min="1"
                  :max="1000" /></ElFormItem></ElCol
            ><ElCol :xs="12" :sm="12"
              ><ElFormItem :label="t('参赛类型')"
                ><ElSelect v-model="categoryForm.participationType" clearable
                  ><ElOption :label="t('正式')" value="OFFICIAL" /><ElOption
                    :label="t('打星')"
                    value="STAR" /><ElOption
                    :label="t('练习')"
                    value="PRACTICE" /></ElSelect></ElFormItem></ElCol
            ><ElCol :xs="12" :sm="12"
              ><ElFormItem :label="t('组别')"
                ><ElInput v-model="categoryForm.groupName" clearable /></ElFormItem></ElCol
            ><ElCol :xs="12" :sm="12"
              ><ElFormItem :label="t('选项')"
                ><ElCheckbox v-model="categoryForm.includeStar">{{ t('允许打星队') }}</ElCheckbox
                ><ElCheckbox v-model="categoryForm.firstBlood">{{
                  t('First Blood 类别')
                }}</ElCheckbox></ElFormItem
              ></ElCol
            ></ElRow
          >
          <div class="award-rules-heading">
            <div>
              <strong>{{ t('获奖规则') }}</strong
              ><small>{{ t('First Blood 类别会选择最终快照中的 First Blood 队伍。') }}</small>
            </div>
          </div>
          <div class="award-rule-row">
            <ElSelect v-model="categoryForm.ruleType"
              ><ElOption :label="t('固定数量')" value="FIXED_COUNT" /><ElOption
                :label="t('排名比例')"
                value="RATIO" /><ElOption :label="t('名次范围')" value="RANK_RANGE" /></ElSelect
            ><ElInputNumber
              v-if="categoryForm.ruleType === 'FIXED_COUNT'"
              v-model="categoryForm.fixedCount"
              :min="1"
            /><ElInputNumber
              v-if="categoryForm.ruleType === 'RATIO'"
              v-model="categoryForm.ratio"
              :min="0.01"
              :max="1"
              :step="0.05"
            /><template v-if="categoryForm.ruleType === 'RANK_RANGE'"
              ><ElInputNumber v-model="categoryForm.rankFrom" :min="1" /><span>{{ t('至') }}</span
              ><ElInputNumber v-model="categoryForm.rankTo" :min="categoryForm.rankFrom"
            /></template></div></ElForm
        ><template #footer
          ><ElButton @click="categoryVisible = false">{{ t('取消') }}</ElButton
          ><ElButton type="primary" :loading="mutating" @click="saveCategory">{{
            t('保存')
          }}</ElButton></template
        ></ElDialog
      >
      <ElDialog v-model="manualVisible" :title="t('手工添加获奖队伍')" width="min(580px, 92vw)"
        ><ElForm label-position="top"
          ><ElFormItem :label="t('奖项类别')"
            ><ElSelect v-model="manualCategoryId" filterable
              ><ElOption
                v-for="category in categories"
                :key="category.id"
                :value="category.id"
                :label="category.name" /></ElSelect></ElFormItem
          ><ElFormItem :label="t('队伍')"
            ><ElSelect v-model="manualTeamId" filterable
              ><ElOption
                v-for="candidate in availableCandidates"
                :key="candidate.teamId"
                :value="candidate.teamId"
                :label="`#${candidate.rank} · ${candidate.teamName}`" /></ElSelect></ElFormItem></ElForm
        ><template #footer
          ><ElButton @click="manualVisible = false">{{ t('取消') }}</ElButton
          ><ElButton
            type="primary"
            :disabled="!manualCategoryId || !manualTeamId"
            :loading="mutating"
            @click="addRecipient"
            >{{ t('确认添加') }}</ElButton
          ></template
        ></ElDialog
      >
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Download, Lock, MagicStick, Plus, Refresh, Unlock } from '@element-plus/icons-vue';
import { useRoute, useRouter } from 'vue-router';
import {
  awardsApi,
  type AwardCategory,
  type AwardCategoryPayload,
  type AwardRecipient,
  type AwardResolverRun,
  type AwardSet,
} from '../api/awards';
import { ApiError, getErrorMessage } from '../api/client';
import { contestApi } from '../api/contest';
import type { Contest } from '../api/types';
import { formatDateTime } from '../utils/format';
import { useI18n } from '../i18n';
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
const categoryVisible = ref(false);
const editing = ref<AwardCategory | null>(null);
const manualVisible = ref(false);
const manualCategoryId = ref<number | null>(null);
const manualTeamId = ref<number | null>(null);
const categoryForm = reactive({
  code: '',
  name: '',
  displayOrder: 1,
  includeStar: false,
  groupName: '',
  participationType: null as AwardCategory['participationType'],
  firstBlood: false,
  ruleType: 'FIXED_COUNT' as AwardCategory['ruleType'],
  ratio: 0.1,
  fixedCount: 1,
  rankFrom: 1,
  rankTo: 1,
});
let generation = 0;
const uniqueTeams = computed(
  () => new Set(awardSet.value?.recipients.map((item) => item.teamId) ?? []).size,
);
const availableCandidates = computed(() =>
  candidates.value.filter(
    (candidate) =>
      !awardSet.value?.recipients.some(
        (recipient) =>
          recipient.categoryId === manualCategoryId.value && recipient.teamId === candidate.teamId,
      ),
  ),
);
function recipientCount(id: number) {
  return awardSet.value?.recipients.filter((item) => item.categoryId === id).length ?? 0;
}
function ruleLabel(category: AwardCategory) {
  if (category.firstBlood) return t('所有 First Blood 队伍');
  if (category.ruleType === 'FIXED_COUNT')
    return t('前 {count} 支符合条件的队伍', { count: category.fixedCount ?? 0 });
  if (category.ruleType === 'RATIO')
    return t('前 {ratio}%', { ratio: Math.round((category.ratio ?? 0) * 100) });
  return t('第 {from}–{to} 名', { from: category.rankFrom ?? 0, to: category.rankTo ?? 0 });
}
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
function resetForm() {
  Object.assign(categoryForm, {
    code: '',
    name: '',
    displayOrder: categories.value.length + 1,
    includeStar: false,
    groupName: '',
    participationType: null,
    firstBlood: false,
    ruleType: 'FIXED_COUNT',
    ratio: 0.1,
    fixedCount: 1,
    rankFrom: 1,
    rankTo: 1,
  });
}
function openCreate() {
  editing.value = null;
  resetForm();
  categoryVisible.value = true;
}
function openEdit(category: AwardCategory) {
  editing.value = category;
  Object.assign(categoryForm, {
    code: category.code,
    name: category.name,
    displayOrder: category.displayOrder,
    includeStar: category.includeStar,
    groupName: category.groupName ?? '',
    participationType: category.participationType,
    firstBlood: category.firstBlood,
    ruleType: category.ruleType,
    ratio: category.ratio ?? 0.1,
    fixedCount: category.fixedCount ?? 1,
    rankFrom: category.rankFrom ?? 1,
    rankTo: category.rankTo ?? 1,
  });
  categoryVisible.value = true;
}
function payload(): AwardCategoryPayload {
  return {
    code: categoryForm.code.trim().toUpperCase(),
    name: categoryForm.name.trim(),
    displayOrder: categoryForm.displayOrder,
    includeStar: categoryForm.includeStar,
    groupName: categoryForm.groupName.trim() || null,
    participationType: categoryForm.participationType,
    firstBlood: categoryForm.firstBlood,
    rule: {
      ruleType: categoryForm.ruleType,
      ratio: categoryForm.ruleType === 'RATIO' ? categoryForm.ratio : null,
      fixedCount: categoryForm.ruleType === 'FIXED_COUNT' ? categoryForm.fixedCount : null,
      rankFrom: categoryForm.ruleType === 'RANK_RANGE' ? categoryForm.rankFrom : null,
      rankTo: categoryForm.ruleType === 'RANK_RANGE' ? categoryForm.rankTo : null,
    },
  };
}
async function saveCategory() {
  const data = payload();
  if (!contestId.value || !data.code || !data.name) {
    ElMessage.warning(t('请填写奖项代码和名称'));
    return;
  }
  mutating.value = true;
  try {
    const saved = editing.value
      ? await awardsApi.updateCategory(editing.value.id, editing.value.version, data)
      : await awardsApi.createCategory(contestId.value, data);
    categories.value = [...categories.value.filter((item) => item.id !== saved.id), saved].sort(
      (a, b) => a.displayOrder - b.displayOrder,
    );
    categoryVisible.value = false;
    ElMessage.success(t('奖项类别已保存'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    mutating.value = false;
  }
}
async function deleteCategory(category: AwardCategory) {
  try {
    await ElMessageBox.confirm(
      t('删除“{name}”会同时移除该类别的获奖记录，是否继续？', { name: category.name }),
      t('删除奖项类别'),
      { type: 'warning' },
    );
  } catch {
    return;
  }
  mutating.value = true;
  try {
    await awardsApi.deleteCategory(category.id, category.version);
    await loadWorkspace();
    ElMessage.success(t('奖项类别已删除'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    mutating.value = false;
  }
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
function openManual() {
  manualCategoryId.value = categories.value[0]?.id ?? null;
  manualTeamId.value = null;
  manualVisible.value = true;
}
async function addRecipient() {
  if (!contestId.value || !awardSet.value || !manualCategoryId.value || !manualTeamId.value) return;
  mutating.value = true;
  try {
    awardSet.value = await awardsApi.addRecipient(
      contestId.value,
      manualCategoryId.value,
      manualTeamId.value,
      awardSet.value.version,
    );
    manualVisible.value = false;
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    mutating.value = false;
  }
}
async function removeRecipient(value: unknown) {
  const recipient = value as AwardRecipient;
  if (!awardSet.value) return;
  mutating.value = true;
  try {
    awardSet.value = await awardsApi.removeRecipient(recipient.id, awardSet.value.version);
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

.awards-page-header > div > p:last-child,
.award-command-bar small {
  display: none;
  margin: 4px 0 0;
  color: var(--muted);
}

.page-body {
  padding: 0 38px 38px;
}

.award-command-bar,
.award-category-title,
.award-category-tags,
.award-rules-heading {
  display: flex;
  align-items: center;
  gap: 10px;
}

.award-metrics {
  margin-bottom: 18px;
}

.award-metric {
  padding: 20px 22px;
  border: 1px solid #e6eaf0;
  border-radius: 0;
  background: white;
}

.award-metric span,
.award-metric strong {
  display: block;
}

.award-metric span {
  margin-bottom: 7px;
  color: var(--muted);
  font-size: 13px;
}

.award-metric strong {
  color: #172033;
  font-size: 24px;
}

.award-metric strong.locked {
  color: #b45309;
}

.award-command-card {
  margin-bottom: 18px;
  border-color: #eadfca;
  background: #fffbeb;
}

.award-command-bar {
  justify-content: space-between;
}

.award-warning-stack {
  display: grid;
  gap: 10px;
  margin-bottom: 18px;
}

.awards-workspace > .el-col {
  align-self: flex-start;
}

.award-categories-card,
.award-recipients-card {
  border: 1px solid #e5eaf2;
  border-radius: 0;
}

.award-categories-card {
  position: sticky;
  top: 20px;
}

.card-header > div {
  min-width: 0;
}

.award-category-list {
  display: grid;
  gap: 10px;
}

.award-category-item {
  display: grid;
  grid-template-columns: 32px minmax(0, 1fr) auto;
  align-items: center;
  gap: 12px;
  padding: 14px;
  border: 1px solid #e7ebf1;
  border-radius: 0;
  background: #fbfcfe;
}

.award-order-controls {
  display: grid;
  justify-items: center;
  color: var(--muted);
  font-size: 12px;
}

.award-order-controls .el-button {
  width: 26px;
  height: 24px;
  padding: 0;
}

.award-category-content {
  min-width: 0;
}

.award-category-title {
  justify-content: space-between;
}

.award-category-title > div {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 8px;
}

.award-category-title code {
  border-radius: 0;
  padding: 2px 6px;
  color: #92400e;
  background: #fef3c7;
  font-size: 11px;
}

.award-category-content p {
  margin: 7px 0;
  color: #475569;
  font-size: 13px;
}

.award-category-tags {
  flex-wrap: wrap;
}

.award-category-tags > span:last-child {
  color: var(--muted);
  font-size: 12px;
}

.award-category-actions {
  display: grid;
}

.award-team-cell strong,
.award-team-cell > span {
  display: block;
}

.award-team-cell > span {
  margin-top: 5px;
  color: var(--muted);
  font-size: 12px;
}

.award-team-cell .el-tag {
  margin-left: 5px;
}

.award-form-grid .el-select {
  width: 100%;
}

.award-rules-heading {
  justify-content: space-between;
  margin: 8px 0 12px;
}

.award-rules-heading small {
  display: block;
  margin-top: 3px;
  color: var(--muted);
}

.award-rule-row {
  display: flex;
  min-height: 54px;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
  padding: 10px;
  border-radius: 0;
  background: #f6f8fb;
}

@media (max-width: 1180px) {
  .awards-workspace > .el-col {
    flex: 0 0 100%;
    max-width: 100%;
  }

  .award-categories-card {
    position: static;
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

  .award-command-bar {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
