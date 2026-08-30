<template>
  <el-container direction="vertical" class="admin-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <div>
          <ElButton link :icon="ArrowLeft" @click="router.push('/admin/contests')">{{
            t('返回比赛列表')
          }}</ElButton>
          <h1>{{ contest?.name ?? t('比赛管理') }}</h1>
        </div>
        <div v-if="contest" class="admin-page-actions">
          <ElButton v-if="session.isSuperAdmin.value" plain @click="openClone">{{
            t('克隆比赛')
          }}</ElButton>
          <ElButton plain @click="openAnnouncements">{{ t('公告管理') }}</ElButton>
          <ElButton type="primary" plain :icon="Refresh" @click="openBulkRejudge">
            {{ t('批量重判工作台') }}
          </ElButton>
          <ElTag size="large" :type="contest.status === 'RUNNING' ? 'success' : 'info'">
            {{ contestStatusLabel(contest.status) }}
          </ElTag>
        </div>
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

      <ElTabs v-model="activeTab" class="admin-tabs">
        <ElTabPane :label="t('概览与生命周期')" name="overview">
          <OverviewTab
            :contest-id="contestId"
            :contest="contest"
            @scoring-changed="scoringMode = $event"
            @contest-changed="loadContest"
          />
        </ElTabPane>
        <ElTabPane :label="t('队伍分配')" name="teams">
          <TeamsTab :contest-id="contestId" :teams="teams" v-model:contest-teams="contestTeams" />
        </ElTabPane>
        <ElTabPane :label="t('题目分配')" name="problems">
          <ProblemsTab
            :contest-id="contestId"
            :problems="problems"
            v-model:contest-problems="contestProblems"
            :scoring-mode="scoringMode"
            :can-edit-problem-configuration="canEditProblemConfiguration"
          />
        </ElTabPane>
        <ElTabPane :label="t('提交与重判')" name="submissions">
          <SubmissionsTab
            :contest-id="contestId"
            :contest-name="contest?.name ?? null"
            :problems="problems"
            :contest-problems="contestProblems"
          />
        </ElTabPane>
      </ElTabs>

      <ElDialog v-model="cloneVisible" :title="t('克隆比赛')" width="620">
        <ElAlert
          :title="t('将复制题目配置；可选择同时复制当前有效队伍。新比赛始终从草稿状态开始。')"
          type="info"
          :closable="false"
          show-icon
        />
        <ElForm label-position="top" class="dialog-form">
          <ElFormItem :label="t('新比赛名称')"><ElInput v-model="cloneForm.name" /></ElFormItem>
          <ElFormItem :label="t('可见性')">
            <ElRadioGroup v-model="cloneForm.visibility">
              <ElRadioButton value="PRIVATE">{{ t('私有比赛') }}</ElRadioButton>
              <ElRadioButton value="PUBLIC">{{ t('公开比赛') }}</ElRadioButton>
            </ElRadioGroup>
          </ElFormItem>
          <ElRow :gutter="12" class="admin-form-grid">
            <ElCol :xs="24" :sm="8">
              <ElFormItem :label="t('开始时间')"
                ><ElDatePicker v-model="cloneForm.startAt" type="datetime"
              /></ElFormItem>
            </ElCol>
            <ElCol :xs="24" :sm="8">
              <ElFormItem :label="t('封榜时间')"
                ><ElDatePicker v-model="cloneForm.freezeAt" type="datetime"
              /></ElFormItem>
            </ElCol>
            <ElCol :xs="24" :sm="8">
              <ElFormItem :label="t('结束时间')"
                ><ElDatePicker v-model="cloneForm.endAt" type="datetime"
              /></ElFormItem>
            </ElCol>
          </ElRow>
          <ElFormItem
            ><ElCheckbox v-model="cloneForm.copyTeams">{{
              t('复制当前有效队伍及参赛类型')
            }}</ElCheckbox></ElFormItem
          >
          <ElAlert
            v-if="!cloneScheduleComplete"
            :title="t('赛程时间需全部留空，或同时填写开始、封榜和结束时间。')"
            type="warning"
            :closable="false"
            show-icon
          />
        </ElForm>
        <template #footer>
          <ElButton @click="cloneVisible = false">{{ t('取消') }}</ElButton>
          <ElButton
            type="primary"
            :loading="cloning"
            :disabled="!cloneForm.name.trim() || !cloneScheduleComplete"
            @click="cloneContest"
          >
            {{ t('创建副本') }}
          </ElButton>
        </template>
      </ElDialog>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import { ElMessage } from 'element-plus';
import { ArrowLeft, Refresh } from '@element-plus/icons-vue';
import { useRoute, useRouter } from 'vue-router';
import OverviewTab from '../components/contest-detail/OverviewTab.vue';
import ProblemsTab from '../components/contest-detail/ProblemsTab.vue';
import SubmissionsTab from '../components/contest-detail/SubmissionsTab.vue';
import TeamsTab from '../components/contest-detail/TeamsTab.vue';
import { adminContestApi } from '../api/admin-contests';
import { getErrorMessage } from '../api/client';
import { useSession } from '../auth/session';
import type {
  ContestResponse,
  ContestProblem,
  ContestTeamResponse,
  ContestVisibility,
  ProblemResponse,
  TeamResponse,
} from '../api/types';
import { contestStatusLabel } from '../utils/format';
import { useI18n } from '../i18n';

const route = useRoute();
const router = useRouter();
const session = useSession();
const { t } = useI18n();
const contestId = computed(() => Number(route.params.contestId));
const activeTab = ref('overview');
const contest = ref<ContestResponse | null>(null);
const teams = ref<TeamResponse[]>([]);
const contestTeams = ref<ContestTeamResponse[]>([]);
const problems = ref<ProblemResponse[]>([]);
const contestProblems = ref<ContestProblem[]>([]);
const scoringMode = ref('ICPC');
const errorMessage = ref('');
const cloning = ref(false);
const cloneVisible = ref(false);
const cloneForm = reactive({
  name: '',
  visibility: 'PRIVATE' as ContestVisibility,
  startAt: null as Date | null,
  freezeAt: null as Date | null,
  endAt: null as Date | null,
  copyTeams: false,
});

const canEditProblemConfiguration = computed(() => contest.value?.status === 'DRAFT');
const cloneScheduleComplete = computed(() => {
  const count = [cloneForm.startAt, cloneForm.freezeAt, cloneForm.endAt].filter(Boolean).length;
  return count === 0 || count === 3;
});

async function loadContest() {
  try {
    contest.value = await adminContestApi.getContest(contestId.value);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}

let generation = 0;
async function loadAll() {
  const request = ++generation;
  errorMessage.value = '';
  try {
    const [contestValue, teamPage, assignedTeamPage, allProblems, assignedProblems] =
      await Promise.all([
        adminContestApi.getContest(contestId.value),
        adminContestApi.listTeams(),
        adminContestApi.listContestTeams(contestId.value),
        adminContestApi.listAllProblems(contestId.value),
        adminContestApi.listContestProblems(contestId.value),
      ]);
    if (request !== generation) return;
    contest.value = contestValue;
    teams.value = teamPage.content;
    contestTeams.value = assignedTeamPage;
    problems.value = allProblems;
    contestProblems.value = assignedProblems;
  } catch (error) {
    if (request === generation) errorMessage.value = getErrorMessage(error);
  }
}

function openClone() {
  if (!contest.value || !session.isSuperAdmin.value) return;
  cloneForm.name = t('{name} 副本', { name: contest.value.name });
  cloneForm.visibility = contest.value.visibility as ContestVisibility;
  cloneForm.startAt = null;
  cloneForm.freezeAt = null;
  cloneForm.endAt = null;
  cloneForm.copyTeams = false;
  cloneVisible.value = true;
}

async function cloneContest() {
  if (!cloneForm.name.trim() || !cloneScheduleComplete.value) return;
  cloning.value = true;
  try {
    const result = await adminContestApi.cloneContest(contestId.value, {
      name: cloneForm.name.trim(),
      visibility: cloneForm.visibility,
      startAt: cloneForm.startAt?.toISOString() ?? null,
      freezeAt: cloneForm.freezeAt?.toISOString() ?? null,
      endAt: cloneForm.endAt?.toISOString() ?? null,
      copyTeams: cloneForm.copyTeams,
    });
    cloneVisible.value = false;
    ElMessage.success(
      t('比赛已克隆，复制 {problems} 道题目、{teams} 支队伍', {
        problems: result.problemsCopied,
        teams: result.teamsCopied,
      }),
    );
    await router.push(`/admin/contests/${result.contest.id}`);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    cloning.value = false;
  }
}

function openBulkRejudge() {
  void router.push(`/admin/contests/${contestId.value}/rejudge-tasks`);
}

function openAnnouncements() {
  void router.push(`/admin/contests/${contestId.value}/announcements`);
}

watch(
  contestId,
  () => {
    activeTab.value = 'overview';
    cloneVisible.value = false;
    void loadAll();
  },
  { immediate: true },
);
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
.admin-page-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
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
.admin-form-grid :deep(.el-date-editor) {
  width: 100%;
}
@media (max-width: 680px) {
  .admin-page-header {
    align-items: stretch;
    flex-direction: column;
    gap: 14px;
  }
}
</style>
