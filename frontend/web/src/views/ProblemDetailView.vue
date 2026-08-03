<template>
  <el-container direction="vertical" class="page-section">
    <el-main class="page-body">
      <ElSkeleton v-if="loading" :rows="10" animated />
      <ElAlert
        v-else-if="errorMessage"
        :title="errorMessage"
        type="error"
        show-icon
        :closable="false"
      >
        <template #default
          ><ElButton link type="primary" @click="loadProblem">重新加载</ElButton></template
        >
      </ElAlert>
      <template v-else-if="problem">
        <div class="problem-heading">
          <ElButton link :icon="ArrowLeft" @click="router.push(`/contests/${contestId}/problems`)"
            >返回题目列表</ElButton
          >
          <div class="problem-title-line">
            <span class="large-alias" :style="{ '--problem-color': problem.color || '#2563eb' }">
              {{ problem.alias }}
            </span>
            <div>
              <h1>{{ problem.title }}</h1>
              <p>
                {{ problem.timeLimitMs }} ms · {{ problem.memoryLimitMb }} MB · 输出限制
                {{ problem.outputLimitKb }} KiB
              </p>
            </div>
          </div>
        </div>

        <ElRow :gutter="24" align="top" class="problem-layout">
          <ElCol :xs="24" :md="15">
            <article class="statement-card">
              <div v-if="problem.statement" class="statement-language">
                <ElTag effect="plain">{{ problem.statement.langCode }}</ElTag>
              </div>
              <div
                v-if="problem.statement"
                class="markdown-body"
                v-html="problem.statement.renderedHtml"
              ></div>
              <ElEmpty v-else description="暂无已发布题面" />
            </article>
          </ElCol>
          <ElCol :xs="24" :md="9">
            <aside class="submit-card">
              <div>
                <p class="eyebrow">Submit</p>
                <h2>{{ language === 'output' ? '提交输出' : '提交代码' }}</h2>
              </div>
              <ElAlert
                v-if="contest?.status !== 'RUNNING'"
                title="比赛当前不接受提交"
                type="warning"
                show-icon
                :closable="false"
              />
              <ElForm label-position="top" @submit.prevent="submit">
                <ElFormItem label="语言">
                  <ElSelect v-model="language" class="wide-control" placeholder="选择语言">
                    <ElOption
                      v-for="item in problem.languages"
                      :key="item"
                      :label="languageLabel(item)"
                      :value="item"
                    />
                  </ElSelect>
                </ElFormItem>
                <ElFormItem v-if="language === 'output'" :label="'输出 ZIP'">
                  <ElUpload
                    ref="uploadRef"
                    :auto-upload="false"
                    :limit="1"
                    :on-change="onFileChange"
                    :on-remove="onFileRemove"
                    accept=".zip"
                    drag
                  >
                    <ElIcon class="upload-icon"><UploadFilled /></ElIcon>
                    <div>拖放输出 ZIP 到这里，或点击选择</div>
                    <template #tip>
                      <span>最大 64 KiB</span>
                    </template>
                  </ElUpload>
                </ElFormItem>
                <ElFormItem v-else label="源码">
                  <div class="code-input">
                    <ElSegmented v-model="codeMode" :options="codeModeOptions" size="small" />
                    <CodeEditor
                      v-if="codeMode === 'editor'"
                      v-model="source"
                      :language="language"
                      height="320px"
                      class="wide-control"
                    />
                    <ElUpload
                      v-else
                      ref="codeUploadRef"
                      :auto-upload="false"
                      :limit="1"
                      :on-change="onCodeFileChange"
                      :on-remove="onCodeFileRemove"
                      :accept="sourceAccept"
                      drag
                    >
                      <ElIcon class="upload-icon"><UploadFilled /></ElIcon>
                      <div>拖放源码文件到这里，或点击选择</div>
                      <template #tip>
                        <span>扩展名需匹配所选语言（{{ sourceAccept }}），最大 64 KiB</span>
                      </template>
                    </ElUpload>
                  </div>
                </ElFormItem>
                <ElAlert
                  v-if="submitError"
                  :title="submitError"
                  type="error"
                  show-icon
                  :closable="false"
                  class="form-alert"
                />
                <ElButton
                  type="primary"
                  size="large"
                  class="wide-button"
                  native-type="submit"
                  :loading="submitting"
                  :disabled="
                    contest?.status !== 'RUNNING' ||
                    !language ||
                    (language === 'output'
                      ? !sourceFile
                      : codeMode === 'file'
                        ? !codeFile
                        : !source.trim())
                  "
                >
                  {{ language === 'output' ? '提交输出' : '提交代码' }}
                </ElButton>
              </ElForm>
            </aside>
          </ElCol>
        </ElRow>
      </template>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import type { UploadFile, UploadInstance } from 'element-plus';
import { ElMessage } from 'element-plus';
import { ArrowLeft } from '@element-plus/icons-vue';
import CodeEditor from '../components/CodeEditor.vue';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { Contest, ContestProblem } from '../api/types';
import { languageLabel } from '../utils/format';

const props = defineProps<{ contest: Contest | null }>();
const route = useRoute();
const router = useRouter();
const contestId = computed(() => Number(route.params.contestId));
const problemId = computed(() => Number(route.params.problemId));
const problem = ref<ContestProblem | null>(null);
const loading = ref(false);
const errorMessage = ref('');
const language = ref('');
const source = ref('');
const codeMode = ref<'editor' | 'file'>('editor');
const codeModeOptions = [
  { label: '编辑器', value: 'editor' },
  { label: '上传文件', value: 'file' },
];
const sourceFile = ref<File | null>(null);
const codeFile = ref<File | null>(null);
const uploadRef = ref<UploadInstance>();
const codeUploadRef = ref<UploadInstance>();
const submitting = ref(false);
const submitError = ref('');
const contest = computed(() => props.contest);

function sourceFileName(): string {
  const extension: Record<string, string> = {
    c: '.c',
    cpp: '.cpp',
    java: '.java',
    python: '.py',
  };
  return `Main${extension[language.value] ?? '.txt'}`;
}

function sourceAccept(): string {
  if (language.value === 'cpp') return '.cpp,.cc,.cxx';
  if (language.value === 'java') return '.java';
  if (language.value === 'python') return '.py';
  return '.c';
}

async function loadProblem() {
  loading.value = true;
  errorMessage.value = '';
  try {
    const problems = await contestApi.listProblems(contestId.value);
    problem.value = problems.find((item) => item.problemId === problemId.value) ?? null;
    if (!problem.value) {
      errorMessage.value = '题目不存在或不可访问';
      return;
    }
    if (!problem.value.languages.includes(language.value)) {
      language.value = problem.value.languages[0] ?? '';
    }
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}

function onFileChange(uploadFile: UploadFile) {
  const file = uploadFile.raw ?? null;
  submitError.value = '';
  if (file && file.size > 65_536) {
    submitError.value = '提交文件不能超过 64 KiB';
    sourceFile.value = null;
    uploadRef.value?.clearFiles();
    return;
  }
  sourceFile.value = file;
}

function onFileRemove() {
  sourceFile.value = null;
}

function onCodeFileChange(uploadFile: UploadFile) {
  const file = uploadFile.raw ?? null;
  submitError.value = '';
  if (!file) return;
  if (file.size > 65_536) {
    submitError.value = '提交文件不能超过 64 KiB';
    codeFile.value = null;
    codeUploadRef.value?.clearFiles();
    return;
  }
  const lowerName = file.name.toLowerCase();
  const extensions = sourceAccept().split(',');
  if (!extensions.some((extension) => lowerName.endsWith(extension))) {
    submitError.value = `源码文件扩展名需匹配所选语言（${sourceAccept()}）`;
    codeFile.value = null;
    codeUploadRef.value?.clearFiles();
    return;
  }
  codeFile.value = file;
}

function onCodeFileRemove() {
  codeFile.value = null;
}

async function submit() {
  if (!language.value) return;
  let file: File;
  if (language.value === 'output') {
    if (!sourceFile.value) return;
    file = sourceFile.value;
  } else if (codeMode.value === 'file') {
    if (!codeFile.value) return;
    file = codeFile.value;
  } else {
    if (!source.value.trim()) return;
    if (new Blob([source.value]).size > 65_536) {
      submitError.value = '提交内容不能超过 64 KiB';
      return;
    }
    file = new File([source.value], sourceFileName(), { type: 'text/plain' });
  }
  submitting.value = true;
  submitError.value = '';
  try {
    const result = await contestApi.submit(contestId.value, problemId.value, language.value, file);
    ElMessage.success('提交成功，正在等待判题');
    await router.push(`/contests/${contestId.value}/submissions/${result.submissionId}`);
  } catch (error) {
    submitError.value = getErrorMessage(error);
  } finally {
    submitting.value = false;
  }
}

watch(
  [contestId, problemId],
  () => {
    problem.value = null;
    source.value = '';
    sourceFile.value = null;
    codeFile.value = null;
    codeMode.value = 'editor';
    void loadProblem();
  },
  { immediate: true },
);

watch(language, () => {
  codeFile.value = null;
  codeUploadRef.value?.clearFiles();
});
</script>

<style scoped>
.page-section {
  min-height: 50vh;
}

.page-body {
  padding: 0;
}

.eyebrow {
  display: none;
  margin: 0 0 8px;
  color: var(--primary);
  font-size: 12px;
  font-weight: 800;
  letter-spacing: 0.14em;
  text-transform: uppercase;
}

.page-alert,
.form-alert {
  margin-bottom: 20px;
}

.problem-heading {
  margin-bottom: 28px;
}

.problem-title-line {
  display: flex;
  align-items: center;
  gap: 20px;
  margin-top: 18px;
}

.problem-title-line h1 {
  margin-bottom: 5px;
  font-size: clamp(30px, 4vw, 46px);
}

.problem-title-line p {
  margin: 0;
  color: var(--muted);
}

.large-alias {
  display: grid;
  width: 58px;
  height: 58px;
  flex: 0 0 auto;
  place-items: center;
  border: 3px solid var(--problem-color);
  border-radius: 0;
  color: var(--problem-color);
  background: color-mix(in srgb, var(--problem-color) 9%, white);
  font-size: 24px;
  font-weight: 900;
}

.statement-card,
.submit-card {
  border: 1px solid var(--border);
  border-radius: 0;
  background: white;
}

.statement-card {
  min-width: 0;
  padding: clamp(24px, 4vw, 44px);
}

.submit-card {
  position: sticky;
  top: 96px;
  display: grid;
  gap: 22px;
  padding: 24px;
}

.submit-card h2 {
  margin-bottom: 0;
}

.statement-language {
  display: flex;
  justify-content: flex-end;
  margin-bottom: 24px;
}

.markdown-body {
  color: #27364a;
  font-size: 16px;
  line-height: 1.78;
}

.markdown-body h1,
.markdown-body h2,
.markdown-body h3 {
  margin: 1.6em 0 0.65em;
  color: #111c31;
  line-height: 1.3;
}

.markdown-body h1:first-child,
.markdown-body h2:first-child {
  margin-top: 0;
}

.markdown-body pre {
  overflow-x: auto;
  border-radius: 0;
  padding: 18px;
  color: #dbeafe;
  background: #101827;
  font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', monospace;
  font-size: 13px;
  line-height: 1.65;
}

.markdown-body code:not(pre code) {
  border-radius: 0;
  padding: 2px 5px;
  color: #1d4ed8;
  background: #eff6ff;
}

.markdown-body table {
  width: 100%;
  border-collapse: collapse;
}

.markdown-body th,
.markdown-body td {
  padding: 8px 12px;
  border: 1px solid var(--border);
  text-align: left;
}

.markdown-body img {
  max-width: 100%;
}

.wide-button,
.wide-control,
:deep(.el-upload),
:deep(.el-upload-dragger) {
  width: 100%;
}

.upload-icon {
  margin-bottom: 8px;
  font-size: 34px;
}

.code-input {
  display: grid;
  width: 100%;
  gap: 10px;
}

.code-input :deep(.el-segmented) {
  justify-self: start;
}

@media (max-width: 900px) {
  .submit-card {
    position: static;
  }
}

@media (max-width: 640px) {
  .problem-title-line {
    align-items: flex-start;
  }

  .large-alias {
    width: 50px;
    height: 50px;
    border-radius: 0;
    font-size: 21px;
  }

  .statement-card,
  .submit-card {
    border-radius: 0;
    padding: 20px;
  }
}
</style>
