<template>
  <el-container direction="vertical" class="admin-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <div>
          <ElButton link :icon="ArrowLeft" @click="router.push('/admin/contests')">返回比赛列表</ElButton>
          <h1>{{ contest?.name ?? '比赛管理' }}</h1>
        </div>
        <div v-if="contest" class="admin-page-actions">
          <ElButton v-if="session.isSuperAdmin.value" plain @click="openClone">克隆比赛</ElButton>
          <ElButton plain @click="openAnnouncements">公告管理</ElButton>
          <ElButton type="primary" plain :icon="Refresh" @click="openBulkRejudge">
            批量重判工作台
          </ElButton>
          <ElTag size="large" :type="contest.status === 'RUNNING' ? 'success' : 'info'">
            {{ contestStatusLabel(contest.status) }}
          </ElTag>
        </div>
      </div>
    </el-header>

    <el-main class="page-body">
      <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />

    <ElTabs v-model="activeTab" class="admin-tabs">
      <ElTabPane label="概览与生命周期" name="overview">
        <ElRow v-if="contest" :gutter="16" class="admin-two-column">
          <ElCol :xs="24" :md="12">
            <ElCard shadow="never" >
              <template #header><strong>比赛信息</strong></template>
              <ElDescriptions :column="1" border>
                <ElDescriptionsItem label="可见性">{{ contest.visibility === 'PUBLIC' ? '公开' : '私有' }}</ElDescriptionsItem>
                <ElDescriptionsItem label="开始">{{ formatDateTime(contest.startAt) }}</ElDescriptionsItem>
                <ElDescriptionsItem label="封榜">{{ formatDateTime(contest.freezeAt) }}</ElDescriptionsItem>
                <ElDescriptionsItem label="结束">{{ formatDateTime(contest.endAt) }}</ElDescriptionsItem>
              </ElDescriptions>
              <div class="card-action">
                <ElSpace wrap :size="10">
                  <ElButton plain @click="editVisible = true">编辑比赛信息</ElButton>
                  <ElButton v-if="canExtend" type="warning" plain @click="extensionVisible = true">
                    延长比赛
                  </ElButton>
                </ElSpace>
              </div>
            </ElCard>
          </ElCol>

          <ElCol :xs="24" :md="12">
            <ElCard shadow="never" >
              <template #header><strong>生命周期</strong></template>
              <ElSteps direction="vertical" :active="lifecycleIndex" finish-status="success">
                <ElStep v-for="status in lifecycle" :key="status" :title="contestStatusLabel(status)" />
              </ElSteps>
              <div class="lifecycle-actions">
                <ElButton
                  v-for="status in nextStatuses"
                  :key="status"
                  type="primary"
                  :loading="transitioning"
                  @click="transition(status)"
                >
                  {{ transitionLabel(status) }}
                </ElButton>
                <p v-if="nextStatuses.length === 0">当前状态没有后续操作。</p>
              </div>
            </ElCard>
          </ElCol>

          <ElCol :xs="24">
            <ElCard shadow="never" >
              <template #header><strong>OI / IOI 计分策略</strong></template>
              <ElForm label-position="top">
                <ElRow :gutter="12" class="admin-form-grid">
                  <ElCol :xs="24" :sm="8">
                    <ElFormItem label="赛制">
                      <ElSelect v-model="scoringForm.scoringMode" :disabled="!canEditProblemConfiguration">
                        <ElOption label="ICPC（解题数 / 罚时）" value="ICPC" />
                        <ElOption label="OI（积分）" value="OI" />
                        <ElOption label="IOI（积分）" value="IOI" />
                      </ElSelect>
                    </ElFormItem>
                  </ElCol>
                  <ElCol :xs="24" :sm="8">
                    <ElFormItem label="有效提交">
                      <ElSelect v-model="scoringForm.scoreAggregation" :disabled="!canEditProblemConfiguration || scoringForm.scoringMode === 'ICPC'">
                        <ElOption label="最高分" value="BEST" />
                        <ElOption label="最后一次" value="LAST" />
                      </ElSelect>
                    </ElFormItem>
                  </ElCol>
                  <ElCol :xs="24" :sm="8">
                    <ElFormItem label="比赛中反馈">
                      <ElSelect v-model="scoringForm.feedbackPolicy" :disabled="!canEditProblemConfiguration">
                        <ElOption label="完整测试点" value="FULL" />
                        <ElOption label="仅总分" value="SCORE_ONLY" />
                        <ElOption label="不反馈" value="NONE" />
                      </ElSelect>
                    </ElFormItem>
                  </ElCol>
                </ElRow>
              </ElForm>
              <div class="card-action">
                <ElSpace wrap :size="10">
                  <ElButton type="primary" :loading="savingScoring" :disabled="!canEditProblemConfiguration" @click="saveScoringPolicy">保存计分策略</ElButton>
                </ElSpace>
              </div>
            </ElCard>
          </ElCol>
        </ElRow>
      </ElTabPane>

      <ElTabPane label="队伍分配" name="teams">
        <ElCard shadow="never" >
          <ElSpace wrap :size="12" class="assignment-toolbar">
            <ElSelect v-model="teamForm.teamId" filterable placeholder="选择未分配队伍" style="width: 260px">
              <ElOption v-for="team in availableTeams" :key="team.id" :label="team.name" :value="team.id" />
            </ElSelect>
            <ElSelect v-model="teamForm.participationType" style="width: 150px">
              <ElOption label="正式参赛" value="OFFICIAL" />
              <ElOption label="打星队伍" value="STAR" />
              <ElOption label="练习队伍" value="PRACTICE" />
            </ElSelect>
            <ElInput v-model="teamForm.groupName" placeholder="分组（可选）" style="width: 180px" />
            <ElButton type="primary" :disabled="!teamForm.teamId" :loading="assigning" @click="assignTeam">分配队伍</ElButton>
          </ElSpace>
          <ElTable :data="contestTeams" row-key="teamId">
            <ElTableColumn prop="teamName" label="队伍" min-width="220" />
            <ElTableColumn prop="participationType" label="参赛类型" width="140">
              <template #default="{ row }">{{ participationLabel(row.participationType) }}</template>
            </ElTableColumn>
            <ElTableColumn prop="groupName" label="分组" min-width="140">
              <template #default="{ row }">{{ row.groupName || '—' }}</template>
            </ElTableColumn>
            <ElTableColumn label="操作" width="100">
              <template #default="{ row }">
                <ElButton link type="danger" @click="unassignTeam(row.teamId)">移除</ElButton>
              </template>
            </ElTableColumn>
            <template #empty><ElEmpty description="尚未分配队伍" /></template>
          </ElTable>
        </ElCard>
      </ElTabPane>

      <ElTabPane label="题目分配" name="problems">
        <ElCard shadow="never" >
          <ElSpace wrap :size="12" class="assignment-toolbar">
            <ElSelect v-model="problemForm.problemId" filterable placeholder="选择未分配题目" style="width: 260px">
              <ElOption
                v-for="problem in availableProblems"
                :key="problem.id"
                :label="`${problem.slug} · ${problem.title}`"
                :value="problem.id"
              />
            </ElSelect>
            <ElInput v-model="problemForm.alias" placeholder="题号，如 A" style="width: 120px" />
            <ElInputNumber v-model="problemForm.displayOrder" :min="1" :max="1000" controls-position="right" />
            <ElColorPicker v-model="problemForm.color" />
            <ElButton
              type="primary"
              :disabled="!canEditProblemConfiguration || !problemForm.problemId || !problemForm.alias.trim()"
              :loading="assigning"
              @click="assignProblem"
            >
              分配题目
            </ElButton>
          </ElSpace>
          <ElTable :data="sortedContestProblems" row-key="problemId">
            <ElTableColumn prop="alias" label="题号" width="90" />
            <ElTableColumn label="题目" min-width="260">
              <template #default="{ row }">{{ problemName(row.problemId) }}</template>
            </ElTableColumn>
            <ElTableColumn label="顺序" width="150">
              <template #default="{ row, $index }">
                <ElButton link :disabled="!canEditProblemConfiguration || $index === 0 || reordering" @click="moveProblem($index, -1)">上移</ElButton>
                <ElButton link :disabled="!canEditProblemConfiguration || $index === sortedContestProblems.length - 1 || reordering" @click="moveProblem($index, 1)">下移</ElButton>
              </template>
            </ElTableColumn>
            <ElTableColumn label="颜色" width="110">
              <template #default="{ row }">
                <span class="problem-color"><i :style="{ background: row.color || '#cbd5e1' }" />{{ row.color || '未设置' }}</span>
              </template>
            </ElTableColumn>
            <ElTableColumn label="操作" width="290">
              <template #default="{ row }">
                <ElButton link type="primary" :disabled="!canEditProblemConfiguration" @click="openProblemEdit(row)">编辑</ElButton>
                <ElButton link type="primary" :disabled="!canEditProblemConfiguration || scoringForm.scoringMode === 'ICPC'" @click="openSubtasks(row.problemId)">子任务</ElButton>
                <ElButton link type="primary" @click="openProblemContent(row.problemId)">题目内容</ElButton>
                <ElButton link type="danger" :disabled="!canEditProblemConfiguration" @click="unassignProblem(row.problemId)">移除</ElButton>
              </template>
            </ElTableColumn>
            <template #empty><ElEmpty description="尚未分配题目" /></template>
          </ElTable>
        </ElCard>
      </ElTabPane>

      <ElTabPane label="提交与重判" name="submissions">
        <ElCard shadow="never" >
          <ElSpace wrap :size="12" class="assignment-toolbar">
            <strong>判题队列</strong>
            <ElTag v-if="judgeQueueStatus" :type="judgeQueueStatus.drained ? 'success' : 'warning'">
              {{ judgeQueueStatus.drained ? '已排空' : '处理中' }}
            </ElTag>
            <span v-if="judgeQueueStatus">
              待发布 {{ judgeQueueStatus.outboxPending }} · 等待判题
              {{ judgeQueueStatus.pendingSubmissions }} · 判题中
              {{ judgeQueueStatus.judgingSubmissions }} · 发布失败
              {{ judgeQueueStatus.outboxFailed }}
            </span>
            <ElButton link :loading="queueLoading" @click="loadJudgeQueueStatus">刷新状态</ElButton>
          </ElSpace>
        </ElCard>
        <ElCard shadow="never" >
          <template #header>
            <ElSpace wrap :size="12" class="assignment-toolbar">
              <strong>数据导出</strong>
              <ElButton
                :icon="Download"
                :loading="exporting === 'scoreboard'"
                @click="downloadExport('scoreboard')"
              >
                榜单 CSV
              </ElButton>
              <ElButton
                :icon="Download"
                :loading="exporting === 'submissions'"
                @click="downloadExport('submissions')"
              >
                提交 CSV
              </ElButton>
              <ElButton
                :icon="Download"
                :loading="exporting === 'sources'"
                @click="downloadExport('sources')"
              >
                源码 ZIP
              </ElButton>
            </ElSpace>
          </template>
          <ElTable :data="submissions" row-key="id">
            <ElTableColumn prop="id" label="提交 ID" width="110" />
            <ElTableColumn label="题目" min-width="170">
              <template #default="{ row }">{{ problemName(row.problemId) }}</template>
            </ElTableColumn>
            <ElTableColumn prop="teamId" label="队伍 ID" width="110" />
            <ElTableColumn prop="language" label="语言" width="110" />
            <ElTableColumn label="状态" width="140">
              <template #default="{ row }">
                <ElTag :type="statusTagType(row.status)" effect="light">{{ submissionStatusLabel(row.status) }}</ElTag>
              </template>
            </ElTableColumn>
            <ElTableColumn label="提交时间" min-width="170">
              <template #default="{ row }">{{ formatDateTime(row.submittedAt) }}</template>
            </ElTableColumn>
            <ElTableColumn label="操作" width="160" fixed="right">
              <template #default="{ row }">
                <ElButton link @click="openSubmissionDetail(row.id)">详情</ElButton>
                <ElButton
                  link
                  type="primary"
                  :disabled="!row.activeJudgementId || !row.judgedAt"
                  :loading="rejudgingId === row.id"
                  @click="rejudge(row.id)"
                >
                  重判
                </ElButton>
              </template>
            </ElTableColumn>
            <template #empty><ElEmpty description="暂无提交" /></template>
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
        <ElCard shadow="never" >
          <template #header>
            <ElSpace wrap :size="12" class="assignment-toolbar">
              <strong>P2 源码相似度审核</strong>
              <ElSelect v-model="similarityProblemId" clearable placeholder="全部题目" style="width: 180px">
                <ElOption v-for="problem in sortedContestProblems" :key="problem.problemId" :label="problem.alias" :value="problem.problemId" />
              </ElSelect>
              <span>最低相似度</span>
              <ElInputNumber v-model="similarityThreshold" :min="50" :max="100" :step="1" />
              <ElButton type="primary" plain :loading="similarityLoading" @click="loadSimilarityPairs">扫描候选</ElButton>
              <ElButton plain :loading="similarityBackfillLoading" @click="backfillSimilarity">历史回填</ElButton>
            </ElSpace>
          </template>
          <ElAlert title="相似度结果仅用于人工复核，不会自动处罚或改变判题结果。" type="info" :closable="false" show-icon />
          <ElTable :data="similarityPairs" row-key="submissionId" style="margin-top: 12px">
            <ElTableColumn label="题目" width="100"><template #default="{ row }">{{ problemName(row.problemId) }}</template></ElTableColumn>
            <ElTableColumn prop="language" label="语言" width="90" />
            <ElTableColumn label="提交 A" min-width="170"><template #default="{ row }"><ElButton link @click="openSubmissionDetail(row.submissionId)">#{{ row.submissionId }}</ElButton> · 队伍 {{ row.teamId }}</template></ElTableColumn>
            <ElTableColumn label="提交 B" min-width="170"><template #default="{ row }"><ElButton link @click="openSubmissionDetail(row.otherSubmissionId)">#{{ row.otherSubmissionId }}</ElButton> · 队伍 {{ row.otherTeamId }}</template></ElTableColumn>
            <ElTableColumn label="相似度" width="110"><template #default="{ row }"><ElTag type="warning">{{ row.similarityPercent }}%</ElTag></template></ElTableColumn>
            <template #empty><ElEmpty description="没有达到阈值的跨队候选" /></template>
          </ElTable>
        </ElCard>
      </ElTabPane>
    </ElTabs>

    <ElDialog v-model="problemEditVisible" title="编辑比赛题目" width="460">
      <ElForm label-width="90px">
        <ElFormItem label="题号"><ElInput v-model="problemEditForm.alias" maxlength="16" /></ElFormItem>
        <ElFormItem label="展示顺序">
          <ElInputNumber v-model="problemEditForm.displayOrder" :min="1" :max="1000" />
        </ElFormItem>
        <ElFormItem label="气球颜色"><ElColorPicker v-model="problemEditForm.color" /></ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="problemEditVisible = false">取消</ElButton>
        <ElButton
          type="primary"
          :loading="savingProblem"
          :disabled="!problemEditForm.alias.trim()"
          @click="saveProblemEdit"
        >保存</ElButton>
      </template>
    </ElDialog>

    <ElDialog v-model="subtasksVisible" title="子任务与测试点计分" width="900">
      <ElSkeleton v-if="subtasksLoading" :rows="5" animated />
      <template v-else>
        <ElSpace wrap :size="12" class="assignment-toolbar">
          <ElFormItem label="题目满分（千分之一分）">
            <ElInputNumber v-model="subtaskMaxScoreMilli" :min="1" :max="100000000" />
          </ElFormItem>
          <ElButton plain @click="addSubtask">添加子任务</ElButton>
        </ElSpace>
        <ElTable :data="editableSubtasks" row-key="localId">
          <ElTableColumn label="标识" width="130"><template #default="{ row }"><ElInput v-model="row.subtaskKey" maxlength="32" /></template></ElTableColumn>
          <ElTableColumn label="名称" min-width="150"><template #default="{ row }"><ElInput v-model="row.name" maxlength="120" /></template></ElTableColumn>
          <ElTableColumn label="顺序" width="110"><template #default="{ row }"><ElInputNumber v-model="row.displayOrder" :min="1" :max="1000" controls-position="right" /></template></ElTableColumn>
          <ElTableColumn label="分值（千分之一分）" width="180"><template #default="{ row }"><ElInputNumber v-model="row.scoreMilli" :min="1" :max="subtaskMaxScoreMilli" /></template></ElTableColumn>
          <ElTableColumn label="测试点编号" min-width="210"><template #default="{ row }"><ElInput v-model="row.testIndexes" placeholder="1,2,3" /></template></ElTableColumn>
          <ElTableColumn width="70"><template #default="{ $index }"><ElButton link type="danger" @click="editableSubtasks.splice($index, 1)">删除</ElButton></template></ElTableColumn>
        </ElTable>
        <ElAlert v-if="subtaskValidationError" :title="subtaskValidationError" type="warning" :closable="false" show-icon style="margin-top: 12px" />
      </template>
      <template #footer>
        <ElButton @click="subtasksVisible = false">取消</ElButton>
        <ElButton type="primary" :loading="savingSubtasks" :disabled="Boolean(subtaskValidationError) || subtasksLoading" @click="saveSubtasks">保存子任务</ElButton>
      </template>
    </ElDialog>

    <ElDialog v-model="editVisible" title="编辑比赛信息" width="620">
      <ElForm v-if="contest" label-position="top">
        <ElFormItem label="比赛名称"><ElInput v-model="editForm.name" /></ElFormItem>
        <ElFormItem label="可见性">
          <ElRadioGroup v-model="editForm.visibility">
            <ElRadioButton value="PRIVATE">私有比赛</ElRadioButton>
            <ElRadioButton value="PUBLIC">公开比赛</ElRadioButton>
          </ElRadioGroup>
        </ElFormItem>
        <ElRow :gutter="12" class="admin-form-grid">
          <ElCol :xs="24" :sm="8">
            <ElFormItem label="开始时间"><ElDatePicker v-model="editForm.startAt" type="datetime" :disabled="!canEditSchedule" /></ElFormItem>
          </ElCol>
          <ElCol :xs="24" :sm="8">
            <ElFormItem label="封榜时间"><ElDatePicker v-model="editForm.freezeAt" type="datetime" :disabled="!canEditSchedule" /></ElFormItem>
          </ElCol>
          <ElCol :xs="24" :sm="8">
            <ElFormItem label="结束时间"><ElDatePicker v-model="editForm.endAt" type="datetime" :disabled="!canEditSchedule" /></ElFormItem>
          </ElCol>
        </ElRow>
        <ElAlert
          v-if="!canEditSchedule"
          title="比赛开始后，赛程字段已锁定；如需延后结束，请使用“延长比赛”。"
          type="info"
          :closable="false"
          show-icon
        />
      </ElForm>
      <template #footer>
        <ElButton @click="editVisible = false">取消</ElButton>
        <ElButton type="primary" :loading="saving" @click="saveContest">保存</ElButton>
      </template>
    </ElDialog>

    <ElDialog v-model="cloneVisible" title="克隆比赛" width="620">
      <ElAlert
        title="将复制题目配置；可选择同时复制当前有效队伍。新比赛始终从草稿状态开始。"
        type="info"
        :closable="false"
        show-icon
      />
      <ElForm label-position="top" class="dialog-form">
        <ElFormItem label="新比赛名称"><ElInput v-model="cloneForm.name" /></ElFormItem>
        <ElFormItem label="可见性">
          <ElRadioGroup v-model="cloneForm.visibility">
            <ElRadioButton value="PRIVATE">私有比赛</ElRadioButton>
            <ElRadioButton value="PUBLIC">公开比赛</ElRadioButton>
          </ElRadioGroup>
        </ElFormItem>
        <ElRow :gutter="12" class="admin-form-grid">
          <ElCol :xs="24" :sm="8">
            <ElFormItem label="开始时间"><ElDatePicker v-model="cloneForm.startAt" type="datetime" /></ElFormItem>
          </ElCol>
          <ElCol :xs="24" :sm="8">
            <ElFormItem label="封榜时间"><ElDatePicker v-model="cloneForm.freezeAt" type="datetime" /></ElFormItem>
          </ElCol>
          <ElCol :xs="24" :sm="8">
            <ElFormItem label="结束时间"><ElDatePicker v-model="cloneForm.endAt" type="datetime" /></ElFormItem>
          </ElCol>
        </ElRow>
        <ElFormItem><ElCheckbox v-model="cloneForm.copyTeams">复制当前有效队伍及参赛类型</ElCheckbox></ElFormItem>
        <ElAlert
          v-if="!cloneScheduleComplete"
          title="赛程时间需全部留空，或同时填写开始、封榜和结束时间。"
          type="warning"
          :closable="false"
          show-icon
        />
      </ElForm>
      <template #footer>
        <ElButton @click="cloneVisible = false">取消</ElButton>
        <ElButton
          type="primary"
          :loading="cloning"
          :disabled="!cloneForm.name.trim() || !cloneScheduleComplete"
          @click="cloneContest"
        >
          创建副本
        </ElButton>
      </template>
    </ElDialog>

    <ElDialog v-model="extensionVisible" title="延长比赛" width="520">
      <ElAlert
        title="延时会立即同步到参赛队伍、大屏和直播页面，且不能通过此操作缩短比赛。"
        type="warning"
        :closable="false"
        show-icon
      />
      <ElForm label-position="top" class="dialog-form">
        <ElFormItem label="当前结束时间">
          <ElInput :model-value="formatDateTime(contest?.endAt)" disabled />
        </ElFormItem>
        <ElFormItem label="新的结束时间">
          <ElDatePicker
            v-model="extensionForm.endAt"
            type="datetime"
            style="width: 100%"
            :disabled-date="disableExtensionDate"
          />
        </ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="extensionVisible = false">取消</ElButton>
        <ElButton
          type="warning"
          :loading="extending"
          :disabled="!extensionIsLater"
          @click="extendContest"
        >
          确认延时
        </ElButton>
      </template>
    </ElDialog>

    <ElDialog v-model="submissionDetailVisible" title="管理员提交详情" width="900">
      <ElSkeleton v-if="submissionDetailLoading" :rows="6" animated />
      <template v-else-if="submissionDetail">
        <ElDescriptions :column="2" border>
          <ElDescriptionsItem label="提交">#{{ submissionDetail.id }}</ElDescriptionsItem>
          <ElDescriptionsItem label="状态">{{ submissionStatusLabel(submissionDetail.status) }}</ElDescriptionsItem>
          <ElDescriptionsItem label="队伍">{{ submissionDetail.teamName }}</ElDescriptionsItem>
          <ElDescriptionsItem label="题目">{{ submissionDetail.problemAlias }}</ElDescriptionsItem>
          <ElDescriptionsItem label="语言">{{ submissionDetail.language }}</ElDescriptionsItem>
          <ElDescriptionsItem label="提交时间">{{ formatDateTime(submissionDetail.submittedAt) }}</ElDescriptionsItem>
        </ElDescriptions>
        <ElCard shadow="never" class="source-card submission-detail-source">
          <template #header><strong>提交源码</strong></template>
          <pre><code>{{ submissionDetail.source }}</code></pre>
        </ElCard>
      </template>
    </ElDialog>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { ArrowLeft, Download, Refresh } from '@element-plus/icons-vue';
import { useRoute, useRouter } from 'vue-router';
import { adminContestApi, type JudgeQueueStatus, type ScoringPolicy } from '../api/admin-contests';
import { ApiError, getErrorMessage } from '../api/client';
import { useSession } from '../auth/session';
import type {
  Contest,
  ContestProblem,
  ContestStatus,
  ContestTeam,
  ContestVisibility,
  PageResponse,
  Problem,
  SubmissionDetail,
  SubmissionSimilarityPair,
  SubmissionSummary,
  Team,
} from '../api/types';
import { contestStatusLabel, formatDateTime, statusTagType, submissionStatusLabel } from '../utils/format';

const lifecycle: ContestStatus[] = ['DRAFT', 'FROZEN_CONFIG', 'RUNNING', 'PAUSED', 'ENDED', 'ARCHIVED'];
const transitions: Record<ContestStatus, ContestStatus[]> = {
  DRAFT: ['FROZEN_CONFIG'],
  FROZEN_CONFIG: ['RUNNING'],
  RUNNING: ['PAUSED', 'ENDED'],
  PAUSED: ['RUNNING'],
  ENDED: ['ARCHIVED'],
  ARCHIVED: [],
};

const route = useRoute();
const router = useRouter();
const session = useSession();
const contestId = Number(route.params.contestId);
const activeTab = ref('overview');
const contest = ref<Contest | null>(null);
const teams = ref<Team[]>([]);
const contestTeams = ref<ContestTeam[]>([]);
const problems = ref<Problem[]>([]);
const contestProblems = ref<ContestProblem[]>([]);
const submissionPage = ref<PageResponse<SubmissionSummary>>({ content: [], page: 0, size: 30, totalElements: 0, totalPages: 0 });
const submissionCurrentPage = ref(1);
const submissions = computed(() => submissionPage.value.content);
const judgeQueueStatus = ref<JudgeQueueStatus | null>(null);
const errorMessage = ref('');
const assigning = ref(false);
const reordering = ref(false);
const savingProblem = ref(false);
const saving = ref(false);
const savingScoring = ref(false);
const cloning = ref(false);
const extending = ref(false);
const transitioning = ref(false);
const rejudgingId = ref<number | null>(null);
const queueLoading = ref(false);
const exporting = ref<'scoreboard' | 'submissions' | 'sources' | null>(null);
const editVisible = ref(false);
const cloneVisible = ref(false);
const extensionVisible = ref(false);
const problemEditVisible = ref(false);
const subtasksVisible = ref(false);
const subtasksLoading = ref(false);
const savingSubtasks = ref(false);
const subtaskProblemId = ref<number | null>(null);
const subtaskMaxScoreMilli = ref(100_000);
let subtaskLocalId = 0;
interface EditableSubtask { localId: number; subtaskKey: string; name: string; displayOrder: number; scoreMilli: number; testIndexes: string }
const editableSubtasks = reactive<EditableSubtask[]>([]);
const submissionDetailVisible = ref(false);
const submissionDetailLoading = ref(false);
const submissionDetail = ref<SubmissionDetail | null>(null);
const similarityPairs = ref<SubmissionSimilarityPair[]>([]);
const similarityLoading = ref(false);
const similarityBackfillLoading = ref(false);
const similarityThreshold = ref(85);
const similarityProblemId = ref<number | undefined>();
const scoringForm = reactive<Omit<ScoringPolicy, 'contestId'>>({
  scoringMode: 'ICPC',
  scoreAggregation: 'BEST',
  feedbackPolicy: 'FULL',
});
const editForm = reactive({
  name: '',
  visibility: 'PRIVATE' as ContestVisibility,
  startAt: null as Date | null,
  freezeAt: null as Date | null,
  endAt: null as Date | null,
});
const extensionForm = reactive({
  endAt: null as Date | null,
});
const cloneForm = reactive({
  name: '',
  visibility: 'PRIVATE' as ContestVisibility,
  startAt: null as Date | null,
  freezeAt: null as Date | null,
  endAt: null as Date | null,
  copyTeams: false,
});
const teamForm = reactive({
  teamId: null as number | null,
  participationType: 'OFFICIAL' as ContestTeam['participationType'],
  groupName: '',
});
const problemForm = reactive({
  problemId: null as number | null,
  alias: '',
  displayOrder: 0,
  color: '#2563eb' as string | null,
});
const problemEditForm = reactive({
  problemId: null as number | null,
  alias: '',
  displayOrder: 1,
  color: '#2563eb' as string | null,
});

const nextStatuses = computed(() =>
  contest.value ? transitions[contest.value.status as ContestStatus] ?? [] : [],
);
const canEditSchedule = computed(() =>
  contest.value?.status === 'DRAFT' || contest.value?.status === 'FROZEN_CONFIG',
);
const canEditProblemConfiguration = computed(() => contest.value?.status === 'DRAFT');
const canExtend = computed(() =>
  Boolean(
    contest.value?.endAt
      && (contest.value.status === 'RUNNING' || contest.value.status === 'PAUSED'),
  ),
);
const extensionIsLater = computed(() =>
  Boolean(
    contest.value?.endAt
      && extensionForm.endAt
      && extensionForm.endAt.getTime() > new Date(contest.value.endAt).getTime(),
  ),
);
const cloneScheduleComplete = computed(() => {
  const count = [cloneForm.startAt, cloneForm.freezeAt, cloneForm.endAt].filter(Boolean).length;
  return count === 0 || count === 3;
});
const lifecycleIndex = computed(() => {
  const status = contest.value?.status as ContestStatus | undefined;
  return status ? lifecycle.indexOf(status) : 0;
});
const availableTeams = computed(() => {
  const assigned = new Set(contestTeams.value.map((item) => item.teamId));
  return teams.value.filter((team) => !assigned.has(team.id));
});
const availableProblems = computed(() => {
  const assigned = new Set(contestProblems.value.map((item) => item.problemId));
  return problems.value.filter((problem) => !assigned.has(problem.id));
});
const sortedContestProblems = computed(() =>
  [...contestProblems.value].sort((a, b) => a.displayOrder - b.displayOrder),
);
const subtaskValidationError = computed(() => {
  if (!editableSubtasks.length) return '至少需要一个子任务。';
  const keys = new Set<string>();
  const orders = new Set<number>();
  const tests = new Set<number>();
  let score = 0;
  for (const item of editableSubtasks) {
    const key = item.subtaskKey.trim().toUpperCase();
    if (!/^[A-Z0-9_]{1,32}$/.test(key) || keys.has(key)) return '子任务标识需唯一，并使用大写字母、数字或下划线。';
    if (!item.name.trim()) return '子任务名称不能为空。';
    if (orders.has(item.displayOrder)) return '子任务顺序不能重复。';
    keys.add(key); orders.add(item.displayOrder); score += item.scoreMilli;
    const indexes = parseTestIndexes(item.testIndexes);
    if (!indexes.length) return '每个子任务至少需要一个测试点。';
    for (const index of indexes) {
      if (tests.has(index)) return `测试点 ${index} 被多个子任务重复使用。`;
      tests.add(index);
    }
  }
  return score === subtaskMaxScoreMilli.value ? '' : '所有子任务分值之和必须等于题目满分。';
});

watch(editVisible, (visible) => {
  if (!visible || !contest.value) return;
  editForm.name = contest.value.name;
  editForm.visibility = contest.value.visibility as ContestVisibility;
  editForm.startAt = contest.value.startAt ? new Date(contest.value.startAt) : null;
  editForm.freezeAt = contest.value.freezeAt ? new Date(contest.value.freezeAt) : null;
  editForm.endAt = contest.value.endAt ? new Date(contest.value.endAt) : null;
});
watch(extensionVisible, (visible) => {
  if (!visible || !contest.value?.endAt) return;
  extensionForm.endAt = new Date(new Date(contest.value.endAt).getTime() + 30 * 60_000);
});

function openClone() {
  if (!contest.value || !session.isSuperAdmin.value) return;
  cloneForm.name = `${contest.value.name} 副本`;
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
    const result = await adminContestApi.cloneContest(contestId, {
      name: cloneForm.name.trim(),
      visibility: cloneForm.visibility,
      startAt: cloneForm.startAt?.toISOString() ?? null,
      freezeAt: cloneForm.freezeAt?.toISOString() ?? null,
      endAt: cloneForm.endAt?.toISOString() ?? null,
      copyTeams: cloneForm.copyTeams,
    });
    cloneVisible.value = false;
    ElMessage.success(`比赛已克隆，复制 ${result.problemsCopied} 道题目、${result.teamsCopied} 支队伍`);
    await router.push(`/admin/contests/${result.contest.id}`);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    cloning.value = false;
  }
}

async function loadAll() {
  errorMessage.value = '';
  try {
    const [contestValue, teamPage, assignedTeamPage, allProblems, assignedProblems, submissionPageValue, queueStatus, scoringPolicy] =
      await Promise.all([
        adminContestApi.getContest(contestId),
        adminContestApi.listTeams(),
        adminContestApi.listContestTeams(contestId),
        adminContestApi.listAllProblems(contestId),
        adminContestApi.listContestProblems(contestId),
        adminContestApi.listSubmissions(contestId),
        adminContestApi.getJudgeQueueStatus(contestId),
        adminContestApi.getScoringPolicy(contestId),
      ]);
    contest.value = contestValue;
    teams.value = teamPage.content;
    contestTeams.value = assignedTeamPage;
    problems.value = allProblems;
    contestProblems.value = assignedProblems;
    submissionPage.value = submissionPageValue;
    judgeQueueStatus.value = queueStatus;
    Object.assign(scoringForm, scoringPolicy);
    problemForm.displayOrder = assignedProblems.length + 1;
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}

async function saveScoringPolicy() {
  if (!canEditProblemConfiguration.value) return;
  savingScoring.value = true;
  try {
    if (scoringForm.scoringMode === 'ICPC') scoringForm.scoreAggregation = 'BEST';
    const value = await adminContestApi.updateScoringPolicy(contestId, { ...scoringForm });
    Object.assign(scoringForm, value);
    ElMessage.success('计分策略已保存');
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    savingScoring.value = false;
  }
}

async function loadSimilarityPairs() {
  similarityLoading.value = true;
  try {
    similarityPairs.value = await adminContestApi.listSubmissionSimilarityPairs(contestId, {
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
    const result = await adminContestApi.backfillSubmissionSimilarity(contestId);
    ElMessage.success(`已扫描 ${result.scanned}，更新 ${result.updated}，失败 ${result.failed}`);
    await loadSimilarityPairs();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    similarityBackfillLoading.value = false;
  }
}

async function transition(to: ContestStatus) {
  if (!contest.value) return;
  try {
    await ElMessageBox.confirm(
      `确认将比赛从“${contestStatusLabel(contest.value.status)}”切换到“${contestStatusLabel(to)}”？`,
      '确认生命周期操作',
      { type: 'warning' },
    );
  } catch {
    return;
  }
  transitioning.value = true;
  try {
    await adminContestApi.transitionContest(contestId, to);
    contest.value = await adminContestApi.getContest(contestId);
    ElMessage.success('比赛状态已更新');
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    transitioning.value = false;
  }
}

async function saveContest() {
  if (!contest.value || !editForm.name.trim()) return;
  saving.value = true;
  try {
    const basePayload = {
      name: editForm.name.trim(),
      visibility: editForm.visibility,
      // Optimistic concurrency: reject the save if another administrator
      // changed the contest since this page loaded.
      expectedVersion: contest.value.version,
    };
    contest.value = await adminContestApi.updateContest(
      contestId,
      canEditSchedule.value
        ? {
            ...basePayload,
            startAt: editForm.startAt?.toISOString() ?? null,
            freezeAt: editForm.freezeAt?.toISOString() ?? null,
            endAt: editForm.endAt?.toISOString() ?? null,
          }
        : basePayload,
    );
    editVisible.value = false;
    ElMessage.success('比赛信息已保存');
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    saving.value = false;
  }
}

function disableExtensionDate(date: Date) {
  if (!contest.value?.endAt) return false;
  const currentDay = new Date(contest.value.endAt);
  currentDay.setHours(0, 0, 0, 0);
  return date.getTime() < currentDay.getTime();
}

async function extendContest() {
  if (!contest.value?.endAt || !extensionForm.endAt || !extensionIsLater.value) return;
  const previousEndAt = contest.value.endAt;
  const nextEndAt = extensionForm.endAt.toISOString();
  try {
    await ElMessageBox.confirm(
      `确认将结束时间从 ${formatDateTime(previousEndAt)} 延后至 ${formatDateTime(nextEndAt)}？`,
      '确认比赛延时',
      { type: 'warning', confirmButtonText: '确认延时' },
    );
  } catch {
    return;
  }
  extending.value = true;
  try {
    const result = await adminContestApi.extendContest(contestId, previousEndAt, nextEndAt);
    contest.value = {
      ...contest.value,
      endAt: result.endAt,
      version: result.version,
      updatedAt: result.updatedAt,
    };
    extensionVisible.value = false;
    ElMessage.success(`比赛已延长至 ${formatDateTime(result.endAt)}`);
  } catch (error) {
    if (error instanceof ApiError && error.code === 'CONTEST_EXTENSION_STALE') {
      ElMessage.error('比赛结束时间已被其他管理员修改，已刷新比赛信息，请重新确认延时');
      contest.value = await adminContestApi.getContest(contestId);
      extensionVisible.value = false;
    } else {
      ElMessage.error(getErrorMessage(error));
    }
  } finally {
    extending.value = false;
  }
}

async function assignTeam() {
  if (!teamForm.teamId) return;
  assigning.value = true;
  try {
    await adminContestApi.assignTeam(contestId, {
      teamId: teamForm.teamId,
      participationType: teamForm.participationType,
      groupName: teamForm.groupName.trim() || null,
    });
    contestTeams.value = await adminContestApi.listContestTeams(contestId);
    teamForm.teamId = null;
    teamForm.groupName = '';
    ElMessage.success('队伍已分配');
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    assigning.value = false;
  }
}

async function unassignTeam(teamId: number) {
  try {
    await ElMessageBox.confirm('确认从本场比赛移除该队伍？', '移除队伍', { type: 'warning' });
    await adminContestApi.unassignTeam(contestId, teamId);
    contestTeams.value = await adminContestApi.listContestTeams(contestId);
  } catch (error) {
    if (error !== 'cancel' && error !== 'close') ElMessage.error(getErrorMessage(error));
  }
}

async function assignProblem() {
  if (!problemForm.problemId || !problemForm.alias.trim()) return;
  assigning.value = true;
  try {
    await adminContestApi.assignProblem(contestId, {
      problemId: problemForm.problemId,
      alias: problemForm.alias.trim().toUpperCase(),
      displayOrder: problemForm.displayOrder,
      color: problemForm.color,
    });
    contestProblems.value = await adminContestApi.listContestProblems(contestId);
    problemForm.problemId = null;
    problemForm.alias = '';
    problemForm.displayOrder = contestProblems.value.length + 1;
    ElMessage.success('题目已分配');
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    assigning.value = false;
  }
}

function openProblemEdit(value: unknown) {
  const problem = value as ContestProblem;
  problemEditForm.problemId = problem.problemId;
  problemEditForm.alias = problem.alias;
  problemEditForm.displayOrder = problem.displayOrder;
  problemEditForm.color = problem.color;
  problemEditVisible.value = true;
}

function openProblemContent(problemId: number) {
  void router.push(`/admin/problems/${problemId}?contestId=${contestId}`);
}

function parseTestIndexes(value: string): number[] {
  const values = value.split(/[\s,]+/).filter(Boolean).map(Number);
  if (values.some((index) => !Number.isInteger(index) || index < 1 || index > 10_000)) return [];
  return [...new Set(values)].sort((left, right) => left - right);
}

function addSubtask() {
  const displayOrder = editableSubtasks.length + 1;
  editableSubtasks.push({
    localId: ++subtaskLocalId,
    subtaskKey: `S${displayOrder}`,
    name: `子任务 ${displayOrder}`,
    displayOrder,
    scoreMilli: 0,
    testIndexes: '',
  });
}

async function openSubtasks(problemId: number) {
  subtaskProblemId.value = problemId;
  subtasksVisible.value = true;
  subtasksLoading.value = true;
  editableSubtasks.splice(0);
  try {
    const value = await adminContestApi.getProblemSubtasks(contestId, problemId);
    subtaskMaxScoreMilli.value = value.maxScoreMilli;
    for (const item of value.subtasks) {
      editableSubtasks.push({
        localId: ++subtaskLocalId,
        subtaskKey: item.subtaskKey,
        name: item.name,
        displayOrder: item.displayOrder,
        scoreMilli: item.scoreMilli,
        testIndexes: item.testIndexes.join(','),
      });
    }
    if (!editableSubtasks.length) addSubtask();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
    subtasksVisible.value = false;
  } finally {
    subtasksLoading.value = false;
  }
}

async function saveSubtasks() {
  if (!subtaskProblemId.value || subtaskValidationError.value) return;
  savingSubtasks.value = true;
  try {
    await adminContestApi.replaceProblemSubtasks(contestId, subtaskProblemId.value, {
      maxScoreMilli: subtaskMaxScoreMilli.value,
      subtasks: editableSubtasks.map((item) => ({
        subtaskKey: item.subtaskKey.trim().toUpperCase(),
        name: item.name.trim(),
        displayOrder: item.displayOrder,
        scoreMilli: item.scoreMilli,
        testIndexes: parseTestIndexes(item.testIndexes),
      })),
    });
    subtasksVisible.value = false;
    ElMessage.success('子任务计分已保存');
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    savingSubtasks.value = false;
  }
}

async function saveProblemEdit() {
  if (!problemEditForm.problemId || !problemEditForm.alias.trim()) return;
  savingProblem.value = true;
  try {
    await adminContestApi.updateProblemAssignment(contestId, problemEditForm.problemId, {
      alias: problemEditForm.alias.trim().toUpperCase(),
      displayOrder: problemEditForm.displayOrder,
      color: problemEditForm.color,
    });
    contestProblems.value = await adminContestApi.listContestProblems(contestId);
    problemEditVisible.value = false;
    ElMessage.success('比赛题目已更新');
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    savingProblem.value = false;
  }
}

async function unassignProblem(problemId: number) {
  try {
    await ElMessageBox.confirm('确认从本场比赛移除该题目？', '移除题目', { type: 'warning' });
    await adminContestApi.unassignProblem(contestId, problemId);
    contestProblems.value = await adminContestApi.listContestProblems(contestId);
  } catch (error) {
    if (error !== 'cancel' && error !== 'close') ElMessage.error(getErrorMessage(error));
  }
}

async function moveProblem(index: number, delta: number) {
  const reordered = [...sortedContestProblems.value];
  const target = index + delta;
  if (target < 0 || target >= reordered.length) return;
  [reordered[index], reordered[target]] = [reordered[target], reordered[index]];
  reordering.value = true;
  try {
    await adminContestApi.reorderProblems(
      contestId,
      reordered.map((problem, position) => ({ problemId: problem.problemId, displayOrder: position + 1 })),
    );
    contestProblems.value = await adminContestApi.listContestProblems(contestId);
    ElMessage.success('题目顺序已更新');
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
    contestProblems.value = await adminContestApi.listContestProblems(contestId);
  } finally {
    reordering.value = false;
  }
}

function openBulkRejudge() {
  void router.push(`/admin/contests/${contestId}/rejudge-tasks`);
}

function openAnnouncements() {
  void router.push(`/admin/contests/${contestId}/announcements`);
}

async function loadSubmissions() {
  try {
    submissionPage.value = await adminContestApi.listSubmissions(contestId, {
      page: submissionCurrentPage.value - 1,
      size: submissionPage.value.size,
    });
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}

async function loadJudgeQueueStatus() {
  queueLoading.value = true;
  try {
    judgeQueueStatus.value = await adminContestApi.getJudgeQueueStatus(contestId);
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
    submissionDetail.value = await adminContestApi.getSubmission(contestId, submissionId);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
    submissionDetailVisible.value = false;
  } finally {
    submissionDetailLoading.value = false;
  }
}

async function rejudge(submissionId: number) {
  try {
    await ElMessageBox.confirm(`确认重判提交 #${submissionId}？原判定将被标记为历史记录。`, '单次重判', {
      type: 'warning',
    });
  } catch {
    return;
  }
  rejudgingId.value = submissionId;
  try {
    const row = submissions.value.find((item) => item.id === submissionId);
    if (!row?.activeJudgementId) {
      ElMessage.error('该提交当前没有可重判的有效判定，请刷新后重试');
      return;
    }
    const result = await adminContestApi.rejudgeSubmission(contestId, submissionId, row.activeJudgementId);
    ElMessage.success(`提交 #${result.submissionId} 已重新入队`);
    submissionPage.value = await adminContestApi.listSubmissions(contestId, { page: submissionCurrentPage.value - 1, size: submissionPage.value.size });
    await loadJudgeQueueStatus();
  } catch (error) {
    if (error instanceof ApiError && error.code === 'JUDGEMENT_VERSION_STALE') {
      ElMessage.error('该提交的有效判定已被其他管理员更新，已刷新列表，请重新确认');
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
    const blob = kind === 'scoreboard'
      ? await adminContestApi.exportScoreboard(contestId)
      : kind === 'submissions'
        ? await adminContestApi.exportSubmissions(contestId)
        : await adminContestApi.exportSubmissionSources(contestId);
    const contestName = (contest.value?.name ?? `contest-${contestId}`).replace(/[\\/:*?"<>|]/g, '_');
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
    ElMessage.success('导出文件已生成');
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    exporting.value = null;
  }
}

function problemName(problemId: number) {
  const problem = problems.value.find((item) => item.id === problemId);
  const assigned = contestProblems.value.find((item) => item.problemId === problemId);
  return problem ? `${assigned?.alias ? `${assigned.alias} · ` : ''}${problem.title}` : `题目 #${problemId}`;
}

function participationLabel(type: string) {
  return { OFFICIAL: '正式参赛', STAR: '打星队伍', PRACTICE: '练习队伍' }[type] ?? type;
}

function transitionLabel(status: ContestStatus) {
  return {
    DRAFT: '返回草稿',
    FROZEN_CONFIG: '锁定配置',
    RUNNING: contest.value?.status === 'PAUSED' ? '恢复比赛' : '开始比赛',
    PAUSED: '暂停比赛',
    ENDED: '结束比赛',
    ARCHIVED: '归档比赛',
  }[status];
}

onMounted(loadAll);
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
.admin-two-column .el-col {
  margin-bottom: 16px;
}
.card-action {
  margin-top: 20px;
}
.assignment-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  margin-bottom: 22px;
  padding: 18px;
  border-radius: 0;
  background: #f7f9fc;
}
.admin-form-grid :deep(.el-date-editor) {
  width: 100%;
}
.pagination-row {
  margin-top: 24px;
}
.problem-color {
  display: inline-flex;
  gap: 7px;
  align-items: center;
}
.problem-color i {
  width: 14px;
  height: 14px;
  border-radius: 50%;
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
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
  font-size: 13px;
  line-height: 1.65;
  max-height: 540px;
  margin: 0;
  white-space: pre;
}
@media (max-width: 680px) {
  .admin-page-header {
    align-items: stretch;
    flex-direction: column;
    gap: 14px;
  }
}
</style>
