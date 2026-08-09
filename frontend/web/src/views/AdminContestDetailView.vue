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
          <ElRow v-if="contest" :gutter="16" class="admin-two-column">
            <ElCol :xs="24" :md="12">
              <ElCard shadow="never">
                <template #header
                  ><strong>{{ t('比赛信息') }}</strong></template
                >
                <ElDescriptions :column="1" border>
                  <ElDescriptionsItem :label="t('可见性')">{{
                    contest.visibility === 'PUBLIC' ? t('公开') : t('私有')
                  }}</ElDescriptionsItem>
                  <ElDescriptionsItem :label="t('开始')">{{
                    formatDateTime(contest.startAt)
                  }}</ElDescriptionsItem>
                  <ElDescriptionsItem :label="t('封榜')">{{
                    formatDateTime(contest.freezeAt)
                  }}</ElDescriptionsItem>
                  <ElDescriptionsItem :label="t('结束')">{{
                    formatDateTime(contest.endAt)
                  }}</ElDescriptionsItem>
                </ElDescriptions>
                <div class="card-action">
                  <ElSpace wrap :size="10">
                    <ElButton plain @click="editVisible = true">{{ t('编辑比赛信息') }}</ElButton>
                    <ElButton
                      v-if="canExtend"
                      type="warning"
                      plain
                      @click="extensionVisible = true"
                    >
                      {{ t('延长比赛') }}
                    </ElButton>
                  </ElSpace>
                </div>
              </ElCard>
            </ElCol>

            <ElCol :xs="24" :md="12">
              <ElCard shadow="never">
                <template #header
                  ><strong>{{ t('生命周期') }}</strong></template
                >
                <ElSteps direction="vertical" :active="lifecycleIndex" finish-status="success">
                  <ElStep
                    v-for="status in lifecycle"
                    :key="status"
                    :title="contestStatusLabel(status)"
                  />
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
                  <p v-if="nextStatuses.length === 0">{{ t('当前状态没有后续操作。') }}</p>
                </div>
              </ElCard>
            </ElCol>

            <ElCol :xs="24">
              <ElCard shadow="never">
                <template #header
                  ><strong>{{ t('OI / IOI 计分策略') }}</strong></template
                >
                <ElForm label-position="top">
                  <ElRow :gutter="12" class="admin-form-grid">
                    <ElCol :xs="24" :sm="8">
                      <ElFormItem :label="t('赛制')">
                        <ElSelect
                          v-model="scoringForm.scoringMode"
                          :disabled="!canEditProblemConfiguration"
                        >
                          <ElOption :label="t('ICPC（解题数 / 罚时）')" value="ICPC" />
                          <ElOption :label="t('OI（积分）')" value="OI" />
                          <ElOption :label="t('IOI（积分）')" value="IOI" />
                        </ElSelect>
                      </ElFormItem>
                    </ElCol>
                    <ElCol :xs="24" :sm="8">
                      <ElFormItem :label="t('有效提交')">
                        <ElSelect
                          v-model="scoringForm.scoreAggregation"
                          :disabled="
                            !canEditProblemConfiguration || scoringForm.scoringMode === 'ICPC'
                          "
                        >
                          <ElOption :label="t('最高分')" value="BEST" />
                          <ElOption :label="t('最后一次')" value="LAST" />
                        </ElSelect>
                      </ElFormItem>
                    </ElCol>
                    <ElCol :xs="24" :sm="8">
                      <ElFormItem :label="t('比赛中反馈')">
                        <ElSelect
                          v-model="scoringForm.feedbackPolicy"
                          :disabled="!canEditProblemConfiguration"
                        >
                          <ElOption :label="t('完整测试点')" value="FULL" />
                          <ElOption :label="t('仅总分')" value="SCORE_ONLY" />
                          <ElOption :label="t('不反馈')" value="NONE" />
                        </ElSelect>
                      </ElFormItem>
                    </ElCol>
                  </ElRow>
                </ElForm>
                <div class="card-action">
                  <ElSpace wrap :size="10">
                    <ElButton
                      type="primary"
                      :loading="savingScoring"
                      :disabled="!canEditProblemConfiguration"
                      @click="saveScoringPolicy"
                      >{{ t('保存计分策略') }}</ElButton
                    >
                  </ElSpace>
                </div>
              </ElCard>
            </ElCol>
          </ElRow>
        </ElTabPane>

        <ElTabPane :label="t('队伍分配')" name="teams">
          <ElCard shadow="never">
            <ElSpace wrap :size="12" class="assignment-toolbar">
              <ElSelect
                v-model="teamForm.teamId"
                filterable
                :placeholder="t('选择未分配队伍')"
                style="width: 260px"
              >
                <ElOption
                  v-for="team in availableTeams"
                  :key="team.id"
                  :label="team.name"
                  :value="team.id"
                />
              </ElSelect>
              <ElSelect v-model="teamForm.participationType" style="width: 150px">
                <ElOption :label="t('正式参赛')" value="OFFICIAL" />
                <ElOption :label="t('打星队伍')" value="STAR" />
                <ElOption :label="t('练习队伍')" value="PRACTICE" />
              </ElSelect>
              <ElInput
                v-model="teamForm.groupName"
                :placeholder="t('分组（可选）')"
                style="width: 180px"
              />
              <ElButton
                type="primary"
                :disabled="!teamForm.teamId"
                :loading="assigning"
                @click="assignTeam"
                >{{ t('分配队伍') }}</ElButton
              >
            </ElSpace>
            <ElTable :data="contestTeams" row-key="teamId">
              <ElTableColumn prop="teamName" :label="t('队伍')" min-width="220" />
              <ElTableColumn prop="participationType" :label="t('参赛类型')" width="140">
                <template #default="{ row }">{{
                  participationLabel(row.participationType)
                }}</template>
              </ElTableColumn>
              <ElTableColumn prop="groupName" :label="t('分组')" min-width="140">
                <template #default="{ row }">{{ row.groupName || '—' }}</template>
              </ElTableColumn>
              <ElTableColumn :label="t('操作')" width="100">
                <template #default="{ row }">
                  <ElButton link type="danger" @click="unassignTeam(row.teamId)">{{
                    t('移除')
                  }}</ElButton>
                </template>
              </ElTableColumn>
              <template #empty><ElEmpty :description="t('尚未分配队伍')" /></template>
            </ElTable>
          </ElCard>
        </ElTabPane>

        <ElTabPane :label="t('题目分配')" name="problems">
          <ElCard shadow="never">
            <ElSpace wrap :size="12" class="assignment-toolbar">
              <ElSelect
                v-model="problemForm.problemId"
                filterable
                :placeholder="t('选择未分配题目')"
                style="width: 260px"
              >
                <ElOption
                  v-for="problem in availableProblems"
                  :key="problem.id"
                  :label="`${problem.slug} · ${problem.title}`"
                  :value="problem.id"
                />
              </ElSelect>
              <ElInput
                v-model="problemForm.alias"
                :placeholder="t('题号，如 A')"
                style="width: 120px"
              />
              <ElInputNumber
                v-model="problemForm.displayOrder"
                :min="1"
                :max="1000"
                controls-position="right"
              />
              <ElColorPicker v-model="problemForm.color" />
              <ElButton
                type="primary"
                :disabled="
                  !canEditProblemConfiguration ||
                  !problemForm.problemId ||
                  !problemForm.alias.trim()
                "
                :loading="assigning"
                @click="assignProblem"
              >
                {{ t('分配题目') }}
              </ElButton>
            </ElSpace>
            <ElTable :data="sortedContestProblems" row-key="problemId">
              <ElTableColumn prop="alias" :label="t('题号')" width="90" />
              <ElTableColumn :label="t('题目')" min-width="260">
                <template #default="{ row }">{{ problemName(row.problemId) }}</template>
              </ElTableColumn>
              <ElTableColumn :label="t('顺序')" width="150">
                <template #default="{ $index }">
                  <ElButton
                    link
                    :disabled="!canEditProblemConfiguration || $index === 0 || reordering"
                    @click="moveProblem($index, -1)"
                    >{{ t('上移') }}</ElButton
                  >
                  <ElButton
                    link
                    :disabled="
                      !canEditProblemConfiguration ||
                      $index === sortedContestProblems.length - 1 ||
                      reordering
                    "
                    @click="moveProblem($index, 1)"
                    >{{ t('下移') }}</ElButton
                  >
                </template>
              </ElTableColumn>
              <ElTableColumn :label="t('颜色')" width="110">
                <template #default="{ row }">
                  <span class="problem-color"
                    ><i :style="{ background: row.color || '#cbd5e1' }" />{{
                      row.color || t('未设置')
                    }}</span
                  >
                </template>
              </ElTableColumn>
              <ElTableColumn :label="t('操作')" width="290">
                <template #default="{ row }">
                  <ElButton
                    link
                    type="primary"
                    :disabled="!canEditProblemConfiguration"
                    @click="openProblemEdit(row)"
                    >{{ t('编辑') }}</ElButton
                  >
                  <ElButton
                    link
                    type="primary"
                    :disabled="!canEditProblemConfiguration || scoringForm.scoringMode === 'ICPC'"
                    @click="openSubtasks(row.problemId)"
                    >{{ t('子任务') }}</ElButton
                  >
                  <ElButton link type="primary" @click="openProblemContent(row.problemId)">{{
                    t('题目内容')
                  }}</ElButton>
                  <ElButton
                    link
                    type="danger"
                    :disabled="!canEditProblemConfiguration"
                    @click="unassignProblem(row.problemId)"
                    >{{ t('移除') }}</ElButton
                  >
                </template>
              </ElTableColumn>
              <template #empty><ElEmpty :description="t('尚未分配题目')" /></template>
            </ElTable>
          </ElCard>
        </ElTabPane>

        <ElTabPane :label="t('提交与重判')" name="submissions">
          <ElCard shadow="never">
            <ElSpace wrap :size="12" class="assignment-toolbar">
              <strong>{{ t('判题队列') }}</strong>
              <ElTag
                v-if="judgeQueueStatus"
                :type="judgeQueueStatus.drained ? 'success' : 'warning'"
              >
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
                <ElButton
                  type="primary"
                  plain
                  :loading="similarityLoading"
                  @click="loadSimilarityPairs"
                  >{{ t('扫描候选') }}</ElButton
                >
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
                ><template #default="{ row }">{{
                  problemName(row.problemId)
                }}</template></ElTableColumn
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
        </ElTabPane>
      </ElTabs>

      <ElDialog v-model="problemEditVisible" :title="t('编辑比赛题目')" width="460">
        <ElForm label-width="90px">
          <ElFormItem :label="t('题号')"
            ><ElInput v-model="problemEditForm.alias" maxlength="16"
          /></ElFormItem>
          <ElFormItem :label="t('展示顺序')">
            <ElInputNumber v-model="problemEditForm.displayOrder" :min="1" :max="1000" />
          </ElFormItem>
          <ElFormItem :label="t('气球颜色')"
            ><ElColorPicker v-model="problemEditForm.color"
          /></ElFormItem>
        </ElForm>
        <template #footer>
          <ElButton @click="problemEditVisible = false">{{ t('取消') }}</ElButton>
          <ElButton
            type="primary"
            :loading="savingProblem"
            :disabled="!problemEditForm.alias.trim()"
            @click="saveProblemEdit"
            >{{ t('保存') }}</ElButton
          >
        </template>
      </ElDialog>

      <ElDialog v-model="subtasksVisible" :title="t('子任务与测试点计分')" width="900">
        <ElSkeleton v-if="subtasksLoading" :rows="5" animated />
        <template v-else>
          <ElSpace wrap :size="12" class="assignment-toolbar">
            <ElFormItem :label="t('题目满分（千分之一分）')">
              <ElInputNumber v-model="subtaskMaxScoreMilli" :min="1" :max="100000000" />
            </ElFormItem>
            <ElButton plain @click="addSubtask">{{ t('添加子任务') }}</ElButton>
          </ElSpace>
          <ElTable :data="editableSubtasks" row-key="localId">
            <ElTableColumn :label="t('标识')" width="130"
              ><template #default="{ row }"
                ><ElInput v-model="row.subtaskKey" maxlength="32" /></template
            ></ElTableColumn>
            <ElTableColumn :label="t('名称')" min-width="150"
              ><template #default="{ row }"><ElInput v-model="row.name" maxlength="120" /></template
            ></ElTableColumn>
            <ElTableColumn :label="t('顺序')" width="110"
              ><template #default="{ row }"
                ><ElInputNumber
                  v-model="row.displayOrder"
                  :min="1"
                  :max="1000"
                  controls-position="right" /></template
            ></ElTableColumn>
            <ElTableColumn :label="t('分值（千分之一分）')" width="180"
              ><template #default="{ row }"
                ><ElInputNumber
                  v-model="row.scoreMilli"
                  :min="1"
                  :max="subtaskMaxScoreMilli" /></template
            ></ElTableColumn>
            <ElTableColumn :label="t('测试点编号')" min-width="210"
              ><template #default="{ row }"
                ><ElInput v-model="row.testIndexes" placeholder="1,2,3" /></template
            ></ElTableColumn>
            <ElTableColumn width="70"
              ><template #default="{ $index }"
                ><ElButton link type="danger" @click="editableSubtasks.splice($index, 1)">{{
                  t('删除')
                }}</ElButton></template
              ></ElTableColumn
            >
          </ElTable>
          <ElAlert
            v-if="subtaskValidationError"
            :title="subtaskValidationError"
            type="warning"
            :closable="false"
            show-icon
            style="margin-top: 12px"
          />
        </template>
        <template #footer>
          <ElButton @click="subtasksVisible = false">{{ t('取消') }}</ElButton>
          <ElButton
            type="primary"
            :loading="savingSubtasks"
            :disabled="Boolean(subtaskValidationError) || subtasksLoading"
            @click="saveSubtasks"
            >{{ t('保存子任务') }}</ElButton
          >
        </template>
      </ElDialog>

      <ElDialog v-model="editVisible" :title="t('编辑比赛信息')" width="620">
        <ElForm v-if="contest" label-position="top">
          <ElFormItem :label="t('比赛名称')"><ElInput v-model="editForm.name" /></ElFormItem>
          <ElFormItem :label="t('可见性')">
            <ElRadioGroup v-model="editForm.visibility">
              <ElRadioButton value="PRIVATE">{{ t('私有比赛') }}</ElRadioButton>
              <ElRadioButton value="PUBLIC">{{ t('公开比赛') }}</ElRadioButton>
            </ElRadioGroup>
          </ElFormItem>
          <ElRow :gutter="12" class="admin-form-grid">
            <ElCol :xs="24" :sm="8">
              <ElFormItem :label="t('开始时间')"
                ><ElDatePicker
                  v-model="editForm.startAt"
                  type="datetime"
                  :disabled="!canEditSchedule"
              /></ElFormItem>
            </ElCol>
            <ElCol :xs="24" :sm="8">
              <ElFormItem :label="t('封榜时间')"
                ><ElDatePicker
                  v-model="editForm.freezeAt"
                  type="datetime"
                  :disabled="!canEditSchedule"
              /></ElFormItem>
            </ElCol>
            <ElCol :xs="24" :sm="8">
              <ElFormItem :label="t('结束时间')"
                ><ElDatePicker
                  v-model="editForm.endAt"
                  type="datetime"
                  :disabled="!canEditSchedule"
              /></ElFormItem>
            </ElCol>
          </ElRow>
          <ElAlert
            v-if="!canEditSchedule"
            :title="t('比赛开始后，赛程字段已锁定；如需延后结束，请使用“延长比赛”。')"
            type="info"
            :closable="false"
            show-icon
          />
        </ElForm>
        <template #footer>
          <ElButton @click="editVisible = false">{{ t('取消') }}</ElButton>
          <ElButton type="primary" :loading="saving" @click="saveContest">{{ t('保存') }}</ElButton>
        </template>
      </ElDialog>

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

      <ElDialog v-model="extensionVisible" :title="t('延长比赛')" width="520">
        <ElAlert
          :title="t('延时会立即同步到参赛队伍、大屏和直播页面，且不能通过此操作缩短比赛。')"
          type="warning"
          :closable="false"
          show-icon
        />
        <ElForm label-position="top" class="dialog-form">
          <ElFormItem :label="t('当前结束时间')">
            <ElInput :model-value="formatDateTime(contest?.endAt)" disabled />
          </ElFormItem>
          <ElFormItem :label="t('新的结束时间')">
            <ElDatePicker
              v-model="extensionForm.endAt"
              type="datetime"
              style="width: 100%"
              :disabled-date="disableExtensionDate"
            />
          </ElFormItem>
        </ElForm>
        <template #footer>
          <ElButton @click="extensionVisible = false">{{ t('取消') }}</ElButton>
          <ElButton
            type="warning"
            :loading="extending"
            :disabled="!extensionIsLater"
            @click="extendContest"
          >
            {{ t('确认延时') }}
          </ElButton>
        </template>
      </ElDialog>

      <ElDialog v-model="submissionDetailVisible" :title="t('管理员提交详情')" width="900">
        <ElSkeleton v-if="submissionDetailLoading" :rows="6" animated />
        <template v-else-if="submissionDetail">
          <ElDescriptions :column="2" border>
            <ElDescriptionsItem :label="t('提交')">#{{ submissionDetail.id }}</ElDescriptionsItem>
            <ElDescriptionsItem :label="t('状态')">{{
              submissionStatusLabel(submissionDetail.status)
            }}</ElDescriptionsItem>
            <ElDescriptionsItem :label="t('队伍')">{{
              submissionDetail.teamName
            }}</ElDescriptionsItem>
            <ElDescriptionsItem :label="t('题目')">{{
              submissionDetail.problemAlias
            }}</ElDescriptionsItem>
            <ElDescriptionsItem :label="t('语言')">{{
              submissionDetail.language
            }}</ElDescriptionsItem>
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
import CodeEditor from '../components/CodeEditor.vue';
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
import {
  contestStatusLabel,
  formatDateTime,
  statusTagType,
  submissionStatusLabel,
} from '../utils/format';
import { useI18n } from '../i18n';

const lifecycle: ContestStatus[] = [
  'DRAFT',
  'FROZEN_CONFIG',
  'RUNNING',
  'PAUSED',
  'ENDED',
  'ARCHIVED',
];
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
const { t } = useI18n();
const contestId = Number(route.params.contestId);
const activeTab = ref('overview');
const contest = ref<Contest | null>(null);
const teams = ref<Team[]>([]);
const contestTeams = ref<ContestTeam[]>([]);
const problems = ref<Problem[]>([]);
const contestProblems = ref<ContestProblem[]>([]);
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
interface EditableSubtask {
  localId: number;
  subtaskKey: string;
  name: string;
  displayOrder: number;
  scoreMilli: number;
  testIndexes: string;
}
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
  contest.value ? (transitions[contest.value.status as ContestStatus] ?? []) : [],
);
const canEditSchedule = computed(
  () => contest.value?.status === 'DRAFT' || contest.value?.status === 'FROZEN_CONFIG',
);
const canEditProblemConfiguration = computed(() => contest.value?.status === 'DRAFT');
const canExtend = computed(() =>
  Boolean(
    contest.value?.endAt &&
    (contest.value.status === 'RUNNING' || contest.value.status === 'PAUSED'),
  ),
);
const extensionIsLater = computed(() =>
  Boolean(
    contest.value?.endAt &&
    extensionForm.endAt &&
    extensionForm.endAt.getTime() > new Date(contest.value.endAt).getTime(),
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
  if (!editableSubtasks.length) return t('至少需要一个子任务。');
  const keys = new Set<string>();
  const orders = new Set<number>();
  const tests = new Set<number>();
  let score = 0;
  for (const item of editableSubtasks) {
    const key = item.subtaskKey.trim().toUpperCase();
    if (!/^[A-Z0-9_]{1,32}$/.test(key) || keys.has(key))
      return t('子任务标识需唯一，并使用大写字母、数字或下划线。');
    if (!item.name.trim()) return t('子任务名称不能为空。');
    if (orders.has(item.displayOrder)) return t('子任务顺序不能重复。');
    keys.add(key);
    orders.add(item.displayOrder);
    score += item.scoreMilli;
    const indexes = parseTestIndexes(item.testIndexes);
    if (!indexes.length) return t('每个子任务至少需要一个测试点。');
    for (const index of indexes) {
      if (tests.has(index)) return t('测试点 {index} 被多个子任务重复使用。', { index });
      tests.add(index);
    }
  }
  return score === subtaskMaxScoreMilli.value ? '' : t('所有子任务分值之和必须等于题目满分。');
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
    const result = await adminContestApi.cloneContest(contestId, {
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

async function loadAll() {
  errorMessage.value = '';
  try {
    const [
      contestValue,
      teamPage,
      assignedTeamPage,
      allProblems,
      assignedProblems,
      submissionPageValue,
      queueStatus,
      scoringPolicy,
    ] = await Promise.all([
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
    ElMessage.success(t('计分策略已保存'));
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

async function transition(to: ContestStatus) {
  if (!contest.value) return;
  try {
    await ElMessageBox.confirm(
      t('确认将比赛从“{from}”切换到“{to}”？', {
        from: contestStatusLabel(contest.value.status),
        to: contestStatusLabel(to),
      }),
      t('确认生命周期操作'),
      { type: 'warning' },
    );
  } catch {
    return;
  }
  transitioning.value = true;
  try {
    await adminContestApi.transitionContest(contestId, to);
    contest.value = await adminContestApi.getContest(contestId);
    ElMessage.success(t('比赛状态已更新'));
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
    ElMessage.success(t('比赛信息已保存'));
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
      t('确认将结束时间从 {from} 延后至 {to}？', {
        from: formatDateTime(previousEndAt),
        to: formatDateTime(nextEndAt),
      }),
      t('确认比赛延时'),
      { type: 'warning', confirmButtonText: t('确认延时') },
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
    ElMessage.success(t('比赛已延长至 {time}', { time: formatDateTime(result.endAt) }));
  } catch (error) {
    if (error instanceof ApiError && error.code === 'CONTEST_EXTENSION_STALE') {
      ElMessage.error(t('比赛结束时间已被其他管理员修改，已刷新比赛信息，请重新确认延时'));
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
    ElMessage.success(t('队伍已分配'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    assigning.value = false;
  }
}

async function unassignTeam(teamId: number) {
  try {
    await ElMessageBox.confirm(t('确认从本场比赛移除该队伍？'), t('移除队伍'), { type: 'warning' });
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
    ElMessage.success(t('题目已分配'));
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
  const values = value
    .split(/[\s,]+/)
    .filter(Boolean)
    .map(Number);
  if (values.some((index) => !Number.isInteger(index) || index < 1 || index > 10_000)) return [];
  return [...new Set(values)].sort((left, right) => left - right);
}

function addSubtask() {
  const displayOrder = editableSubtasks.length + 1;
  editableSubtasks.push({
    localId: ++subtaskLocalId,
    subtaskKey: `S${displayOrder}`,
    name: t('子任务 {order}', { order: displayOrder }),
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
    ElMessage.success(t('子任务计分已保存'));
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
    ElMessage.success(t('比赛题目已更新'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    savingProblem.value = false;
  }
}

async function unassignProblem(problemId: number) {
  try {
    await ElMessageBox.confirm(t('确认从本场比赛移除该题目？'), t('移除题目'), { type: 'warning' });
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
      reordered.map((problem, position) => ({
        problemId: problem.problemId,
        displayOrder: position + 1,
      })),
    );
    contestProblems.value = await adminContestApi.listContestProblems(contestId);
    ElMessage.success(t('题目顺序已更新'));
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
      contestId,
      submissionId,
      row.activeJudgementId,
    );
    ElMessage.success(t('提交 #{id} 已重新入队', { id: result.submissionId }));
    submissionPage.value = await adminContestApi.listSubmissions(contestId, {
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
        ? await adminContestApi.exportScoreboard(contestId)
        : kind === 'submissions'
          ? await adminContestApi.exportSubmissions(contestId)
          : await adminContestApi.exportSubmissionSources(contestId);
    const contestName = (contest.value?.name ?? `contest-${contestId}`).replace(
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

function problemName(problemId: number) {
  const problem = problems.value.find((item) => item.id === problemId);
  const assigned = contestProblems.value.find((item) => item.problemId === problemId);
  return problem
    ? `${assigned?.alias ? `${assigned.alias} · ` : ''}${problem.title}`
    : t('题目 #{id}', { id: problemId });
}

function participationLabel(type: string) {
  const label = { OFFICIAL: '正式参赛', STAR: '打星队伍', PRACTICE: '练习队伍' }[type];
  return label ? t(label) : type;
}

function transitionLabel(status: ContestStatus) {
  return t(
    {
      DRAFT: '返回草稿',
      FROZEN_CONFIG: '锁定配置',
      RUNNING: contest.value?.status === 'PAUSED' ? '恢复比赛' : '开始比赛',
      PAUSED: '暂停比赛',
      ENDED: '结束比赛',
      ARCHIVED: '归档比赛',
    }[status],
  );
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
  font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', monospace;
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
