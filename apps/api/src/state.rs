use std::{sync::Arc, time::Duration};

use sqlx::PgPool;
use tokio::sync::watch;

use crate::config::DeploymentMode;
use crate::features::{
    announcements::AnnouncementService,
    audit_logs::AuditLogService,
    auth::{AuthService, CsrfSigner},
    awards::AwardService,
    balloons::BalloonService,
    clarifications::ClarificationService,
    competition::CompetitionService,
    contest_management_scopes::ContestManagementScopeService,
    contest_problems::ContestProblemService,
    contests::ContestService,
    judge_dispatch::RabbitJudgeTaskPublisher,
    presentation::PresentationService,
    printing::{CupsGateway, PrintingService},
    problems::ProblemService,
    realtime::{RealtimeHub, RealtimeOutbox},
    redis::RedisHandle,
    resolver::ResolverService,
    scoreboard::{ScoreboardCache, ScoreboardProjection, ScoreboardService},
    staff_accounts::StaffAccountService,
    submissions::{BatchRejudgeService, SubmissionService},
    teams::TeamService,
};
use crate::object_storage::ObjectStorageHandle;

#[derive(Clone)]
pub struct AppState {
    database: PgPool,
    readiness_timeout: Duration,
    deployment_mode: DeploymentMode,
    /// Optional bearer token guarding `/metrics`; `None` keeps it open for
    /// loop-back or firewalled deployments.
    metrics_token: Option<Arc<str>>,
    auth: Arc<AuthService>,
    awards: Arc<AwardService>,
    balloons: Arc<BalloonService>,
    csrf: Arc<CsrfSigner>,
    clarifications: Arc<ClarificationService>,
    competition: Arc<CompetitionService>,
    announcements: Arc<AnnouncementService>,
    staff_accounts: Arc<StaffAccountService>,
    contest_management_scopes: Arc<ContestManagementScopeService>,
    contest_problems: Arc<ContestProblemService>,
    audit_logs: Arc<AuditLogService>,
    contests: Arc<ContestService>,
    problems: Arc<ProblemService>,
    printing: Arc<PrintingService>,
    presentation: Arc<PresentationService>,
    realtime: RealtimeHub,
    outbox: Option<RealtimeOutbox>,
    scoreboard_projection: Option<ScoreboardProjection>,
    resolver: Arc<ResolverService>,
    scoreboard: Arc<ScoreboardService>,
    submissions: Arc<SubmissionService>,
    batch_rejudge: Arc<BatchRejudgeService>,
    teams: Arc<TeamService>,
    object_storage: Option<ObjectStorageHandle>,
    judge_publisher: Option<Arc<RabbitJudgeTaskPublisher>>,
    cups_gateway: Option<Arc<dyn CupsGateway>>,
    /// Notified with `true` when the process begins shutting down. SSE streams
    /// select on it so long-lived responses end instead of deadlocking the
    /// graceful shutdown. Defaults to an already-dropped sender, which keeps
    /// streams on their natural termination (hub close) when not wired up.
    shutdown: watch::Receiver<bool>,
}

impl AppState {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        database: PgPool,
        readiness_timeout: Duration,
        session_ttl: Duration,
        secure_cookies: bool,
        csrf_secret: &[u8],
        realtime_channel_capacity: usize,
        realtime_redis_enabled: bool,
        redis: Option<RedisHandle>,
    ) -> Self {
        Self::build(
            database,
            readiness_timeout,
            session_ttl,
            secure_cookies,
            csrf_secret,
            realtime_channel_capacity,
            realtime_redis_enabled,
            redis,
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
        redis: Option<RedisHandle>,
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
            redis,
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
        redis: Option<RedisHandle>,
        object_storage: Option<ObjectStorageHandle>,
    ) -> Self {
        // The realtime outbox lives exclusively in Redis; without a handle the
        // enqueues degrade to logged no-ops (see `realtime::outbox`).
        let outbox = redis.as_ref().map(|handle| RealtimeOutbox::new(handle.clone()));
        // The live scoreboard projection is likewise Redis-native; without a
        // handle the judgement path keeps the PostgreSQL projection.
        let scoreboard_projection =
            redis.as_ref().map(|handle| ScoreboardProjection::new(handle.clone()));
        let auth = Arc::new(
            AuthService::new(database.clone(), session_ttl, secure_cookies)
                .with_redis_option(redis),
        );
        let awards =
            Arc::new(AwardService::new(database.clone()).with_outbox_option(outbox.clone()));
        let balloons =
            Arc::new(BalloonService::new(database.clone()).with_outbox_option(outbox.clone()));
        let csrf = Arc::new(CsrfSigner::new(csrf_secret));
        let clarifications = Arc::new(
            ClarificationService::new(database.clone()).with_outbox_option(outbox.clone()),
        );
        let competition = Arc::new(CompetitionService::new(database.clone()));
        let announcements =
            Arc::new(AnnouncementService::new(database.clone()).with_outbox_option(outbox.clone()));
        let staff_accounts = Arc::new(StaffAccountService::new(database.clone()));
        let contest_management_scopes =
            Arc::new(ContestManagementScopeService::new(database.clone()));
        let contest_problems = Arc::new(ContestProblemService::new(database.clone()));
        let audit_logs = Arc::new(AuditLogService::new(database.clone()));
        let contests =
            Arc::new(ContestService::new(database.clone()).with_outbox_option(outbox.clone()));
        let problems = Arc::new(ProblemService::new(database.clone()));
        let printing =
            Arc::new(PrintingService::new(database.clone()).with_outbox_option(outbox.clone()));
        let presentation =
            Arc::new(PresentationService::new(database.clone()).with_outbox_option(outbox.clone()));
        let realtime = RealtimeHub::new(realtime_channel_capacity, realtime_redis_enabled);
        let resolver =
            Arc::new(ResolverService::new(database.clone()).with_outbox_option(outbox.clone()));
        let scoreboard = Arc::new(
            ScoreboardService::new(database.clone())
                .with_projection_option(scoreboard_projection.clone()),
        );
        let submissions = Arc::new(
            SubmissionService::new(database.clone())
                .with_outbox_option(outbox.clone())
                .with_projection_option(scoreboard_projection.clone()),
        );
        let batch_rejudge = Arc::new(BatchRejudgeService::new(database.clone()));
        let teams = Arc::new(TeamService::new(database.clone()).with_outbox_option(outbox.clone()));
        let (_, shutdown) = watch::channel(false);
        Self {
            database,
            readiness_timeout,
            deployment_mode: DeploymentMode::Standard,
            metrics_token: None,
            auth,
            awards,
            balloons,
            csrf,
            clarifications,
            competition,
            announcements,
            staff_accounts,
            contest_management_scopes,
            contest_problems,
            audit_logs,
            contests,
            problems,
            printing,
            presentation,
            realtime,
            outbox,
            scoreboard_projection,
            resolver,
            scoreboard,
            submissions,
            batch_rejudge,
            teams,
            object_storage,
            judge_publisher: None,
            cups_gateway: None,
            shutdown,
        }
    }

    /// Wires the process shutdown channel into SSE streams. Must share the
    /// same watch channel the background runners use, so one signal ends both.
    #[must_use]
    pub fn with_shutdown(mut self, shutdown: watch::Receiver<bool>) -> Self {
        self.shutdown = shutdown;
        self
    }

    #[must_use]
    pub fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.shutdown.clone()
    }

    #[must_use]
    pub fn with_judge_publisher(mut self, publisher: Arc<RabbitJudgeTaskPublisher>) -> Self {
        self.judge_publisher = Some(publisher);
        self
    }

    #[must_use]
    pub fn with_deployment_mode(mut self, mode: DeploymentMode) -> Self {
        self.deployment_mode = mode;
        self.contests = Arc::new(
            ContestService::new(self.database.clone())
                .with_competition_mode(mode.is_competition())
                .with_outbox_option(self.outbox.clone()),
        );
        self
    }

    #[must_use]
    pub fn with_metrics_token(mut self, token: Option<String>) -> Self {
        self.metrics_token = token.map(Arc::from);
        self
    }

    #[must_use]
    pub fn metrics_token(&self) -> Option<&str> {
        self.metrics_token.as_deref()
    }

    #[must_use]
    pub fn with_scoreboard_cache(mut self, cache: ScoreboardCache) -> Self {
        self.scoreboard = Arc::new(
            ScoreboardService::new(self.database.clone())
                .with_cache(cache)
                .with_projection_option(self.scoreboard_projection.clone()),
        );
        self
    }

    /// The Redis live-scoreboard projection handle; shared by the judge result
    /// consumer, the rejudge path, and the contest lifecycle runner.
    #[must_use]
    pub fn scoreboard_projection(&self) -> Option<ScoreboardProjection> {
        self.scoreboard_projection.clone()
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
    pub const fn deployment_mode(&self) -> DeploymentMode {
        self.deployment_mode
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
    pub fn competition(&self) -> &CompetitionService {
        &self.competition
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
    pub fn contest_management_scopes(&self) -> &ContestManagementScopeService {
        &self.contest_management_scopes
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

    /// The Redis realtime outbox handle; `None` only in processes started
    /// without Redis (enqueues then degrade to logged no-ops).
    #[must_use]
    pub fn realtime_outbox(&self) -> Option<&RealtimeOutbox> {
        self.outbox.as_ref()
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
