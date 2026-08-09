<template>
  <ElCard shadow="never" class="award-recipients-card">
    <template #header>
      <div class="card-header">
        <div>
          <strong>{{ t('获奖名单') }}</strong
          ><small>{{ t('{count} 条记录', { count: awardSet?.recipients.length ?? 0 }) }}</small>
        </div>
        <ElButton
          :icon="Plus"
          :disabled="!awardSet || awardSet.status === 'FROZEN'"
          @click="openManual"
          >{{ t('手工添加') }}</ElButton
        >
      </div>
    </template>
    <ElTable :data="awardSet?.recipients ?? []" row-key="id">
      <ElTableColumn :label="t('奖项')" min-width="150">
        <template #default="{ row }">
          <ElTag effect="plain">{{ row.categoryCode }}</ElTag
          ><span> {{ row.categoryName }}</span>
        </template>
      </ElTableColumn>
      <ElTableColumn :label="t('队伍')" min-width="220">
        <template #default="{ row }">
          <div class="award-team-cell">
            <strong
              >{{ row.teamName
              }}<ElTag v-if="row.isManual" size="small" type="warning">{{
                t('手工')
              }}</ElTag></strong
            ><span>{{ row.school ?? '—' }} · Team #{{ row.teamId }}</span>
          </div>
        </template>
      </ElTableColumn>
      <ElTableColumn prop="rank" :label="t('排名')" width="80" />
      <ElTableColumn prop="solved" :label="t('解题')" width="80" />
      <ElTableColumn prop="penaltyMinutes" :label="t('罚时')" width="90" />
      <ElTableColumn :label="t('操作')" width="90">
        <template #default="{ row }">
          <ElButton
            v-if="row.isManual"
            link
            type="danger"
            :disabled="awardSet?.status === 'FROZEN'"
            @click="removeRecipient(row)"
            >{{ t('移除') }}</ElButton
          >
        </template>
      </ElTableColumn>
      <template #empty><ElEmpty :description="t('尚未生成奖项名单')" /></template>
    </ElTable>
  </ElCard>

  <ElDialog v-model="manualVisible" :title="t('手工添加获奖队伍')" width="min(580px, 92vw)">
    <ElForm label-position="top">
      <ElFormItem :label="t('奖项类别')">
        <ElSelect v-model="manualCategoryId" filterable>
          <ElOption
            v-for="category in categories"
            :key="category.id"
            :value="category.id"
            :label="category.name"
          />
        </ElSelect>
      </ElFormItem>
      <ElFormItem :label="t('队伍')">
        <ElSelect v-model="manualTeamId" filterable>
          <ElOption
            v-for="candidate in availableCandidates"
            :key="candidate.teamId"
            :value="candidate.teamId"
            :label="`#${candidate.rank} · ${candidate.teamName}`"
          />
        </ElSelect>
      </ElFormItem>
    </ElForm>
    <template #footer>
      <ElButton @click="manualVisible = false">{{ t('取消') }}</ElButton>
      <ElButton
        type="primary"
        :disabled="!manualCategoryId || !manualTeamId"
        :loading="mutating"
        @click="addRecipient"
        >{{ t('确认添加') }}</ElButton
      >
    </template>
  </ElDialog>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import { ElMessage } from 'element-plus';
import { Plus } from '@element-plus/icons-vue';
import {
  awardsApi,
  type AwardCandidate,
  type AwardCategory,
  type AwardRecipient,
  type AwardSet,
} from '../../api/awards';
import { getErrorMessage } from '../../api/client';
import { useI18n } from '../../i18n';

const props = defineProps<{
  contestId: number | null;
  awardSet: AwardSet | null;
  categories: AwardCategory[];
  candidates: AwardCandidate[];
  mutating: boolean;
}>();
const emit = defineEmits<{
  'update:award-set': [value: AwardSet];
}>();
const { t } = useI18n();

const manualVisible = ref(false);
const manualCategoryId = ref<number | null>(null);
const manualTeamId = ref<number | null>(null);

const availableCandidates = computed(() =>
  props.candidates.filter(
    (candidate) =>
      !props.awardSet?.recipients.some(
        (recipient) =>
          recipient.categoryId === manualCategoryId.value && recipient.teamId === candidate.teamId,
      ),
  ),
);

function openManual() {
  manualCategoryId.value = props.categories[0]?.id ?? null;
  manualTeamId.value = null;
  manualVisible.value = true;
}
async function addRecipient() {
  if (!props.contestId || !props.awardSet || !manualCategoryId.value || !manualTeamId.value) return;
  try {
    const next = await awardsApi.addRecipient(
      props.contestId,
      manualCategoryId.value,
      manualTeamId.value,
      props.awardSet.version,
    );
    emit('update:award-set', next);
    manualVisible.value = false;
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}
async function removeRecipient(value: unknown) {
  const recipient = value as AwardRecipient;
  if (!props.awardSet) return;
  try {
    const next = await awardsApi.removeRecipient(recipient.id, props.awardSet.version);
    emit('update:award-set', next);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}
</script>

<style scoped>
.card-header > div {
  min-width: 0;
}
.award-recipients-card {
  border: 1px solid #e5eaf2;
  border-radius: 0;
}
.award-team-cell strong,
.award-team-cell > span {
  display: block;
}
.award-team-cell > span {
  margin-top: 5px;
  color: var(--muted);
  font-size: 12px;
}
.award-team-cell .el-tag {
  margin-left: 5px;
}
</style>
