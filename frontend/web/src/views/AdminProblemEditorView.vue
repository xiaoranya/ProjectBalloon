<template>
  <el-container direction="vertical" class="admin-page">
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
  ProblemResponse,
  ProblemAttachmentResponse,
  ProblemStatementResponse,
  ProblemTestdataVersionResponse,
} from '../api/types';
import { useI18n } from '../i18n';
import { numericQueryId } from '../utils/route-params';

const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const isNew = computed(
  () => route.params.problemId === undefined || route.params.problemId === 'new',
);
const problemId = computed(() => Number(route.params.problemId));
const problem = ref<ProblemResponse | null>(null);
const statements = ref<ProblemStatementResponse[]>([]);
const attachments = ref<ProblemAttachmentResponse[]>([]);
const testdataVersions = ref<ProblemTestdataVersionResponse[]>([]);
const activeTab = ref('basic');
const loading = ref(false);
const basicSaving = ref(false);
const errorMessage = ref('');
const basicTabRef = ref<{ save: () => Promise<void> }>();

function applyRefreshedProblem(value: ProblemResponse | null) {
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
  const contestId = numericQueryId(route.query.contestId);
  void router.push(contestId ? `/admin/contests/${contestId}` : '/admin/problems');
}

onMounted(loadProblem);
</script>

<style scoped>
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
