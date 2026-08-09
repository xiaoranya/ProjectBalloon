<template>
  <ElCard shadow="never">
    <ElSpace wrap :size="12" class="assignment-toolbar">
      <strong>{{ t('判题队列') }}</strong>
      <ElTag v-if="judgeQueueStatus" :type="judgeQueueStatus.drained ? 'success' : 'warning'">
        {{ judgeQueueStatus.drained ? t('已排空') : t('处理中') }}
      </ElTag>
      <span v-if="judgeQueueStatus">
        {{
          t('待发布 {outbox} · 等待判题 {pending} · 判题中 {judging} · 发布失败 {failed}', {
            outbox: judgeQueueStatus.outboxPending,
            pending: judgeQueueStatus.pendingSubmissions,
            judging: judgeQueueStatus.judgingSubmissions,
            failed: judgeQueueStatus.outboxFailed,
          })
        }}
      </span>
      <ElButton link :loading="queueLoading" @click="loadJudgeQueueStatus">{{
        t('刷新状态')
      }}</ElButton>
    </ElSpace>
  </ElCard>
  <ElCard shadow="never">
    <template #header>
      <ElSpace wrap :size="12" class="assignment-toolbar">
        <strong>{{ t('数据导出') }}</strong>
        <ElButton
          :icon="Download"
          :loading="exporting === 'scoreboard'"
          @click="downloadExport('scoreboard')"
        >
          {{ t('榜单 CSV') }}
        </ElButton>
        <ElButton
          :icon="Download"
          :loading="exporting === 'submissions'"
          @click="downloadExport('submissions')"
        >
          {{ t('提交 CSV') }}
        </ElButton>
        <ElButton
          :icon="Download"
          :loading="exporting === 'sources'"
          @click="downloadExport('sources')"
        >
          {{ t('源码 ZIP') }}
        </ElButton>
      </ElSpace>
    </template>
    <ElTable :data="submissions" row-key="id">
      <ElTableColumn prop="id" :label="t('提交 ID')" width="110" />
      <ElTableColumn :label="t('题目')" min-width="170">
        <template #default="{ row }">{{ problemName(row.problemId) }}</template>
      </ElTableColumn>
      <ElTableColumn prop="teamId" :label="t('队伍 ID')" width="110" />
      <ElTableColumn prop="language" :label="t('语言')" width="110" />
      <ElTableColumn :label="t('状态')" width="140">
        <template #default="{ row }">
          <ElTag :type="statusTagType(row.status)" effect="light">{{
            submissionStatusLabel(row.status)
          }}</ElTag>
        </template>
      </ElTableColumn>
      <ElTableColumn :label="t('提交时间')" min-width="170">
        <template #default="{ row }">{{ formatDateTime(row.submittedAt) }}</template>
      </ElTableColumn>
      <ElTableColumn :label="t('操作')" width="160" fixed="right">
        <template #default="{ row }">
          <ElButton link @click="openSubmissionDetail(row.id)">{{ t('详情') }}</ElButton>
          <ElButton
            link
            type="primary"
            :disabled="!row.activeJudgementId || !row.judgedAt"
            :loading="rejudgingId === row.id"
            @click="rejudge(row.id)"
          >
            {{ t('重判') }}
          </ElButton>
        </template>
      </ElTableColumn>
      <template #empty><ElEmpty :description="t('暂无提交')" /></template>
    </ElTable>
    <ElRow justify="end" class="pagination-row">
      <ElPagination
        v-model:current-page="submissionCurrentPage"
        :page-size="submissionPage.size"
        :total="submissionPage.totalElements"
        layout="prev, pager, next, total"
        @current-change="loadSubmissions"
      />
    </ElRow>
  </ElCard>
  <ElCard shadow="never">
    <template #header>
      <ElSpace wrap :size="12" class="assignment-toolbar">
        <strong>{{ t('P2 源码相似度审核') }}</strong>
        <ElSelect
          v-model="similarityProblemId"
          clearable
          :placeholder="t('全部题目')"
          style="width: 180px"
        >
          <ElOption
            v-for="problem in sortedContestProblems"
            :key="problem.problemId"
            :label="problem.alias"
            :value="problem.problemId"
          />
        </ElSelect>
        <span>{{ t('最低相似度') }}</span>
        <ElInputNumber v-model="similarityThreshold" :min="50" :max="100" :step="1" />
        <ElButton type="primary" plain :loading="similarityLoading" @click="loadSimilarityPairs">{{
          t('扫描候选')
        }}</ElButton>
        <ElButton plain :loading="similarityBackfillLoading" @click="backfillSimilarity">{{
          t('历史回填')
        }}</ElButton>
      </ElSpace>
    </template>
    <ElAlert
      :title="t('相似度结果仅用于人工复核，不会自动处罚或改变判题结果。')"
      type="info"
      :closable="false"
      show-icon
    />
    <ElTable :data="similarityPairs" row-key="submissionId" style="margin-top: 12px">
      <ElTableColumn :label="t('题目')" width="100"
        ><template #default="{ row }">{{ problemName(row.problemId) }}</template></ElTableColumn
      >
      <ElTableColumn prop="language" :label="t('语言')" width="90" />
      <ElTableColumn :label="t('提交 A')" min-width="170"
        ><template #default="{ row }"
          ><ElButton link @click="openSubmissionDetail(row.submissionId)"
            >#{{ row.submissionId }}</ElButton
          >
          · {{ t('队伍') }} {{ row.teamId }}</template
        ></ElTableColumn
      >
      <ElTableColumn :label="t('提交 B')" min-width="170"
        ><template #default="{ row }"
          ><ElButton link @click="openSubmissionDetail(row.otherSubmissionId)"
            >#{{ row.otherSubmissionId }}</ElButton
          >
          · {{ t('队伍') }} {{ row.otherTeamId }}</template
        ></ElTableColumn
      >
      <ElTableColumn :label="t('相似度')" width="110"
        ><template #default="{ row }"
          ><ElTag type="warning">{{ row.similarityPercent }}%</ElTag></template
        ></ElTableColumn
      >
      <template #empty><ElEmpty :description="t('没有达到阈值的跨队候选')" /></template>
    </ElTable>
  </ElCard>

  <ElDialog v-model="submissionDetailVisible" :title="t('管理员提交详情')" width="900">
    <ElSkeleton v-if="submissionDetailLoading" :rows="6" animated />
    <template v-else-if="submissionDetail">
      <ElDescriptions :column="2" border>
        <ElDescriptionsItem :label="t('提交')">#{{ submissionDetail.id }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="t('状态')">{{
          submissionStatusLabel(submissionDetail.status)
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="t('队伍')">{{ submissionDetail.teamName }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="t('题目')">{{
          submissionDetail.problemAlias
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="t('语言')">{{ submissionDetail.language }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="t('提交时间')">{{
          formatDateTime(submissionDetail.submittedAt)
        }}</ElDescriptionsItem>
      </ElDescriptions>
      <ElCard shadow="never" class="source-card submission-detail-source">
        <template #header
          ><strong>{{ t('提交源码') }}</strong></template
        >
        <ElEmpty
          v-if="submissionDetail.language === 'output'"
          :description="t('输出题提交为 ZIP 归档，不支持在线查看')"
        /><CodeEditor
          v-else
          v-model="submissionDetail.source"
          readonly
          :language="submissionDetail.language"
          height="320px"
        />
      </ElCard>
    </template>
  </ElDialog>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Download } from '@element-plus/icons-vue';
import { adminContestApi, type JudgeQueueStatus } from '../../api/admin-contests';
import { ApiError, getErrorMessage } from '../../api/client';
import CodeEditor from '../CodeEditor.vue';
import type {
  ContestProblem,
  PageResponse,
  Problem,
  SubmissionDetail,
  SubmissionSimilarityPair,
  SubmissionSummary,
} from '../../api/types';
import { formatDateTime, statusTagType, submissionStatusLabel } from '../../utils/format';
import { useI18n } from '../../i18n';

const props = defineProps<{
  contestId: number;
  contestName: string | null;
  problems: Problem[];
  contestProblems: ContestProblem[];
}>();
const { t } = useI18n();

const submissionPage = ref<PageResponse<SubmissionSummary>>({
  content: [],
  page: 0,
  size: 30,
  totalElements: 0,
  totalPages: 0,
});
const submissionCurrentPage = ref(1);
const submissions = computed(() => submissionPage.value.content);
const judgeQueueStatus = ref<JudgeQueueStatus | null>(null);
const queueLoading = ref(false);
const exporting = ref<'scoreboard' | 'submissions' | 'sources' | null>(null);
const rejudgingId = ref<number | null>(null);
const submissionDetailVisible = ref(false);
const submissionDetailLoading = ref(false);
const submissionDetail = ref<SubmissionDetail | null>(null);
const similarityPairs = ref<SubmissionSimilarityPair[]>([]);
const similarityLoading = ref(false);
const similarityBackfillLoading = ref(false);
const similarityThreshold = ref(85);
const similarityProblemId = ref<number | undefined>();

const sortedContestProblems = computed(() =>
  [...props.contestProblems].sort((a, b) => a.displayOrder - b.displayOrder),
);

onMounted(async () => {
  try {
    submissionPage.value = await adminContestApi.listSubmissions(props.contestId);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
  void loadJudgeQueueStatus();
});

async function loadSubmissions() {
  try {
    submissionPage.value = await adminContestApi.listSubmissions(props.contestId, {
      page: submissionCurrentPage.value - 1,
      size: submissionPage.value.size,
    });
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}

async function loadJudgeQueueStatus() {
  queueLoading.value = true;
  try {
    judgeQueueStatus.value = await adminContestApi.getJudgeQueueStatus(props.contestId);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    queueLoading.value = false;
  }
}

async function openSubmissionDetail(submissionId: number) {
  submissionDetailVisible.value = true;
  submissionDetailLoading.value = true;
  submissionDetail.value = null;
  try {
    submissionDetail.value = await adminContestApi.getSubmission(props.contestId, submissionId);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
    submissionDetailVisible.value = false;
  } finally {
    submissionDetailLoading.value = false;
  }
}

async function rejudge(submissionId: number) {
  try {
    await ElMessageBox.confirm(
      t('确认重判提交 #{id}？原判定将被标记为历史记录。', { id: submissionId }),
      t('单次重判'),
      {
        type: 'warning',
      },
    );
  } catch {
    return;
  }
  rejudgingId.value = submissionId;
  try {
    const row = submissions.value.find((item) => item.id === submissionId);
    if (!row?.activeJudgementId) {
      ElMessage.error(t('该提交当前没有可重判的有效判定，请刷新后重试'));
      return;
    }
    const result = await adminContestApi.rejudgeSubmission(
      props.contestId,
      submissionId,
      row.activeJudgementId,
    );
    ElMessage.success(t('提交 #{id} 已重新入队', { id: result.submissionId }));
    submissionPage.value = await adminContestApi.listSubmissions(props.contestId, {
      page: submissionCurrentPage.value - 1,
      size: submissionPage.value.size,
    });
    await loadJudgeQueueStatus();
  } catch (error) {
    if (error instanceof ApiError && error.code === 'JUDGEMENT_VERSION_STALE') {
      ElMessage.error(t('该提交的有效判定已被其他管理员更新，已刷新列表，请重新确认'));
      await loadSubmissions();
    } else {
      ElMessage.error(getErrorMessage(error));
    }
  } finally {
    rejudgingId.value = null;
  }
}

async function downloadExport(kind: 'scoreboard' | 'submissions' | 'sources') {
  if (exporting.value) return;
  exporting.value = kind;
  try {
    const blob =
      kind === 'scoreboard'
        ? await adminContestApi.exportScoreboard(props.contestId)
        : kind === 'submissions'
          ? await adminContestApi.exportSubmissions(props.contestId)
          : await adminContestApi.exportSubmissionSources(props.contestId);
    const contestName = (props.contestName ?? `contest-${props.contestId}`).replace(
      /[\\/:*?"<>|]/g,
      '_',
    );
    const suffix = {
      scoreboard: 'scoreboard.csv',
      submissions: 'submissions.csv',
      sources: 'submission-sources.zip',
    }[kind];
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `${contestName}-${suffix}`;
    anchor.click();
    URL.revokeObjectURL(url);
    ElMessage.success(t('导出文件已生成'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    exporting.value = null;
  }
}

async function loadSimilarityPairs() {
  similarityLoading.value = true;
  try {
    similarityPairs.value = await adminContestApi.listSubmissionSimilarityPairs(props.contestId, {
      problemId: similarityProblemId.value,
      minSimilarityPercent: similarityThreshold.value,
    });
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    similarityLoading.value = false;
  }
}

async function backfillSimilarity() {
  similarityBackfillLoading.value = true;
  try {
    const result = await adminContestApi.backfillSubmissionSimilarity(props.contestId);
    ElMessage.success(
      t('已扫描 {scanned}，更新 {updated}，失败 {failed}', {
        scanned: result.scanned,
        updated: result.updated,
        failed: result.failed,
      }),
    );
    await loadSimilarityPairs();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    similarityBackfillLoading.value = false;
  }
}

function problemName(problemId: number) {
  const problem = props.problems.find((item) => item.id === problemId);
  const assigned = props.contestProblems.find((item) => item.problemId === problemId);
  return problem
    ? `${assigned?.alias ? `${assigned.alias} · ` : ''}${problem.title}`
    : t('题目 #{id}', { id: problemId });
}
</script>

<style scoped>
.assignment-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  margin-bottom: 22px;
  padding: 18px;
  border-radius: 0;
  background: #f7f9fc;
}
.pagination-row {
  margin-top: 24px;
}
.source-card {
  border-radius: 0;
}
.source-card pre {
  overflow-x: auto;
  border-radius: 0;
  padding: 18px;
  color: #dbeafe;
  background: #101827;
  font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', monospace;
  font-size: 13px;
  line-height: 1.65;
  max-height: 540px;
  margin: 0;
  white-space: pre;
}
</style>
