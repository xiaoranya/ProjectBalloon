<template>
  <ElCard shadow="never" class="problem-editor-card">
    <template #header
      ><strong>{{ t('当前测试数据版本') }}</strong></template
    >
    <ElAlert
      :title="
        t(
          '每次上传都会生成不可变版本。可下载任意历史版本，或在题目尚未用于已冻结比赛时重新激活旧版本。',
        )
      "
      type="info"
      show-icon
      :closable="false"
      class="form-alert"
    />
    <ElRow :gutter="14" justify="space-between" class="testdata-summary">
      <ElCol :xs="24" :sm="7">
        <div class="testdata-stat">
          <span>{{ t('当前版本') }}</span>
          <strong>v{{ problem?.testdataVersion ?? 0 }}</strong>
        </div>
      </ElCol>
      <ElCol :xs="24" :sm="9">
        <div class="testdata-stat">
          <span>SHA-256</span>
          <code>{{ problem?.testdataSha256 ?? t('尚未上传') }}</code>
        </div>
      </ElCol>
      <ElCol :xs="24" :sm="8">
        <ElButton :disabled="!problem?.testdataVersion" @click="downloadTestdata">{{
          t('下载当前 ZIP')
        }}</ElButton>
      </ElCol>
    </ElRow>
    <ElSpace wrap :size="14" class="file-upload-row">
      <input
        ref="testdataInput"
        type="file"
        accept=".zip,application/zip"
        @change="selectTestdata"
      />
      <ElButton
        type="primary"
        :disabled="!testdataFile"
        :loading="uploadingTestdata"
        @click="uploadTestdata"
        >{{ t('上传新版本') }}</ElButton
      >
    </ElSpace>
    <ElTable :data="testdataVersions" row-key="version">
      <ElTableColumn :label="t('版本')" width="130">
        <template #default="{ row }">
          <strong>v{{ row.version }}</strong>
          <ElTag v-if="row.active" size="small" type="success">{{ t('当前') }}</ElTag>
        </template>
      </ElTableColumn>
      <ElTableColumn prop="caseCount" :label="t('用例数')" width="120" />
      <ElTableColumn :label="t('压缩包大小')" width="150"
        ><template #default="{ row }">{{
          row.bytes == null ? '—' : formatBytes(row.bytes)
        }}</template></ElTableColumn
      >
      <ElTableColumn prop="sha256" label="SHA-256" min-width="300" show-overflow-tooltip />
      <ElTableColumn :label="t('操作')" width="180">
        <template #default="{ row }">
          <ElButton link @click="downloadTestdataVersion(row.version)">{{ t('下载') }}</ElButton>
          <ElButton
            link
            type="primary"
            :disabled="row.active"
            :loading="activatingVersion === row.version"
            @click="activateTestdataVersion(row.version)"
            >{{ t('激活') }}</ElButton
          >
        </template>
      </ElTableColumn>
      <template #empty><ElEmpty :description="t('尚未上传测试数据')" /></template>
    </ElTable>
  </ElCard>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { adminProblemApi } from '../../api/admin-problems';
import { getErrorMessage } from '../../api/client';
import type { ProblemResponse, ProblemTestdataVersionResponse } from '../../api/types';
import { formatBytes } from '../../utils/format';
import { useI18n } from '../../i18n';

const props = defineProps<{
  problem: ProblemResponse | null;
  initialTestdataVersions: ProblemTestdataVersionResponse[];
}>();
const emit = defineEmits<{
  'problem-refreshed': [value: ProblemResponse | null];
  'error-message': [message: string];
}>();
const { t } = useI18n();

const testdataVersions = ref<ProblemTestdataVersionResponse[]>([]);
const testdataFile = ref<File | null>(null);
const testdataInput = ref<HTMLInputElement>();
const uploadingTestdata = ref(false);
const activatingVersion = ref<number | null>(null);

watch(
  () => props.initialTestdataVersions,
  (value) => {
    testdataVersions.value = value;
  },
  { immediate: true },
);

function selectTestdata(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0] ?? null;
  if (
    file &&
    (!file.name.toLowerCase().endsWith('.zip') || file.size === 0 || file.size > 256 * 1024 * 1024)
  ) {
    ElMessage.error(t('测试数据必须是大于 0 字节且不超过 256 MiB 的 ZIP 文件'));
    testdataFile.value = null;
    (event.target as HTMLInputElement).value = '';
    return;
  }
  testdataFile.value = file;
}

async function uploadTestdata() {
  if (!props.problem || !testdataFile.value) return;
  const id = props.problem.id;
  try {
    await ElMessageBox.confirm(
      t('确认上传“{name}”并创建新的当前测试数据版本？', { name: testdataFile.value.name }),
      t('上传测试数据'),
      { type: 'warning', confirmButtonText: t('确认上传') },
    );
  } catch {
    return;
  }
  uploadingTestdata.value = true;
  try {
    const refreshed = await adminProblemApi.uploadTestdata(id, testdataFile.value);
    emit('problem-refreshed', refreshed.problem);
    testdataVersions.value = await adminProblemApi.listTestdataVersions(id);
    testdataFile.value = null;
    if (testdataInput.value) testdataInput.value.value = '';
    ElMessage.success(t('测试数据 v{version} 已上传', { version: refreshed.result.version }));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    uploadingTestdata.value = false;
  }
}

async function downloadTestdata() {
  if (!props.problem || props.problem.testdataVersion === 0) return;
  try {
    const blob = await adminProblemApi.downloadTestdata(props.problem.id);
    downloadBlob(
      blob,
      `problem-${props.problem.id}-testdata-v${props.problem.testdataVersion}.zip`,
    );
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}

async function downloadTestdataVersion(version: number) {
  if (!props.problem) return;
  try {
    const blob = await adminProblemApi.downloadTestdataVersion(props.problem.id, version);
    downloadBlob(blob, `problem-${props.problem.id}-testdata-v${version}.zip`);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}

async function activateTestdataVersion(version: number) {
  if (!props.problem || version === props.problem.testdataVersion) return;
  const id = props.problem.id;
  const expectedCurrentVersion = props.problem.testdataVersion;
  try {
    await ElMessageBox.confirm(
      t('确认将测试数据 v{version} 设为当前版本？后续提交将使用该版本。', { version }),
      t('激活历史版本'),
      { type: 'warning', confirmButtonText: t('确认激活') },
    );
  } catch {
    return;
  }
  activatingVersion.value = version;
  try {
    const refreshed = await adminProblemApi.activateTestdataVersion(
      id,
      version,
      expectedCurrentVersion,
    );
    emit('problem-refreshed', refreshed.problem);
    testdataVersions.value = await adminProblemApi.listTestdataVersions(id);
    ElMessage.success(t('测试数据 v{version} 已激活', { version }));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    activatingVersion.value = null;
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
.form-alert {
  margin-bottom: 20px;
}
.testdata-summary {
  margin-bottom: 24px;
}
.testdata-summary .el-col {
  margin-bottom: 14px;
}
.testdata-summary .testdata-stat {
  display: grid;
  min-width: 140px;
  gap: 5px;
}
.testdata-summary .el-col:last-child {
  display: flex;
  justify-content: flex-end;
}
.testdata-summary span {
  color: var(--muted);
  font-size: 13px;
}
.testdata-summary code {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.file-upload-row {
  width: 100%;
  flex-wrap: wrap;
  margin-bottom: 24px;
}
</style>
