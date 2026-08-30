import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AdminCompetitionView from './AdminCompetitionView.vue';
import type { Workstation, WorkstationBinding } from '../api/competition';
import { setLocale } from '../i18n';

const mocks = vi.hoisted(() => ({
  workstations: vi.fn(),
  bindings: vi.fn(),
  createWorkstation: vi.fn(),
  bind: vi.fn(),
  rotate: vi.fn(),
  revoke: vi.fn(),
}));
vi.mock('../api/competition', () => ({
  competitionApi: {
    workstations: mocks.workstations,
    bindings: mocks.bindings,
    createWorkstation: mocks.createWorkstation,
    bind: mocks.bind,
    rotate: mocks.rotate,
    revoke: mocks.revoke,
  },
}));

const station = {
  id: 5,
  ipAddress: '10.0.0.5',
  seatNo: 'A01',
  label: null,
  enabled: true,
} as Workstation;

const binding = {
  id: 9,
  contestId: 1,
  workstationId: 5,
  seatNo: 'A01',
  ipAddress: '10.0.0.5',
  teamId: 3,
  teamName: 'Ball Team',
  pairingCode: 'AB-CD23',
  boundAt: '2026-07-22T01:00:00Z',
  revokedAt: null,
  createdAt: '2026-07-22T01:00:00Z',
} as WorkstationBinding;

describe('AdminCompetitionView', () => {
  beforeEach(() => {
    setLocale('zh-CN');
    vi.clearAllMocks();
    mocks.workstations.mockResolvedValue([station]);
    mocks.bindings.mockResolvedValue([binding]);
    mocks.createWorkstation.mockResolvedValue(undefined);
  });

  it('lists registered workstations on mount', async () => {
    const wrapper = mount(AdminCompetitionView);
    await flushPromises();

    expect(mocks.workstations).toHaveBeenCalledOnce();
    expect(wrapper.text()).toContain('A01');
    expect(wrapper.text()).toContain('10.0.0.5');
  });

  it('loads the contest bindings from the toolbar', async () => {
    const wrapper = mount(AdminCompetitionView);
    await flushPromises();
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('加载比赛绑定'))!
      .trigger('click');
    await flushPromises();

    expect(mocks.bindings).toHaveBeenCalledWith(1);
    expect(wrapper.text()).toContain('Ball Team');
  });

  it('registers a workstation from the toolbar form', async () => {
    const wrapper = mount(AdminCompetitionView);
    await flushPromises();
    await wrapper.find('input[placeholder="终端 IP"]').setValue('10.0.0.9');
    await wrapper.find('input[placeholder="机位号"]').setValue('A09');
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('登记终端'))!
      .trigger('click');
    await flushPromises();

    expect(mocks.createWorkstation).toHaveBeenCalledWith({
      ipAddress: '10.0.0.9',
      seatNo: 'A09',
      label: undefined,
    });
    expect(mocks.workstations).toHaveBeenCalledTimes(2);
  });

  it('surfaces load failures as an alert', async () => {
    mocks.workstations.mockRejectedValueOnce(new Error('会话已过期'));
    const wrapper = mount(AdminCompetitionView);
    await flushPromises();

    expect(wrapper.text()).toContain('会话已过期');
  });
});
