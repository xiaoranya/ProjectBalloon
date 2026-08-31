<template>
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
      <ElInput v-model="teamForm.groupName" :placeholder="t('分组（可选）')" style="width: 180px" />
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
        <template #default="{ row }">{{ participationLabel(row.participationType) }}</template>
      </ElTableColumn>
      <ElTableColumn prop="groupName" :label="t('分组')" min-width="140">
        <template #default="{ row }">{{ row.groupName || '—' }}</template>
      </ElTableColumn>
      <ElTableColumn :label="t('操作')" width="100">
        <template #default="{ row }">
          <ElButton link type="danger" @click="unassignTeam(row.teamId)">{{ t('移除') }}</ElButton>
        </template>
      </ElTableColumn>
      <template #empty><ElEmpty :description="t('尚未分配队伍')" /></template>
    </ElTable>
  </ElCard>
</template>

<script setup lang="ts">
import { computed, reactive, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { adminContestApi } from '../../api/admin-contests';
import { getErrorMessage } from '../../api/client';
import type { ContestTeamResponse, TeamResponse } from '../../api/types';
import { useI18n } from '../../i18n';

const props = defineProps<{
  contestId: number;
  teams: TeamResponse[];
  contestTeams: ContestTeamResponse[];
}>();
const emit = defineEmits<{ 'update:contest-teams': [value: ContestTeamResponse[]] }>();
const { t } = useI18n();

const assigning = ref(false);
const teamForm = reactive({
  teamId: null as number | null,
  participationType: 'OFFICIAL' as ContestTeamResponse['participationType'],
  groupName: '',
});

const availableTeams = computed(() => {
  const assigned = new Set(props.contestTeams.map((item) => item.teamId));
  return props.teams.filter((team) => !assigned.has(team.id));
});

async function assignTeam() {
  if (!teamForm.teamId) return;
  assigning.value = true;
  try {
    await adminContestApi.assignTeam(props.contestId, {
      teamId: teamForm.teamId,
      participationType: teamForm.participationType,
      groupName: teamForm.groupName.trim() || null,
    });
    emit('update:contest-teams', await adminContestApi.listContestTeams(props.contestId));
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
    await adminContestApi.unassignTeam(props.contestId, teamId);
    emit('update:contest-teams', await adminContestApi.listContestTeams(props.contestId));
  } catch (error) {
    if (error !== 'cancel' && error !== 'close') ElMessage.error(getErrorMessage(error));
  }
}

function participationLabel(type: string) {
  const label = { OFFICIAL: '正式参赛', STAR: '打星队伍', PRACTICE: '练习队伍' }[type];
  return label ? t(label) : type;
}
</script>

<style scoped></style>
