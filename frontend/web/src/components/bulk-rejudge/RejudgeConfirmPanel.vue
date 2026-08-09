<template>
  <ElCard shadow="never" class="rejudge-confirm-card">
    <template #header>
      <div class="card-header">
        <div>
          <strong>{{ t('2. 确认创建') }}</strong>
          <small>{{ t('数量快照与幂等键共同保护重复操作。') }}</small>
        </div>
      </div>
    </template>

    <ElEmpty v-if="!previewResult" :description="t('先预览筛选结果')" :image-size="76" />
    <template v-else>
      <div class="rejudge-preview-count" aria-live="polite">
        <span>{{ t('匹配提交') }}</span>
        <strong>{{ previewResult.matchedSubmissions }}</strong>
        <small>{{ t('Rust 创建上限为 10,000 条') }}</small>
      </div>

      <ElAlert
        v-if="previewResult.matchedSubmissions === 0"
        class="rejudge-inline-alert"
        type="info"
        show-icon
        :closable="false"
        :title="t('当前筛选没有可重判的已完成提交。')"
      />
      <ElAlert
        v-else-if="previewResult.matchedSubmissions > 10_000"
        class="rejudge-inline-alert"
        type="warning"
        show-icon
        :closable="false"
        :title="t('匹配数量超过单任务上限，请缩小筛选范围。')"
      />

      <ElForm label-position="top" class="rejudge-confirm-form">
        <ElFormItem :label="t('幂等键')">
          <ElInput v-model="idempotencyKey" maxlength="128" show-word-limit>
            <template #append>
              <ElButton
                :icon="Refresh"
                :aria-label="t('生成新幂等键')"
                @click="emit('regenerate-key')"
              />
            </template>
          </ElInput>
          <p class="form-help">
            {{ t('需为 8–128 字节。网络重试时保留同一键；只在发起一项新操作时更换。') }}
          </p>
        </ElFormItem>
        <ElFormItem
          :label="t('输入 {text} 以继续', { text: confirmationRequirement || t('确认文本') })"
        >
          <ElInput
            v-model="confirmationText"
            autocomplete="off"
            :placeholder="confirmationRequirement"
          />
        </ElFormItem>
      </ElForm>

      <ElAlert
        class="rejudge-inline-alert"
        type="warning"
        show-icon
        :closable="false"
        :title="t('重判会让匹配提交重新进入评测队列，并可能短暂改变榜单；任务不能整体撤销。')"
      />
      <ElButton
        type="danger"
        class="wide-button"
        :loading="creating"
        :disabled="!canCreate"
        @click="emit('create')"
      >
        {{ t('创建批量重判任务') }}
      </ElButton>
    </template>
  </ElCard>
</template>

<script setup lang="ts">
import { Refresh } from '@element-plus/icons-vue';
import type { BatchRejudgePreview } from '../../api/bulk-rejudge';
import { useI18n } from '../../i18n';

const emit = defineEmits<{ create: []; 'regenerate-key': [] }>();
const idempotencyKey = defineModel<string>('idempotencyKey', { required: true });
const confirmationText = defineModel<string>('confirmationText', { required: true });

defineProps<{
  previewResult: BatchRejudgePreview | null;
  creating: boolean;
  canCreate: boolean;
  confirmationRequirement: string;
}>();

const { t } = useI18n();
</script>

<style scoped>
.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.card-header > div {
  min-width: 0;
}
.card-header small {
  display: block;
  margin: 4px 0 0;
  color: var(--muted);
}
.rejudge-preview-count {
  display: grid;
  place-items: center;
  margin: 2px 0 18px;
  padding: 22px;
  border: 1px solid #dbeafe;
  border-radius: 0;
  background: #f7faff;
  text-align: center;
}
.rejudge-preview-count span,
.rejudge-preview-count small {
  color: var(--muted);
  font-size: 12px;
}
.rejudge-preview-count strong {
  margin: 4px 0;
  color: #172033;
  font-size: 42px;
  font-variant-numeric: tabular-nums;
  line-height: 1;
}
.rejudge-confirm-form {
  display: grid;
  gap: 14px;
}
.rejudge-confirm-form :deep(.el-form-item:last-child) {
  margin-bottom: 0;
}
.form-help {
  color: var(--el-text-color-secondary);
  font-size: 0.8rem;
}
.wide-button {
  width: 100%;
}
</style>
