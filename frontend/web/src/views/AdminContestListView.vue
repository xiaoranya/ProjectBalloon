<template>
  <section class="admin-page">
    <header class="admin-page-header">
      <div>
        <p class="eyebrow">Contest Operations</p>
        <h1>比赛管理</h1>
        <p>创建比赛并进入配置、运行和赛后管理流程。</p>
      </div>
      <ElButton v-if="session.isSuperAdmin.value" type="primary" :icon="Plus" @click="openCreate">
        创建比赛
      </ElButton>
    </header>

    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />

    <ElCard shadow="never" class="admin-card">
      <ElTable v-loading="loading" :data="page.content" row-key="id" @row-click="openContest">
        <ElTableColumn label="比赛" min-width="240">
          <template #default="{ row }">
            <div class="admin-primary-cell">
              <strong>{{ row.name }}</strong>
              <small>#{{ row.id }} · {{ row.visibility === 'PUBLIC' ? '公开' : '私有' }}</small>
            </div>
          </template>
        </ElTableColumn>
        <ElTableColumn label="状态" width="130">
          <template #default="{ row }"><ElTag :type="contestTagType(row.status)">{{ contestStatusLabel(row.status) }}</ElTag></template>
        </ElTableColumn>
        <ElTableColumn label="开始时间" min-width="180"><template #default="{ row }">{{ formatDateTime(row.startAt) }}</template></ElTableColumn>
        <ElTableColumn label="结束时间" min-width="180"><template #default="{ row }">{{ formatDateTime(row.endAt) }}</template></ElTableColumn>
        <ElTableColumn label="操作" width="150" fixed="right">
          <template #default="{ row }">
            <ElButton link type="primary" @click.stop="openContest(row)">管理</ElButton>
            <ElButton link @click.stop="openEdit(row)">编辑</ElButton>
          </template>
        </ElTableColumn>
        <template #empty><ElEmpty description="暂无比赛" /></template>
      </ElTable>
      <div class="pagination-row">
        <ElPagination
          v-model:current-page="currentPage"
          :page-size="page.size"
          :total="page.totalElements"
          layout="prev, pager, next, total"
          @current-change="loadContests"
        />
      </div>
    </ElCard>

    <ElDialog v-model="dialogVisible" :title="editing ? '编辑比赛' : '创建比赛'" width="660">
      <ElForm ref="formRef" :model="form" :rules="rules" label-position="top">
        <ElFormItem label="比赛名称" prop="name"><ElInput v-model="form.name" maxlength="120" show-word-limit /></ElFormItem>
        <ElFormItem label="可见性" prop="visibility">
          <ElRadioGroup v-model="form.visibility">
            <ElRadioButton value="PRIVATE">私有比赛</ElRadioButton>
            <ElRadioButton value="PUBLIC">公开比赛</ElRadioButton>
          </ElRadioGroup>
        </ElFormItem>
        <div class="admin-form-grid">
          <ElFormItem label="开始时间"><ElDatePicker v-model="form.startAt" type="datetime" /></ElFormItem>
          <ElFormItem label="封榜时间"><ElDatePicker v-model="form.freezeAt" type="datetime" /></ElFormItem>
          <ElFormItem label="结束时间"><ElDatePicker v-model="form.endAt" type="datetime" /></ElFormItem>
        </div>
        <ElAlert v-if="dialogError" :title="dialogError" type="error" show-icon :closable="false" />
      </ElForm>
      <template #footer>
        <ElButton @click="dialogVisible = false">取消</ElButton>
        <ElButton type="primary" :loading="saving" @click="save">{{ editing ? '保存修改' : '创建比赛' }}</ElButton>
      </template>
    </ElDialog>
  </section>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
import { useRouter } from 'vue-router';
import type { FormInstance, FormRules } from 'element-plus';
import { Plus } from '@element-plus/icons-vue';
import { adminContestApi } from '../api/admin-contests';
import { getErrorMessage } from '../api/client';
import type { Contest, ContestVisibility, PageResponse } from '../api/types';
import { useSession } from '../auth/session';
import { contestStatusLabel, formatDateTime } from '../utils/format';

interface ContestForm {
  name: string;
  visibility: ContestVisibility;
  startAt: Date | null;
  freezeAt: Date | null;
  endAt: Date | null;
}

const router = useRouter();
const session = useSession();
const page = ref<PageResponse<Contest>>({ content: [], page: 0, size: 25, totalElements: 0, totalPages: 0 });
const currentPage = ref(1);
const loading = ref(false);
const saving = ref(false);
const errorMessage = ref('');
const dialogError = ref('');
const dialogVisible = ref(false);
const editing = ref<Contest | null>(null);
const formRef = ref<FormInstance>();
const form = reactive<ContestForm>({ name: '', visibility: 'PRIVATE', startAt: null, freezeAt: null, endAt: null });
const rules: FormRules = { name: [{ required: true, message: '请输入比赛名称', trigger: 'blur' }] };

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
function openCreate() { editing.value = null; dialogError.value = ''; resetForm(); dialogVisible.value = true; }
function openEdit(row: unknown) { editing.value = row as Contest; dialogError.value = ''; resetForm(editing.value); dialogVisible.value = true; }
function openContest(row: unknown) { void router.push(`/admin/contests/${(row as Contest).id}`); }

async function save() {
  if (!(await formRef.value?.validate().catch(() => false))) return;
  const configured = [form.startAt, form.freezeAt, form.endAt].filter(Boolean).length;
  if (configured !== 0 && configured !== 3) {
    dialogError.value = '赛程时间必须全部填写，或全部留空。';
    return;
  }
  saving.value = true;
  dialogError.value = '';
  const payload = {
    name: form.name.trim(), visibility: form.visibility,
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
function contestTagType(status: string) { return status === 'RUNNING' ? 'success' : status === 'PAUSED' ? 'warning' : status === 'ENDED' || status === 'ARCHIVED' ? 'info' : 'primary'; }
onMounted(loadContests);
</script>
