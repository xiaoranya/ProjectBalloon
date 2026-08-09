<template>
  <el-container direction="vertical" class="admin-page team-import-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <h1>{{ t('队伍批量导入') }}</h1>
        <ElButton @click="fillExample">{{ t('填入示例') }}</ElButton>
      </div>
    </el-header>
    <el-main class="page-body">
      <ElAlert
        v-if="session.isContestAdmin.value"
        type="warning"
        show-icon
        :closable="false"
        class="page-alert"
        :title="t('比赛管理员只能导入到已授权比赛。下拉框已请求可管理比赛；后端权限校验始终是最终边界。')"
      />
      <ElAlert
        v-if="errorMessage"
        type="error"
        show-icon
        :closable="false"
        class="page-alert"
        :title="errorMessage"
      />

      <ElCard shadow="never">
        <template #header><strong>{{ t('批次设置') }}</strong></template>
        <ElRow :gutter="16" align="bottom" class="team-import-options-grid">
          <ElCol :xs="24" :md="11">
            <ElFormItem :label="t('比赛')">
              <ElSelect
                v-model="contestId"
                :clearable="!session.isContestAdmin.value"
                filterable
                :placeholder="t('超级管理员可不分配比赛')"
              >
                <ElOption
                  v-for="contest in contests"
                  :key="contest.id"
                  :label="`${contest.name} (#${contest.id})`"
                  :value="contest.id"
                />
              </ElSelect>
            </ElFormItem>
          </ElCol>
          <ElCol :xs="24" :md="5">
            <ElFormItem :label="t('参赛类型')">
              <ElSelect v-model="participationType" :disabled="contestId === null">
                <ElOption :label="t('正式队')" value="OFFICIAL" />
                <ElOption :label="t('打星队')" value="STAR" />
                <ElOption :label="t('练习队')" value="PRACTICE" />
              </ElSelect>
            </ElFormItem>
          </ElCol>
          <ElCol :xs="24" :md="8">
            <ElFormItem :label="t('幂等键前缀')">
              <ElInput
                v-model="idempotencyKey"
                maxlength="96"
                show-word-limit
                :placeholder="t('留空时生成 UUID')"
              />
            </ElFormItem>
          </ElCol>
          <ElCol :xs="24" :md="11">
            <ElFormItem :label="t('账号策略')">
              <ElCheckbox v-model="requirePasswordReset">{{ t('账号须在首次登录时修改密码') }}</ElCheckbox>
            </ElFormItem>
          </ElCol>
        </ElRow>
        <p class="form-hint">
          {{ t('超过 100 行会拆成独立原子批次；批间不具备原子性。每批使用“前缀-part-N-UUID”唯一键。成员不属于 Rust 批次契约，将在队伍创建成功后逐个添加，成员失败不会伪装成队伍行失败。取消勾选“账号须在首次登录时修改密码”仅应在初始密码由线下渠道交付时使用。') }}
        </p>
      </ElCard>

      <ElCard shadow="never">
        <template #header>
          <div class="card-header">
            <strong>{{ t('粘贴 / 编辑 JSON') }}</strong
            ><small>{{ t('{teams} 支队伍，预计 {batches} 个后台批次', { teams: drafts.length, batches: plannedBatchCount }) }}</small>
          </div>
        </template>
        <CodeEditor
          v-model="source"
          language="json"
          height="360px"
          :readonly="importing"
          :placeholder="t('粘贴 JSON 数组')"
        />
        <ElRow justify="end" class="team-import-source-actions-row">
          <ElSpace wrap>
            <ElButton :disabled="importing || !source.trim()" @click="parseSource"
              >{{ t('解析并编辑') }}</ElButton
            >
            <ElButton :disabled="importing || !drafts.length" @click="syncSource"
              >{{ t('同步编辑到 JSON') }}</ElButton
            >
            <ElButton
              type="primary"
              :loading="importing"
              :disabled="invalidCount > 0 || drafts.length === 0"
              @click="submitImport"
              >{{ t('开始导入') }}</ElButton
            >
          </ElSpace>
        </ElRow>
      </ElCard>

      <ElCard v-if="drafts.length" shadow="never">
        <template #header>
          <div class="card-header">
            <strong>{{ t('队伍与成员') }}</strong
            ><small>{{ invalidCount ? t('{count} 项待修正', { count: invalidCount }) : t('字段校验通过') }}</small>
          </div>
        </template>
        <div v-for="(team, index) in drafts" :key="team.clientId" class="team-import-editor">
          <div class="team-import-editor-heading">
            <strong>{{ t('第 {index} 支：{name}', { index: index + 1, name: team.name || t('未命名') }) }}</strong>
            <ElButton text type="danger" :disabled="importing" @click="removeTeam(index)"
              >{{ t('删除') }}</ElButton
            >
          </div>
          <div class="team-import-grid">
            <ElInput v-model="team.name" :placeholder="t('队名 *')" />
            <ElInput v-model="team.school" :placeholder="t('学校')" />
            <ElInput v-model="team.seatNo" :placeholder="t('座位号')" />
            <ElInput v-model="team.groupName" :placeholder="t('分组')" />
            <ElInput v-model="team.username" :placeholder="t('登录名 *')" />
            <ElInput v-model="team.initialPassword" :placeholder="t('初始密码 *')" show-password />
            <ElCheckbox v-model="team.star">{{ t('该队强制按打星队导入') }}</ElCheckbox>
          </div>
          <div v-if="teamErrors(team).length" class="field-error">
            {{ teamErrors(team).join('；') }}
          </div>
          <div class="team-import-members">
            <div
              v-for="(member, memberIndex) in team.members"
              :key="member.clientId"
              class="team-import-member-row"
            >
              <ElInput v-model="member.name" :placeholder="t('成员姓名 *')" />
              <ElInput v-model="member.email" :placeholder="t('邮箱')" />
              <ElInput v-model="member.phone" :placeholder="t('电话')" />
              <ElInput v-model="member.roleName" :placeholder="t('角色（如教练/队员）')" />
              <ElButton text type="danger" @click="team.members.splice(memberIndex, 1)"
                >{{ t('移除') }}</ElButton
              >
            </div>
            <ElButton text @click="addMember(team)">{{ t('添加成员') }}</ElButton>
          </div>
        </div>
        <ElButton @click="addTeam">{{ t('添加队伍') }}</ElButton>
      </ElCard>

      <ElCard v-if="batchResults.length" shadow="never">
        <template #header>
          <div class="card-header">
            <div>
              <strong>{{ t('后台批次结果') }}</strong
              ><small>{{ t('成功 {success} / {total}', { success: successfulBatchCount, total: batchResults.length }) }}</small>
            </div>
            <div>
              <ElButton v-if="credentials.length" @click="copyCredentials">{{ t('一次性复制账号') }}</ElButton>
              <ElButton v-if="credentials.length" @click="downloadCredentials"
                >{{ t('一次性下载 CSV') }}</ElButton
              >
              <ElButton v-if="credentials.length" type="danger" plain @click="clearCredentials"
                >{{ t('清除密码') }}</ElButton
              >
            </div>
          </div>
        </template>
        <ElTable :data="batchResults" row-key="number">
          <ElTableColumn prop="number" :label="t('批次')" width="70" />
          <ElTableColumn prop="range" :label="t('输入范围')" width="120" />
          <ElTableColumn prop="idempotencyKey" :label="t('幂等键')" min-width="300" />
          <ElTableColumn prop="batchId" :label="t('后台 batchId')" min-width="220" />
          <ElTableColumn :label="t('状态')" width="130"
            ><template #default="{ row }"
              ><ElTag
                :type="
                  row.status === 'SUCCESS' ? 'success' : row.status === 'FAILED' ? 'danger' : 'info'
                "
                >{{ resultStatusLabel(row.status) }}</ElTag
              ></template
            ></ElTableColumn
          >
          <ElTableColumn prop="message" :label="t('结果')" min-width="260" />
        </ElTable>
        <ElAlert
          v-if="credentials.length"
          type="warning"
          show-icon
          :closable="false"
          :title="t('密码只保存在当前页面内存中；复制、下载或清除后即从结果中移除，不写入 localStorage/sessionStorage。')"
        />
        <ElTable v-if="credentials.length" :data="credentials" row-key="inputIndex">
          <ElTableColumn prop="inputIndex" :label="t('原始行')" width="90"
            ><template #default="{ row }">{{ row.inputIndex + 1 }}</template></ElTableColumn
          >
          <ElTableColumn prop="teamName" :label="t('队伍')" />
          <ElTableColumn prop="username" :label="t('账号')" />
          <ElTableColumn prop="initialPassword" :label="t('初始密码')" />
        </ElTable>
      </ElCard>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { adminApi } from '../api/admin';
import { getErrorMessage } from '../api/client';
import { teamImportApi } from '../api/team-import';
import type { Contest } from '../api/types';
import CodeEditor from '../components/CodeEditor.vue';
import type {
  ParticipationType,
  TeamBatchImportRequest,
  TeamMemberRequest,
} from '../api/team-import';
import { useSession } from '../auth/session';
import { useI18n } from '../i18n';

interface MemberDraft extends TeamMemberRequest {
  clientId: string;
}
interface TeamDraft {
  clientId: string;
  name: string;
  school: string;
  seatNo: string;
  groupName: string;
  star: boolean;
  username: string;
  initialPassword: string;
  members: MemberDraft[];
}
interface BatchResult {
  number: number;
  range: string;
  idempotencyKey: string;
  status: 'PENDING' | 'SUCCESS' | 'FAILED' | 'SKIPPED';
  batchId: string;
  message: string;
}
interface CredentialResult {
  inputIndex: number;
  teamName: string;
  username: string;
  initialPassword: string;
  teamId: number;
  userId: number | null;
}

const session = useSession();
const { t } = useI18n();
const contests = ref<Contest[]>([]);
const contestId = ref<number | null>(null);
const participationType = ref<ParticipationType>('OFFICIAL');
const requirePasswordReset = ref(true);
const idempotencyKey = ref('');
const source = ref('');
const drafts = ref<TeamDraft[]>([]);
const batchResults = ref<BatchResult[]>([]);
const credentials = ref<CredentialResult[]>([]);
const importing = ref(false);
const errorMessage = ref('');
const plannedBatchCount = computed(() => Math.ceil(drafts.value.length / 100));
const invalidCount = computed(() =>
  drafts.value.reduce(
    (count, team) =>
      count + teamErrors(team).length + team.members.filter((member) => !member.name.trim()).length,
    0,
  ),
);
const successfulBatchCount = computed(
  () => batchResults.value.filter((batch) => batch.status === 'SUCCESS').length,
);

onMounted(async () => {
  try {
    contests.value = await adminApi.listAllManageableContests();
    if (session.isContestAdmin.value) contestId.value = contests.value[0]?.id ?? null;
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
});

function newId() {
  return crypto.randomUUID();
}
function blankMember(): MemberDraft {
  return { clientId: newId(), name: '', email: null, phone: null, roleName: null };
}
function blankTeam(): TeamDraft {
  return {
    clientId: newId(),
    name: '',
    school: '',
    seatNo: '',
    groupName: '',
    star: false,
    username: '',
    initialPassword: '',
    members: [],
  };
}
function text(value: unknown) {
  return typeof value === 'string' ? value : '';
}
function optionalText(value: unknown) {
  return typeof value === 'string' ? value : '';
}

function normalizeTeam(value: unknown): TeamDraft {
  const row = value && typeof value === 'object' ? (value as Record<string, unknown>) : {};
  const members = Array.isArray(row.members) ? row.members : [];
  return {
    clientId: newId(),
    name: text(row.name),
    school: optionalText(row.school),
    seatNo: optionalText(row.seatNo),
    groupName: optionalText(row.groupName),
    star: row.star === true,
    username: text(row.username),
    initialPassword: text(row.initialPassword),
    members: members.map((value) => {
      const member = value && typeof value === 'object' ? (value as Record<string, unknown>) : {};
      return {
        clientId: newId(),
        name: text(member.name),
        email: nullable(text(member.email)),
        phone: nullable(text(member.phone)),
        roleName: nullable(text(member.roleName)),
      };
    }),
  };
}

function parseSource() {
  errorMessage.value = '';
  try {
    const parsed: unknown = JSON.parse(source.value);
    if (!Array.isArray(parsed) || parsed.length === 0)
      throw new Error(t('请输入至少一支队伍的 JSON 数组'));
    drafts.value = parsed.map(normalizeTeam);
    batchResults.value = [];
    credentials.value = [];
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}

function syncSource() {
  source.value = JSON.stringify(
    drafts.value.map(({ clientId: _id, members, ...team }) => ({
      ...team,
      members: members.map(({ clientId: _memberId, ...member }) => member),
    })),
    null,
    2,
  );
}

function fillExample() {
  source.value = JSON.stringify(
    [
      {
        name: 'Alpha',
        school: 'Example University',
        seatNo: 'A-01',
        groupName: 'Regional',
        star: false,
        username: 'alpha-team',
        initialPassword: 'ChangeMe123!',
        members: [{ name: 'Alice', email: 'alice@example.com', phone: '', roleName: t('队员') }],
      },
    ],
    null,
    2,
  );
  parseSource();
}

function addTeam() {
  drafts.value.push(blankTeam());
  syncSource();
}
function removeTeam(index: number) {
  drafts.value.splice(index, 1);
  syncSource();
}
function addMember(team: TeamDraft) {
  team.members.push(blankMember());
}
function nullable(value: string) {
  return value.trim() || null;
}
function teamErrors(team: TeamDraft) {
  const errors: string[] = [];
  if (!team.name.trim()) errors.push(t('队名必填'));
  if (team.name.trim().length > 255) errors.push(t('队名最多 255 字符'));
  if (!/^[A-Za-z0-9._-]{3,64}$/.test(team.username.trim()))
    errors.push(t('登录名需为 3–64 位字母、数字、点、下划线或横线'));
  if (team.initialPassword.length < 8 || team.initialPassword.length > 128)
    errors.push(t('初始密码需为 8–128 字符'));
  if (team.groupName.trim().length > 128) errors.push(t('分组最多 128 字符'));
  return errors;
}

function requestRows(chunk: TeamDraft[]): TeamBatchImportRequest['teams'] {
  return chunk.map((team) => ({
    name: team.name.trim(),
    school: nullable(team.school),
    seatNo: nullable(team.seatNo),
    groupName: nullable(team.groupName),
    star: team.star,
    username: team.username.trim().toLowerCase(),
    initialPassword: team.initialPassword,
  }));
}

async function submitImport() {
  if (session.isContestAdmin.value && contestId.value === null) {
    errorMessage.value = t('比赛管理员必须选择一个可管理比赛');
    return;
  }
  try {
    await ElMessageBox.confirm(
      t('将 {teams} 支队伍拆为 {batches} 个独立原子批次。若后续批次失败，已成功批次不会回滚。', { teams: drafts.value.length, batches: plannedBatchCount.value }),
      t('确认批量导入'),
      { type: 'warning' },
    );
  } catch {
    return;
  }
  importing.value = true;
  errorMessage.value = '';
  credentials.value = [];
  const importId = crypto.randomUUID();
  const prefix = idempotencyKey.value.trim() || `team-import-${importId}`;
  const chunks = Array.from({ length: plannedBatchCount.value }, (_, index) =>
    drafts.value.slice(index * 100, (index + 1) * 100),
  );
  batchResults.value = chunks.map((chunk, index) => ({
    number: index + 1,
    range: `${index * 100 + 1}–${index * 100 + chunk.length}`,
    idempotencyKey: makeBatchKey(prefix, index + 1),
    status: 'PENDING',
    batchId: '',
    message: t('等待提交'),
  }));
  for (let index = 0; index < chunks.length; index++) {
    const chunk = chunks[index];
    const resultRow = batchResults.value[index];
    let response;
    try {
      response = await teamImportApi.importTeams({
        teams: requestRows(chunk),
        contestId: contestId.value,
        participationType: contestId.value === null ? null : participationType.value,
        requirePasswordReset: requirePasswordReset.value,
        idempotencyKey: resultRow.idempotencyKey,
      });
    } catch (error) {
      resultRow.status = 'FAILED';
      resultRow.message = getErrorMessage(error);
      for (let later = index + 1; later < batchResults.value.length; later++) {
        batchResults.value[later].status = 'SKIPPED';
        batchResults.value[later].message = t('前序批次失败，未提交');
      }
      errorMessage.value = t('第 {batch} 批失败：{message}。输入已保留，可核对幂等键后重试。', { batch: index + 1, message: resultRow.message });
      break;
    }
    resultRow.status = 'SUCCESS';
    resultRow.batchId = response.batchId;
    resultRow.message = t('已创建 {count} 支队伍', { count: response.created.length });
    const memberErrors: string[] = [];
    for (const created of response.created) {
      const inputIndex = index * 100 + created.index;
      const draft = drafts.value[inputIndex];
      if (created.username)
        credentials.value.push({
          inputIndex,
          teamName: draft.name,
          username: created.username,
          initialPassword: draft.initialPassword,
          teamId: created.teamId,
          userId: created.userId,
        });
      for (const member of draft.members) {
        try {
          await teamImportApi.addMember(created.teamId, {
            name: member.name.trim(),
            email: member.email ? nullable(member.email) : null,
            phone: member.phone ? nullable(member.phone) : null,
            roleName: member.roleName ? nullable(member.roleName) : null,
          });
        } catch (error) {
          memberErrors.push(`${draft.name}/${member.name}: ${getErrorMessage(error)}`);
        }
      }
    }
    if (memberErrors.length)
      resultRow.message += t('；成员添加失败 {count} 项：{errors}', { count: memberErrors.length, errors: memberErrors.join('；') });
  }
  importing.value = false;
  if (!errorMessage.value) ElMessage.success(t('导入完成，共创建 {count} 个账号', { count: credentials.value.length }));
}

function credentialCsv() {
  const rows = credentials.value.map((row) => [
    row.inputIndex + 1,
    row.teamName,
    row.username,
    row.initialPassword,
    row.teamId,
    row.userId,
  ]);
  return [['inputIndex', 'teamName', 'username', 'initialPassword', 'teamId', 'userId'], ...rows]
    .map((row) => row.map((value) => `"${String(value ?? '').replaceAll('"', '""')}"`).join(','))
    .join('\n');
}
async function copyCredentials() {
  await navigator.clipboard.writeText(credentialCsv());
  clearCredentials();
  ElMessage.success(t('账号已复制，页面中的密码已清除'));
}
function downloadCredentials() {
  const url = URL.createObjectURL(
    new Blob([`\uFEFF${credentialCsv()}`], { type: 'text/csv;charset=utf-8' }),
  );
  const link = document.createElement('a');
  link.href = url;
  link.download = `team-accounts-${new Date().toISOString().slice(0, 10)}.csv`;
  link.click();
  URL.revokeObjectURL(url);
  clearCredentials();
  ElMessage.success(t('CSV 已下载，页面中的密码已清除'));
}
function clearCredentials() {
  credentials.value = [];
}
function makeBatchKey(prefix: string, batchNumber: number) {
  const suffix = `-part-${batchNumber}-${crypto.randomUUID()}`;
  return `${prefix.slice(0, 128 - suffix.length)}${suffix}`;
}
function resultStatusLabel(status: BatchResult['status']) {
  return t({ PENDING: '等待', SUCCESS: '成功', FAILED: '失败', SKIPPED: '未提交' }[status]);
}
</script>

<style scoped>
.admin-page {
  width: min(1320px, 100%);
  margin: 0 auto;
}
.team-import-page {
  max-width: 1500px;
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
.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.card-header > div {
  min-width: 0;
}
.card-header small {
  display: block;
  margin: 4px 0 0;
  color: var(--muted);
}
.team-import-options-grid {
  margin-bottom: 14px;
}
.team-import-options-grid :deep(.el-form-item) {
  margin-bottom: 0;
}
.team-import-options-grid :deep(.el-select) {
  width: 100%;
}
.team-import-source-actions-row {
  margin-top: 14px;
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
