export interface FieldError {
  field: string;
  message: string;
}

export interface ApiErrorBody {
  code?: string;
  error?: string;
  message?: string;
  detail?: string;
  fieldErrors?: FieldError[];
}

export interface PageResponse<T> {
  content: T[];
  page: number;
  size: number;
  totalElements: number;
  totalPages: number;
}

export type UserType =
  | 'TEAM'
  | 'INDIVIDUAL'
  | 'SUPER_ADMIN'
  | 'CONTEST_ADMIN'
  | 'BALLOON_STAFF'
  | 'AWARD_OPERATOR'
  | 'JUDGE'
  | 'PRINTER'
  | 'RESOLVER_OPERATOR'
  | 'SCREEN_OPERATOR'
  | 'LIVE_OPERATOR';

export type StaffUserType = Exclude<UserType, 'TEAM' | 'INDIVIDUAL'>;

export interface AuditLog {
  id: number;
  actorUserId: number | null;
  action: string;
  targetType: string | null;
  targetId: string | null;
  requestIp: string | null;
  result: string;
  createdAt: string;
}

export interface RealtimeOutboxHealth {
  pending: number;
  failed: number;
  redisConnected?: boolean;
}

export interface WorkerFleetHealth {
  online: number;
  stale: number;
  capacity: number;
  activeTasks: number;
}

export interface RabbitMqHealth {
  status: 'up' | 'down';
  queuedTasks: number;
  queuedResults: number;
  deadTasks: number;
}

export interface JudgeDispatchHealth {
  pending: number;
  failed: number;
  workers: WorkerFleetHealth;
  rabbitmq?: RabbitMqHealth;
}

export interface DependencyHealth {
  status: 'up' | 'down';
}

export interface ObjectCleanupHealth {
  pending: number;
  failed: number;
  missingReferences: number;
}

export interface HealthResponse {
  status: 'up' | 'down';
  service: string;
  time: string;
  realtimeOutbox?: RealtimeOutboxHealth;
  judgeDispatch?: JudgeDispatchHealth;
  objectStorage?: DependencyHealth;
  objectCleanup?: ObjectCleanupHealth;
  cups?: DependencyHealth;
}

export interface ContestAdminScope {
  userId: number;
  username: string;
  displayName: string;
  enabled: boolean;
  contestIds: number[];
}

export interface CurrentUser {
  id: number;
  username: string;
  displayName: string;
  userType: UserType;
  roles: string[];
  passwordResetRequired: boolean;
}

export interface StaffAccount {
  id: number;
  username: string;
  displayName: string;
  userType: StaffUserType;
  enabled: boolean;
  passwordResetRequired: boolean;
  lastLoginAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export type ContestStatus =
  | 'DRAFT'
  | 'FROZEN_CONFIG'
  | 'RUNNING'
  | 'PAUSED'
  | 'ENDED'
  | 'ARCHIVED';
export type ContestVisibility = 'PUBLIC' | 'PRIVATE';

export interface Contest {
  id: number;
  name: string;
  status: ContestStatus;
  visibility: ContestVisibility;
  startAt: string | null;
  endAt: string | null;
  freezeAt: string | null;
  version: number;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
}

export interface ContestExtension {
  contestId: number;
  previousEndAt: string;
  endAt: string;
  version: number;
  updatedAt: string;
}

export interface LifecycleTransition {
  contestId: number;
  from: ContestStatus;
  to: ContestStatus;
  version: number;
  transitionedAt: string;
}

export interface Team {
  id: number;
  name: string;
  school: string | null;
  seatNo: string | null;
  groupName: string | null;
  star: boolean;
  version: number;
  account: {
    userId: number;
    username: string;
    enabled: boolean;
    passwordResetRequired: boolean;
  } | null;
  deletedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ContestTeam {
  id: number;
  contestId: number;
  teamId: number;
  teamName: string;
  participationType: 'OFFICIAL' | 'STAR' | 'PRACTICE';
  groupName: string | null;
  createdAt: string;
}

export type JudgeLanguage = 'c' | 'cpp' | 'java' | 'python' | 'output';
export type ProblemAttachmentKind = 'SAMPLE' | 'SUPPLEMENT';

export interface Problem {
  id: number;
  slug: string;
  title: string;
  timeLimitMs: number;
  memoryLimitMb: number;
  outputLimitKb: number;
  languages: JudgeLanguage[];
  testdataVersion: number;
  testdataSha256: string | null;
  defaultLangCode: string;
  createdBy: number | null;
  version: number;
  createdAt: string;
  updatedAt: string;
  judgeMode: 'STANDARD' | 'INTERACTIVE' | 'OUTPUT_ONLY';
  interactorObjectKey: string | null;
  interactorSha256: string | null;
}

export interface ProblemStatement {
  problemId: number;
  langCode: string;
  body: string;
  renderedHtml: string;
  updatedAt: string;
}

export interface ProblemAttachment {
  id: number;
  problemId: number;
  kind: ProblemAttachmentKind;
  originalFilename: string;
  contentType: string | null;
  bytes: number;
  sha256: string;
  createdAt: string;
}

export interface ProblemTestdata {
  problemId: number;
  version: number;
  caseCount: number | null;
  bytes: number | null;
  sha256: string;
  createdAt: string;
}

export interface ProblemTestdataVersion extends ProblemTestdata {
  uploadedByUserId: number | null;
  active: boolean;
}

export interface RejudgeResult {
  submissionId: number;
  previousJudgementId: string;
  judgementId: string;
  status: string;
  queuedAt: string;
}

export interface PublishedStatement {
  langCode: string;
  renderedHtml: string;
  updatedAt: string;
}

export interface ContestProblemAssignment {
  contestId: number;
  problemId: number;
  alias: string;
  displayOrder: number;
  color: string | null;
  createdAt: string;
}

export interface ContestProblem extends Omit<ContestProblemAssignment, 'createdAt'> {
  slug: string;
  title: string;
  timeLimitMs: number;
  memoryLimitMb: number;
  outputLimitKb: number;
  languages: string[];
  statement: PublishedStatement | null;
}

export interface SubmitResult {
  submissionId: number;
  judgementId: string;
  status: string;
  submittedAt: string;
}

export interface SubmissionSummary {
  id: number;
  contestId: number;
  problemId: number;
  problemAlias: string;
  teamId: number;
  teamName: string;
  language: string;
  sourceSizeBytes: number;
  status: string;
  submittedAt: string;
  judgedAt: string | null;
  activeJudgementId: string | null;
  verdict: string | null;
  totalTimeMs: number | null;
  peakMemoryKb: number | null;
  scoreMilli: number | null;
}

export interface SubmissionSimilarityGroup {
  problemId: number;
  language: string;
  fingerprint: string;
  submissionIds: number[];
  teamIds: number[];
  submissionCount: number;
}

export interface SubmissionSimilarityPair {
  problemId: number;
  language: string;
  submissionId: number;
  teamId: number;
  otherSubmissionId: number;
  otherTeamId: number;
  hammingDistance: number;
  similarityPercent: number;
}

export interface SubmissionSimilarityBackfillResult {
  scanned: number;
  updated: number;
  failed: number;
}

export interface RunDetail {
  testIndex: number;
  verdict: string | null;
  timeMs: number | null;
  memoryKb: number | null;
  exitCode: number | null;
  stderrTail: string | null;
}

export interface JudgementDetail {
  id: string;
  verdict: string | null;
  totalTimeMs: number | null;
  peakMemoryKb: number | null;
  compileLog: string | null;
  workerId: string | null;
  startedAt: string | null;
  completedAt: string | null;
  createdAt: string;
  version: number;
  superseded: boolean;
  active: boolean;
  scoreMilli: number | null;
  runs: RunDetail[];
  subtaskScores: JudgementSubtaskScore[];
}

export interface JudgementSubtaskScore {
  subtaskKey: string;
  name: string;
  scoreMilli: number;
  maxScoreMilli: number;
  passedTests: number;
  totalTests: number;
}

export interface SubmissionDetail extends SubmissionSummary {
  source: string;
  sourceSha256: string | null;
  judgements: JudgementDetail[];
}

export interface ScoreboardProblem {
  problemId: number;
  alias: string;
  displayOrder: number;
  firstBloodTeamId: number | null;
  firstBloodAt: string | null;
}

export interface ScoreboardCell {
  problemId: number;
  wrongAttempts: number;
  solved: boolean;
  solvedAt: string | null;
  penaltyMinutes: number;
  scoreMilli: number;
  firstBlood: boolean;
}

export interface ScoreboardRow {
  rank: number;
  officialRank: number | null;
  teamId: number;
  teamName: string;
  school: string | null;
  participationType: string;
  groupName: string | null;
  isStar: boolean;
  solvedCount: number;
  penaltyMinutes: number;
  totalScoreMilli: number;
  lastSolvedAt: string | null;
  problems: ScoreboardCell[];
}

export interface Scoreboard {
  contestId: number;
  variant: string;
  frozen: boolean;
  scoringMode: 'ICPC' | 'OI' | 'IOI';
  scoreAggregation: 'BEST' | 'LAST';
  generatedAt: string;
  problems: ScoreboardProblem[];
  rows: ScoreboardRow[];
}
