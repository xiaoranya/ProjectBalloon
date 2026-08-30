<template>
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
            <ElButton v-if="canExtend" type="warning" plain @click="extensionVisible = true">{{
              t('延长比赛')
            }}</ElButton>
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
                  :disabled="!canEditProblemConfiguration || scoringForm.scoringMode === 'ICPC'"
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
            ><ElDatePicker v-model="editForm.startAt" type="datetime" :disabled="!canEditSchedule"
          /></ElFormItem>
        </ElCol>
        <ElCol :xs="24" :sm="8">
          <ElFormItem :label="t('封榜时间')"
            ><ElDatePicker v-model="editForm.freezeAt" type="datetime" :disabled="!canEditSchedule"
          /></ElFormItem>
        </ElCol>
        <ElCol :xs="24" :sm="8">
          <ElFormItem :label="t('结束时间')"
            ><ElDatePicker v-model="editForm.endAt" type="datetime" :disabled="!canEditSchedule"
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
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { adminContestApi, type ScoringPolicy } from '../../api/admin-contests';
import { ApiError, getErrorMessage } from '../../api/client';
import type { ContestResponse, ContestStatus, ContestVisibility } from '../../api/types';
import { contestStatusLabel, formatDateTime } from '../../utils/format';
import { useI18n } from '../../i18n';

const props = defineProps<{
  contestId: number;
  contest: ContestResponse | null;
}>();
const emit = defineEmits<{ 'scoring-changed': [mode: string]; 'contest-changed': [] }>();
const { t } = useI18n();

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

const scoringForm = reactive<Omit<ScoringPolicy, 'contestId'>>({
  scoringMode: 'ICPC',
  scoreAggregation: 'BEST',
  feedbackPolicy: 'FULL',
});
const savingScoring = ref(false);
const saving = ref(false);
const extending = ref(false);
const transitioning = ref(false);
const editVisible = ref(false);
const extensionVisible = ref(false);
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

const nextStatuses = computed(() =>
  props.contest ? (transitions[props.contest.status as ContestStatus] ?? []) : [],
);
const canEditSchedule = computed(
  () => props.contest?.status === 'DRAFT' || props.contest?.status === 'FROZEN_CONFIG',
);
const canEditProblemConfiguration = computed(() => props.contest?.status === 'DRAFT');
const canExtend = computed(() =>
  Boolean(
    props.contest?.endAt &&
    (props.contest.status === 'RUNNING' || props.contest.status === 'PAUSED'),
  ),
);
const extensionIsLater = computed(() =>
  Boolean(
    props.contest?.endAt &&
    extensionForm.endAt &&
    extensionForm.endAt.getTime() > new Date(props.contest.endAt).getTime(),
  ),
);
const lifecycleIndex = computed(() => {
  const status = props.contest?.status as ContestStatus | undefined;
  return status ? lifecycle.indexOf(status) : 0;
});

watch(editVisible, (visible) => {
  if (!visible || !props.contest) return;
  editForm.name = props.contest.name;
  editForm.visibility = props.contest.visibility as ContestVisibility;
  editForm.startAt = props.contest.startAt ? new Date(props.contest.startAt) : null;
  editForm.freezeAt = props.contest.freezeAt ? new Date(props.contest.freezeAt) : null;
  editForm.endAt = props.contest.endAt ? new Date(props.contest.endAt) : null;
});
watch(extensionVisible, (visible) => {
  if (!visible || !props.contest?.endAt) return;
  extensionForm.endAt = new Date(new Date(props.contest.endAt).getTime() + 30 * 60_000);
});

onMounted(async () => {
  try {
    const value = await adminContestApi.getScoringPolicy(props.contestId);
    Object.assign(scoringForm, value);
    emit('scoring-changed', value.scoringMode);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
});

async function saveScoringPolicy() {
  if (!canEditProblemConfiguration.value) return;
  savingScoring.value = true;
  try {
    if (scoringForm.scoringMode === 'ICPC') scoringForm.scoreAggregation = 'BEST';
    const value = await adminContestApi.updateScoringPolicy(props.contestId, { ...scoringForm });
    Object.assign(scoringForm, value);
    emit('scoring-changed', value.scoringMode);
    ElMessage.success(t('计分策略已保存'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    savingScoring.value = false;
  }
}

async function transition(to: ContestStatus) {
  if (!props.contest) return;
  try {
    await ElMessageBox.confirm(
      t('确认将比赛从“{from}”切换到“{to}”？', {
        from: contestStatusLabel(props.contest.status),
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
    await adminContestApi.transitionContest(props.contestId, to);
    emit('contest-changed');
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    transitioning.value = false;
  }
}

async function saveContest() {
  if (!props.contest || !editForm.name.trim()) return;
  saving.value = true;
  try {
    const basePayload = {
      name: editForm.name.trim(),
      visibility: editForm.visibility,
      // Optimistic concurrency: reject the save if another administrator
      // changed the contest since this page loaded.
      expectedVersion: props.contest.version,
    };
    await adminContestApi.updateContest(
      props.contestId,
      canEditSchedule.value
        ? {
            ...basePayload,
            startAt: editForm.startAt?.toISOString() ?? null,
            freezeAt: editForm.freezeAt?.toISOString() ?? null,
            endAt: editForm.endAt?.toISOString() ?? null,
          }
        : basePayload,
    );
    emit('contest-changed');
    editVisible.value = false;
    ElMessage.success(t('比赛信息已保存'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    saving.value = false;
  }
}

function disableExtensionDate(date: Date) {
  if (!props.contest?.endAt) return false;
  const currentDay = new Date(props.contest.endAt);
  currentDay.setHours(0, 0, 0, 0);
  return date.getTime() < currentDay.getTime();
}

async function extendContest() {
  if (!props.contest?.endAt || !extensionForm.endAt || !extensionIsLater.value) return;
  const previousEndAt = props.contest.endAt;
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
    const result = await adminContestApi.extendContest(props.contestId, previousEndAt, nextEndAt);
    emit('contest-changed');
    extensionVisible.value = false;
    ElMessage.success(t('比赛已延长至 {time}', { time: formatDateTime(result.endAt) }));
  } catch (error) {
    if (error instanceof ApiError && error.code === 'CONTEST_EXTENSION_STALE') {
      ElMessage.error(t('比赛结束时间已被其他管理员修改，已刷新比赛信息，请重新确认延时'));
      emit('contest-changed');
      extensionVisible.value = false;
    } else {
      ElMessage.error(getErrorMessage(error));
    }
  } finally {
    extending.value = false;
  }
}

function transitionLabel(status: ContestStatus) {
  return t(
    {
      DRAFT: '返回草稿',
      FROZEN_CONFIG: '锁定配置',
      RUNNING: props.contest?.status === 'PAUSED' ? '恢复比赛' : '开始比赛',
      PAUSED: '暂停比赛',
      ENDED: '结束比赛',
      ARCHIVED: '归档比赛',
    }[status],
  );
}
</script>

<style scoped>
.admin-two-column .el-col {
  margin-bottom: 16px;
}
.card-action {
  margin-top: 20px;
}
.admin-form-grid :deep(.el-date-editor) {
  width: 100%;
}
</style>
