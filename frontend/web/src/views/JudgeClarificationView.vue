<template>
  <el-container direction="vertical" class="judge-page clarification-judge-page">
    <el-header height="auto" class="page-head">
      <div class="page-title-row">
        <div>
          <p class="eyebrow">Clarification Desk</p>
          <h1>{{ t('答疑工作台') }}</h1>
        </div>
        <div
          class="clarification-live-state"
          :class="{ connected: realtimeConnected }"
          aria-live="polite"
        >
          <span />{{ realtimeConnected ? t('实时更新') : t('轮询更新') }}
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

      <ElCard shadow="never" class="clarification-filter-card">
        <ElSpace wrap :size="16" class="judge-toolbar">
          <ElSelect
            v-model="selectedContestId"
            filterable
            :placeholder="t('选择比赛')"
            @change="changeContest"
          >
            <ElOption
              v-for="contest in contests"
              :key="contest.id"
              :label="contest.name"
              :value="contest.id"
            />
          </ElSelect>
          <ElRadioGroup v-model="statusFilter" @change="loadClarifications(false)">
            <ElRadioButton value="ALL">{{ t('全部') }}</ElRadioButton>
            <ElRadioButton value="PENDING">{{ t('待回复') }}</ElRadioButton>
            <ElRadioButton value="ANSWERED">{{ t('已回复') }}</ElRadioButton>
            <ElRadioButton value="CLOSED">{{ t('已关闭') }}</ElRadioButton>
          </ElRadioGroup>
          <ElButton
            :icon="Refresh"
            :loading="loading"
            :disabled="!selectedContestId"
            @click="loadClarifications(false)"
          >
            {{ t('刷新') }}
          </ElButton>
        </ElSpace>
      </ElCard>

      <ElCard shadow="never" class="clarification-list-card">
        <ElTable
          v-loading="loading"
          :data="clarifications"
          row-key="id"
          :empty-text="t('当前筛选下没有答疑')"
          highlight-current-row
          @row-click="openDetail"
        >
          <ElTableColumn prop="id" label="#" width="76" />
          <ElTableColumn :label="t('状态')" width="110">
            <template #default="{ row }"
              ><ElTag :type="statusType(row.status)">{{ statusLabel(row.status) }}</ElTag></template
            >
          </ElTableColumn>
          <ElTableColumn :label="t('范围')" width="100">
            <template #default="{ row }">{{
              row.problemAlias ? t('题目 {alias}', { alias: row.problemAlias }) : t('通用')
            }}</template>
          </ElTableColumn>
          <ElTableColumn :label="t('队伍')" min-width="180">
            <template #default="{ row }">
              <div class="admin-primary-cell">
                <strong>{{ row.teamName ?? t('队伍 #{id}', { id: row.teamId }) }}</strong
                ><small>{{ t('队伍 ID {id}', { id: row.teamId }) }}</small>
              </div>
            </template>
          </ElTableColumn>
          <ElTableColumn prop="question" :label="t('问题')" min-width="300" show-overflow-tooltip />
          <ElTableColumn :label="t('提问时间')" min-width="170">
            <template #default="{ row }">{{ formatDateTime(row.createdAt) }}</template>
          </ElTableColumn>
          <ElTableColumn :label="t('操作')" width="110" fixed="right">
            <template #default="{ row }"
              ><ElButton link type="primary" @click.stop="openDetail(row as Clarification)">{{
                t('查看详情')
              }}</ElButton></template
            >
          </ElTableColumn>
        </ElTable>
      </ElCard>

      <ElDrawer
        v-model="detailVisible"
        :title="t('答疑详情')"
        size="min(620px, 94vw)"
        @closed="selected = null"
      >
        <ElSkeleton v-if="detailLoading" :rows="8" animated />
        <div v-else-if="selected" class="clarification-detail">
          <div class="clarification-card-meta">
            <div>
              <ElTag :type="statusType(selected.status)">{{ statusLabel(selected.status) }}</ElTag>
              <ElTag type="info" effect="plain">{{
                selected.problemAlias
                  ? t('题目 {alias}', { alias: selected.problemAlias })
                  : t('通用问题')
              }}</ElTag>
            </div>
            <span>{{ t('版本 {version}', { version: selected.version }) }}</span>
          </div>
          <h3>{{ t('提问队伍') }}</h3>
          <p class="clarification-detail-meta">
            {{ selected.teamName ?? t('未命名队伍') }} ·
            {{ t('队伍 #{id}', { id: selected.teamId }) }} ·
            {{ t('用户 #{id}', { id: selected.askedByUserId }) }} ·
            {{ formatDateTime(selected.createdAt) }}
          </p>
          <h3>{{ t('问题') }}</h3>
          <div class="clarification-bubble question">{{ selected.question }}</div>
          <template v-if="selected.reply">
            <h3>{{ t('当前回复') }}</h3>
            <div class="clarification-bubble reply">{{ selected.reply }}</div>
            <p class="clarification-detail-meta">
              {{ selected.replyVisibility === 'PUBLIC' ? t('公开回复') : t('仅该队可见') }} ·
              {{ t('回复用户 #{id}', { id: selected.repliedByUserId ?? '—' }) }} ·
              {{ formatDateTime(selected.repliedAt) }}
            </p>
          </template>

          <ElForm
            v-if="selected.status !== 'CLOSED'"
            label-position="top"
            class="clarification-reply-form"
          >
            <ElFormItem :label="t('回复内容')">
              <ElInput
                v-model="replyForm.reply"
                type="textarea"
                :rows="6"
                maxlength="8000"
                show-word-limit
              />
            </ElFormItem>
            <ElFormItem :label="t('可见范围')">
              <ElRadioGroup v-model="replyForm.visibility">
                <ElRadio value="PRIVATE">{{ t('仅该队可见') }}</ElRadio>
                <ElRadio value="PUBLIC">{{ t('公开回复') }}</ElRadio>
              </ElRadioGroup>
            </ElFormItem>
            <ElButton
              type="primary"
              :loading="replying"
              :disabled="!replyForm.reply.trim()"
              @click="reply"
            >
              {{ selected.reply ? t('更新回复') : t('提交回复') }}
            </ElButton>
          </ElForm>

          <div class="clarification-actions">
            <ElButton
              v-if="selected.status !== 'CLOSED'"
              type="warning"
              plain
              :loading="closing"
              @click="closeClarification"
              >{{ t('关闭问题') }}</ElButton
            >
            <ElButton v-if="canConvert" type="success" plain @click="openConversion">{{
              t('转为公告')
            }}</ElButton>
            <ElTag v-if="selected.convertedAnnouncementId" type="success">
              {{ t('已转公告 #{id}', { id: selected.convertedAnnouncementId }) }}
            </ElTag>
          </div>
        </div>
      </ElDrawer>

      <ElDialog v-model="conversionVisible" :title="t('转为比赛公告')" width="min(580px, 92vw)">
        <ElAlert
          :title="t('仅公开回复可转为公告；留空时后端将使用问题摘要和当前回复。')"
          type="info"
          show-icon
          :closable="false"
          class="form-alert"
        />
        <ElForm label-position="top">
          <ElFormItem :label="t('公告标题（可选）')"
            ><ElInput v-model="conversionForm.title" maxlength="255" show-word-limit
          /></ElFormItem>
          <ElFormItem :label="t('公告正文（可选）')"
            ><ElInput
              v-model="conversionForm.body"
              type="textarea"
              :rows="6"
              maxlength="16000"
              show-word-limit
          /></ElFormItem>
        </ElForm>
        <template #footer>
          <ElButton @click="conversionVisible = false">{{ t('取消') }}</ElButton>
          <ElButton type="success" :loading="converting" @click="convert">{{
            t('发布公告')
          }}</ElButton>
        </template>
      </ElDialog>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Refresh } from '@element-plus/icons-vue';
import {
  clarificationApi,
  type Clarification,
  type ClarificationReplyVisibility,
  type ClarificationStatus,
} from '../api/clarifications';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { Contest } from '../api/types';
import {
  subscribeContestEvents,
  type ContestRealtimeSubscription,
} from '../realtime/contest-events';
import { formatDateTime } from '../utils/format';
import { useI18n } from '../i18n';

const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const contests = ref<Contest[]>([]);
const selectedContestId = ref<number | null>(null);
const statusFilter = ref<ClarificationStatus | 'ALL'>('PENDING');
const clarifications = ref<Clarification[]>([]);
const selected = ref<Clarification | null>(null);
const loading = ref(false);
const detailLoading = ref(false);
const detailVisible = ref(false);
const replying = ref(false);
const closing = ref(false);
const converting = ref(false);
const conversionVisible = ref(false);
const errorMessage = ref('');
const realtimeConnected = ref(false);
const replyForm = reactive<{ reply: string; visibility: ClarificationReplyVisibility }>({
  reply: '',
  visibility: 'PRIVATE',
});
const conversionForm = reactive({ title: '', body: '' });
let realtime: ContestRealtimeSubscription | undefined;

const canConvert = computed(
  () =>
    selected.value?.status === 'ANSWERED' &&
    selected.value.replyVisibility === 'PUBLIC' &&
    selected.value.convertedAnnouncementId === null,
);

function statusLabel(status: ClarificationStatus) {
  return t({ PENDING: '待回复', ANSWERED: '已回复', CLOSED: '已关闭' }[status]);
}

function statusType(status: ClarificationStatus): 'warning' | 'success' | 'info' {
  return status === 'PENDING' ? 'warning' : status === 'ANSWERED' ? 'success' : 'info';
}

async function loadClarifications(silent = true) {
  if (!selectedContestId.value) return;
  if (!silent) loading.value = true;
  try {
    clarifications.value = await clarificationApi.listAll(
      selectedContestId.value,
      statusFilter.value === 'ALL' ? undefined : statusFilter.value,
    );
    errorMessage.value = '';
    if (selected.value) {
      const changed = clarifications.value.find((item) => item.id === selected.value?.id);
      if (changed) selected.value = changed;
    }
  } catch (error) {
    if (!silent) errorMessage.value = getErrorMessage(error);
  } finally {
    if (!silent) loading.value = false;
  }
}

function connectRealtime() {
  realtime?.stop();
  realtimeConnected.value = false;
  if (!selectedContestId.value) return;
  realtime = subscribeContestEvents({
    contestId: selectedContestId.value,
    scope: 'STAFF',
    eventTypes: ['CLARIFICATION_UPDATED'],
    onEvent: (event) => {
      void loadClarifications();
      if (selected.value?.id === Number(event.payload.clarificationId))
        void refreshDetail(selected.value.id);
    },
    onConnectionChange: (connected) => {
      realtimeConnected.value = connected;
    },
    poll: () => loadClarifications(),
  });
}

async function changeContest() {
  await router.replace({
    query: selectedContestId.value ? { contestId: String(selectedContestId.value) } : {},
  });
  selected.value = null;
  detailVisible.value = false;
  connectRealtime();
  await loadClarifications(false);
}

async function refreshDetail(id: number) {
  try {
    selected.value = await clarificationApi.get(id);
    replyForm.reply = selected.value.reply ?? '';
    replyForm.visibility = selected.value.replyVisibility ?? 'PRIVATE';
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}

async function openDetail(row: Clarification) {
  detailVisible.value = true;
  detailLoading.value = true;
  try {
    await refreshDetail(row.id);
  } finally {
    detailLoading.value = false;
  }
}

async function reply() {
  if (!selected.value || !replyForm.reply.trim()) return;
  replying.value = true;
  try {
    selected.value = await clarificationApi.reply(selected.value.id, {
      reply: replyForm.reply.trim(),
      visibility: replyForm.visibility,
    });
    ElMessage.success(t('回复已保存'));
    await loadClarifications();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    replying.value = false;
  }
}

async function closeClarification() {
  if (!selected.value) return;
  try {
    await ElMessageBox.confirm(t('关闭后不能再次回复，确定继续吗？'), t('关闭答疑'), {
      type: 'warning',
    });
  } catch {
    return;
  }
  closing.value = true;
  try {
    const id = selected.value.id;
    await clarificationApi.close(id);
    await refreshDetail(id);
    await loadClarifications();
    ElMessage.success(t('问题已关闭'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    closing.value = false;
  }
}

function openConversion() {
  conversionForm.title = '';
  conversionForm.body = '';
  conversionVisible.value = true;
}

async function convert() {
  if (!selected.value) return;
  converting.value = true;
  try {
    const announcement = await clarificationApi.convert(selected.value.id, {
      title: conversionForm.title.trim() || null,
      body: conversionForm.body.trim() || null,
    });
    selected.value = { ...selected.value, convertedAnnouncementId: announcement.id };
    conversionVisible.value = false;
    await loadClarifications();
    ElMessage.success(t('已发布公告 #{id}', { id: announcement.id }));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    converting.value = false;
  }
}

onMounted(async () => {
  loading.value = true;
  try {
    const page = await contestApi.listContests();
    contests.value = page.content;
    const queryContestId = Number(route.query.contestId);
    selectedContestId.value = contests.value.some((contest) => contest.id === queryContestId)
      ? queryContestId
      : (contests.value[0]?.id ?? null);
    if (selectedContestId.value) {
      connectRealtime();
      await loadClarifications();
    }
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
});

onUnmounted(() => realtime?.stop());
</script>

<style scoped>
.judge-page {
  width: min(1440px, 100%);
  margin: 0 auto;
}

.page-head {
  height: auto;
  padding: clamp(28px, 5vw, 58px) clamp(28px, 5vw, 58px) 0;
}

.page-body {
  padding: 0 clamp(28px, 5vw, 58px) clamp(28px, 5vw, 58px);
}

.page-title-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 20px;
  margin-bottom: 32px;
}

.page-title-row h1 {
  margin-bottom: 8px;
  font-size: clamp(32px, 4vw, 48px);
  letter-spacing: -0.035em;
}

.page-title-row p {
  display: none;
  margin-bottom: 0;
  color: var(--muted);
}

.eyebrow {
  display: none;
  margin: 0 0 8px;
  color: var(--primary);
  font-size: 12px;
  font-weight: 800;
  letter-spacing: 0.14em;
  text-transform: uppercase;
}

.page-alert,
.form-alert {
  margin-bottom: 20px;
}

.clarification-filter-card {
  margin-bottom: 22px;
}

.clarification-list-card {
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: 0;
}

.clarification-live-state {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 14px;
  border-radius: 0;
  padding: 8px 12px;
  color: var(--muted);
  background: #e9eef5;
  font-size: 12px;
}

.clarification-live-state span {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #94a3b8;
}

.clarification-live-state.connected {
  color: #166534;
  background: #dcfce7;
}

.clarification-live-state.connected span {
  background: #22c55e;
}

.judge-toolbar {
  display: flex;
  width: 100%;
}

.judge-toolbar :deep(.el-space__item) {
  display: flex;
  align-items: center;
}

.judge-toolbar :deep(.el-space__item:first-child) {
  flex: 0 0 min(440px, 42%);
}

.judge-toolbar :deep(.el-space__item:first-child .el-select) {
  width: 100%;
}

.clarification-card-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
}

.clarification-card-meta > div {
  display: flex;
  flex-wrap: wrap;
  gap: 7px;
}

.clarification-card-meta > span {
  color: var(--muted);
  font-size: 12px;
}

.clarification-detail h3 {
  margin: 26px 0 10px;
  color: #263650;
}

.clarification-detail-meta {
  color: var(--muted);
}

.clarification-bubble {
  padding: 18px 20px;
  border-radius: 0;
  line-height: 1.75;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.clarification-bubble.question {
  border: 1px solid #dbe4f0;
  background: #f8fafc;
}

.clarification-bubble.reply {
  border: 1px solid #bfdbfe;
  background: #eff6ff;
}

.clarification-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  margin-top: 30px;
  padding-top: 22px;
  border-top: 1px solid #e5eaf2;
}

.clarification-reply-form {
  margin-top: 28px;
  padding-top: 22px;
  border-top: 1px solid var(--border);
}

.admin-primary-cell strong,
.admin-primary-cell small {
  display: block;
}

.admin-primary-cell small {
  margin-top: 4px;
  color: var(--muted);
}

@media (max-width: 900px) {
  .judge-toolbar :deep(.el-space__item:first-child) {
    flex-basis: 100%;
  }
}

@media (max-width: 640px) {
  .page-title-row {
    align-items: stretch;
    flex-direction: column;
  }

  .judge-toolbar :deep(.el-radio-group) {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
  }
}
</style>
