# Git Usage Rules

## Commit

1. Commits must be composed of small units. For example, if you need to modify 3 modules to add a single feature, you can commit by module, or even by function within each module. Small commits help your colleagues understand the code more easily. **In short, commit frequently.**
2. Only commit after completion. Do not commit just for saving purposes during development. If you really need to save your work, you can create a new branch and delete it later. **However, please avoid committing just for saving purposes.**
3. Following rule #2, "completion" means that sufficient testing has been done on the changed code. **Please test thoroughly before committing.**
4. **Clearly summarize and make the purpose of each commit explicit.**
5. Rules for commit messages are described separately below.

## Branch and Pull Request

1. A branch is a collection of commits. Once you have completed modifying various code to solve a specific problem or add a feature, bundle those changes into a branch and proceed with the PR process.
2. Reviewers should not be assigned based on hierarchy. Instead, designate people who may be affected by your work.
3. Merge only after all reviewers have approved. Resolve any conflicts of opinion during the review process **by whatever means necessary.**
4. There may be times when you need to merge branches. When merging, we recommend creating a new merge commit. **Avoid Rebase and Merge unless absolutely necessary.** It causes many problems, such as making it difficult to determine when branches were merged and deleted.
5. Rules for branch naming and PR messages are described separately below.

## CI/CD

This project uses **GitHub Actions** for CI/CD configuration.

### Basic Principles

1. All PRs must pass CI verification before merging. **Do not merge PRs that fail CI.**
2. The CI pipeline must include at least **build**, **test**, and **lint** checks.
3. Direct pushes to main/master branch are prohibited. **Always merge through PRs.**
4. CD (deployment) is automatically triggered when tags are created or specific branches are merged.

### Workflow File Location

GitHub Actions workflow files are located at the following path:

```
.github/workflows/
├── ci.yml          # CI pipeline executed on PR and push
├── cd.yml          # Deployment pipeline (executed on tag creation)
└── release.yml     # Release automation (optional)
```

### CI Pipeline Components

| Stage | Description                                |
| ----- | ------------------------------------------ |
| Build | Project build and compilation verification |
| Test  | Unit tests and integration tests execution |

### Precautions

1. **Secrets Management**: Sensitive information such as API keys and tokens must be managed through GitHub Secrets. Never hardcode them in the code.
2. **Utilize Caching**: Actively use dependency caching to reduce build time.
3. **Set Timeouts**: Set appropriate timeouts on workflows to prevent infinite waiting.
4. **Failure Notifications**: It is recommended to set up notifications via Slack or similar when important workflows fail.

---

## Naming Conventions

### Commit

A commit consists of a title and body. The title and body must be **separated by a blank line**, and **the title length should preferably not exceed 50 characters.** Additionally, a common prefix is specified for the title to easily categorize what role each commit serves. Since our rules are not strict, you are free to use CMF (Commit Message Formatter) or similar tools. If the change is simple and detailed information is unnecessary, **the body may be omitted.**

- Title Prefix Rules

| Prefix   | Description                                                                              |
| -------- | ---------------------------------------------------------------------------------------- |
| FEAT     | Add new feature                                                                          |
| FIX      | Bug fix                                                                                  |
| DOCS     | Documentation modifications and additions                                                |
| STYLE    | Code style changes (code formatting, missing semicolons, etc.)                           |
| REFACTOR | Code refactoring                                                                         |
| TEST     | Adding test code, refactoring test code                                                  |
| CHORE    | Build task modifications, package manager modifications (.gitignore, package.json, etc.) |

- Examples

  > FEAT: Add WebServer port configuration feature

  Added functionality to change the web server port (previously fixed at port 80) through configuration at server startup by adding a boundPort key to config.json and reading this key at bootstrap time

  > FEAT: Add DDS Topic QoS configuration feature

  Added functionality to read QoS policies such as Reliability and Durability from external configuration files when creating DataWriter/DataReader

  > FIX: Fix memory leak on DDS Participant termination

  Fixed an issue where Publisher/Subscriber were not properly released during DomainParticipant deletion

### Branch

1. Branch names should be all lowercase. Use hyphens (-) as separators instead of underscores (\_).

- Main Branches

| Branch  | Description                                                       |
| ------- | ----------------------------------------------------------------- |
| main    | Production deployment branch. Always maintain a stable state      |
| release | Pre-deployment testing branch. QA and integration testing         |
| develop | Development integration branch. Where feature branches are merged |

- Working Branch Prefix Rules

| Prefix   | Description                                                        |
| -------- | ------------------------------------------------------------------ |
| feature/ | Adding new features                                                |
| fix/     | Bug fixes                                                          |
| hotfix/  | Urgent fixes for operational issues that bypass the review process |

- Examples

> feature/dds-topic-qos-configuration
> fix/participant-memory-leak
> hotfix/connection-timeout-crash

### Tag

1. Tags should include the 'v' prefix in the following format:

   v1.2.3

### PR / Issue

1. All PRs and Issues should use registered templates.

```
.github/PULL_REQUEST_TEMPLATE.md
.github/ISSUE_TEMPLATE/bug_report.md
.github/ISSUE_TEMPLATE/enhancement_request.md
```
