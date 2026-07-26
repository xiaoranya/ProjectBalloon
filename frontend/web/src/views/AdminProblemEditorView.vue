<template>
  <section class="admin-page problem-editor-page">
    <header class="admin-page-header compact">
      <div>
        <ElButton link :icon="ArrowLeft" @click="backToList">返回题库</ElButton>
        <p class="eyebrow">Problem Editor</p>
        <h1>{{ isNew ? '创建题目' : problem?.title ?? '题目编辑' }}</h1>
        <p>{{ isNew ? '先保存基本信息，再维护题面和文件。' : `${problem?.slug ?? ''} · 数据版本 v${problem?.testdataVersion ?? 0}` }}</p>
      </div>
      <ElButton type="primary" :loading="saving" @click="saveProblem">{{ isNew ? '创建题目' : '保存基本信息' }}</ElButton>
    </header>

    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />

    <ElSkeleton v-if="loading" :rows="8" animated />
    <ElTabs v-else v-model="activeTab" class="admin-tabs">
      <ElTabPane label="基本信息" name="basic">
        <ElCard shadow="never" class="admin-card problem-editor-card">
          <ElForm ref="formRef" :model="form" :rules="rules" label-position="top">
            <div class="admin-two-column">
              <ElFormItem label="题目标识" prop="slug">
                <ElInput v-model="form.slug" maxlength="64" placeholder="例如 two-sum" />
              </ElFormItem>
              <ElFormItem label="题目标题" prop="title">
                <ElInput v-model="form.title" maxlength="255" />
              </ElFormItem>
            </div>
            <ElFormItem label="允许的提交语言" prop="languages">
              <ElCheckboxGroup v-model="form.languages">
                <ElCheckboxButton v-for="option in languageOptions" :key="option.value" :value="option.value">
                  {{ option.label }}
                </ElCheckboxButton>
              </ElCheckboxGroup>
            </ElFormItem>
            <div class="admin-form-grid problem-limit-grid">
              <ElFormItem label="时间限制（ms）" prop="timeLimitMs">
                <ElInputNumber v-model="form.timeLimitMs" :min="1" :max="60000" controls-position="right" />
              </ElFormItem>
              <ElFormItem label="内存限制（MiB）" prop="memoryLimitMb">
                <ElInputNumber v-model="form.memoryLimitMb" :min="16" :max="8192" controls-position="right" />
              </ElFormItem>
              <ElFormItem label="输出限制（KiB）" prop="outputLimitKb">
                <ElInputNumber v-model="form.outputLimitKb" :min="1" :max="262144" controls-position="right" />
              </ElFormItem>
            </div>
            <ElFormItem label="默认题面语言" prop="defaultLangCode">
              <ElInput v-model="form.defaultLangCode" maxlength="5" placeholder="en 或 zh-CN" class="problem-lang-code" />
            </ElFormItem>
            <ElAlert v-if="problem" type="info" :closable="false" show-icon>
              当前并发版本为 {{ problem.version }}；保存时会通过 expectedVersion 防止覆盖其他管理员的修改。
            </ElAlert>
          </ElForm>
        </ElCard>
      </ElTabPane>

      <ElTabPane label="多语言题面" name="statements" :disabled="isNew">
        <ElCard shadow="never" class="admin-card problem-editor-card">
          <template #header>
            <div class="problem-card-heading">
              <div><strong>题面编辑</strong><small>Rust 后端按语言代码 upsert Markdown 题面。</small></div>
              <ElButton :icon="Plus" @click="addStatementDraft">添加语言</ElButton>
            </div>
          </template>
          <ElEmpty v-if="statementDrafts.length === 0" description="尚未添加题面语言" />
          <div v-for="(statement, index) in statementDrafts" :key="statement.key" class="statement-editor">
            <div class="statement-editor-heading">
              <ElInput v-model="statement.langCode" maxlength="5" placeholder="en 或 zh-CN" :disabled="statement.savedBody !== null" />
              <ElTag v-if="statement.savedBody === statement.body" type="success">已保存</ElTag>
              <ElButton v-if="statement.savedBody !== null" link type="danger" @click="deleteStatement(statement)">删除题面</ElButton>
              <ElButton v-if="statement.savedBody === null" link type="danger" @click="statementDrafts.splice(index, 1)">移除草稿</ElButton>
            </div>
            <ElInput v-model="statement.body" type="textarea" :rows="14" placeholder="Markdown 题面正文" />
            <div class="problem-editor-actions">
              <ElButton type="primary" :loading="statement.saving" @click="saveStatement(statement)">保存此语言题面</ElButton>
            </div>
          </div>
        </ElCard>
      </ElTabPane>

      <ElTabPane label="附件" name="attachments" :disabled="isNew">
        <ElCard shadow="never" class="admin-card problem-editor-card">
          <template #header><strong>题目附件</strong></template>
          <div class="file-upload-row">
            <ElSelect v-model="attachmentKind" aria-label="附件类型">
              <ElOption label="样例附件" value="SAMPLE" />
              <ElOption label="补充材料" value="SUPPLEMENT" />
            </ElSelect>
            <input ref="attachmentInput" type="file" @change="selectAttachment" />
            <ElButton type="primary" :disabled="!attachmentFile" :loading="uploadingAttachment" @click="uploadAttachment">上传附件</ElButton>
          </div>
          <ElTable :data="attachments" row-key="id">
            <ElTableColumn prop="originalFilename" label="文件名" min-width="240" />
            <ElTableColumn prop="kind" label="类型" width="130" />
            <ElTableColumn label="大小" width="130"><template #default="{ row }">{{ formatBytes(row.bytes) }}</template></ElTableColumn>
            <ElTableColumn label="操作" width="170">
              <template #default="{ row }">
                <ElButton link type="primary" @click="downloadAttachment(row as ProblemAttachment)">下载</ElButton>
                <ElButton link type="danger" @click="removeAttachment(row as ProblemAttachment)">删除</ElButton>
              </template>
            </ElTableColumn>
            <template #empty><ElEmpty description="尚未上传附件" /></template>
          </ElTable>
        </ElCard>
      </ElTabPane>

      <ElTabPane label="测试数据" name="testdata" :disabled="isNew">
        <ElCard shadow="never" class="admin-card problem-editor-card">
          <template #header><strong>当前测试数据版本</strong></template>
          <ElAlert
            title="每次上传都会生成不可变版本。可下载任意历史版本，或在题目尚未用于已冻结比赛时重新激活旧版本。"
            type="info"
            show-icon
            :closable="false"
            class="form-alert"
          />
          <div class="testdata-summary">
            <div><span>当前版本</span><strong>v{{ problem?.testdataVersion ?? 0 }}</strong></div>
            <div><span>SHA-256</span><code>{{ problem?.testdataSha256 ?? '尚未上传' }}</code></div>
            <ElButton :disabled="!problem?.testdataVersion" @click="downloadTestdata">下载当前 ZIP</ElButton>
          </div>
          <div class="file-upload-row">
            <input ref="testdataInput" type="file" accept=".zip,application/zip" @change="selectTestdata" />
            <ElButton type="primary" :disabled="!testdataFile" :loading="uploadingTestdata" @click="uploadTestdata">上传新版本</ElButton>
          </div>
          <ElTable :data="testdataVersions" row-key="version">
            <ElTableColumn label="版本" width="130">
              <template #default="{ row }">
                <strong>v{{ row.version }}</strong>
                <ElTag v-if="row.active" size="small" type="success">当前</ElTag>
              </template>
            </ElTableColumn>
            <ElTableColumn prop="caseCount" label="用例数" width="120" />
            <ElTableColumn label="压缩包大小" width="150"><template #default="{ row }">{{ row.bytes == null ? '—' : formatBytes(row.bytes) }}</template></ElTableColumn>
            <ElTableColumn prop="sha256" label="SHA-256" min-width="300" show-overflow-tooltip />
            <ElTableColumn label="操作" width="180">
              <template #default="{ row }">
                <ElButton link @click="downloadTestdataVersion(row.version)">下载</ElButton>
                <ElButton
                  link
                  type="primary"
                  :disabled="row.active"
                  :loading="activatingVersion === row.version"
                  @click="activateTestdataVersion(row.version)"
                >激活</ElButton>
              </template>
            </ElTableColumn>
            <template #empty><ElEmpty description="尚未上传测试数据" /></template>
          </ElTable>
        </ElCard>
      </ElTabPane>
    </ElTabs>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { ArrowLeft, Plus } from '@element-plus/icons-vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import type { FormInstance, FormRules } from 'element-plus';
import { useRoute, useRouter } from 'vue-router';
import { adminProblemApi } from '../api/admin-problems';
import { ApiError, getErrorMessage } from '../api/client';
import type {
  JudgeLanguage,
  Problem,
  ProblemAttachment,
  ProblemAttachmentKind,
  ProblemTestdataVersion,
} from '../api/types';
import { formatBytes } from '../utils/format';

interface ProblemForm {
  slug: string;
  title: string;
  timeLimitMs: number;
  memoryLimitMb: number;
  outputLimitKb: number;
  languages: JudgeLanguage[];
  defaultLangCode: string;
}

interface StatementDraft {
  key: number;
  langCode: string;
  body: string;
  savedBody: string | null;
  saving: boolean;
}

const route = useRoute();
const router = useRouter();
const isNew = computed(() => route.params.problemId === undefined || route.params.problemId === 'new');
const problemId = computed(() => Number(route.params.problemId));
const problem = ref<Problem | null>(null);
const activeTab = ref('basic');
const loading = ref(false);
const saving = ref(false);
const errorMessage = ref('');
const formRef = ref<FormInstance>();
const form = reactive<ProblemForm>({
  slug: '', title: '', timeLimitMs: 1000, memoryLimitMb: 256, outputLimitKb: 65536,
  languages: ['c', 'cpp', 'java', 'python'], defaultLangCode: 'en',
});
const languageOptions: Array<{ value: JudgeLanguage; label: string }> = [
  { value: 'c', label: 'C' }, { value: 'cpp', label: 'C++' },
  { value: 'java', label: 'Java' }, { value: 'python', label: 'Python' },
];
const langCodePattern = /^[a-z]{2}(?:-[A-Z]{2})?$/;
const rules: FormRules = {
  slug: [{ required: true, message: '请输入题目标识', trigger: 'blur' }, { pattern: /^[a-z0-9]+(?:-[a-z0-9]+)*$/, message: '仅允许小写字母、数字和单连字符', trigger: 'blur' }],
  title: [{ required: true, message: '请输入题目标题', trigger: 'blur' }],
  languages: [{ type: 'array', required: true, min: 1, message: '至少选择一种提交语言', trigger: 'change' }],
  defaultLangCode: [{ required: true, message: '请输入默认题面语言', trigger: 'blur' }, { pattern: langCodePattern, message: '请输入 en 或 zh-CN 格式', trigger: 'blur' }],
};

let statementKey = 0;
const statementDrafts = ref<StatementDraft[]>([]);
const attachments = ref<ProblemAttachment[]>([]);
const testdataVersions = ref<ProblemTestdataVersion[]>([]);
const attachmentKind = ref<ProblemAttachmentKind>('SAMPLE');
const attachmentFile = ref<File | null>(null);
const attachmentInput = ref<HTMLInputElement>();
const uploadingAttachment = ref(false);
const testdataFile = ref<File | null>(null);
const testdataInput = ref<HTMLInputElement>();
const uploadingTestdata = ref(false);
const activatingVersion = ref<number | null>(null);

function applyProblem(value: Problem) {
  problem.value = value;
  form.slug = value.slug;
  form.title = value.title;
  form.timeLimitMs = value.timeLimitMs;
  form.memoryLimitMb = value.memoryLimitMb;
  form.outputLimitKb = value.outputLimitKb;
  form.languages = [...value.languages];
  form.defaultLangCode = value.defaultLangCode;
}

function applyRefreshedProblem(value: Problem | null) {
  if (value) {
    // Child-resource mutations only need the new concurrency token and test-data pointer;
    // preserve unsaved metadata currently being edited in the form, and never regress when
    // concurrently running refreshes resolve out of order.
    if (!problem.value || value.version >= problem.value.version) problem.value = value;
  } else {
    problem.value = null;
    errorMessage.value = '操作已成功，但题目版本刷新失败。请重新打开本页后再继续修改。';
  }
}

async function loadProblem() {
  if (isNew.value) return;
  if (!Number.isInteger(problemId.value) || problemId.value <= 0) {
    errorMessage.value = '题目编号不正确';
    return;
  }
  loading.value = true;
  try {
    const [loadedProblem, versions, loadedAttachments, loadedStatements] = await Promise.all([
      adminProblemApi.getProblem(problemId.value),
      adminProblemApi.listTestdataVersions(problemId.value),
      adminProblemApi.listAttachments(problemId.value),
      adminProblemApi.listStatements(problemId.value),
    ]);
    applyProblem(loadedProblem);
    testdataVersions.value = versions;
    attachments.value = loadedAttachments;
    statementDrafts.value = loadedStatements.map((statement) => ({
      key: ++statementKey,
      langCode: statement.langCode,
      body: statement.body,
      savedBody: statement.body,
      saving: false,
    }));
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}

function payload() {
  return {
    slug: form.slug.trim().toLowerCase(), title: form.title.trim(),
    timeLimitMs: form.timeLimitMs, memoryLimitMb: form.memoryLimitMb,
    outputLimitKb: form.outputLimitKb, languages: [...form.languages],
    defaultLangCode: form.defaultLangCode.trim(),
  };
}

async function saveProblem() {
  if (!(await formRef.value?.validate().catch(() => false))) return;
  saving.value = true;
  errorMessage.value = '';
  try {
    if (isNew.value) {
      const created = await adminProblemApi.createProblem(payload());
      ElMessage.success('题目已创建');
      await router.replace(`/admin/problems/${created.id}`);
      applyProblem(created);
    } else if (problem.value) {
      const updated = await adminProblemApi.updateProblem(problem.value.id, {
        ...payload(), expectedVersion: problem.value.version,
      });
      applyProblem(updated);
      ElMessage.success('基本信息已保存');
    }
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
    if (error instanceof ApiError && error.code === 'PROBLEM_VERSION_STALE' && problem.value) {
      problem.value = await adminProblemApi.getProblem(problem.value.id);
      errorMessage.value = '题目已被其他管理员修改。已刷新并发版本，但保留了你的表单内容，请核对后重新保存。';
    }
  } finally {
    saving.value = false;
  }
}

function addStatementDraft() {
  const preferred = problem.value?.defaultLangCode ?? 'en';
  const existingCodes = new Set(statementDrafts.value.map((statement) => statement.langCode));
  const langCode = existingCodes.has(preferred) ? '' : preferred;
  statementDrafts.value.push({ key: ++statementKey, langCode, body: '', savedBody: null, saving: false });
}

async function saveStatement(statement: StatementDraft) {
  if (!problem.value || !langCodePattern.test(statement.langCode)) {
    ElMessage.error('题面语言代码应为 en 或 zh-CN 格式');
    return;
  }
  if (!statement.body.trim()) {
    ElMessage.error('请输入题面正文');
    return;
  }
  if (statementDrafts.value.some((item) => item.key !== statement.key && item.langCode === statement.langCode)) {
    ElMessage.error('同一语言只能保留一个题面编辑器');
    return;
  }
  statement.saving = true;
  try {
    const refreshed = await adminProblemApi.upsertStatement(problem.value.id, statement.langCode, statement.body);
    applyRefreshedProblem(refreshed.problem);
    statement.savedBody = statement.body;
    ElMessage.success(`${statement.langCode} 题面已保存`);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    statement.saving = false;
  }
}

async function deleteStatement(statement: StatementDraft) {
  if (!problem.value || statement.savedBody === null) return;
  try {
    await ElMessageBox.confirm(`确认删除 ${statement.langCode} 题面？`, '删除题面', { type: 'warning' });
    await adminProblemApi.deleteStatement(problem.value.id, statement.langCode);
    statementDrafts.value = statementDrafts.value.filter((item) => item.key !== statement.key);
    ElMessage.success(`${statement.langCode} 题面已删除`);
  } catch (error) {
    if (error !== 'cancel' && error !== 'close') ElMessage.error(getErrorMessage(error));
  }
}

function selectAttachment(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0] ?? null;
  if (file && (file.size === 0 || file.size > 20 * 1024 * 1024)) {
    ElMessage.error('附件必须大于 0 字节且不能超过 20 MiB');
    attachmentFile.value = null;
    (event.target as HTMLInputElement).value = '';
    return;
  }
  attachmentFile.value = file;
}

async function uploadAttachment() {
  if (!problem.value || !attachmentFile.value) return;
  uploadingAttachment.value = true;
  try {
    const refreshed = await adminProblemApi.uploadAttachment(problem.value.id, attachmentKind.value, attachmentFile.value);
    attachments.value.push(refreshed.result);
    applyRefreshedProblem(refreshed.problem);
    attachmentFile.value = null;
    if (attachmentInput.value) attachmentInput.value.value = '';
    ElMessage.success('附件已上传');
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    uploadingAttachment.value = false;
  }
}

async function removeAttachment(attachment: ProblemAttachment) {
  if (!problem.value) return;
  try {
    await ElMessageBox.confirm(`确认删除附件“${attachment.originalFilename}”？`, '删除附件', { type: 'warning' });
    const refreshed = await adminProblemApi.deleteAttachment(problem.value.id, attachment.id);
    applyRefreshedProblem(refreshed.problem);
    attachments.value = attachments.value.filter((item) => item.id !== attachment.id);
    ElMessage.success('附件已删除');
  } catch (error) {
    if (error !== 'cancel' && error !== 'close') ElMessage.error(getErrorMessage(error));
  }
}

async function downloadAttachment(attachment: ProblemAttachment) {
  if (!problem.value) return;
  try {
    downloadBlob(await adminProblemApi.downloadAttachment(problem.value.id, attachment.id), attachment.originalFilename);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}

function selectTestdata(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0] ?? null;
  if (file && (!file.name.toLowerCase().endsWith('.zip') || file.size === 0 || file.size > 256 * 1024 * 1024)) {
    ElMessage.error('测试数据必须是大于 0 字节且不超过 256 MiB 的 ZIP 文件');
    testdataFile.value = null;
    (event.target as HTMLInputElement).value = '';
    return;
  }
  testdataFile.value = file;
}

async function uploadTestdata() {
  if (!problem.value || !testdataFile.value) return;
  const id = problem.value.id;
  try {
    await ElMessageBox.confirm(
      `确认上传“${testdataFile.value.name}”并创建新的当前测试数据版本？`,
      '上传测试数据',
      { type: 'warning', confirmButtonText: '确认上传' },
    );
  } catch {
    return;
  }
  uploadingTestdata.value = true;
  try {
    const refreshed = await adminProblemApi.uploadTestdata(id, testdataFile.value);
    applyRefreshedProblem(refreshed.problem);
    testdataVersions.value = await adminProblemApi.listTestdataVersions(id);
    testdataFile.value = null;
    if (testdataInput.value) testdataInput.value.value = '';
    ElMessage.success(`测试数据 v${refreshed.result.version} 已上传`);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    uploadingTestdata.value = false;
  }
}

async function downloadTestdata() {
  if (!problem.value || problem.value.testdataVersion === 0) return;
  try {
    const blob = await adminProblemApi.downloadTestdata(problem.value.id);
    downloadBlob(blob, `problem-${problem.value.id}-testdata-v${problem.value.testdataVersion}.zip`);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}

async function downloadTestdataVersion(version: number) {
  if (!problem.value) return;
  try {
    const blob = await adminProblemApi.downloadTestdataVersion(problem.value.id, version);
    downloadBlob(blob, `problem-${problem.value.id}-testdata-v${version}.zip`);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}

async function activateTestdataVersion(version: number) {
  if (!problem.value || version === problem.value.testdataVersion) return;
  const id = problem.value.id;
  const expectedCurrentVersion = problem.value.testdataVersion;
  try {
    await ElMessageBox.confirm(
      `确认将测试数据 v${version} 设为当前版本？后续提交将使用该版本。`,
      '激活历史版本',
      { type: 'warning', confirmButtonText: '确认激活' },
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
    applyRefreshedProblem(refreshed.problem);
    testdataVersions.value = await adminProblemApi.listTestdataVersions(id);
    ElMessage.success(`测试数据 v${version} 已激活`);
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

function backToList() {
  const contestId = Number(route.query.contestId);
  void router.push(Number.isInteger(contestId) && contestId > 0
    ? `/admin/contests/${contestId}`
    : '/admin/problems');
}
onMounted(loadProblem);
</script>
