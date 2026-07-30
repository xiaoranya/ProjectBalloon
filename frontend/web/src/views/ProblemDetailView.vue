<template>
  <section class="page-section">
    <ElSkeleton v-if="loading" :rows="10" animated />
    <ElAlert v-else-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false">
      <template #default><ElButton link type="primary" @click="loadProblem">重新加载</ElButton></template>
    </ElAlert>
    <template v-else-if="problem">
      <div class="problem-heading">
        <ElButton link :icon="ArrowLeft" @click="router.push(`/contests/${contestId}/problems`)">返回题目列表</ElButton>
        <div class="problem-title-line">
          <span class="large-alias" :style="{ '--problem-color': problem.color || '#2563eb' }">
            {{ problem.alias }}
          </span>
          <div>
            <h1>{{ problem.title }}</h1>
            <p>{{ problem.timeLimitMs }} ms · {{ problem.memoryLimitMb }} MB · 输出限制 {{ problem.outputLimitKb }} KiB</p>
          </div>
        </div>
      </div>

      <div class="problem-layout">
        <article class="statement-card">
          <div v-if="problem.statement" class="statement-language">
            <ElTag effect="plain">{{ problem.statement.langCode }}</ElTag>
          </div>
          <div v-if="problem.statement" class="markdown-body" v-html="problem.statement.renderedHtml"></div>
          <ElEmpty v-else description="暂无已发布题面" />
        </article>

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
            <ElFormItem :label="language === 'output' ? '输出 ZIP' : '源码文件'">
              <ElUpload
                ref="uploadRef"
                :auto-upload="false"
                :limit="1"
                :on-change="onFileChange"
                :on-remove="onFileRemove"
                :accept="acceptedExtensions"
                drag
              >
                <ElIcon class="upload-icon"><UploadFilled /></ElIcon>
                <div>拖放源码到这里，或点击选择</div>
                <template #tip>
                  <span>最大 256 KiB，扩展名需与提交类型匹配</span>
                </template>
              </ElUpload>
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
              :disabled="contest?.status !== 'RUNNING' || !sourceFile || !language"
            >
              {{ language === 'output' ? '提交输出' : '提交代码' }}
            </ElButton>
          </ElForm>
        </aside>
      </div>
    </template>
  </section>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import type { UploadFile, UploadInstance } from 'element-plus';
import { ElMessage } from 'element-plus';
import { ArrowLeft } from '@element-plus/icons-vue';
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
const sourceFile = ref<File | null>(null);
const uploadRef = ref<UploadInstance>();
const submitting = ref(false);
const submitError = ref('');
const contest = computed(() => props.contest);

const acceptedExtensions = computed(() => {
  const extensions: Record<string, string> = {
    c: '.c',
    cpp: '.cc,.cpp,.cxx',
    java: '.java',
    python: '.py',
    output: '.zip',
  };
  return extensions[language.value] ?? '';
});

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
  if (file && file.size > 262_144) {
    submitError.value = '提交文件不能超过 256 KiB';
    sourceFile.value = null;
    uploadRef.value?.clearFiles();
    return;
  }
  sourceFile.value = file;
}

function onFileRemove() {
  sourceFile.value = null;
}

async function submit() {
  if (!sourceFile.value || !language.value) return;
  submitting.value = true;
  submitError.value = '';
  try {
    const result = await contestApi.submit(contestId.value, problemId.value, language.value, sourceFile.value);
    ElMessage.success('提交成功，正在等待判题');
    await router.push(`/contests/${contestId.value}/submissions/${result.submissionId}`);
  } catch (error) {
    submitError.value = getErrorMessage(error);
  } finally {
    submitting.value = false;
  }
}

watch([contestId, problemId], () => {
  problem.value = null;
  sourceFile.value = null;
  void loadProblem();
}, { immediate: true });
</script>
