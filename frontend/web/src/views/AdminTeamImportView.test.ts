import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import AdminTeamImportView from './AdminTeamImportView.vue';

const mocks = vi.hoisted(() => ({
  listAllManageableContests: vi.fn(),
  importTeams: vi.fn(),
  addMember: vi.fn(),
  confirm: vi.fn(),
  success: vi.fn(),
}));
vi.mock('../api/admin', () => ({
  adminApi: { listAllManageableContests: mocks.listAllManageableContests },
}));
vi.mock('../api/team-import', () => ({
  teamImportApi: { importTeams: mocks.importTeams, addMember: mocks.addMember },
}));
vi.mock('../auth/session', () => ({ useSession: () => ({ isSuperAdmin: { value: true } }) }));
vi.mock('../components/CodeEditor.vue', () => ({
  default: {
    name: 'CodeEditor',
    props: ['modelValue', 'language', 'readonly', 'height', 'placeholder'],
    emits: ['update:modelValue'],
    template:
      '<textarea :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
  },
}));
vi.mock('element-plus', async (importOriginal) => {
  const actual = await importOriginal<typeof import('element-plus')>();
  return {
    ...actual,
    ElMessage: { success: mocks.success },
    ElMessageBox: { confirm: mocks.confirm },
  };
});

function teams(count: number) {
  return Array.from({ length: count }, (_, index) => ({
    name: `Team ${index + 1}`,
    school: '',
    seatNo: '',
    groupName: 'G',
    star: false,
    username: `team-${index + 1}`,
    initialPassword: 'ChangeMe123!',
    members: [],
  }));
}

async function prepare(wrapper: ReturnType<typeof mount>, rows: unknown[]) {
  await wrapper.find('textarea').setValue(JSON.stringify(rows));
  const parseButton = wrapper
    .findAll('button')
    .find((button) => button.text().includes('解析并编辑'))!;
  await parseButton.trigger('click');
  await flushPromises();
}

async function submit(wrapper: ReturnType<typeof mount>) {
  const button = wrapper
    .findAll('button')
    .find((candidate) => candidate.text().includes('开始导入'))!;
  await button.trigger('click');
  await flushPromises();
}

describe('AdminTeamImportView', () => {
  beforeEach(() => {
    mocks.listAllManageableContests.mockResolvedValue([]);
    mocks.confirm.mockResolvedValue(undefined);
    mocks.addMember.mockResolvedValue({});
    let uuid = 0;
    vi.stubGlobal('crypto', {
      randomUUID: () => `00000000-0000-4000-8000-${String(++uuid).padStart(12, '0')}`,
    });
  });
  afterEach(() => vi.clearAllMocks());

  it('splits 101 teams into 100 and 1 with visibly unique idempotency keys', async () => {
    mocks.importTeams
      .mockResolvedValueOnce({ batchId: 'batch-a', totalRequested: 100, created: [] })
      .mockResolvedValueOnce({ batchId: 'batch-b', totalRequested: 1, created: [] });
    const wrapper = mount(AdminTeamImportView);
    await flushPromises();
    await prepare(wrapper, teams(101));
    await submit(wrapper);

    expect(mocks.importTeams).toHaveBeenCalledTimes(2);
    expect(mocks.importTeams.mock.calls[0][0].teams).toHaveLength(100);
    expect(mocks.importTeams.mock.calls[1][0].teams).toHaveLength(1);
    expect(mocks.importTeams.mock.calls[0][0].idempotencyKey).not.toBe(
      mocks.importTeams.mock.calls[1][0].idempotencyKey,
    );
    expect(wrapper.text()).toContain('batch-a');
    expect(wrapper.text()).toContain('batch-b');
    expect(wrapper.text()).toContain('-part-1-');
    expect(wrapper.text()).toContain('-part-2-');
  });

  it('retains editable input and skips later batches when a whole Rust batch fails', async () => {
    mocks.importTeams.mockRejectedValueOnce(new Error('duplicate team'));
    const wrapper = mount(AdminTeamImportView);
    await flushPromises();
    await prepare(wrapper, teams(101));
    const originalSource = (wrapper.find('textarea').element as HTMLTextAreaElement).value;
    await submit(wrapper);

    expect(mocks.importTeams).toHaveBeenCalledTimes(1);
    expect((wrapper.find('textarea').element as HTMLTextAreaElement).value).toBe(originalSource);
    expect(wrapper.text()).toContain('第 1 批失败');
    expect(wrapper.text()).toContain('前序批次失败，未提交');
    expect(wrapper.text()).not.toContain('逐行失败');
  });

  it('shows request passwords only from memory and never writes browser storage', async () => {
    const localSet = vi.spyOn(Storage.prototype, 'setItem');
    mocks.importTeams.mockResolvedValueOnce({
      batchId: 'batch-credentials',
      totalRequested: 1,
      created: [{ index: 0, teamId: 11, userId: 12, username: 'team-1' }],
    });
    const wrapper = mount(AdminTeamImportView);
    await flushPromises();
    await prepare(wrapper, teams(1));
    await submit(wrapper);

    expect(wrapper.text()).toContain('ChangeMe123!');
    expect(localSet).not.toHaveBeenCalled();
  });
});
