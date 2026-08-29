<template>
  <el-container direction="vertical" class="admin-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <h1>{{ t('题库管理') }}</h1>
        <ElButton type="primary" :icon="Plus" @click="createProblem">{{ t('创建题目') }}</ElButton>
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

      <ElCard shadow="never">
        <ElTable v-loading="loading" :data="page.content" row-key="id" @row-click="editProblem">
          <ElTableColumn :label="t('题目')" min-width="280">
            <template #default="{ row }">
              <div class="admin-primary-cell">
                <strong>{{ row.title }}</strong>
                <small>{{ row.slug }} · #{{ row.id }}</small>
              </div>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('语言')" min-width="190">
            <template #default="{ row }">
              <ElTag
                v-for="language in row.languages"
                :key="language"
                class="problem-language-tag"
                effect="plain"
              >
                {{ languageLabel(language) }}
              </ElTag>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('资源限制')" min-width="220">
            <template #default="{ row }"
              >{{ row.timeLimitMs }} ms · {{ row.memoryLimitMb }} MiB ·
              {{ row.outputLimitKb }} KiB</template
            >
          </ElTableColumn>
          <ElTableColumn :label="t('测试数据')" width="130">
            <template #default="{ row }">
              <ElTag :type="row.testdataVersion > 0 ? 'success' : 'info'"
                >v{{ row.testdataVersion }}</ElTag
              >
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('版本')" width="90"
            ><template #default="{ row }">{{ row.version }}</template></ElTableColumn
          >
          <ElTableColumn :label="t('操作')" width="200" fixed="right">
            <template #default="{ row }">
              <ElButton link type="primary" @click.stop="editProblem(row as Problem)">{{
                t('编辑')
              }}</ElButton>
              <ElButton link type="success" @click.stop="openPublication(row as Problem)">{{
                t('发布')
              }}</ElButton>
              <ElButton link type="danger" @click.stop="removeProblem(row as Problem)">{{
                t('删除')
              }}</ElButton>
            </template>
          </ElTableColumn>
          <template #empty><ElEmpty :description="t('暂无题目')" /></template>
        </ElTable>
        <ElRow justify="end" class="pagination-row">
          <ElPagination
            v-model:current-page="currentPage"
            :page-size="page.size"
            :total="page.totalElements"
            layout="prev, pager, next, total"
            @current-change="loadProblems"
          />
        </ElRow>
      </ElCard>

      <ElDialog
        v-model="publicationVisible"
        :title="t('发布题目：{title}', { title: publicationProblem?.title ?? '' })"
        width="min(880px, 94%)"
        destroy-on-hide
      >
        <PublicationPanel
          v-if="publicationProblem"
          :key="publicationProblem.id"
          :problem-id="publicationProblem.id"
        />
      </ElDialog>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { Plus } from '@element-plus/icons-vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { useRouter } from 'vue-router';
import { adminProblemApi } from '../api/admin-problems';
import { getErrorMessage } from '../api/client';
import PublicationPanel from '../components/problem-editor/PublicationPanel.vue';
import type { PageResponse, Problem } from '../api/types';
import { languageLabel } from '../utils/format';
import { useI18n } from '../i18n';

const router = useRouter();
const { t } = useI18n();
const page = ref<PageResponse<Problem>>({
  content: [],
  page: 0,
  size: 50,
  totalElements: 0,
  totalPages: 0,
});
const currentPage = ref(1);
const loading = ref(false);
const errorMessage = ref('');
const publicationVisible = ref(false);
const publicationProblem = ref<Problem | null>(null);

let loadGeneration = 0;
async function loadProblems() {
  const generation = ++loadGeneration;
  const requestedPage = currentPage.value - 1;
  loading.value = true;
  errorMessage.value = '';
  try {
    const result = await adminProblemApi.listProblems(requestedPage, page.value.size);
    if (generation === loadGeneration) page.value = result;
  } catch (error) {
    if (generation === loadGeneration) errorMessage.value = getErrorMessage(error);
  } finally {
    if (generation === loadGeneration) loading.value = false;
  }
}

function createProblem() {
  void router.push('/admin/problems/new');
}

function editProblem(row: unknown) {
  void router.push(`/admin/problems/${(row as Problem).id}`);
}

function openPublication(problem: Problem) {
  publicationProblem.value = problem;
  publicationVisible.value = true;
}

async function removeProblem(problem: Problem) {
  try {
    await ElMessageBox.confirm(
      t('确认删除题目“{title}”？已分配到比赛的题目无法删除。', { title: problem.title }),
      t('删除题目'),
      { type: 'warning', confirmButtonText: t('确认删除') },
    );
    await adminProblemApi.deleteProblem(problem.id);
    ElMessage.success(t('题目已删除'));
    await loadProblems();
  } catch (error) {
    if (error !== 'cancel' && error !== 'close') errorMessage.value = getErrorMessage(error);
  }
}

onMounted(loadProblems);
</script>

<style scoped>
.admin-page {
  width: min(1320px, 100%);
  margin: 0 auto;
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
.pagination-row {
  margin-top: 24px;
}
.admin-primary-cell strong,
.admin-primary-cell small {
  display: block;
}
.admin-primary-cell small {
  margin-top: 4px;
  color: var(--muted);
}
.problem-language-tag {
  margin: 2px 4px 2px 0;
}
@media (max-width: 680px) {
  .admin-page-header {
    align-items: stretch;
    flex-direction: column;
    gap: 14px;
  }
}
</style>
