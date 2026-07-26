# Observability Configuration

This directory is the standalone Rust deployment input for Prometheus,
Grafana, Loki, and Promtail. It does not depend on the nested legacy Java
project.

- Mount `prometheus/prometheus.yml` at `/etc/prometheus/prometheus.yml`.
- Mount `prometheus/rules/` at `/etc/prometheus/rules/`.
- Mount `loki/loki.yml` at `/etc/loki/config.yml` and persist `/loki`.
- Mount `promtail/promtail.yml` at `/etc/promtail/config.yml`, the Docker socket
  read-only, and persist `/var/lib/promtail`.
- Mount Grafana `provisioning/` and `dashboards/` at their matching paths.

The service names in these files (`api`, `prometheus`, `loki`, `rabbitmq`,
`postgres-exporter`, and `node-exporter`) are the required Compose DNS names.
Do not publish Prometheus, Loki, Promtail, or `/metrics` outside the monitoring
network. Grafana authentication must use an operator-provided admin password.

Start after the application/data network exists:

```text
GRAFANA_ADMIN_PASSWORD='<strong secret>' \
POSTGRES_EXPORTER_DSN='postgresql://monitor:<password>@postgres:5432/project_balloon?sslmode=disable' \
docker compose -f deploy/observability/compose.yml up -d
```

Grafana binds to `127.0.0.1:3000` by default. Override
`GRAFANA_BIND_ADDRESS` only when a protected reverse proxy is configured.
