<template>
  <ElCard shadow="never" class="problem-editor-card">
    <ElForm ref="formRef" :model="form" :rules="rules" label-position="top">
      <ElRow :gutter="12" class="admin-two-column">
        <ElCol :xs="24" :md="12">
          <ElFormItem :label="t('题目标识')" prop="slug">
            <ElInput v-model="form.slug" maxlength="64" :placeholder="t('例如 two-sum')" />
          </ElFormItem>
        </ElCol>
        <ElCol :xs="24" :md="12">
          <ElFormItem :label="t('题目标题')" prop="title">
            <ElInput v-model="form.title" maxlength="255" />
          </ElFormItem>
        </ElCol>
      </ElRow>
      <ElFormItem :label="t('允许的提交语言')" prop="languages">
        <ElCheckboxGroup v-model="form.languages" :disabled="form.judgeMode === 'OUTPUT_ONLY'">
          <ElCheckboxButton
            v-for="option in languageOptions"
            :key="option.value"
            :value="option.value"
          >
            {{ option.label }}
          </ElCheckboxButton>
        </ElCheckboxGroup>
      </ElFormItem>
      <ElRow :gutter="12" class="admin-form-grid">
        <ElCol :xs="24" :sm="8">
          <ElFormItem :label="t('判题模式')">
            <ElSelect v-model="form.judgeMode" @change="changeJudgeMode">
              <ElOption :label="t('标准输入输出')" value="STANDARD" />
              <ElOption :label="t('交互题')" value="INTERACTIVE" />
              <ElOption label="Output-only" value="OUTPUT_ONLY" />
            </ElSelect>
          </ElFormItem>
        </ElCol>
        <ElCol v-if="form.judgeMode === 'INTERACTIVE'" :xs="24" :sm="8">
          <ElFormItem :label="t('Interactor 对象键')">
            <ElInput v-model="form.interactorObjectKey" maxlength="512" />
          </ElFormItem>
        </ElCol>
        <ElCol v-if="form.judgeMode === 'INTERACTIVE'" :xs="24" :sm="8">
          <ElFormItem label="Interactor SHA-256">
            <ElInput v-model="form.interactorSha256" maxlength="64" />
          </ElFormItem>
        </ElCol>
      </ElRow>
      <ElAlert
        v-if="form.judgeMode === 'OUTPUT_ONLY'"
        :title="t('参赛者需上传包含 1.out、2.out 等根目录输出文件的 ZIP。')"
        type="info"
        :closable="false"
        show-icon
      />
      <ElSpace
        v-if="problem && form.judgeMode === 'INTERACTIVE'"
        wrap
        :size="14"
        class="file-upload-row"
      >
        <input
          ref="interactorInput"
          type="file"
          class="visually-hidden-input"
          @change="selectInteractor"
        />
        <ElButton @click="interactorInput?.click()">{{ t('选择文件') }}</ElButton>
        <span v-if="interactorFile" class="file-upload-name">{{ interactorFile.name }}</span>
        <ElButton
          type="primary"
          :disabled="!interactorFile"
          :loading="uploadingInteractor"
          @click="uploadInteractor"
          >{{ t('上传 Interactor ELF') }}</ElButton
        >
        <code v-if="problem.interactorSha256">{{ problem.interactorSha256 }}</code>
      </ElSpace>
      <ElRow :gutter="12" class="admin-form-grid problem-limit-grid">
        <ElCol :xs="24" :sm="8">
          <ElFormItem :label="t('时间限制（ms）')" prop="timeLimitMs">
            <ElInputNumber
              v-model="form.timeLimitMs"
              :min="1"
              :max="60000"
              controls-position="right"
            />
          </ElFormItem>
        </ElCol>
        <ElCol :xs="24" :sm="8">
          <ElFormItem :label="t('内存限制（MiB）')" prop="memoryLimitMb">
            <ElInputNumber
              v-model="form.memoryLimitMb"
              :min="16"
              :max="8192"
              controls-position="right"
            />
          </ElFormItem>
        </ElCol>
        <ElCol :xs="24" :sm="8">
          <ElFormItem :label="t('输出限制（KiB）')" prop="outputLimitKb">
            <ElInputNumber
              v-model="form.outputLimitKb"
              :min="1"
              :max="262144"
              controls-position="right"
            />
          </ElFormItem>
        </ElCol>
      </ElRow>
      <ElFormItem :label="t('默认题面语言')" prop="defaultLangCode">
        <ElInput
          v-model="form.defaultLangCode"
          maxlength="5"
          :placeholder="t('en 或 zh-CN')"
          class="problem-lang-code"
        />
      </ElFormItem>
      <ElAlert v-if="problem" type="info" :closable="false" show-icon>
        {{
          t('当前并发版本为 {version}；保存时会通过 expectedVersion 防止覆盖其他管理员的修改。', {
            version: problem.version,
          })
        }}
      </ElAlert>
    </ElForm>
  </ElCard>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import { ElMessage } from 'element-plus';
import type { FormInstance, FormRules } from 'element-plus';
import { useRouter } from 'vue-router';
import { adminProblemApi } from '../../api/admin-problems';
import { ApiError, getErrorMessage } from '../../api/client';
import type { JudgeLanguage, ProblemResponse } from '../../api/types';
import { useI18n } from '../../i18n';

interface ProblemForm {
  slug: string;
  title: string;
  timeLimitMs: number;
  memoryLimitMb: number;
  outputLimitKb: number;
  languages: JudgeLanguage[];
  defaultLangCode: string;
  judgeMode: 'STANDARD' | 'INTERACTIVE' | 'OUTPUT_ONLY';
  interactorObjectKey: string;
  interactorSha256: string;
}

const props = defineProps<{
  problem: ProblemResponse | null;
  isNew: boolean;
}>();
const emit = defineEmits<{
  'problem-refreshed': [value: ProblemResponse | null];
  'error-message': [message: string];
  'saving-changed': [saving: boolean];
}>();
const router = useRouter();
const { t } = useI18n();

const saving = ref(false);
const formRef = ref<FormInstance>();
const form = reactive<ProblemForm>({
  slug: '',
  title: '',
  timeLimitMs: 1000,
  memoryLimitMb: 256,
  outputLimitKb: 65536,
  languages: ['c', 'cpp', 'java', 'python'],
  defaultLangCode: 'en',
  judgeMode: 'STANDARD',
  interactorObjectKey: '',
  interactorSha256: '',
});
const interactorFile = ref<File | null>(null);
const interactorInput = ref<HTMLInputElement>();
const uploadingInteractor = ref(false);
const languageOptions: Array<{ value: JudgeLanguage; label: string }> = [
  { value: 'c', label: 'C' },
  { value: 'cpp', label: 'C++' },
  { value: 'java', label: 'Java' },
  { value: 'python', label: 'Python' },
];
const langCodePattern = /^[a-z]{2}(?:-[A-Z]{2})?$/;
const rules = computed<FormRules>(() => ({
  slug: [
    { required: true, message: t('请输入题目标识'), trigger: 'blur' },
    {
      pattern: /^[a-z0-9]+(?:-[a-z0-9]+)*$/,
      message: t('仅允许小写字母、数字和单连字符'),
      trigger: 'blur',
    },
  ],
  title: [{ required: true, message: t('请输入题目标题'), trigger: 'blur' }],
  languages: [
    {
      type: 'array',
      required: true,
      min: 1,
      message: t('至少选择一种提交语言'),
      trigger: 'change',
    },
  ],
  defaultLangCode: [
    { required: true, message: t('请输入默认题面语言'), trigger: 'blur' },
    { pattern: langCodePattern, message: t('请输入 en 或 zh-CN 格式'), trigger: 'blur' },
  ],
}));

let initialized = false;
watch(
  () => props.problem,
  (value) => {
    if (value && !initialized) {
      applyProblem(value);
      initialized = true;
    }
  },
  { immediate: true },
);

function applyProblem(value: ProblemResponse) {
  form.slug = value.slug;
  form.title = value.title;
  form.timeLimitMs = value.timeLimitMs;
  form.memoryLimitMb = value.memoryLimitMb;
  form.outputLimitKb = value.outputLimitKb;
  form.languages = [...value.languages];
  form.defaultLangCode = value.defaultLangCode;
  form.judgeMode = value.judgeMode;
  form.interactorObjectKey = value.interactorObjectKey ?? '';
  form.interactorSha256 = value.interactorSha256 ?? '';
}

function payload() {
  return {
    slug: form.slug.trim().toLowerCase(),
    title: form.title.trim(),
    timeLimitMs: form.timeLimitMs,
    memoryLimitMb: form.memoryLimitMb,
    outputLimitKb: form.outputLimitKb,
    languages: [...form.languages],
    defaultLangCode: form.defaultLangCode.trim(),
    judgeMode: form.judgeMode,
    interactorObjectKey: form.judgeMode === 'INTERACTIVE' ? form.interactorObjectKey.trim() : null,
    interactorSha256: form.judgeMode === 'INTERACTIVE' ? form.interactorSha256.trim() : null,
  };
}

function changeJudgeMode(mode: string) {
  if (mode === 'OUTPUT_ONLY') form.languages = ['output'];
  else if (form.languages.includes('output')) form.languages = ['c', 'cpp', 'java', 'python'];
}

function selectInteractor(event: Event) {
  interactorFile.value = (event.target as HTMLInputElement).files?.[0] ?? null;
}

async function uploadInteractor() {
  if (!props.problem || !interactorFile.value) return;
  uploadingInteractor.value = true;
  try {
    const updated = await adminProblemApi.uploadInteractor(props.problem.id, interactorFile.value);
    applyProblem(updated);
    emit('problem-refreshed', updated);
    interactorFile.value = null;
    ElMessage.success(t('Interactor 已上传并启用交互题模式'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    uploadingInteractor.value = false;
  }
}

async function save() {
  if (!(await formRef.value?.validate().catch(() => false))) return;
  saving.value = true;
  emit('saving-changed', true);
  try {
    if (props.isNew) {
      const created = await adminProblemApi.createProblem(payload());
      ElMessage.success(t('题目已创建'));
      await router.replace(`/admin/problems/${created.id}`);
      applyProblem(created);
      emit('problem-refreshed', created);
    } else if (props.problem) {
      const updated = await adminProblemApi.updateProblem(props.problem.id, {
        ...payload(),
        expectedVersion: props.problem.version,
      });
      applyProblem(updated);
      emit('problem-refreshed', updated);
      ElMessage.success(t('基本信息已保存'));
    }
  } catch (error) {
    emit('error-message', getErrorMessage(error));
    if (error instanceof ApiError && error.code === 'PROBLEM_VERSION_STALE' && props.problem) {
      const refreshed = await adminProblemApi.getProblem(props.problem.id);
      emit('problem-refreshed', refreshed);
      emit(
        'error-message',
        t('题目已被其他管理员修改。已刷新并发版本，但保留了你的表单内容，请核对后重新保存。'),
      );
    }
  } finally {
    saving.value = false;
    emit('saving-changed', false);
  }
}

defineExpose({ save });
</script>

<style scoped>
.problem-editor-card {
  margin-bottom: 20px;
}
.problem-editor-card :deep(.el-input-number),
.problem-limit-grid :deep(.el-input-number) {
  width: 100%;
}
.problem-lang-code {
  width: min(240px, 100%);
}
.file-upload-row {
  width: 100%;
  flex-wrap: wrap;
  margin-bottom: 24px;
}
.visually-hidden-input {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
.file-upload-name {
  color: var(--muted);
  font-size: 13px;
}
</style>
