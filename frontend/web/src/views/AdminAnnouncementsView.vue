<template>
  <el-container direction="vertical" class="admin-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <div>
          <ElButton link :icon="ArrowLeft" @click="router.push(`/admin/contests/${contestId}`)">
            {{ t('返回比赛管理') }}
          </ElButton>
          <h1>{{ t('公告管理') }}</h1>
        </div>
        <div class="admin-page-actions">
          <ElButton :icon="Refresh" :loading="loading" @click="load">{{ t('刷新') }}</ElButton>
          <ElButton type="primary" @click="openCreate">{{ t('新建公告') }}</ElButton>
        </div>
      </div>
    </el-header>
    <el-main class="page-body">
      <ElAlert
        v-if="errorMessage"
        class="page-alert"
        type="error"
        show-icon
        :closable="false"
        :title="errorMessage"
      />

      <ElCard shadow="never">
        <ElTable v-loading="loading" :data="announcements" row-key="id">
          <ElTableColumn :label="t('状态')" width="120">
            <template #default="{ row }">
              <ElTag :type="statusType(row.status)">{{ statusLabel(row.status) }}</ElTag>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('公告')" min-width="360">
            <template #default="{ row }">
              <div class="announcement-title">
                <ElTag v-if="row.pinned" size="small" type="danger">{{ t('置顶') }}</ElTag>
                <strong>{{ row.title }}</strong>
              </div>
              <p class="announcement-body">{{ row.body }}</p>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('发布时间')" min-width="190">
            <template #default="{ row }">
              {{
                formatDateTime(
                  row.scheduledAt ?? row.publishedAt ?? row.cancelledAt ?? row.withdrawnAt,
                )
              }}
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('版本')" width="80" prop="version" />
          <ElTableColumn :label="t('操作')" width="270" fixed="right">
            <template #default="{ row }">
              <template v-if="row.status === 'SCHEDULED'">
                <ElButton link type="primary" @click="openEdit(row)">{{ t('编辑计划') }}</ElButton>
                <ElButton link type="danger" @click="cancelSchedule(row)">{{
                  t('取消计划')
                }}</ElButton>
              </template>
              <template v-else-if="row.status === 'PUBLISHED'">
                <ElButton link type="primary" @click="openEdit(row)">{{ t('编辑') }}</ElButton>
                <ElButton link @click="togglePin(row)">{{
                  row.pinned ? t('取消置顶') : t('置顶')
                }}</ElButton>
                <ElButton link type="danger" @click="withdraw(row)">{{ t('撤回') }}</ElButton>
              </template>
              <span v-else class="muted">{{ t('不可再修改') }}</span>
            </template>
          </ElTableColumn>
          <template #empty><ElEmpty :description="t('暂无公告')" /></template>
        </ElTable>
      </ElCard>

      <ElDialog v-model="editorVisible" :title="editorTitle" width="680px">
        <ElForm label-position="top">
          <ElFormItem :label="t('标题')">
            <ElInput v-model="form.title" maxlength="255" show-word-limit />
          </ElFormItem>
          <ElFormItem :label="t('内容')">
            <ElInput
              v-model="form.body"
              type="textarea"
              :rows="8"
              maxlength="16000"
              show-word-limit
            />
          </ElFormItem>
          <ElFormItem v-if="!editing || editing.status === 'SCHEDULED'" :label="t('发布方式')">
            <ElRadioGroup v-model="form.mode" :disabled="editing?.status === 'SCHEDULED'">
              <ElRadioButton value="immediate">{{ t('立即发布') }}</ElRadioButton>
              <ElRadioButton value="scheduled">{{ t('定时发布') }}</ElRadioButton>
            </ElRadioGroup>
          </ElFormItem>
          <ElFormItem v-if="form.mode === 'scheduled'" :label="t('计划发布时间')">
            <ElDatePicker
              v-model="form.scheduledAt"
              type="datetime"
              :placeholder="t('选择比赛结束前的未来时间')"
              :disabled-date="disablePastDate"
            />
          </ElFormItem>
          <ElFormItem
            ><ElCheckbox v-model="form.pinned">{{ t('置顶公告') }}</ElCheckbox></ElFormItem
          >
        </ElForm>
        <template #footer>
          <ElButton @click="editorVisible = false">{{ t('取消') }}</ElButton>
          <ElButton type="primary" :loading="saving" :disabled="!canSave" @click="save">
            {{ editing ? t('保存') : form.mode === 'scheduled' ? t('创建计划') : t('发布') }}
          </ElButton>
        </template>
      </ElDialog>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { ArrowLeft, Refresh } from '@element-plus/icons-vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { useRoute, useRouter } from 'vue-router';
import { announcementApi, type Announcement, type AnnouncementStatus } from '../api/announcements';
import { getErrorMessage } from '../api/client';
import { formatDateTime } from '../utils/format';
import { useI18n } from '../i18n';

const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const contestId = Number(route.params.contestId);
const announcements = ref<Announcement[]>([]);
const loading = ref(false);
const saving = ref(false);
const errorMessage = ref('');
const editorVisible = ref(false);
const editing = ref<Announcement | null>(null);
const form = reactive({
  title: '',
  body: '',
  pinned: false,
  mode: 'immediate' as 'immediate' | 'scheduled',
  scheduledAt: null as Date | null,
});

const editorTitle = computed(() => {
  if (!editing.value) return t('新建公告');
  return editing.value.status === 'SCHEDULED' ? t('编辑定时公告') : t('编辑已发布公告');
});
const canSave = computed(() =>
  Boolean(
    form.title.trim() &&
    form.body.trim() &&
    (form.mode === 'immediate' || (form.scheduledAt && form.scheduledAt.getTime() > Date.now())),
  ),
);

function statusLabel(status: AnnouncementStatus) {
  return t(
    { SCHEDULED: '待发布', PUBLISHED: '已发布', WITHDRAWN: '已撤回', CANCELLED: '已取消' }[status],
  );
}

function statusType(status: AnnouncementStatus) {
  return (
    { SCHEDULED: 'warning', PUBLISHED: 'success', WITHDRAWN: 'info', CANCELLED: 'info' } as const
  )[status];
}

function disablePastDate(date: Date) {
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  return date.getTime() < today.getTime();
}

async function load() {
  loading.value = true;
  errorMessage.value = '';
  try {
    announcements.value = await announcementApi.list(contestId, true);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}

function openCreate() {
  editing.value = null;
  form.title = '';
  form.body = '';
  form.pinned = false;
  form.mode = 'immediate';
  form.scheduledAt = new Date(Date.now() + 10 * 60_000);
  editorVisible.value = true;
}

function openEdit(tableRow: Record<string, unknown>) {
  const item = tableRow as unknown as Announcement;
  editing.value = item;
  form.title = item.title;
  form.body = item.body;
  form.pinned = item.pinned;
  form.mode = item.status === 'SCHEDULED' ? 'scheduled' : 'immediate';
  form.scheduledAt = item.scheduledAt ? new Date(item.scheduledAt) : null;
  editorVisible.value = true;
}

async function save() {
  if (!canSave.value) return;
  saving.value = true;
  const payload = {
    title: form.title.trim(),
    body: form.body.trim(),
    pinned: form.pinned,
    scheduledAt: form.mode === 'scheduled' ? form.scheduledAt!.toISOString() : null,
  };
  try {
    if (editing.value?.status === 'SCHEDULED') {
      await announcementApi.schedule(editing.value.id, payload);
    } else if (editing.value?.status === 'PUBLISHED') {
      await announcementApi.update(editing.value.id, {
        title: payload.title,
        body: payload.body,
        pinned: payload.pinned,
        expectedVersion: editing.value.version,
      });
    } else {
      await announcementApi.create(contestId, payload);
    }
    editorVisible.value = false;
    ElMessage.success(
      editing.value ? t('公告已保存') : payload.scheduledAt ? t('定时公告已创建') : t('公告已发布'),
    );
    await load();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    saving.value = false;
  }
}

async function cancelSchedule(tableRow: Record<string, unknown>) {
  const item = tableRow as unknown as Announcement;
  try {
    await ElMessageBox.confirm(t('取消后该计划不可恢复，确认继续？'), t('取消定时公告'), {
      type: 'warning',
    });
    await announcementApi.cancel(item.id);
    ElMessage.success(t('定时公告已取消'));
    await load();
  } catch (error) {
    if (error instanceof Error) ElMessage.error(getErrorMessage(error));
  }
}

async function togglePin(tableRow: Record<string, unknown>) {
  const item = tableRow as unknown as Announcement;
  try {
    await announcementApi.pin(item.id, !item.pinned);
    await load();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}

async function withdraw(tableRow: Record<string, unknown>) {
  const item = tableRow as unknown as Announcement;
  try {
    await ElMessageBox.confirm(t('撤回后该公告不可恢复，确认继续？'), t('撤回公告'), {
      type: 'warning',
    });
    await announcementApi.withdraw(item.id);
    ElMessage.success(t('公告已撤回'));
    await load();
  } catch (error) {
    if (error instanceof Error) ElMessage.error(getErrorMessage(error));
  }
}

onMounted(load);
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
.announcement-title {
  display: flex;
  align-items: center;
  gap: 8px;
}
.announcement-body {
  margin: 6px 0 0;
  color: #64748b;
  white-space: pre-wrap;
}
.muted {
  color: #94a3b8;
}
@media (max-width: 680px) {
  .admin-page-header {
    align-items: stretch;
    flex-direction: column;
    gap: 14px;
  }
}
@media (max-width: 640px) {
  .page-head {
    padding: 24px 16px 0;
  }
  .page-body {
    padding: 0 16px 24px;
  }
  .admin-page-header {
    align-items: stretch;
    flex-direction: column;
    gap: 16px;
  }
}
</style>
