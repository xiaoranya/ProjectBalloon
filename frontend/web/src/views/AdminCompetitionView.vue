<template>
  <section class="competition-admin">
    <header>
      <div>
        <h1>{{ t('比赛终端绑定') }}</h1>
        <p>{{ t('按静态 IP 登记终端，并为每场比赛绑定参赛队。') }}</p>
      </div>
      <ElButton :icon="Refresh" :loading="loading" @click="load">{{ t('刷新') }}</ElButton>
    </header>

    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon />

    <div class="toolbar">
      <ElInputNumber v-model="contestId" :min="1" controls-position="right" />
      <ElButton type="primary" @click="loadBindings">{{ t('加载比赛绑定') }}</ElButton>
      <ElInput v-model="workstationForm.ipAddress" :placeholder="t('终端 IP')" />
      <ElInput v-model="workstationForm.seatNo" :placeholder="t('机位号')" />
      <ElInput v-model="workstationForm.label" :placeholder="t('备注（可选）')" />
      <ElButton :icon="Plus" @click="createWorkstation">{{ t('登记终端') }}</ElButton>
    </div>

    <ElTable :data="workstations" border>
      <ElTableColumn prop="seatNo" :label="t('机位')" width="120" />
      <ElTableColumn prop="ipAddress" :label="t('IP 地址')" width="180" />
      <ElTableColumn prop="label" :label="t('备注')" />
      <ElTableColumn :label="t('状态')" width="100">
        <template #default="scope">{{ t(scope.row.enabled ? '启用' : '停用') }}</template>
      </ElTableColumn>
      <ElTableColumn :label="t('绑定队伍')" min-width="260">
        <template #default="scope">
          <div class="bind-control">
            <ElInputNumber v-model="teamIds[scope.row.id]" :min="1" :placeholder="t('队伍 ID')" />
            <ElButton size="small" @click="bind(scope.row.id)">{{ t('绑定') }}</ElButton>
          </div>
        </template>
      </ElTableColumn>
    </ElTable>

    <h2>{{ t('当前比赛绑定') }}</h2>
    <ElTable :data="bindings" border>
      <ElTableColumn prop="seatNo" :label="t('机位')" width="120" />
      <ElTableColumn prop="ipAddress" :label="t('IP 地址')" width="180" />
      <ElTableColumn prop="teamName" :label="t('队伍')" />
      <ElTableColumn :label="t('状态')" width="100">
        <template #default="scope">{{ t(scope.row.revokedAt ? '已撤销' : '有效') }}</template>
      </ElTableColumn>
      <ElTableColumn :label="t('操作')" width="180">
        <template #default="scope">
          <ElButton size="small" :disabled="!!scope.row.revokedAt" @click="rotate(scope.row)">{{
            t('轮换配对码')
          }}</ElButton>
          <ElButton
            size="small"
            type="danger"
            text
            :disabled="!!scope.row.revokedAt"
            @click="revoke(scope.row)"
            >{{ t('撤销') }}</ElButton
          >
        </template>
      </ElTableColumn>
    </ElTable>

    <ElDialog v-model="codeVisible" :title="t('配对码')" width="420px">
      <p>{{ t('配对码仅在本次操作后显示，请交给对应机位。') }}</p>
      <ElInput :model-value="pairingCode" readonly size="large" />
    </ElDialog>
  </section>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
import { Plus, Refresh } from '@element-plus/icons-vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { getErrorMessage } from '../api/client';
import { competitionApi, type Workstation, type WorkstationBinding } from '../api/competition';
import { useI18n } from '../i18n';

const { t } = useI18n();

const loading = ref(false);
const errorMessage = ref('');
const contestId = ref(1);
const workstations = ref<Workstation[]>([]);
const bindings = ref<WorkstationBinding[]>([]);
const teamIds = reactive<Record<number, number | undefined>>({});
const workstationForm = reactive({ ipAddress: '', seatNo: '', label: '' });
const codeVisible = ref(false);
const pairingCode = ref('');

function showCode(binding: WorkstationBinding) {
  pairingCode.value = binding.pairingCode ?? '';
  codeVisible.value = true;
}
async function run(action: () => Promise<void>) {
  errorMessage.value = '';
  try {
    await action();
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}
async function load() {
  loading.value = true;
  await run(async () => {
    workstations.value = await competitionApi.workstations();
  });
  loading.value = false;
}
async function loadBindings() {
  await run(async () => {
    bindings.value = await competitionApi.bindings(contestId.value);
  });
}
async function createWorkstation() {
  await run(async () => {
    await competitionApi.createWorkstation({
      ...workstationForm,
      label: workstationForm.label || undefined,
    });
    workstationForm.ipAddress = '';
    workstationForm.seatNo = '';
    workstationForm.label = '';
    await load();
  });
}
async function bind(workstationId: number) {
  const teamId = teamIds[workstationId];
  if (!teamId) return ElMessage.warning(t('请输入队伍 ID'));
  await run(async () => {
    showCode(await competitionApi.bind(contestId.value, workstationId, teamId));
    await loadBindings();
  });
}
async function rotate(value: unknown) {
  const binding = value as WorkstationBinding;
  await run(async () => {
    showCode(await competitionApi.rotate(contestId.value, binding.id));
  });
}
async function revoke(value: unknown) {
  const binding = value as WorkstationBinding;
  await ElMessageBox.confirm(t('撤销后该终端的现有会话会立即失效。'), t('确认撤销'), {
    type: 'warning',
  });
  await run(async () => {
    await competitionApi.revoke(contestId.value, binding.id);
    await loadBindings();
  });
}
onMounted(load);
</script>

<style scoped>
.competition-admin {
  display: grid;
  gap: 20px;
}
header {
  display: flex;
  align-items: start;
  justify-content: space-between;
}
h1,
h2,
p {
  margin: 0;
}
header p {
  margin-top: 6px;
  color: var(--muted);
}
h2 {
  font-size: 18px;
}
.toolbar {
  display: grid;
  grid-template-columns: 140px 140px repeat(3, minmax(150px, 1fr)) auto;
  gap: 10px;
}
.bind-control {
  display: flex;
  gap: 8px;
}
@media (max-width: 1100px) {
  .toolbar {
    grid-template-columns: 1fr 1fr;
  }
}
</style>
