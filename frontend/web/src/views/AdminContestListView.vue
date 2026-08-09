<template>
  <el-container direction="vertical" class="admin-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <h1>{{ t('比赛管理') }}</h1>
        <ElButton v-if="session.isSuperAdmin.value" type="primary" :icon="Plus" @click="openCreate">
          {{ t('创建比赛') }}
        </ElButton>
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
        <ElTable v-loading="loading" :data="page.content" row-key="id" @row-click="openContest">
          <ElTableColumn :label="t('比赛')" min-width="240">
            <template #default="{ row }">
              <div class="admin-primary-cell">
                <strong>{{ row.name }}</strong>
                <small
                  >#{{ row.id }} · {{ row.visibility === 'PUBLIC' ? t('公开') : t('私有') }}</small
                >
              </div>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('状态')" width="130">
            <template #default="{ row }"
              ><ElTag :type="contestTagType(row.status)">{{
                contestStatusLabel(row.status)
              }}</ElTag></template
            >
          </ElTableColumn>
          <ElTableColumn :label="t('开始时间')" min-width="180"
            ><template #default="{ row }">{{
              formatDateTime(row.startAt)
            }}</template></ElTableColumn
          >
          <ElTableColumn :label="t('结束时间')" min-width="180"
            ><template #default="{ row }">{{ formatDateTime(row.endAt) }}</template></ElTableColumn
          >
          <ElTableColumn :label="t('操作')" width="150" fixed="right">
            <template #default="{ row }">
              <ElButton link type="primary" @click.stop="openContest(row)">{{
                t('管理')
              }}</ElButton>
              <ElButton link @click.stop="openEdit(row)">{{ t('编辑') }}</ElButton>
            </template>
          </ElTableColumn>
          <template #empty><ElEmpty :description="t('暂无比赛')" /></template>
        </ElTable>
        <ElRow justify="end" class="pagination-row">
          <ElPagination
            v-model:current-page="currentPage"
            :page-size="page.size"
            :total="page.totalElements"
            layout="prev, pager, next, total"
            @current-change="loadContests"
          />
        </ElRow>
      </ElCard>

      <ElDialog
        v-model="dialogVisible"
        :title="editing ? t('编辑比赛') : t('创建比赛')"
        width="660"
      >
        <ElForm ref="formRef" :model="form" :rules="rules" label-position="top">
          <ElFormItem :label="t('比赛名称')" prop="name"
            ><ElInput v-model="form.name" maxlength="120" show-word-limit
          /></ElFormItem>
          <ElFormItem :label="t('可见性')" prop="visibility">
            <ElRadioGroup v-model="form.visibility">
              <ElRadioButton value="PRIVATE">{{ t('私有比赛') }}</ElRadioButton>
              <ElRadioButton value="PUBLIC">{{ t('公开比赛') }}</ElRadioButton>
            </ElRadioGroup>
          </ElFormItem>
          <ElRow :gutter="12" class="admin-form-grid">
            <ElCol :xs="24" :sm="8">
              <ElFormItem :label="t('开始时间')"
                ><ElDatePicker v-model="form.startAt" type="datetime"
              /></ElFormItem>
            </ElCol>
            <ElCol :xs="24" :sm="8">
              <ElFormItem :label="t('封榜时间')"
                ><ElDatePicker v-model="form.freezeAt" type="datetime"
              /></ElFormItem>
            </ElCol>
            <ElCol :xs="24" :sm="8">
              <ElFormItem :label="t('结束时间')"
                ><ElDatePicker v-model="form.endAt" type="datetime"
              /></ElFormItem>
            </ElCol>
          </ElRow>
          <ElAlert
            v-if="dialogError"
            :title="dialogError"
            type="error"
            show-icon
            :closable="false"
          />
        </ElForm>
        <template #footer>
          <ElButton @click="dialogVisible = false">{{ t('取消') }}</ElButton>
          <ElButton type="primary" :loading="saving" @click="save">{{
            editing ? t('保存修改') : t('创建比赛')
          }}</ElButton>
        </template>
      </ElDialog>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { useRouter } from 'vue-router';
import type { FormInstance, FormRules } from 'element-plus';
import { Plus } from '@element-plus/icons-vue';
import { adminContestApi } from '../api/admin-contests';
import { getErrorMessage } from '../api/client';
import type { Contest, ContestVisibility, PageResponse } from '../api/types';
import { useSession } from '../auth/session';
import { contestStatusLabel, formatDateTime } from '../utils/format';
import { useI18n } from '../i18n';

interface ContestForm {
  name: string;
  visibility: ContestVisibility;
  startAt: Date | null;
  freezeAt: Date | null;
  endAt: Date | null;
}

const router = useRouter();
const session = useSession();
const { t } = useI18n();
const page = ref<PageResponse<Contest>>({
  content: [],
  page: 0,
  size: 25,
  totalElements: 0,
  totalPages: 0,
});
const currentPage = ref(1);
const loading = ref(false);
const saving = ref(false);
const errorMessage = ref('');
const dialogError = ref('');
const dialogVisible = ref(false);
const editing = ref<Contest | null>(null);
const formRef = ref<FormInstance>();
const form = reactive<ContestForm>({
  name: '',
  visibility: 'PRIVATE',
  startAt: null,
  freezeAt: null,
  endAt: null,
});
const rules = computed<FormRules>(() => ({
  name: [{ required: true, message: t('请输入比赛名称'), trigger: 'blur' }],
}));

async function loadContests() {
  loading.value = true;
  errorMessage.value = '';
  try {
    page.value = await adminContestApi.listContests(currentPage.value - 1, page.value.size);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}

function resetForm(contest?: Contest) {
  form.name = contest?.name ?? '';
  form.visibility = contest?.visibility ?? 'PRIVATE';
  form.startAt = contest?.startAt ? new Date(contest.startAt) : null;
  form.freezeAt = contest?.freezeAt ? new Date(contest.freezeAt) : null;
  form.endAt = contest?.endAt ? new Date(contest.endAt) : null;
}
function openCreate() {
  editing.value = null;
  dialogError.value = '';
  resetForm();
  dialogVisible.value = true;
}
function openEdit(row: unknown) {
  editing.value = row as Contest;
  dialogError.value = '';
  resetForm(editing.value);
  dialogVisible.value = true;
}
function openContest(row: unknown) {
  void router.push(`/admin/contests/${(row as Contest).id}`);
}

async function save() {
  if (!(await formRef.value?.validate().catch(() => false))) return;
  const configured = [form.startAt, form.freezeAt, form.endAt].filter(Boolean).length;
  if (configured !== 0 && configured !== 3) {
    dialogError.value = t('赛程时间必须全部填写，或全部留空。');
    return;
  }
  saving.value = true;
  dialogError.value = '';
  const payload = {
    name: form.name.trim(),
    visibility: form.visibility,
    startAt: form.startAt?.toISOString() ?? null,
    freezeAt: form.freezeAt?.toISOString() ?? null,
    endAt: form.endAt?.toISOString() ?? null,
  };
  try {
    if (editing.value) await adminContestApi.updateContest(editing.value.id, payload);
    else await adminContestApi.createContest(payload);
    dialogVisible.value = false;
    await loadContests();
  } catch (error) {
    dialogError.value = getErrorMessage(error);
  } finally {
    saving.value = false;
  }
}
function contestTagType(status: string) {
  return status === 'RUNNING'
    ? 'success'
    : status === 'PAUSED'
      ? 'warning'
      : status === 'ENDED' || status === 'ARCHIVED'
        ? 'info'
        : 'primary';
}
onMounted(loadContests);
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
