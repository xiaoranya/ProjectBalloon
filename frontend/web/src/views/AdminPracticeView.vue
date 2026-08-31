<template>
  <el-container direction="vertical" class="admin-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <h1>{{ t('日常练习') }}</h1>
        <ElButton type="primary" :loading="savingSettings" @click="saveSettings">{{
          t('保存设置')
        }}</ElButton>
      </div>
    </el-header>
    <el-main class="page-body">
      <ElAlert
        v-if="errorMessage"
        :title="errorMessage"
        type="error"
        show-icon
        :closable="false"
        class="page-alert"
      />
      <ElRow :gutter="20" class="settings-grid">
        <ElCol :xs="24" :md="9"
          ><ElCard shadow="never"
            ><template #header
              ><strong>{{ t('平台配额') }}</strong></template
            ><ElForm label-position="top"
              ><ElFormItem :label="t('每日提交上限')"
                ><ElInputNumber
                  v-model="settings.dailySubmissionLimit"
                  :min="1"
                  :max="10000" /></ElFormItem
              ><ElFormItem :label="t('并发判题上限')"
                ><ElInputNumber
                  v-model="settings.concurrentJudgingLimit"
                  :min="1"
                  :max="20" /></ElFormItem
              ><ElFormItem :label="t('源码保留天数')"
                ><ElInputNumber
                  v-model="settings.sourceRetentionDays"
                  :min="1"
                  :max="3650" /></ElFormItem></ElForm></ElCard
        ></ElCol>
        <ElCol :xs="24" :md="15"
          ><ElCard shadow="never"
            ><template #header
              ><strong>{{ t('题解管理') }}</strong></template
            ><ElForm label-position="top"
              ><ElFormItem :label="t('公开题目')"
                ><ElSelect
                  v-model="editorialProblemId"
                  filterable
                  clearable
                  :placeholder="t('选择题目')"
                  @change="loadEditorial"
                  ><ElOption
                    v-for="problem in problems"
                    :key="problem.id"
                    :label="`${problem.slug} · ${problem.title}`"
                    :value="problem.id" /></ElSelect></ElFormItem
              ><ElFormItem :label="t('语言')"
                ><ElInput
                  v-model="editorialLang"
                  maxlength="8"
                  @change="loadEditorial" /></ElFormItem
              ><template v-if="editorialProblemId"
                ><ElFormItem :label="t('标题')"
                  ><ElInput v-model="editorial.title" maxlength="255" /></ElFormItem
                ><ElFormItem :label="t('解锁条件')"
                  ><ElSelect v-model="editorial.unlockPolicy"
                    ><ElOption :label="t('始终可见')" value="ALWAYS" /><ElOption
                      :label="t('提交后解锁')"
                      value="AFTER_ATTEMPT" /><ElOption
                      :label="t('通过后解锁')"
                      value="AFTER_ACCEPTED" /></ElSelect></ElFormItem
                ><ElFormItem :label="t('Markdown 内容')"
                  ><CodeEditor
                    v-model="editorial.body"
                    language="markdown"
                    height="360px" /></ElFormItem
                ><ElCheckbox v-model="editorial.published">{{ t('发布题解') }}</ElCheckbox>
                <div class="editorial-actions">
                  <ElButton type="primary" :loading="savingEditorial" @click="saveEditorial">{{
                    t('保存题解')
                  }}</ElButton>
                </div></template
              ><ElEmpty v-else :description="t('选择题目开始维护题解')" /></ElForm></ElCard
        ></ElCol>
      </ElRow>
    </el-main>
  </el-container>
</template>
<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
import { ElMessage } from 'element-plus';
import { getErrorMessage } from '../api/client';
import { trainingApi, type BankProblem } from '../api/training';
import CodeEditor from '../components/CodeEditor.vue';
import { useI18n } from '../i18n';
const { t } = useI18n();
const problems = ref<BankProblem[]>([]),
  editorialProblemId = ref<number>(),
  editorialLang = ref('en'),
  savingSettings = ref(false),
  savingEditorial = ref(false),
  errorMessage = ref('');
const settings = reactive({
  dailySubmissionLimit: 200,
  concurrentJudgingLimit: 3,
  sourceRetentionDays: 365,
});
const editorial = reactive({
  title: '',
  body: '',
  unlockPolicy: 'AFTER_ATTEMPT',
  published: false,
});
async function loadEditorial() {
  if (!editorialProblemId.value) return;
  try {
    const value = await trainingApi.adminEditorial(editorialProblemId.value, editorialLang.value);
    editorial.title = value.title;
    editorial.body = value.bodyMarkdown ?? '';
    editorial.unlockPolicy = value.unlockPolicy;
    editorial.published = value.unlocked;
  } catch (error) {
    if ((error as { status?: number }).status === 404) {
      editorial.title = '';
      editorial.body = '';
      editorial.unlockPolicy = 'AFTER_ATTEMPT';
      editorial.published = false;
    } else errorMessage.value = getErrorMessage(error);
  }
}
async function saveSettings() {
  savingSettings.value = true;
  try {
    await trainingApi.updatePracticeSettings(settings);
    ElMessage.success(t('练习设置已保存'));
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    savingSettings.value = false;
  }
}
async function saveEditorial() {
  if (!editorialProblemId.value) return;
  savingEditorial.value = true;
  try {
    await trainingApi.saveEditorial(editorialProblemId.value, editorialLang.value, editorial);
    ElMessage.success(t('题解已保存'));
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    savingEditorial.value = false;
  }
}
onMounted(async () => {
  try {
    const [value, bank] = await Promise.all([
      trainingApi.practiceSettings(),
      trainingApi.problemBank(0, 100),
    ]);
    Object.assign(settings, value);
    problems.value = bank.content;
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
});
</script>
<style scoped>
.settings-grid :deep(.el-select),
.settings-grid :deep(.el-input) {
  width: 100%;
}
.editorial-actions {
  margin-top: 18px;
}
</style>
