<template>
  <ElCard shadow="never">
    <ElSpace wrap :size="12" class="assignment-toolbar">
      <ElSelect
        v-model="problemForm.problemId"
        filterable
        :placeholder="t('选择未分配题目')"
        style="width: min(260px, 100%)"
      >
        <ElOption
          v-for="problem in availableProblems"
          :key="problem.id"
          :label="`${problem.slug} · ${problem.title}`"
          :value="problem.id"
        />
      </ElSelect>
      <ElInput v-model="problemForm.alias" :placeholder="t('题号，如 A')" style="width: 120px" />
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
          !canEditProblemConfiguration || !problemForm.problemId || !problemForm.alias.trim()
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
            :disabled="!canEditProblemConfiguration || scoringMode === 'ICPC'"
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

  <ElDialog v-model="problemEditVisible" :title="t('编辑比赛题目')" width="min(460px, 94%)">
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

  <ElDialog v-model="subtasksVisible" :title="t('子任务与测试点计分')" width="min(900px, 94%)">
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
</template>

<script setup lang="ts">
import { computed, reactive, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { useRouter } from 'vue-router';
import { adminContestApi } from '../../api/admin-contests';
import { getErrorMessage } from '../../api/client';
import type { ContestProblem, ProblemResponse } from '../../api/types';
import { useI18n } from '../../i18n';

const props = defineProps<{
  contestId: number;
  problems: ProblemResponse[];
  contestProblems: ContestProblem[];
  scoringMode: string;
  canEditProblemConfiguration: boolean;
}>();
const emit = defineEmits<{ 'update:contest-problems': [value: ContestProblem[]] }>();
const router = useRouter();
const { t } = useI18n();

const assigning = ref(false);
const reordering = ref(false);
const savingProblem = ref(false);
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

const availableProblems = computed(() => {
  const assigned = new Set(props.contestProblems.map((item) => item.problemId));
  return props.problems.filter((problem) => !assigned.has(problem.id));
});
const sortedContestProblems = computed(() =>
  [...props.contestProblems].sort((a, b) => a.displayOrder - b.displayOrder),
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

async function assignProblem() {
  if (!problemForm.problemId || !problemForm.alias.trim()) return;
  assigning.value = true;
  try {
    await adminContestApi.assignProblem(props.contestId, {
      problemId: problemForm.problemId,
      alias: problemForm.alias.trim().toUpperCase(),
      displayOrder: problemForm.displayOrder,
      color: problemForm.color,
    });
    const value = await adminContestApi.listContestProblems(props.contestId);
    emit('update:contest-problems', value);
    problemForm.problemId = null;
    problemForm.alias = '';
    problemForm.displayOrder = value.length + 1;
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
  void router.push(`/admin/problems/${problemId}?contestId=${props.contestId}`);
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
    const value = await adminContestApi.getProblemSubtasks(props.contestId, problemId);
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
    await adminContestApi.replaceProblemSubtasks(props.contestId, subtaskProblemId.value, {
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
    await adminContestApi.updateProblemAssignment(props.contestId, problemEditForm.problemId, {
      alias: problemEditForm.alias.trim().toUpperCase(),
      displayOrder: problemEditForm.displayOrder,
      color: problemEditForm.color,
    });
    emit('update:contest-problems', await adminContestApi.listContestProblems(props.contestId));
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
    await adminContestApi.unassignProblem(props.contestId, problemId);
    emit('update:contest-problems', await adminContestApi.listContestProblems(props.contestId));
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
      props.contestId,
      reordered.map((problem, position) => ({
        problemId: problem.problemId,
        displayOrder: position + 1,
      })),
    );
    emit('update:contest-problems', await adminContestApi.listContestProblems(props.contestId));
    ElMessage.success(t('题目顺序已更新'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
    emit('update:contest-problems', await adminContestApi.listContestProblems(props.contestId));
  } finally {
    reordering.value = false;
  }
}

function problemName(problemId: number) {
  const problem = props.problems.find((item) => item.id === problemId);
  const assigned = props.contestProblems.find((item) => item.problemId === problemId);
  return problem
    ? `${assigned?.alias ? `${assigned.alias} · ` : ''}${problem.title}`
    : t('题目 #{id}', { id: problemId });
}
</script>

<style scoped>
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
</style>
