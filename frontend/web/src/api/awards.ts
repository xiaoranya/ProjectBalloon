import { apiRequest } from './client';

export type AwardRuleType = 'FIXED_COUNT' | 'RATIO' | 'RANK_RANGE';
export interface AwardRule { ruleType: AwardRuleType; ratio: number | null; fixedCount: number | null; rankFrom: number | null; rankTo: number | null; }
export interface AwardCategory { id: number; contestId: number; code: string; name: string; displayOrder: number; includeStar: boolean; groupName: string | null; participationType: 'OFFICIAL' | 'STAR' | 'PRACTICE' | null; firstBlood: boolean; version: number; ruleType: AwardRuleType; ratio: number | null; fixedCount: number | null; rankFrom: number | null; rankTo: number | null; }
export type AwardCategoryPayload = Omit<AwardCategory, 'id' | 'contestId' | 'version' | 'ruleType' | 'ratio' | 'fixedCount' | 'rankFrom' | 'rankTo'> & { rule: AwardRule };
export interface AwardRecipient { id: number; categoryId: number; categoryCode: string; categoryName: string; teamId: number; teamName: string; school: string | null; rank: number | null; solved: number | null; penaltyMinutes: number | null; participationType: string | null; groupName: string | null; isStar: boolean; isManual: boolean; }
export interface AwardConflict { teamId: number; teamName: string; categoryCodes: string[]; }
export interface AwardSet { id: number; contestId: number; resolverRunId: number; finalScoreboardSnapshotId: number; status: 'DRAFT' | 'FROZEN'; version: number; generatedAt: string; frozenAt: string | null; recipients: AwardRecipient[]; conflicts: AwardConflict[]; }
export interface AwardCandidate { teamId: number; teamName: string; school: string | null; rank: number; participationType: string; groupName: string | null; isStar: boolean; }
export interface AwardResolverRun { id: number; completedAt: string; }
export type AwardPresentationStatus = 'WAITING' | 'PRESENTING' | 'COMPLETED';
export interface AwardPresentationRecipient { id: number; problemId: number | null; problemAlias: string | null; teamId: number; teamName: string; school: string | null; seatNo: string | null; groupName: string | null; participationType: string | null; star: boolean; rank: number | null; solved: number | null; penaltyMinutes: number | null; }
export interface AwardPresentationCategory { id: number; code: string; name: string; displayOrder: number; groupName: string | null; firstBlood: boolean; recipients: AwardPresentationRecipient[]; }
export interface AwardPresentation { contestId: number; contestName: string; contestStatus: string; serverTime: string; status: AwardPresentationStatus; currentCategoryId: number; autoRotate: boolean; intervalSeconds: number; stateUpdatedAt: string; categories: AwardPresentationCategory[]; }
export interface AwardHostScriptSection { categoryId: number; code: string; name: string; firstBlood: boolean; current: boolean; cueText: string; recipients: AwardPresentationRecipient[]; }
export interface AwardHostScript { contestId: number; contestName: string; serverTime: string; presentationStatus: AwardPresentationStatus; currentCategoryId: number; nextCategoryId: number | null; autoRotate: boolean; intervalSeconds: number; stateUpdatedAt: string; version: number | null; updatedAt: string | null; openingText: string; closingText: string; sections: AwardHostScriptSection[]; }

export const awardsApi = {
  listCategories(contestId: number) { return apiRequest<AwardCategory[]>(`/api/admin/contests/${contestId}/award-categories`); },
  createCategory(contestId: number, payload: AwardCategoryPayload) { return apiRequest<AwardCategory>(`/api/admin/contests/${contestId}/award-categories`, { method: 'POST', body: payload }); },
  updateCategory(id: number, expectedVersion: number, payload: AwardCategoryPayload) { return apiRequest<AwardCategory>(`/api/admin/award-categories/${id}`, { method: 'PUT', body: { expectedVersion, ...payload } }); },
  deleteCategory(id: number, expectedVersion: number) { return apiRequest<void>(`/api/admin/award-categories/${id}`, { method: 'DELETE', body: { expectedVersion } }); },
  completedRuns(contestId: number) { return apiRequest<AwardResolverRun[]>(`/api/admin/contests/${contestId}/awards/resolver-runs`); },
  get(contestId: number) { return apiRequest<AwardSet>(`/api/admin/contests/${contestId}/awards`); },
  generate(contestId: number, resolverRunId: number) { return apiRequest<AwardSet>(`/api/admin/contests/${contestId}/awards`, { method: 'POST', body: { resolverRunId } }); },
  candidates(contestId: number) { return apiRequest<AwardCandidate[]>(`/api/admin/contests/${contestId}/awards/candidates`); },
  addRecipient(contestId: number, categoryId: number, teamId: number, expectedSetVersion: number) { return apiRequest<AwardSet>(`/api/admin/contests/${contestId}/awards/manual`, { method: 'POST', body: { categoryId, teamId, expectedSetVersion } }); },
  removeRecipient(id: number, expectedVersion: number) { return apiRequest<AwardSet>(`/api/admin/award-recipients/${id}`, { method: 'DELETE', body: { expectedVersion } }); },
  freeze(contestId: number, expectedVersion: number) { return apiRequest<AwardSet>(`/api/admin/contests/${contestId}/awards/freeze`, { method: 'POST', body: { expectedVersion } }); },
  unfreeze(contestId: number, expectedVersion: number) { return apiRequest<AwardSet>(`/api/admin/contests/${contestId}/awards/unfreeze`, { method: 'POST', body: { expectedVersion } }); },
  csv(contestId: number) { return apiRequest<Blob>(`/api/admin/contests/${contestId}/awards.csv`, { responseType: 'blob', headers: { Accept: 'text/csv' } }); },
  presentation(contestId: number) { return apiRequest<AwardPresentation>(`/api/public/contests/${contestId}/awards/presentation`); },
  updatePresentation(contestId: number, payload: { currentCategoryId: number | null; status: AwardPresentationStatus; autoRotate: boolean; intervalSeconds: number }) { return apiRequest<AwardPresentation>(`/api/contests/${contestId}/awards/presentation`, { method: 'PUT', body: payload }); },
  hostScript(contestId: number) { return apiRequest<AwardHostScript>(`/api/contests/${contestId}/awards/host-script`); },
  saveHostScript(contestId: number, payload: { openingText: string; closingText: string; sections: Array<{ categoryId: number; cueText: string }>; expectedVersion: number | null }) { return apiRequest<AwardHostScript>(`/api/contests/${contestId}/awards/host-script`, { method: 'PUT', body: payload }); },
  certificates(contestId: number) { return apiRequest<Blob>(`/api/contests/${contestId}/awards/certificates/export`, { responseType: 'blob', headers: { Accept: 'text/csv' } }); },
};
