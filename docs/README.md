---
title: ProjectBalloon Documentation
description: >-
  The manual for ProjectBalloon: installing the platform, running an
  official contest, and using the web interface as a contestant, administrator,
  or on-site staff member.
---

# ProjectBalloon Documentation

ProjectBalloon is an offline XCPC/ICPC contest platform. This documentation is
the manual for installing the platform, running an official contest, and using
the web interface as a contestant, administrator, or on-site staff member.

## Where to Start

| If you are... | Start here |
| --- | --- |
| Setting up a new deployment | [Quickstart](ops/quickstart.md) |
| A contestant using the platform | [Contestant Guide](user/contestant/) |
| A contest administrator | [Administrator Guide](user/admin/) |
| On-site staff (screens, balloons, printing, Resolver, awards) | [On-Site Operations Guide](user/onsite/) |
| Running an on-site contest with workstation login | [Competition Mode](ops/competition-mode.md) |
| Deploying or operating the server | [Installation](ops/install.md) and [Operations](ops/ops.md) |
| Looking up a configuration variable or an endpoint | [Configuration Reference](ops/configuration.md) and [API contract](https://github.com/xiaoranya/ProjectBalloon/blob/main/docs/api/openapi.yaml) |

## Documentation Sections

- **User Guide** — `user/`: role-based manuals for contestants (including
  the [Daily Practice Guide](user/contestant/practice.md)), contest
  administrators, and on-site staff. These pages assume the deployment
  described in the [Quickstart](ops/quickstart.md) is running.
- **Operations** — `ops/`: install, configure, operate, troubleshoot, back up,
  recover, and pressure-test a deployment.
- **Reference** — exact values for environment variables, routes, permissions,
  and the HTTP contract.
- **Development and Internals** — `architecture/`, `dev/`, `api/`, and
  `requirements/`: system design, coding rules, and requirement traceability.
  These are engineering documents kept in the repository; they are not part of
  the published manual.

## Language

English is the canonical language. Every page has a Chinese mirror under
`zh-CN/` with the same relative path; code blocks, commands, routes, and
permission codes stay in English in both languages. The OpenAPI contract
(`api/openapi.yaml`) is machine-generated and is not mirrored.

## Writing for This Site

New and changed documentation must follow the
[Documentation Style Guide](STYLE.md). See also
[`CONTRIBUTING.md`](https://github.com/xiaoranya/ProjectBalloon/blob/main/CONTRIBUTING.md) for contribution workflow.
