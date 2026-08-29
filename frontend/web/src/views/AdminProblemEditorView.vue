<template>
  <el-container direction="vertical" class="admin-page problem-editor-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <div>
          <ElButton link :icon="ArrowLeft" @click="backToList">{{ t('返回题库') }}</ElButton>
          <h1>{{ isNew ? t('创建题目') : (problem?.title ?? t('题目编辑')) }}</h1>
        </div>
        <ElButton type="primary" :loading="basicSaving" @click="basicTabRef?.save()">{{
          isNew ? t('创建题目') : t('保存基本信息')
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

      <ElSkeleton v-if="loading" :rows="8" animated />
      <ElTabs v-else v-model="activeTab" class="admin-tabs">
        <ElTabPane :label="t('基本信息')" name="basic">
          <BasicInfoTab
            ref="basicTabRef"
            :problem="problem"
            :is-new="isNew"
            @problem-refreshed="applyRefreshedProblem"
            @error-message="errorMessage = $event"
            @saving-changed="basicSaving = $event"
          />
        </ElTabPane>
        <ElTabPane :label="t('多语言题面')" name="statements" :disabled="isNew">
          <StatementsTab
            :problem="problem"
            :statements="statements"
            @problem-refreshed="applyRefreshedProblem"
            @error-message="errorMessage = $event"
          />
        </ElTabPane>
        <ElTabPane :label="t('附件')" name="attachments" :disabled="isNew">
          <AttachmentsTab
            :problem="problem"
            :initial-attachments="attachments"
            @problem-refreshed="applyRefreshedProblem"
            @error-message="errorMessage = $event"
          />
        </ElTabPane>
        <ElTabPane :label="t('测试数据')" name="testdata" :disabled="isNew">
          <TestdataTab
            :problem="problem"
            :initial-testdata-versions="testdataVersions"
            @problem-refreshed="applyRefreshedProblem"
            @error-message="errorMessage = $event"
          />
        </ElTabPane>
        <ElTabPane :label="t('发布')" name="publication" :disabled="isNew" lazy>
          <PublicationPanel v-if="problem" :problem-id="problem.id" />
        </ElTabPane>
      </ElTabs>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { ArrowLeft } from '@element-plus/icons-vue';
import { useRoute, useRouter } from 'vue-router';
import AttachmentsTab from '../components/problem-editor/AttachmentsTab.vue';
import BasicInfoTab from '../components/problem-editor/BasicInfoTab.vue';
import PublicationPanel from '../components/problem-editor/PublicationPanel.vue';
import StatementsTab from '../components/problem-editor/StatementsTab.vue';
import TestdataTab from '../components/problem-editor/TestdataTab.vue';
import { adminProblemApi } from '../api/admin-problems';
import { getErrorMessage } from '../api/client';
import type {
  Problem,
  ProblemAttachment,
  ProblemStatement,
  ProblemTestdataVersion,
} from '../api/types';
import { useI18n } from '../i18n';

const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const isNew = computed(
  () => route.params.problemId === undefined || route.params.problemId === 'new',
);
const problemId = computed(() => Number(route.params.problemId));
const problem = ref<Problem | null>(null);
const statements = ref<ProblemStatement[]>([]);
const attachments = ref<ProblemAttachment[]>([]);
const testdataVersions = ref<ProblemTestdataVersion[]>([]);
const activeTab = ref('basic');
const loading = ref(false);
const basicSaving = ref(false);
const errorMessage = ref('');
const basicTabRef = ref<{ save: () => Promise<void> }>();

function applyRefreshedProblem(value: Problem | null) {
  if (value) {
    // Child-resource mutations only need the new concurrency token and test-data pointer;
    // preserve unsaved metadata currently being edited in the form, and never regress when
    // concurrently running refreshes resolve out of order.
    if (!problem.value || value.version >= problem.value.version) problem.value = value;
  } else {
    problem.value = null;
    errorMessage.value = t('操作已成功，但题目版本刷新失败。请重新打开本页后再继续修改。');
  }
}

async function loadProblem() {
  if (isNew.value) return;
  if (!Number.isInteger(problemId.value) || problemId.value <= 0) {
    errorMessage.value = t('题目编号不正确');
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
    problem.value = loadedProblem;
    testdataVersions.value = versions;
    attachments.value = loadedAttachments;
    statements.value = loadedStatements;
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}

function backToList() {
  const contestId = Number(route.query.contestId);
  void router.push(
    Number.isInteger(contestId) && contestId > 0
      ? `/admin/contests/${contestId}`
      : '/admin/problems',
  );
}

onMounted(loadProblem);
</script>

<style scoped>
.admin-page {
  width: min(1320px, 100%);
  margin: 0 auto;
}
.problem-editor-page {
  max-width: 1500px;
}
.page-head {
  height: auto;
  padding: 42px 42px 0;
}
.page-body {
  padding: 0 42px 42px;
}
.admin-page-header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  margin-bottom: 28px;
}
.admin-page-header.compact {
  align-items: center;
}
.admin-page-header h1 {
  margin: 5px 0 6px;
  font-size: clamp(28px, 4vw, 40px);
  color: #13213b;
}
.page-alert {
  margin-bottom: 20px;
}
.admin-tabs :deep(.el-tabs__header) {
  margin-bottom: 22px;
}
.admin-tabs :deep(.el-tabs__header .el-tabs__item) {
  height: 48px;
  font-size: 15px;
}
@media (max-width: 680px) {
  .admin-page-header {
    align-items: stretch;
    flex-direction: column;
    gap: 14px;
  }
}
</style>
