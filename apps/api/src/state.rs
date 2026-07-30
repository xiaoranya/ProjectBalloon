use std::{sync::Arc, time::Duration};

use sqlx::PgPool;

use crate::features::{
    announcements::AnnouncementService,
    audit_logs::AuditLogService,
    auth::{AuthService, CsrfSigner},
    awards::AwardService,
    balloons::BalloonService,
    clarifications::ClarificationService,
    contest_admin_scopes::ContestAdminScopeService,
    contest_problems::ContestProblemService,
    contests::ContestService,
    judge_dispatch::RabbitJudgeTaskPublisher,
    presentation::PresentationService,
    printing::{CupsGateway, PrintingService},
    problems::ProblemService,
    realtime::RealtimeHub,
    resolver::ResolverService,
    scoreboard::{ScoreboardCache, ScoreboardService},
    staff_accounts::StaffAccountService,
    submissions::{BatchRejudgeService, SubmissionService},
    teams::TeamService,
};
use crate::object_storage::ObjectStorageHandle;

#[derive(Clone)]
pub struct AppState {
    database: PgPool,
    readiness_timeout: Duration,
    auth: Arc<AuthService>,
    awards: Arc<AwardService>,
    balloons: Arc<BalloonService>,
    csrf: Arc<CsrfSigner>,
    clarifications: Arc<ClarificationService>,
    announcements: Arc<AnnouncementService>,
    staff_accounts: Arc<StaffAccountService>,
    contest_admin_scopes: Arc<ContestAdminScopeService>,
    contest_problems: Arc<ContestProblemService>,
    audit_logs: Arc<AuditLogService>,
    contests: Arc<ContestService>,
    problems: Arc<ProblemService>,
    printing: Arc<PrintingService>,
    presentation: Arc<PresentationService>,
    realtime: RealtimeHub,
    resolver: Arc<ResolverService>,
    scoreboard: Arc<ScoreboardService>,
    submissions: Arc<SubmissionService>,
    batch_rejudge: Arc<BatchRejudgeService>,
    teams: Arc<TeamService>,
    object_storage: Option<ObjectStorageHandle>,
    judge_publisher: Option<Arc<RabbitJudgeTaskPublisher>>,
    cups_gateway: Option<Arc<dyn CupsGateway>>,
}

impl AppState {
    #[must_use]
    pub fn new(
        database: PgPool,
        readiness_timeout: Duration,
        session_ttl: Duration,
        secure_cookies: bool,
        csrf_secret: &[u8],
        realtime_channel_capacity: usize,
        realtime_redis_enabled: bool,
    ) -> Self {
        Self::build(
            database,
            readiness_timeout,
            session_ttl,
            secure_cookies,
            csrf_secret,
            realtime_channel_capacity,
            realtime_redis_enabled,
            None,
        )
    }

    #[must_use]
    // State construction keeps each runtime dependency explicit for startup validation.
    #[allow(clippy::too_many_arguments)]
    pub fn with_object_storage(
        database: PgPool,
        readiness_timeout: Duration,
        session_ttl: Duration,
        secure_cookies: bool,
        csrf_secret: &[u8],
        realtime_channel_capacity: usize,
        realtime_redis_enabled: bool,
        object_storage: ObjectStorageHandle,
    ) -> Self {
        Self::build(
            database,
            readiness_timeout,
            session_ttl,
            secure_cookies,
            csrf_secret,
            realtime_channel_capacity,
            realtime_redis_enabled,
            Some(object_storage),
        )
    }

    // State construction keeps each runtime dependency explicit for startup validation.
    #[allow(clippy::too_many_arguments)]
    fn build(
        database: PgPool,
        readiness_timeout: Duration,
        session_ttl: Duration,
        secure_cookies: bool,
        csrf_secret: &[u8],
        realtime_channel_capacity: usize,
        realtime_redis_enabled: bool,
        object_storage: Option<ObjectStorageHandle>,
    ) -> Self {
        let auth = Arc::new(AuthService::new(database.clone(), session_ttl, secure_cookies));
        let awards = Arc::new(AwardService::new(database.clone()));
        let balloons = Arc::new(BalloonService::new(database.clone()));
        let csrf = Arc::new(CsrfSigner::new(csrf_secret));
        let clarifications = Arc::new(ClarificationService::new(database.clone()));
        let announcements = Arc::new(AnnouncementService::new(database.clone()));
        let staff_accounts = Arc::new(StaffAccountService::new(database.clone()));
        let contest_admin_scopes = Arc::new(ContestAdminScopeService::new(database.clone()));
        let contest_problems = Arc::new(ContestProblemService::new(database.clone()));
        let audit_logs = Arc::new(AuditLogService::new(database.clone()));
        let contests = Arc::new(ContestService::new(database.clone()));
        let problems = Arc::new(ProblemService::new(database.clone()));
        let printing = Arc::new(PrintingService::new(database.clone()));
        let presentation = Arc::new(PresentationService::new(database.clone()));
        let realtime = RealtimeHub::new(realtime_channel_capacity, realtime_redis_enabled);
        let resolver = Arc::new(ResolverService::new(database.clone()));
        let scoreboard = Arc::new(ScoreboardService::new(database.clone()));
        let submissions = Arc::new(SubmissionService::new(database.clone()));
        let batch_rejudge = Arc::new(BatchRejudgeService::new(database.clone()));
        let teams = Arc::new(TeamService::new(database.clone()));
        Self {
            database,
            readiness_timeout,
            auth,
            awards,
            balloons,
            csrf,
            clarifications,
            announcements,
            staff_accounts,
            contest_admin_scopes,
            contest_problems,
            audit_logs,
            contests,
            problems,
            printing,
            presentation,
            realtime,
            resolver,
            scoreboard,
            submissions,
            batch_rejudge,
            teams,
            object_storage,
            judge_publisher: None,
            cups_gateway: None,
        }
    }

    #[must_use]
    pub fn with_judge_publisher(mut self, publisher: Arc<RabbitJudgeTaskPublisher>) -> Self {
        self.judge_publisher = Some(publisher);
        self
    }

    #[must_use]
    pub fn with_scoreboard_cache(mut self, cache: ScoreboardCache) -> Self {
        self.scoreboard = Arc::new(ScoreboardService::new(self.database.clone()).with_cache(cache));
        self
    }

    #[must_use]
    pub fn with_cups_gateway(mut self, gateway: Arc<dyn CupsGateway>) -> Self {
        self.cups_gateway = Some(gateway);
        self
    }

    #[must_use]
    pub const fn database(&self) -> &PgPool {
        &self.database
    }

    #[must_use]
    pub const fn readiness_timeout(&self) -> Duration {
        self.readiness_timeout
    }

    #[must_use]
    pub fn auth(&self) -> &AuthService {
        &self.auth
    }

    #[must_use]
    pub fn awards(&self) -> &AwardService {
        &self.awards
    }

    #[must_use]
    pub fn balloons(&self) -> &BalloonService {
        &self.balloons
    }

    #[must_use]
    pub fn csrf(&self) -> &CsrfSigner {
        &self.csrf
    }

    #[must_use]
    pub fn clarifications(&self) -> &ClarificationService {
        &self.clarifications
    }

    #[must_use]
    pub fn announcements(&self) -> &AnnouncementService {
        &self.announcements
    }

    #[must_use]
    pub fn staff_accounts(&self) -> &StaffAccountService {
        &self.staff_accounts
    }

    #[must_use]
    pub fn contest_admin_scopes(&self) -> &ContestAdminScopeService {
        &self.contest_admin_scopes
    }

    #[must_use]
    pub fn contest_problems(&self) -> &ContestProblemService {
        &self.contest_problems
    }

    #[must_use]
    pub fn audit_logs(&self) -> &AuditLogService {
        &self.audit_logs
    }

    #[must_use]
    pub fn contests(&self) -> &ContestService {
        &self.contests
    }

    #[must_use]
    pub fn problems(&self) -> &ProblemService {
        &self.problems
    }

    #[must_use]
    pub fn printing(&self) -> &PrintingService {
        &self.printing
    }

    #[must_use]
    pub fn presentation(&self) -> &PresentationService {
        &self.presentation
    }

    #[must_use]
    pub const fn realtime(&self) -> &RealtimeHub {
        &self.realtime
    }

    #[must_use]
    pub fn resolver(&self) -> &ResolverService {
        &self.resolver
    }

    #[must_use]
    pub fn scoreboard(&self) -> &ScoreboardService {
        &self.scoreboard
    }

    #[must_use]
    pub fn submissions(&self) -> &SubmissionService {
        &self.submissions
    }

    #[must_use]
    pub fn batch_rejudge(&self) -> &BatchRejudgeService {
        &self.batch_rejudge
    }

    #[must_use]
    pub fn teams(&self) -> &TeamService {
        &self.teams
    }

    #[must_use]
    pub const fn object_storage(&self) -> Option<&ObjectStorageHandle> {
        self.object_storage.as_ref()
    }

    #[must_use]
    pub fn judge_publisher(&self) -> Option<&Arc<RabbitJudgeTaskPublisher>> {
        self.judge_publisher.as_ref()
    }

    #[must_use]
    pub fn cups_gateway(&self) -> Option<&Arc<dyn CupsGateway>> {
        self.cups_gateway.as_ref()
    }
}
