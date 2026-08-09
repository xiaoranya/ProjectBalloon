<template>
  <ElCard shadow="never"
    ><template #header
      ><div class="card-header">
        <div>
          <strong>{{ t('播放列表与同步分组') }}</strong
          ><small>{{ t('同组大屏按服务器时间轴同步播放，锁屏会覆盖播放列表。') }}</small>
        </div>
        <ElButton :loading="loading" @click="load">{{ t('刷新') }}</ElButton>
      </div></template
    >
    <ElTabs>
      <ElTabPane :label="t('播放列表')"
        ><div class="toolbar">
          <ElButton type="primary" @click="editPlaylist()">{{ t('新建播放列表') }}</ElButton>
        </div>
        <ElTable :data="playlists" row-key="id"
          ><ElTableColumn prop="name" :label="t('名称')" /><ElTableColumn
            :label="t('内容')"
            min-width="260"
            ><template #default="{ row }"
              ><ElTag v-for="item in row.items" :key="item.id"
                >{{ label(item.targetView) }} · {{ item.durationSeconds }}s</ElTag
              ></template
            ></ElTableColumn
          ><ElTableColumn :label="t('循环')" width="80"
            ><template #default="{ row }">{{
              row.loopEnabled ? t('是') : t('否')
            }}</template></ElTableColumn
          ><ElTableColumn :label="t('操作')" width="150"
            ><template #default="{ row }"
              ><ElButton link @click="editPlaylist(row)">{{ t('编辑') }}</ElButton
              ><ElButton link type="danger" @click="removePlaylist(row)">{{
                t('删除')
              }}</ElButton></template
            ></ElTableColumn
          ></ElTable
        ></ElTabPane
      >
      <ElTabPane :label="t('同步分组')"
        ><div class="toolbar">
          <ElButton type="primary" @click="editGroup()">{{ t('新建分组') }}</ElButton>
        </div>
        <ElTable :data="groups" row-key="id"
          ><ElTableColumn prop="name" :label="t('分组')" /><ElTableColumn :label="t('成员')"
            ><template #default="{ row }">{{
              row.instanceIds.map(instanceName).join('、') || t('无成员')
            }}</template></ElTableColumn
          ><ElTableColumn :label="t('状态')" width="140"
            ><template #default="{ row }"
              ><ElTag>{{ row.playbackStatus }}</ElTag
              ><ElTag v-if="row.lockedView" type="danger">{{
                t('锁定 {view}', { view: label(row.lockedView) })
              }}</ElTag></template
            ></ElTableColumn
          ><ElTableColumn :label="t('控制')" min-width="420"
            ><template #default="{ row }"
              ><ElSelect
                v-model="selectedPlaylist[row.id]"
                :placeholder="t('播放列表')"
                style="width: 150px"
                ><ElOption
                  v-for="item in playlists"
                  :key="item.id"
                  :label="item.name"
                  :value="item.id" /></ElSelect
              ><ElButton @click="control(row, 'PLAY')">{{ t('播放') }}</ElButton
              ><ElButton v-if="row.playbackStatus === 'PLAYING'" @click="control(row, 'PAUSE')">{{
                t('暂停')
              }}</ElButton
              ><ElButton v-if="row.playbackStatus === 'PAUSED'" @click="control(row, 'RESUME')">{{
                t('继续')
              }}</ElButton
              ><ElButton @click="control(row, 'STOP')">{{ t('停止') }}</ElButton
              ><ElDropdown @command="(value) => control(row, 'LOCK', value)"
                ><ElButton>{{ t('锁屏') }}</ElButton
                ><template #dropdown
                  ><ElDropdownMenu
                    ><ElDropdownItem v-for="target in targets" :key="target" :command="target">{{
                      label(target)
                    }}</ElDropdownItem></ElDropdownMenu
                  ></template
                ></ElDropdown
              ><ElButton v-if="row.lockedView" @click="control(row, 'UNLOCK')">{{
                t('解锁')
              }}</ElButton></template
            ></ElTableColumn
          ><ElTableColumn :label="t('管理')" width="130"
            ><template #default="{ row }"
              ><ElButton link @click="editGroup(row)">{{ t('编辑') }}</ElButton
              ><ElButton link type="danger" @click="removeGroup(row)">{{
                t('删除')
              }}</ElButton></template
            ></ElTableColumn
          ></ElTable
        ></ElTabPane
      >
    </ElTabs>
    <ElDialog
      v-model="playlistDialog"
      :title="playlistEditing ? t('编辑播放列表') : t('新建播放列表')"
      width="620"
      ><ElForm label-position="top"
        ><ElFormItem :label="t('名称')"
          ><ElInput v-model="playlistForm.name" maxlength="120" /></ElFormItem
        ><ElCheckbox v-model="playlistForm.loopEnabled">{{ t('循环播放') }}</ElCheckbox>
        <div v-for="(item, index) in playlistForm.items" :key="index" class="item-row">
          <ElSelect v-model="item.targetView"
            ><ElOption
              v-for="target in targets"
              :key="target"
              :value="target"
              :label="label(target)" /></ElSelect
          ><ElInputNumber v-model="item.durationSeconds" :min="5" :max="3600" /><ElButton
            :disabled="playlistForm.items.length === 1"
            @click="playlistForm.items.splice(index, 1)"
            >{{ t('移除') }}</ElButton
          >
        </div>
        <ElButton
          :disabled="playlistForm.items.length >= 20"
          @click="playlistForm.items.push({ targetView: 'SCOREBOARD', durationSeconds: 15 })"
          >{{ t('添加画面') }}</ElButton
        ></ElForm
      ><template #footer
        ><ElButton @click="playlistDialog = false">{{ t('取消') }}</ElButton
        ><ElButton type="primary" @click="savePlaylist">{{ t('保存') }}</ElButton></template
      ></ElDialog
    >
    <ElDialog
      v-model="groupDialog"
      :title="groupEditing ? t('编辑分组') : t('新建分组')"
      width="520"
      ><ElForm label-position="top"
        ><ElFormItem :label="t('名称')"
          ><ElInput v-model="groupForm.name" maxlength="120" /></ElFormItem
        ><ElFormItem :label="t('大屏成员')"
          ><ElSelect v-model="groupForm.instanceIds" multiple style="width: 100%"
            ><ElOption
              v-for="item in activeInstances"
              :key="item.id"
              :value="item.id"
              :label="item.name" /></ElSelect></ElFormItem></ElForm
      ><template #footer
        ><ElButton @click="groupDialog = false">{{ t('取消') }}</ElButton
        ><ElButton type="primary" @click="saveGroup">{{ t('保存') }}</ElButton></template
      ></ElDialog
    >
  </ElCard>
</template>
<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { getErrorMessage } from '../api/client';
import {
  screenApi,
  type ScreenGroup,
  type ScreenGroupAction,
  type ScreenInstance,
  type ScreenPlaylist,
  type ScreenViewTarget,
} from '../api/screen';
import { useI18n } from '../i18n';
const { t } = useI18n();
const props = defineProps<{ contestId: number; instances: ScreenInstance[] }>();
const playlists = ref<ScreenPlaylist[]>([]);
const groups = ref<ScreenGroup[]>([]);
const loading = ref(false);
const playlistDialog = ref(false);
const groupDialog = ref(false);
const playlistEditing = ref<ScreenPlaylist | null>(null);
const groupEditing = ref<ScreenGroup | null>(null);
const selectedPlaylist = reactive<Record<number, number | undefined>>({});
const targets: ScreenViewTarget[] = [
  'SCOREBOARD',
  'FIRST_BLOOD',
  'BALLOONS',
  'FREEZE_COUNTDOWN',
  'STATISTICS',
  'RESOLVER',
  'AWARDS',
];
const playlistForm = reactive({
  name: '',
  loopEnabled: true,
  items: [{ targetView: 'SCOREBOARD' as ScreenViewTarget, durationSeconds: 15 }],
});
const groupForm = reactive({ name: '', instanceIds: [] as number[] });
const activeInstances = computed(() => props.instances.filter((item) => !item.revokedAt));
function label(value: ScreenViewTarget) {
  return t(
    {
      SCOREBOARD: '榜单',
      FIRST_BLOOD: '首杀',
      BALLOONS: '气球',
      FREEZE_COUNTDOWN: '封榜倒计时',
      STATISTICS: '统计',
      RESOLVER: 'Resolver',
      AWARDS: '颁奖',
    }[value],
  );
}
function instanceName(id: number) {
  return props.instances.find((item) => item.id === id)?.name ?? `#${id}`;
}
async function load() {
  loading.value = true;
  try {
    [playlists.value, groups.value] = await Promise.all([
      screenApi.playlists(props.contestId),
      screenApi.groups(props.contestId),
    ]);
    groups.value.forEach((row) => {
      selectedPlaylist[row.id] = row.playlistId ?? selectedPlaylist[row.id];
    });
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    loading.value = false;
  }
}
function editPlaylist(value?: unknown) {
  const row = value as ScreenPlaylist | undefined;
  playlistEditing.value = row ?? null;
  Object.assign(playlistForm, {
    name: row?.name ?? '',
    loopEnabled: row?.loopEnabled ?? true,
    items: row?.items.map((item) => ({
      targetView: item.targetView,
      durationSeconds: item.durationSeconds,
    })) ?? [{ targetView: 'SCOREBOARD', durationSeconds: 15 }],
  });
  playlistDialog.value = true;
}
async function savePlaylist() {
  try {
    const body = {
      name: playlistForm.name,
      loopEnabled: playlistForm.loopEnabled,
      items: playlistForm.items,
      ...(playlistEditing.value ? { expectedVersion: playlistEditing.value.version } : {}),
    };
    if (playlistEditing.value) await screenApi.updatePlaylist(playlistEditing.value.id, body);
    else await screenApi.createPlaylist(props.contestId, body);
    playlistDialog.value = false;
    await load();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}
async function removePlaylist(value: unknown) {
  const row = value as ScreenPlaylist;
  try {
    await ElMessageBox.confirm(t('删除播放列表“{name}”？', { name: row.name }), t('确认删除'));
    await screenApi.deletePlaylist(row.id);
    await load();
  } catch (error) {
    if (error !== 'cancel') ElMessage.error(getErrorMessage(error));
  }
}
function editGroup(value?: unknown) {
  const row = value as ScreenGroup | undefined;
  groupEditing.value = row ?? null;
  groupForm.name = row?.name ?? '';
  groupForm.instanceIds = [...(row?.instanceIds ?? [])];
  groupDialog.value = true;
}
async function saveGroup() {
  try {
    const body = {
      name: groupForm.name,
      instanceIds: groupForm.instanceIds,
      ...(groupEditing.value ? { expectedVersion: groupEditing.value.version } : {}),
    };
    if (groupEditing.value) await screenApi.updateGroup(groupEditing.value.id, body);
    else await screenApi.createGroup(props.contestId, body);
    groupDialog.value = false;
    await load();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}
async function removeGroup(value: unknown) {
  const row = value as ScreenGroup;
  try {
    await ElMessageBox.confirm(t('删除同步分组“{name}”？', { name: row.name }), t('确认删除'));
    await screenApi.deleteGroup(row.id);
    await load();
  } catch (error) {
    if (error !== 'cancel') ElMessage.error(getErrorMessage(error));
  }
}
async function control(value: unknown, action: ScreenGroupAction, target?: ScreenViewTarget) {
  const row = value as ScreenGroup;
  try {
    const options =
      action === 'PLAY'
        ? { playlistId: selectedPlaylist[row.id] }
        : action === 'LOCK'
          ? { targetView: target }
          : {};
    if (action === 'PLAY' && !options.playlistId) {
      ElMessage.warning(t('请先选择播放列表'));
      return;
    }
    await screenApi.controlGroup(row.id, action, row.version, options);
    await load();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}
watch(
  () => props.contestId,
  () => void load(),
  { immediate: true },
);
</script>
<style scoped>
.card-header {
  display: flex;
  justify-content: space-between;
}
.card-header div {
  display: grid;
}
.card-header small {
  color: #8492a6;
}
.toolbar {
  margin-bottom: 12px;
}
.item-row {
  display: grid;
  grid-template-columns: 1fr 160px auto;
  gap: 10px;
  margin: 12px 0;
}
.el-tag + .el-tag {
  margin-left: 6px;
}
</style>
