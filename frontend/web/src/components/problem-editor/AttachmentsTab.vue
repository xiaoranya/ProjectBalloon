<template>
  <ElCard shadow="never" class="problem-editor-card">
    <template #header
      ><strong>{{ t('题目附件') }}</strong></template
    >
    <ElSpace wrap :size="14" class="file-upload-row">
      <ElSelect v-model="attachmentKind" :aria-label="t('附件类型')">
        <ElOption :label="t('样例附件')" value="SAMPLE" />
        <ElOption :label="t('补充材料')" value="SUPPLEMENT" />
      </ElSelect>
      <input ref="attachmentInput" type="file" @change="selectAttachment" />
      <ElButton
        type="primary"
        :disabled="!attachmentFile"
        :loading="uploadingAttachment"
        @click="uploadAttachment"
        >{{ t('上传附件') }}</ElButton
      >
    </ElSpace>
    <ElTable :data="attachments" row-key="id">
      <ElTableColumn prop="originalFilename" :label="t('文件名')" min-width="240" />
      <ElTableColumn prop="kind" :label="t('类型')" width="130" />
      <ElTableColumn :label="t('大小')" width="130"
        ><template #default="{ row }">{{ formatBytes(row.bytes) }}</template></ElTableColumn
      >
      <ElTableColumn :label="t('操作')" width="170">
        <template #default="{ row }">
          <ElButton
            link
            type="primary"
            @click="downloadAttachment(row as ProblemAttachmentResponse)"
            >{{ t('下载') }}</ElButton
          >
          <ElButton
            link
            type="danger"
            @click="removeAttachment(row as ProblemAttachmentResponse)"
            >{{ t('删除') }}</ElButton
          >
        </template>
      </ElTableColumn>
      <template #empty><ElEmpty :description="t('尚未上传附件')" /></template>
    </ElTable>
  </ElCard>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { adminProblemApi } from '../../api/admin-problems';
import { getErrorMessage } from '../../api/client';
import type {
  ProblemResponse,
  ProblemAttachmentResponse,
  ProblemAttachmentKind,
} from '../../api/types';
import { formatBytes } from '../../utils/format';
import { useI18n } from '../../i18n';

const props = defineProps<{
  problem: ProblemResponse | null;
  initialAttachments: ProblemAttachmentResponse[];
}>();
const emit = defineEmits<{
  'problem-refreshed': [value: ProblemResponse | null];
  'error-message': [message: string];
}>();
const { t } = useI18n();

const attachments = ref<ProblemAttachmentResponse[]>([]);
const attachmentKind = ref<ProblemAttachmentKind>('SAMPLE');
const attachmentFile = ref<File | null>(null);
const attachmentInput = ref<HTMLInputElement>();
const uploadingAttachment = ref(false);

watch(
  () => props.initialAttachments,
  (value) => {
    attachments.value = value;
  },
  { immediate: true },
);

function selectAttachment(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0] ?? null;
  if (file && (file.size === 0 || file.size > 20 * 1024 * 1024)) {
    ElMessage.error(t('附件必须大于 0 字节且不能超过 20 MiB'));
    attachmentFile.value = null;
    (event.target as HTMLInputElement).value = '';
    return;
  }
  attachmentFile.value = file;
}

async function uploadAttachment() {
  if (!props.problem || !attachmentFile.value) return;
  uploadingAttachment.value = true;
  try {
    const refreshed = await adminProblemApi.uploadAttachment(
      props.problem.id,
      attachmentKind.value,
      attachmentFile.value,
    );
    attachments.value.push(refreshed.result);
    emit('problem-refreshed', refreshed.problem);
    attachmentFile.value = null;
    if (attachmentInput.value) attachmentInput.value.value = '';
    ElMessage.success(t('附件已上传'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    uploadingAttachment.value = false;
  }
}

async function removeAttachment(attachment: ProblemAttachmentResponse) {
  if (!props.problem) return;
  try {
    await ElMessageBox.confirm(
      t('确认删除附件“{name}”？', { name: attachment.originalFilename }),
      t('删除附件'),
      {
        type: 'warning',
      },
    );
    const refreshed = await adminProblemApi.deleteAttachment(props.problem.id, attachment.id);
    emit('problem-refreshed', refreshed.problem);
    attachments.value = attachments.value.filter((item) => item.id !== attachment.id);
    ElMessage.success(t('附件已删除'));
  } catch (error) {
    if (error !== 'cancel' && error !== 'close') ElMessage.error(getErrorMessage(error));
  }
}

async function downloadAttachment(attachment: ProblemAttachmentResponse) {
  if (!props.problem) return;
  try {
    downloadBlob(
      await adminProblemApi.downloadAttachment(props.problem.id, attachment.id),
      attachment.originalFilename,
    );
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}
</script>

<style scoped>
.problem-editor-card {
  margin-bottom: 20px;
}
.file-upload-row {
  width: 100%;
  flex-wrap: wrap;
  margin-bottom: 24px;
}
.file-upload-row :deep(.el-select) {
  width: 160px;
}
@media (max-width: 680px) {
  .file-upload-row {
    align-items: stretch;
    flex-direction: column;
  }
  .file-upload-row :deep(.el-select) {
    width: 100%;
  }
}
</style>
