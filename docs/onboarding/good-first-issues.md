# Good First Issue Selection Guide

This guide helps new contributors choose an issue that is ready, scoped, and safe to start.

## Before You Claim an Issue

1. Check that the issue has a `good first issue` or `help wanted` label.
2. Check that it is open and has no assignee unless you are the assignee.
3. Read the description, files list, approach, and acceptance criteria.
4. Confirm the issue is in an area you can test locally: docs, website, contracts, or offchain server.
5. Read the linked contribution guide and comment with your plan when the issue asks for confirmation.
6. Wait for maintainer confirmation or assignment before starting work.

## Choose by Area

| Area | Typical files | Useful checks |
|------|---------------|---------------|
| Documentation | `docs/**`, `README.md` | Markdown link check, `npm test` documentation tests |
| Website | `website/**` | `npm test`, `npm run lint`, `npm run format:check` |
| Contracts | `contracts/**` | `cargo fmt`, `cargo clippy`, `cargo test --all` |
| Offchain server | `offchain-server/**` | Offchain README commands and PostgreSQL test setup |
| CI and tooling | `.github/workflows/**`, `Makefile` | Workflow YAML validation and affected package tests |

## Size the Work

A good starter issue usually touches one subsystem and can be verified with one or two commands. Avoid issues that require mainnet keys, production database access, or coordinated contract upgrades unless a maintainer explicitly asks for that work.

## Claiming and Tracking

- Comment with a short implementation plan.
- Wait for assignment or an explicit maintainer confirmation.
- Create a branch named `docs/<issue-id>-short-title`, `fix/<issue-id>-short-title`, or `feature/<issue-id>-short-title`.
- Open one focused PR and include `Closes #<issue-id>` in the description.
- If one PR closes several assigned issues, list every issue with `Closes #<issue-id>`.

## Example Starter Issues

- [Add a Mermaid diagram to a contract API doc](../api/access_control.md)
- [Refresh an onboarding page](README.md)
- [Fix a broken local Markdown link](../README.md)
