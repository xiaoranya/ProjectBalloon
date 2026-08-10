---
title: Documentation Style Guide
description: How ProjectBalloon documentation is written for the published
  documentation website: audience, page anatomy, tone, links, and review checklist.
---

# Documentation Style Guide

This guide defines how ProjectBalloon documentation is written for the
published documentation website. It applies to every page under `docs/` that
is part of the manual: user guides, operations guides, and reference pages.
Internal engineering notes (architecture, development, requirements) follow the
same structural rules but are not published as part of the user-facing manual.

## Audience First

Every page must make its audience and purpose obvious in the first paragraph:

- **User guides** are for people who operate the platform through the web UI:
  contestants, contest administrators, and on-site staff.
- **Operations guides** are for the people who install, run, and recover the
  deployment on site.
- **Reference pages** (configuration variables, API contract) are for people
  who need exact values or endpoints.
- **Internal notes** (architecture, development, requirements) are for people
  who change the codebase.

Never assume the reader has read the repository. Prefer describing what a
reader does and sees over how the code implements it.

## Page Anatomy

Every published page has the same skeleton:

1. **Frontmatter** — YAML with `title` and `description`:

   ```yaml
   ---
   title: Contestant Guide
   description: How contestants register, submit code, read verdicts, and use
     the scoreboard, Clarifications, and printing.
   ---
   ```

2. **H1 title** — matches `title`, is a noun phrase ("Contestant Guide"), not
   a sentence.
3. **Introduction** — one or two sentences stating the audience and what the
   page covers. Optional "Before you start" line linking prerequisites.
4. **Task-oriented body** — steps the reader performs, in the order they
   perform them. Use `##` for sections and `###` for sub-tasks.
5. **Reference material** — tables or lists for exact values (routes,
   permissions, verdicts, variables).
6. **"See also"** — related pages as a short list at the end.

## Voice and Tone

- Use the imperative mood: "Import teams", "Verify the health endpoint".
- Use active voice and present tense.
- Write short paragraphs and single-purpose bullets.
- Prefer a procedure over a bullet dump: if the reader must do things in
  order, number the steps.
- Do not include repository-internal status ("Status", "Document Map",
  "Documentation Rules", "Source Layout") in published pages. Move that kind
  of content here or into the repository README.
- Avoid "This document describes ..." openings. State the outcome directly.

## Links

- Use links relative to the `docs/` directory, not repository-root paths:
  - Correct: `[Installation](ops/install.md)`
  - Wrong: `docs/ops/install.md`
- Write descriptive link text ("see the Installation guide"), not bare paths.
- Keep the `.md` extension so links work on GitHub and in the site builder.
- Use one link per related page; do not link the same target repeatedly in
  one page.

## Code and Commands

- Put commands in fenced code blocks without a `$` prompt.
- Use `text` fences for shell output and `yaml`/`json`/`toml` for structured
  content.
- Commands and paths in the manual assume the deployed system
  (`/etc/project-balloon`, `sudo systemctl ...`). Development commands belong
  in the development section.
- Never include real secrets, tokens, or passwords.

## Terminology

- Product name: **ProjectBalloon**.
- Routes, permission codes, environment variables, and verdicts stay in
  English in both languages: `/contests`, `CONTEST_MANAGE`,
  `PROJECT_BALLOON_SESSION_TTL_SECONDS`, `ACCEPTED`.
- Keep terminology identical to the UI labels where possible.
- "Contestant" means a team member; "staff" means contest personnel with
  workbench permissions; "administrator" means an admin-UI user.

## Language and Mirroring

- English is canonical; every English page has a Chinese mirror under
  `docs/zh-CN/` with the same relative path and structure.
- Keep the two versions in sync in the same change.
- Machine-generated content (the OpenAPI contract) is not mirrored.

## Keeping Documentation Current

- Update the affected manual pages in the same change as a behavior,
  configuration, route, or workflow change.
- When a page changes, check every link to and from it.
- Before publishing, run the checklist in "Review Checklist" below.

## Review Checklist

- [ ] Frontmatter has `title` and `description`.
- [ ] First paragraph states audience and scope.
- [ ] Steps are numbered where order matters; bullets are single-purpose.
- [ ] Links are `docs/`-relative with descriptive text and no `docs/` prefix.
- [ ] No repository-internal status or layout notes in published pages.
- [ ] Chinese mirror updated in the same change.
- [ ] Terminology matches the UI and this guide.
