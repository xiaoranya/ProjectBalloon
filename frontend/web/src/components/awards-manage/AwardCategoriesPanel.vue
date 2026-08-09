<template>
  <ElCard shadow="never" class="award-categories-card">
    <template #header>
      <div class="card-header">
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
      </div>
    </template>
    <ElEmpty v-if="!categories.length" :description="t('尚未配置奖项类别')" />
    <div v-else class="award-category-list">
      <article v-for="category in categories" :key="category.id" class="award-category-item">
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
    </div>
  </ElCard>

  <ElDialog
    v-model="categoryVisible"
    :title="editing ? t('编辑奖项类别') : t('新增奖项类别')"
    width="min(720px, 94vw)"
  >
    <ElForm label-position="top">
      <ElRow :gutter="16" class="award-form-grid">
        <ElCol :xs="12" :sm="12">
          <ElFormItem :label="t('代码')"
            ><ElInput v-model="categoryForm.code" maxlength="64"
          /></ElFormItem>
        </ElCol>
        <ElCol :xs="12" :sm="12">
          <ElFormItem :label="t('名称')"
            ><ElInput v-model="categoryForm.name" maxlength="128"
          /></ElFormItem>
        </ElCol>
        <ElCol :xs="12" :sm="12">
          <ElFormItem :label="t('显示顺序')"
            ><ElInputNumber v-model="categoryForm.displayOrder" :min="1" :max="1000"
          /></ElFormItem>
        </ElCol>
        <ElCol :xs="12" :sm="12">
          <ElFormItem :label="t('参赛类型')"
            ><ElSelect v-model="categoryForm.participationType" clearable>
              <ElOption :label="t('正式')" value="OFFICIAL" /><ElOption
                :label="t('打星')"
                value="STAR" /><ElOption :label="t('练习')" value="PRACTICE" /></ElSelect
          ></ElFormItem>
        </ElCol>
        <ElCol :xs="12" :sm="12">
          <ElFormItem :label="t('组别')"
            ><ElInput v-model="categoryForm.groupName" clearable
          /></ElFormItem>
        </ElCol>
        <ElCol :xs="12" :sm="12">
          <ElFormItem :label="t('选项')"
            ><ElCheckbox v-model="categoryForm.includeStar">{{ t('允许打星队') }}</ElCheckbox
            ><ElCheckbox v-model="categoryForm.firstBlood">{{
              t('First Blood 类别')
            }}</ElCheckbox></ElFormItem
          >
        </ElCol>
      </ElRow>
      <div class="award-rules-heading">
        <div>
          <strong>{{ t('获奖规则') }}</strong
          ><small>{{ t('First Blood 类别会选择最终快照中的 First Blood 队伍。') }}</small>
        </div>
      </div>
      <div class="award-rule-row">
        <ElSelect v-model="categoryForm.ruleType">
          <ElOption :label="t('固定数量')" value="FIXED_COUNT" /><ElOption
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
        /></template>
      </div>
    </ElForm>
    <template #footer>
      <ElButton @click="categoryVisible = false">{{ t('取消') }}</ElButton>
      <ElButton type="primary" :loading="saving" @click="saveCategory">{{ t('保存') }}</ElButton>
    </template>
  </ElDialog>
</template>

<script setup lang="ts">
import { reactive, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Plus } from '@element-plus/icons-vue';
import {
  awardsApi,
  type AwardCategory,
  type AwardCategoryPayload,
  type AwardSet,
} from '../../api/awards';
import { getErrorMessage } from '../../api/client';
import { useI18n } from '../../i18n';

const props = defineProps<{
  contestId: number | null;
  categories: AwardCategory[];
  awardSet: AwardSet | null;
}>();
const emit = defineEmits<{
  'update:categories': [value: AwardCategory[]];
  refresh: [];
}>();
const { t } = useI18n();

const categoryVisible = ref(false);
const editing = ref<AwardCategory | null>(null);
const saving = ref(false);
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

function recipientCount(id: number) {
  return props.awardSet?.recipients.filter((item) => item.categoryId === id).length ?? 0;
}
function ruleLabel(category: AwardCategory) {
  if (category.firstBlood) return t('所有 First Blood 队伍');
  if (category.ruleType === 'FIXED_COUNT')
    return t('前 {count} 支符合条件的队伍', { count: category.fixedCount ?? 0 });
  if (category.ruleType === 'RATIO')
    return t('前 {ratio}%', { ratio: Math.round((category.ratio ?? 0) * 100) });
  return t('第 {from}–{to} 名', { from: category.rankFrom ?? 0, to: category.rankTo ?? 0 });
}

function resetForm() {
  Object.assign(categoryForm, {
    code: '',
    name: '',
    displayOrder: props.categories.length + 1,
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
  if (!props.contestId || !data.code || !data.name) {
    ElMessage.warning(t('请填写奖项代码和名称'));
    return;
  }
  saving.value = true;
  try {
    const saved = editing.value
      ? await awardsApi.updateCategory(editing.value.id, editing.value.version, data)
      : await awardsApi.createCategory(props.contestId, data);
    emit(
      'update:categories',
      [...props.categories.filter((item) => item.id !== saved.id), saved].sort(
        (a, b) => a.displayOrder - b.displayOrder,
      ),
    );
    categoryVisible.value = false;
    ElMessage.success(t('奖项类别已保存'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    saving.value = false;
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
  saving.value = true;
  try {
    await awardsApi.deleteCategory(category.id, category.version);
    emit('refresh');
    ElMessage.success(t('奖项类别已删除'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    saving.value = false;
  }
}
</script>

<style scoped>
.card-header > div {
  min-width: 0;
}
.award-categories-card {
  position: sticky;
  top: 20px;
  border: 1px solid #e5eaf2;
  border-radius: 0;
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
.award-category-title,
.award-category-tags {
  display: flex;
  align-items: center;
  gap: 10px;
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
.award-form-grid .el-select {
  width: 100%;
}
.award-rules-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
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
  .award-categories-card {
    position: static;
  }
}
</style>
