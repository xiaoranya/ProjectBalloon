<template>
  <ElCard shadow="never" class="rejudge-filter-card">
    <template #header>
      <div class="card-header">
        <div>
          <strong>{{ t('1. 筛选与预览') }}</strong>
          <small>{{ t('比赛范围由 URL 固定；全部筛选均可留空。') }}</small>
        </div>
      </div>
    </template>

    <ElForm label-position="top">
      <ElRow :gutter="14" class="rejudge-filter-grid-row">
        <ElCol :xs="24" :sm="12" :md="6">
          <ElFormItem :label="t('题目')">
            <ElSelect v-model="filter.problemId" clearable filterable :placeholder="t('全部题目')">
              <ElOption
                v-for="problem in contestProblems"
                :key="problem.problemId"
                :label="`${problem.alias} · ${problem.title}`"
                :value="problem.problemId"
              />
            </ElSelect>
          </ElFormItem>
        </ElCol>
        <ElCol :xs="24" :sm="12" :md="6">
          <ElFormItem :label="t('队伍')">
            <ElSelect v-model="filter.teamId" clearable filterable :placeholder="t('全部队伍')">
              <ElOption
                v-for="team in contestTeams"
                :key="team.teamId"
                :label="`${team.teamName} (#${team.teamId})`"
                :value="team.teamId"
              />
            </ElSelect>
          </ElFormItem>
        </ElCol>
        <ElCol :xs="24" :sm="12" :md="6">
          <ElFormItem :label="t('语言')">
            <ElSelect v-model="filter.language" clearable :placeholder="t('全部语言')">
              <ElOption label="C" value="c" />
              <ElOption label="C++" value="cpp" />
              <ElOption label="Java" value="java" />
              <ElOption label="Go" value="go" />
              <ElOption label="Rust" value="rust" />
              <ElOption label="Python" value="python" />
            </ElSelect>
          </ElFormItem>
        </ElCol>
        <ElCol :xs="24" :sm="12" :md="6">
          <ElFormItem :label="t('当前判罚')">
            <ElSelect v-model="filter.verdict" clearable :placeholder="t('全部最终判罚')">
              <ElOption
                v-for="option in verdictOptions"
                :key="option.value"
                :label="option.label"
                :value="option.value"
              />
            </ElSelect>
          </ElFormItem>
        </ElCol>
        <ElCol :xs="24" :sm="12" :md="12">
          <ElFormItem :label="t('提交时间')">
            <ElDatePicker
              v-model="filter.submittedRange"
              type="datetimerange"
              :start-placeholder="t('起始时间')"
              :end-placeholder="t('结束时间')"
              :range-separator="t('至')"
            />
          </ElFormItem>
        </ElCol>
      </ElRow>
    </ElForm>

    <ElSpace wrap :size="14" class="rejudge-preview-actions-row">
      <ElButton type="primary" plain :loading="previewing" @click="emit('preview')">
        {{ t('预览影响范围') }}
      </ElButton>
      <span class="muted-text">{{ t('预览只统计，不创建判题任务。') }}</span>
    </ElSpace>

    <ElAlert
      v-if="previewResult && previewStale"
      class="rejudge-inline-alert"
      type="warning"
      show-icon
      :closable="false"
      :title="t('筛选条件已变化，当前预览已失效，请重新预览。')"
    />
  </ElCard>
</template>

<script setup lang="ts">
import type { BatchRejudgePreview, BatchRejudgeVerdict } from '../../api/bulk-rejudge';
import type { ContestProblem, ContestTeamResponse } from '../../api/types';
import { useI18n } from '../../i18n';

interface RejudgeFilterState {
  problemId: number | null;
  teamId: number | null;
  language: string | null;
  verdict: BatchRejudgeVerdict | null;
  submittedRange: [Date, Date] | null;
}

const emit = defineEmits<{ preview: [] }>();
const filter = defineModel<RejudgeFilterState>('filter', { required: true });

defineProps<{
  contestProblems: ContestProblem[];
  contestTeams: ContestTeamResponse[];
  verdictOptions: { value: BatchRejudgeVerdict; label: string }[];
  previewing: boolean;
  previewResult: BatchRejudgePreview | null;
  previewStale: boolean;
}>();

const { t } = useI18n();
</script>

<style scoped>
.card-header > div {
  min-width: 0;
}
.card-header small {
  display: block;
  margin: 4px 0 0;
  color: var(--muted);
}
.rejudge-filter-grid-row :deep(.el-select),
.rejudge-filter-grid-row :deep(.el-date-editor) {
  width: 100%;
}
.rejudge-preview-actions-row {
  margin-top: 2px;
}
.muted-text {
  color: var(--muted);
}
</style>
